use anyhow::Result;
use crossterm::style::Stylize;
use std::io::Write;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::app::config::{Config, SessionEntry};
use crate::terminal::render::render_qr;
use crate::whatsapp::client::WhatsAppClient;
use crate::whatsapp::models::Session;

/// `wpp login [PHONE] [--phone NUMBER] [--qr] [--alias NAME]`
///
/// Creates or reuses an OpenWA session, shows QR or pairing code,
/// waits for authentication, and saves the session to config.
///
/// After QR authentication, duplicate OpenWA sessions belonging
/// to the same WhatsApp phone are reconciled automatically.
pub async fn run(phone: Option<String>, alias: Option<String>) -> Result<()> {
    let mut config = Config::load()?;

    // Only reject duplicates when the user explicitly provided
    // an alias. When omitted, an existing same-phone session
    // can be reused.
    if let Some(ref alias) = alias {
        if config.alias_exists(alias) {
            eprintln!(
                "{}",
                format!(
                    "Session alias '{alias}' already exists. Use \
           `wpp switch {alias}` to activate it, \
           or choose a different alias with \
           `wpp login --alias <name>`."
                )
                .yellow()
            );

            return Ok(());
        }
    }

    let api_key = config.openwa_api_key();

    let client = WhatsAppClient::openwa(&config.base_url, api_key.clone());

    // ── 1. Reuse an existing session when the phone is known ───

    let existing_session = match phone.as_deref() {
        Some(phone_number) => find_reusable_session(&mut config, &client, phone_number).await?,

        None => None,
    };

    let id = match existing_session.as_ref() {
        Some(session) => {
            eprintln!(
                "{}",
                format!(" Reusing existing session '{}'...", session.name).cyan()
            );

            session.id.clone()
        }

        None => {
            // ── 2. Create session in OpenWA ──────────────────────

            eprintln!("{}", " Creating session...".dark_grey());

            // The OpenWA session name is independent from the
            // user-facing wpp alias.
            let session_name = generate_session_name();

            let session = match client.create_session(&session_name).await {
                Ok(s) => s,

                Err(e) => {
                    match &e {
                        crate::error::WppError::Api { status: 401, .. } => {
                            eprintln!("{} OpenWA authentication failed (401)", "✗".red().bold());

                            eprintln!(
                                " {}",
                                "Set WPP_OPENWA_API_KEY or OPENWA_API_KEY \
                     (or place .api-key in openwa/data/)."
                                    .dark_grey()
                            );
                        }

                        _ => {
                            eprintln!(
                                "{} Could not connect to OpenWA at {}",
                                "✗".red().bold(),
                                config.base_url
                            );

                            eprintln!(
                                " {}",
                                "Make sure OpenWA is running \
                     (e.g. `cd openwa && npm run dev`)"
                                    .dark_grey()
                            );
                        }
                    }

                    return Err(e.into());
                }
            };

            session.id.clone()
        }
    };

    // ── 3. Reused session is already authenticated ─────────────

    if let Some(session) = existing_session.as_ref() {
        if session.status == "ready" {
            return activate_ready_session(&mut config, &session, alias, api_key);
        }
    }

    // ── 4. Start the session engine ────────────────────────────

    eprintln!("{}", " Starting engine...".dark_grey());

    client.start_session(&id).await?;

    // Give the engine a moment to initialise and generate a QR.
    tokio::time::sleep(std::time::Duration::from_secs(3)).await;

    // ── 5. Show QR or pairing code ─────────────────────────────

    if let Some(ref phone_number) = phone {
        // Pairing-code flow.

        eprintln!("{}", " Requesting pairing code...".dark_grey());

        let mut pairing = None;

        for _ in 0..15 {
            match client.request_pairing_code(&id, phone_number).await {
                Ok(p) => {
                    pairing = Some(p);
                    break;
                }

                Err(_) => {
                    tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                }
            }
        }

        let pairing = pairing.ok_or_else(|| {
            anyhow::anyhow!(
                "Could not get pairing code after 30 s. \
           Is OpenWA running with ENGINE_TYPE=baileys?"
            )
        })?;

        println!();

        println!(
            " Enter this code in WhatsApp → Linked Devices → \
       Link with phone number:"
        );

        println!();

        println!("  {}", format_pairing_code(&pairing.code).green().bold());

        println!();
    } else {
        // QR-code flow.

        eprintln!("{}", " Waiting for QR code...".dark_grey());

        let mut qr_data = None;

        for _ in 0..15 {
            match client.get_qr(&id).await {
                Ok(qr) => {
                    qr_data = Some(qr.qr_code);
                    break;
                }

                Err(_) => {
                    tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                }
            }
        }

        let qr_data = qr_data.ok_or_else(|| {
            anyhow::anyhow!(
                "Could not get QR code after 30 s. \
           Is OpenWA running with ENGINE_TYPE=baileys?"
            )
        })?;

        // OpenWA gives us an image. render_qr() decodes that image,
        // extracts the actual QR payload, and generates a fresh
        // terminal QR from the payload.
        render_qr(&qr_data)?;

        println!();

        println!(" Scan this QR with WhatsApp → Linked Devices → Link a Device");

        println!();
    }

    // ── 6. Poll until authenticated ────────────────────────────

    eprint!("{}", " Waiting for authentication".dark_grey());

    let mut authenticated = false;

    for _ in 0..90 {
        // ~3 minutes

        tokio::time::sleep(std::time::Duration::from_secs(2)).await;

        if let Ok(s) = client.get_session(&id).await {
            match s.status.as_str() {
                "ready" => {
                    eprintln!();

                    let who = s
                        .push_name
                        .as_deref()
                        .or(s.phone.as_deref())
                        .unwrap_or("WhatsApp");

                    println!(" {} Logged in as {}", "✓".green().bold(), who.bold());

                    if let Some(ref ph) = s.phone {
                        println!("  Phone: {ph}");
                    }

                    reconcile_authenticated_session(&mut config, &client, &s, alias, api_key)
                        .await?;

                    authenticated = true;
                    break;
                }

                "failed" => {
                    eprintln!();

                    eprintln!(" {} Session failed to authenticate.", "✗".red().bold());

                    break;
                }

                _ => {
                    eprint!(".");
                    std::io::stderr().flush()?;
                }
            }
        }
    }

    if !authenticated {
        eprintln!();

        eprintln!(
            "{}",
            " Timed out waiting for authentication. \
       Run `wpp login` to try again."
                .yellow()
        );
    }

    Ok(())
}

