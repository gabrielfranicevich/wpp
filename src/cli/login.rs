use anyhow::Result;
use crossterm::style::Stylize;
use std::io::Write;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::app::config::{Config, SessionEntry};
use crate::terminal::render::render_qr;
use crate::whatsapp::client::WhatsAppClient;

/// `wpp login [PHONE] [--phone NUMBER] [--qr] [--alias NAME]`
///
/// Creates a new session in OpenWA, shows QR or pairing code,
/// waits for authentication, and saves the session to config.
pub async fn run(
  phone: Option<String>,
  alias: Option<String>,
) -> Result<()> {
  let mut config = Config::load()?;

  // Only reject duplicates when the user explicitly provided
  // an alias. When omitted, an alias is generated after the
  // WhatsApp session becomes authenticated.
  if let Some(ref alias) = alias {
    if config.sessions.contains_key(alias) {
      eprintln!(
        "{}",
        format!(
          "Session '{alias}' already exists. Use `wpp switch {alias}` \
           to activate it,\n\
           or choose a different alias with `wpp login --alias <name>`."
        )
        .yellow()
      );

      return Ok(());
    }
  }

  let api_key = config.openwa_api_key();

  let client = WhatsAppClient::openwa(
    &config.base_url,
    api_key.clone(),
  );

  // ── 1. Create session in OpenWA ────────────────────────────

  eprintln!(
    "{}",
    " Creating session...".dark_grey()
  );

  // The OpenWA session name is independent from the user-facing
  // wpp alias. This lets `wpp login` work without --alias.
  let session_name = generate_session_name();

  let session = match client.create_session(&session_name).await {
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

  let id = session.id.clone();

  // ── 2. Start the session engine ────────────────────────────

  eprintln!(
    "{}",
    " Starting engine...".dark_grey()
  );

  client.start_session(&id).await?;

  // Give the engine a moment to initialise and generate a QR.
  tokio::time::sleep(
    std::time::Duration::from_secs(3)
  ).await;

  // ── 3. Show QR or pairing code ─────────────────────────────

  if let Some(ref phone_number) = phone {
    // Pairing-code flow.

    eprintln!(
      "{}",
      " Requesting pairing code...".dark_grey()
    );

    let mut pairing = None;

    for _ in 0..15 {
      match client
        .request_pairing_code(&id, phone_number)
        .await
      {
        Ok(p) => {
          pairing = Some(p);
          break;
        }

        Err(_) => {
          tokio::time::sleep(
            std::time::Duration::from_secs(2)
          ).await;
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

    println!(
      "  {}",
      format_pairing_code(&pairing.code)
        .green()
        .bold()
    );

    println!();
  } else {
    // QR-code flow.

    eprintln!(
      "{}",
      " Waiting for QR code...".dark_grey()
    );

    let mut qr_data = None;

    for _ in 0..15 {
      match client.get_qr(&id).await {
        Ok(qr) => {
          qr_data = Some(qr.qr_code);
          break;
        }

        Err(_) => {
          tokio::time::sleep(
            std::time::Duration::from_secs(2)
          ).await;
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

    println!(
      " Scan this QR with WhatsApp → Linked Devices → Link a Device"
    );

    println!();
  }

  // ── 4. Poll until authenticated ────────────────────────────

  eprint!(
    "{}",
    " Waiting for authentication".dark_grey()
  );

  let mut authenticated = false;

  for _ in 0..90 {
    // ~3 minutes

    tokio::time::sleep(
      std::time::Duration::from_secs(2)
    ).await;

    if let Ok(s) = client.get_session(&id).await {
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

          if let Some(ref ph) = s.phone {
            println!("  Phone: {ph}");
          }

          // Generate the user-facing alias only after authentication,
          // because the phone number is available at this point.
          let final_alias = match alias {
            Some(alias) => alias,
            None => {
              config.next_session_alias(
                s.phone.as_deref()
              )
            }
          };

          // Persist the authenticated session.
          config.sessions.insert(
            final_alias.clone(),
            SessionEntry {
              id: id.clone(),
              phone: s.phone.clone(),
              push_name: s.push_name.clone(),
            },
          );

          config.active_session = Some(final_alias);

          if config.api_key.is_none() {
            config.api_key = api_key;
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