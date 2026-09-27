use anyhow::Result;
use crossterm::style::Stylize;
use std::io::Write;
use std::time::{
  SystemTime,
  UNIX_EPOCH,
};

use crate::app::config::{
  Config,
  SessionEntry,
};
use crate::terminal::render::render_qr;
use crate::whatsapp::client::WhatsAppClient;
use crate::whatsapp::models::Session;

/// `wpp login [PHONE] [--phone NUMBER] [--qr] [--alias NAME]`
///
/// Creates or reuses an OpenWA session, shows QR or pairing code,
/// waits for authentication, and saves the session to config.
pub async fn run(
  phone: Option<String>,
  alias: Option<String>,
) -> Result<()> {
  let mut config =
    Config::load()?;

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

  let api_key =
    config.openwa_api_key();

  let client =
    WhatsAppClient::openwa(
      &config.base_url,
      api_key.clone(),
    );

  // ── 1. Reuse an existing session when the phone is known ───

  let existing_session =
    match phone.as_deref() {
      Some(phone_number) =>
        find_reusable_session(
          &mut config,
          &client,
          phone_number,
        )
        .await?,

      None => None,
    };

  let id =
    match existing_session.as_ref() {
      Some(session) => {
        eprintln!(
          "{}",
          format!(
            " Reusing existing session '{}'...",
            session.name
          )
          .cyan()
        );

        session.id.clone()
      }

      None => {
        // ── 2. Create session in OpenWA ──────────────────────

        eprintln!(
          "{}",
          " Creating session..."
            .dark_grey()
        );

        // The OpenWA session name is independent from the
        // user-facing wpp alias.
        let session_name =
          generate_session_name();

        let session =
          match client
            .create_session(&session_name)
            .await
          {
            Ok(s) => s,

            Err(e) => {
              match &e {
                crate::error::WppError::Api {
                  status: 401,
                  ..
                } => {
                  eprintln!(
                    "{} OpenWA authentication failed (401)",
                    "✗".red().bold()
                  );

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

  if let Some(session) =
    existing_session.as_ref()
  {
    if session.status == "ready" {
      return activate_ready_session(
        &mut config,
        &session,
        alias,
        api_key,
      );
    }
  }

  // ── 4. Start the session engine ────────────────────────────

  eprintln!(
    "{}",
    " Starting engine..."
      .dark_grey()
  );

  client
    .start_session(&id)
    .await?;

  // Give the engine a moment to initialise and generate a QR.
  tokio::time::sleep(
    std::time::Duration::from_secs(3)
  )
  .await;

  // ── 5. Show QR or pairing code ─────────────────────────────

  if let Some(
    ref phone_number
  ) = phone {
    // Pairing-code flow.

    eprintln!(
      "{}",
      " Requesting pairing code..."
        .dark_grey()
    );

    let mut pairing = None;

    for _ in 0..15 {
      match client
        .request_pairing_code(
          &id,
          phone_number,
        )
        .await
      {
        Ok(p) => {
          pairing = Some(p);
          break;
        }

        Err(_) => {
          tokio::time::sleep(
            std::time::Duration::from_secs(2)
          )
          .await;
        }
      }
    }

    let pairing =
      pairing.ok_or_else(|| {
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

    println!(
      "  {}",
      format_pairing_code(
        &pairing.code
      )
      .green()
      .bold()
    );

    println!();
  } else {
    // QR-code flow.

    eprintln!(
      "{}",
      " Waiting for QR code..."
        .dark_grey()
    );

    let mut qr_data = None;

    for _ in 0..15 {
      match client
        .get_qr(&id)
        .await
      {
        Ok(qr) => {
          qr_data =
            Some(qr.qr_code);
          break;
        }

        Err(_) => {
          tokio::time::sleep(
            std::time::Duration::from_secs(2)
          )
          .await;
        }
      }
    }

    let qr_data =
      qr_data.ok_or_else(|| {
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

    println!(
      " Scan this QR with WhatsApp → Linked Devices → Link a Device"
    );

    println!();
  }

  // ── 6. Poll until authenticated ────────────────────────────

  eprint!(
    "{}",
    " Waiting for authentication"
      .dark_grey()
  );

  let mut authenticated = false;

  for _ in 0..90 {
    // ~3 minutes

    tokio::time::sleep(
      std::time::Duration::from_secs(2)
    )
    .await;

    if let Ok(s) =
      client.get_session(&id).await
    {
      match s.status.as_str() {
        "ready" => {
          eprintln!();

          let who = s
            .push_name
            .as_deref()
            .or(s.phone.as_deref())
            .unwrap_or("WhatsApp");

          println!(
            " {} Logged in as {}",
            "✓".green().bold(),
            who.bold()
          );

          if let Some(ref ph) =
            s.phone
          {
            println!(
              "  Phone: {ph}"
            );
          }

          // When reusing an existing local session without
          // an explicit alias, keep one of its existing aliases.
          //
          // New sessions still get a generated phone-based alias.
          let final_alias =
            match alias {
              Some(alias) => alias,

              None => {
                existing_alias(
                  &config,
                  &id,
                )
                .unwrap_or_else(|| {
                  config.next_session_alias(
                    s.phone.as_deref()
                  )
                })
              }
            };

          config.upsert_session(
            SessionEntry {
              id: id.clone(),
              aliases: Vec::new(),
              phone:
                s.phone.clone(),
              push_name:
                s.push_name.clone(),
            },
            final_alias.clone(),
          )?;

          config.active_session =
            Some(final_alias);

          if config.api_key.is_none() {
            config.api_key =
              api_key;
          }

          config.save()?;

          authenticated = true;
          break;
        }

        "failed" => {
          eprintln!();

          eprintln!(
            " {} Session failed to authenticate.",
            "✗".red().bold()
          );

          break;
        }

        _ => {
          eprint!(".");
          std::io::stderr()
            .flush()?;
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
  if let Some((_, entry)) =
    config.find_session_by_phone(phone)
  {
    let session_id =
      entry.id.clone();

    match client
      .get_session(&session_id)
      .await
    {
      Ok(session) => {
        return Ok(Some(session));
      }

      Err(
        crate::error::WppError::Api {
          status: 404,
          ..
        }
      ) => {
        // The local reference is stale.
        if config
          .remove_session_by_id(
            &session_id
          )
          .is_some()
        {
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
  let sessions =
    client.list_sessions().await?;

  let normalized_phone =
    normalize_phone(phone);

  if normalized_phone.is_empty() {
    return Ok(None);
  }

  let matches: Vec<&Session> =
    sessions
      .iter()
      .filter(|session| {
        session
          .phone
          .as_deref()
          .map(normalize_phone)
          .is_some_and(|session_phone| {
            session_phone
              == normalized_phone
          })
      })
      .collect();

  match matches.len() {
    0 => Ok(None),

    1 => {
      Ok(Some(matches[0].clone()))
    }

    _ => {
      let descriptions =
        matches
          .iter()
          .map(|session| {
            format!(
              "{} ({})",
              session.name,
              session.id
            )
          })
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
  let existing_alias =
    existing_alias(
      config,
      &session.id,
    );

  let already_active =
    existing_alias.as_deref()
      .is_some_and(|existing| {
        config.active_session
          .as_deref()
          == Some(existing)
      });

  // Same phone + same active session + no new alias:
  // there is nothing else to do.
  if alias.is_none()
    && already_active
  {
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

  let final_alias =
    match alias {
      Some(alias) => alias,

      None =>
        existing_alias.unwrap_or_else(|| {
          config.next_session_alias(
            session.phone.as_deref()
          )
        }),
    };

  config.upsert_session(
    SessionEntry {
      id: session.id.clone(),
      aliases: Vec::new(),
      phone:
        session.phone.clone(),
      push_name:
        session.push_name.clone(),
    },
    final_alias.clone(),
  )?;

  config.active_session =
    Some(final_alias.clone());

  if config.api_key.is_none() {
    config.api_key =
      api_key;
  }

  config.save()?;

  let who = session
    .push_name
    .as_deref()
    .or(session.phone.as_deref())
    .unwrap_or("WhatsApp");

  println!(
    " {} Reused session for {}",
    "✓".green().bold(),
    who.bold()
  );

  if let Some(ref phone) =
    session.phone
  {
    println!(
      "  Phone: {phone}"
    );
  }

  println!(
    "  Alias: {final_alias}"
  );

  Ok(())
}

/// Return one existing alias for an OpenWA session, if wpp
/// already knows that session.
fn existing_alias(
  config: &Config,
  session_id: &str,
) -> Option<String> {
  config
    .sessions
    .get(session_id)
    .and_then(|entry| {
      entry.aliases.first().cloned()
    })
}

/// Normalize a phone number for exact comparison.
fn normalize_phone(
  value: &str,
) -> String {
  value
    .chars()
    .filter(|c| c.is_ascii_digit())
    .collect()
}

/// Generate an internal OpenWA session name.
///
/// This is deliberately separate from the user-facing alias.
/// OpenWA requires a valid unique session name even when the
/// user doesn't provide `--alias`.
fn generate_session_name() -> String {
  let millis =
    SystemTime::now()
      .duration_since(
        UNIX_EPOCH
      )
      .unwrap_or_default()
      .as_millis();

  format!("wpp-{millis}")
}

/// Format pairing code with a dash in the middle:
/// "ABCD-EFGH"
fn format_pairing_code(
  code: &str,
) -> String {
  if code.len() == 8 {
    format!(
      "{}-{}",
      &code[..4],
      &code[4..]
    )
  } else {
    code.to_string()
  }
}