/// Reconcile the newly authenticated session against another
/// OpenWA session using the same WhatsApp phone.
///
/// The newly authenticated session is never persisted locally
/// before reconciliation, so aliases can be transferred cleanly.
async fn reconcile_authenticated_session(
    config: &mut Config,
    client: &WhatsAppClient,
    authenticated: &Session,
    requested_alias: Option<String>,
    api_key: Option<String>,
) -> Result<()> {
    let Some(phone) = authenticated.phone.as_deref() else {
        return persist_authenticated_session(
            config,
            authenticated,
            requested_alias,
            api_key,
            false,
        );
    };

    let normalized_phone = normalize_phone(phone);

    if normalized_phone.is_empty() {
        return persist_authenticated_session(
            config,
            authenticated,
            requested_alias,
            api_key,
            false,
        );
    }

    let sessions = client.list_sessions().await?;

    let duplicates: Vec<Session> = sessions
        .into_iter()
        .filter(|session| {
            session.id != authenticated.id
                && session
                    .phone
                    .as_deref()
                    .map(normalize_phone)
                    .is_some_and(|session_phone| session_phone == normalized_phone)
        })
        .collect();

    if duplicates.is_empty() {
        return persist_authenticated_session(
            config,
            authenticated,
            requested_alias,
            api_key,
            false,
        );
    }

    if duplicates.len() > 1 {
        let descriptions = duplicates
            .iter()
            .map(|session| format!("{} ({}, {})", session.name, session.id, session.status))
            .collect::<Vec<_>>()
            .join(", ");

        anyhow::bail!(
            "Multiple OpenWA sessions already use phone '{}': {}. \
       Run `wpp session` to inspect them before retrying.",
            phone,
            descriptions
        );
    }

    let duplicate = duplicates
        .into_iter()
        .next()
        .expect("duplicate list checked above");

    let authenticated_aliases = aliases_for_session(config, &authenticated.id);

    let duplicate_aliases = aliases_for_session(config, &duplicate.id);

    if duplicate.status == "ready" {
        /*
         * Existing session is active.
         *
         * Keep it, transfer any aliases from the newly authenticated
         * session, add the explicitly requested alias, delete the new
         * duplicate and activate the existing session.
         */

        eprintln!(
            "{}",
            format!(
                " Duplicate phone detected. Keeping active session '{}'...",
                duplicate.name
            )
            .yellow()
        );

        delete_remote_session(client, authenticated).await?;

        // The new session may already have a local reference when the
        // pairing flow reused an inactive session.
        config.remove_session_by_id(&authenticated.id);

        let primary_alias = requested_alias
            .clone()
            .or_else(|| duplicate_aliases.first().cloned())
            .or_else(|| authenticated_aliases.first().cloned())
            .unwrap_or_else(|| config.next_session_alias(duplicate.phone.as_deref()));

        config.upsert_session(
            SessionEntry {
                id: duplicate.id.clone(),
                aliases: Vec::new(),
                phone: duplicate.phone.clone(),
                push_name: duplicate.push_name.clone(),
            },
            primary_alias.clone(),
        )?;

        for alias in duplicate_aliases
            .into_iter()
            .chain(authenticated_aliases.into_iter())
            .chain(requested_alias.into_iter())
        {
            add_alias_if_needed(config, &duplicate.id, alias)?;
        }

        config.active_session = Some(primary_alias.clone());

        if config.api_key.is_none() {
            config.api_key = api_key;
        }

        config.save()?;

        println!(
            " {} Kept session {}",
            "✓".green().bold(),
            duplicate.name.bold()
        );

        println!("  Alias: {}", primary_alias);

        println!(
            " {} Removed duplicate session {}",
            "✓".green().bold(),
            authenticated.id
        );

        return Ok(());
    }

    /*
     * Existing session is inactive.
     *
     * Keep the newly authenticated session, transfer all aliases
     * from the old session, add the explicitly requested alias,
     * delete the inactive duplicate and activate the new session.
     */

    eprintln!(
        "{}",
        format!(
            " Duplicate phone detected. Replacing inactive session '{}'...",
            duplicate.name
        )
        .yellow()
    );

    delete_remote_session(client, &duplicate).await?;

    config.remove_session_by_id(&duplicate.id);

    let primary_alias = requested_alias
        .clone()
        .or_else(|| authenticated_aliases.first().cloned())
        .or_else(|| duplicate_aliases.first().cloned())
        .unwrap_or_else(|| config.next_session_alias(authenticated.phone.as_deref()));

    config.upsert_session(
        SessionEntry {
            id: authenticated.id.clone(),
            aliases: Vec::new(),
            phone: authenticated.phone.clone(),
            push_name: authenticated.push_name.clone(),
        },
        primary_alias.clone(),
    )?;

    for alias in authenticated_aliases
        .into_iter()
        .chain(duplicate_aliases.into_iter())
        .chain(requested_alias.into_iter())
    {
        add_alias_if_needed(config, &authenticated.id, alias)?;
    }

    config.active_session = Some(primary_alias.clone());

    if config.api_key.is_none() {
        config.api_key = api_key;
    }

    config.save()?;

    println!(
        " {} Replaced inactive session {}",
        "✓".green().bold(),
        duplicate.name
    );

    println!("  New session: {}", authenticated.id);

    println!("  Alias: {}", primary_alias);

    println!(
        " {} Removed duplicate session {}",
        "✓".green().bold(),
        duplicate.id
    );

    Ok(())
}

/// Persist a normal authenticated session when no duplicate was found.
///
/// When `reused` is true, an already-known alias is preferred.
/// Otherwise a new phone-based alias is generated when necessary.
fn persist_authenticated_session(
    config: &mut Config,
    session: &Session,
    requested_alias: Option<String>,
    api_key: Option<String>,
    reused: bool,
) -> Result<()> {
    let existing_alias = existing_alias(config, &session.id);

    let final_alias = match requested_alias {
        Some(alias) => alias,

        None => {
            if reused {
                existing_alias
                    .unwrap_or_else(|| config.next_session_alias(session.phone.as_deref()))
            } else {
                config.next_session_alias(session.phone.as_deref())
            }
        }
    };

    config.upsert_session(
        SessionEntry {
            id: session.id.clone(),
            aliases: Vec::new(),
            phone: session.phone.clone(),
            push_name: session.push_name.clone(),
        },
        final_alias.clone(),
    )?;

    config.active_session = Some(final_alias.clone());

    if config.api_key.is_none() {
        config.api_key = api_key;
    }

    config.save()?;

    if reused {
        println!(
            " {} Reused session for {}",
            "✓".green().bold(),
            session
                .push_name
                .as_deref()
                .or(session.phone.as_deref())
                .unwrap_or("WhatsApp")
                .bold()
        );
    }

    println!("  Alias: {final_alias}");

    Ok(())
}

/// Persist and activate a session that is already authenticated.
///
/// A reused session keeps an existing alias when no alias was given.
/// A new alias is added to the same OpenWA session.
fn activate_ready_session(
    config: &mut Config,
    session: &Session,
    alias: Option<String>,
    api_key: Option<String>,
) -> Result<()> {
    let existing_alias = existing_alias(config, &session.id);

    let already_active = existing_alias
        .as_deref()
        .is_some_and(|existing| config.active_session.as_deref() == Some(existing));

    // Same phone + same active session + no new alias:
    // there is nothing else to do.
    if alias.is_none() && already_active {
        let who = session
            .push_name
            .as_deref()
            .or(session.phone.as_deref())
            .unwrap_or("WhatsApp");

        println!(
            " {} Session already active as {}",
            "✓".green().bold(),
            who.bold()
        );

        return Ok(());
    }

    let final_alias = match alias {
        Some(alias) => alias,

        None => {
            existing_alias.unwrap_or_else(|| config.next_session_alias(session.phone.as_deref()))
        }
    };

    config.upsert_session(
        SessionEntry {
            id: session.id.clone(),
            aliases: Vec::new(),
            phone: session.phone.clone(),
            push_name: session.push_name.clone(),
        },
        final_alias.clone(),
    )?;

    config.active_session = Some(final_alias.clone());

    if config.api_key.is_none() {
        config.api_key = api_key;
    }

    config.save()?;

    let who = session
        .push_name
        .as_deref()
        .or(session.phone.as_deref())
        .unwrap_or("WhatsApp");

    println!(" {} Reused session for {}", "✓".green().bold(), who.bold());

    if let Some(ref phone) = session.phone {
        println!("  Phone: {phone}");
    }

    println!("  Alias: {final_alias}");

    Ok(())
}

/// Search for a reusable session for a known phone.
///
/// Local config is checked first. If the local session is stale,
/// it is removed and OpenWA is queried directly.
///
/// When OpenWA contains exactly one matching session, that session
/// can also be adopted by wpp even if it was not previously known
/// locally.
async fn find_reusable_session(
    config: &mut Config,
    client: &WhatsAppClient,
    phone: &str,
) -> Result<Option<Session>> {
    if let Some((_, entry)) = config.find_session_by_phone(phone) {
        let session_id = entry.id.clone();

        match client.get_session(&session_id).await {
            Ok(session) => {
                return Ok(Some(session));
            }

            Err(crate::error::WppError::Api { status: 404, .. }) => {
                // The local reference is stale.
                if config.remove_session_by_id(&session_id).is_some() {
                    config.save()?;
                }
            }

            Err(error) => {
                return Err(error.into());
            }
        }
    }

    // The phone may belong to an OpenWA session that wpp does not
    // know about yet. Reuse it instead of creating a duplicate.
    let sessions = client.list_sessions().await?;

    let normalized_phone = normalize_phone(phone);

    if normalized_phone.is_empty() {
        return Ok(None);
    }

    let matches: Vec<&Session> = sessions
        .iter()
        .filter(|session| {
            session
                .phone
                .as_deref()
                .map(normalize_phone)
                .is_some_and(|session_phone| session_phone == normalized_phone)
        })
        .collect();

    match matches.len() {
        0 => Ok(None),

        1 => Ok(Some(matches[0].clone())),

        _ => {
            let descriptions = matches
                .iter()
                .map(|session| format!("{} ({})", session.name, session.id))
                .collect::<Vec<_>>()
                .join(", ");

            anyhow::bail!(
                "Multiple OpenWA sessions match phone '{}': {}",
                phone,
                descriptions
            );
        }
    }
}

/// Delete an OpenWA session.
///
/// Logout is best-effort because inactive sessions may reject it.
/// The DELETE operation remains the authoritative cleanup step.
async fn delete_remote_session(client: &WhatsAppClient, session: &Session) -> Result<()> {
    eprintln!(
        "{}",
        format!(" Deleting duplicate session '{}'...", session.name).dark_grey()
    );

    match client.logout(&session.id).await {
        Ok(_) => {}

        Err(error) => {
            eprintln!(
                " {} Logout skipped for duplicate: {}",
                "!".yellow().bold(),
                error
            );
        }
    }

    client.delete_session(&session.id).await?;

    Ok(())
}

/// Return all aliases currently associated with an OpenWA session.
fn aliases_for_session(config: &Config, session_id: &str) -> Vec<String> {
    config
        .sessions
        .get(session_id)
        .map(|entry| entry.aliases.clone())
        .unwrap_or_default()
}

/// Add an alias only when it is not already associated with
/// the target session.
///
/// If the alias belongs to another session, fail rather than
/// silently reassigning it.
fn add_alias_if_needed(config: &mut Config, session_id: &str, alias: String) -> Result<()> {
    if let Some((_, entry)) = config.find_session(&alias) {
        if entry.id == session_id {
            return Ok(());
        }

        anyhow::bail!(
            "Session alias '{}' already belongs to OpenWA session '{}'.",
            alias,
            entry.id
        );
    }

    config.add_alias(session_id, alias)?;

    Ok(())
}

/// Return one existing alias for an OpenWA session, if wpp
/// already knows that session.
fn existing_alias(config: &Config, session_id: &str) -> Option<String> {
    config
        .sessions
        .get(session_id)
        .and_then(|entry| entry.aliases.first().cloned())
}

/// Normalize a phone number for exact comparison.
fn normalize_phone(value: &str) -> String {
    value.chars().filter(|c| c.is_ascii_digit()).collect()
}

/// Generate an internal OpenWA session name.
///
/// This is deliberately separate from the user-facing alias.
/// OpenWA requires a valid unique session name even when the
/// user doesn't provide `--alias`.
fn generate_session_name() -> String {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();

    format!("wpp-{millis}")
}

/// Format pairing code with a dash in the middle:
/// "ABCD-EFGH"
fn format_pairing_code(code: &str) -> String {
    if code.len() == 8 {
        format!("{}-{}", &code[..4], &code[4..])
    } else {
        code.to_string()
    }
}
