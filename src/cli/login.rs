use anyhow::Result;
use crossterm::style::Stylize;
use std::time::{
  Duration,
  Instant,
  SystemTime,
  UNIX_EPOCH,
};

use crate::app::config::{
  BackendKind,
  Config,
  SessionEntry,
};
use crate::terminal::render::render_qr_payload;
use crate::whatsapp::models::Session;
use crate::whatsapp::native::{
  NativeAuthEvent,
  NativeAuthMode,
  NativeClient,
};

const AUTH_TIMEOUT: Duration =
  Duration::from_secs(180);

const CONNECTED_CHECK_TIMEOUT: Duration =
  Duration::from_secs(30);

/// `wpp login [PHONE] [--phone NUMBER] [--qr] [--alias NAME]`
///
/// Creates or reuses a native whatsapp-rust session,
/// shows a QR code or pairing code, waits for authentication,
/// and saves the session to config.
pub async fn run(
  phone: Option<String>,
  alias: Option<String>,
) -> Result<()> {
  let mut config = Config::load()?;

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

  let (
    session_id,
    existing_alias,
  ) = match phone.as_deref() {
    Some(phone) => {
      match find_native_session_by_phone(
        &config,
        phone,
      )? {
        Some(selection) => selection,

        None => (
          generate_native_session_id(),
          None,
        ),
      }
    }

    None => (
      generate_native_session_id(),
      None,
    ),
  };

  let auth_mode = match phone.as_deref() {
    Some(phone) => {
      eprintln!(
        "{}",
        " Starting native WhatsApp session..."
          .dark_grey()
      );

      NativeAuthMode::PairingCode(
        phone.to_string(),
      )
    }

    None => {
      eprintln!(
        "{}",
        " Starting native WhatsApp session..."
          .dark_grey()
      );

      NativeAuthMode::Qr
    }
  };

  if existing_alias.is_some() {
    eprintln!(
      "{}",
      " Reusing existing native session..."
        .cyan()
    );
  }

  let storage_path =
    config.native_session_path(&session_id)?;

  let mut client =
    NativeClient::open(
      &storage_path,
      auth_mode,
    )
    .await?;

  let authenticated =
    authenticate(&mut client, phone.is_some())
      .await;

  if let Err(error) = authenticated {
    client.shutdown().await;
    return Err(error);
  }

  let authenticated_phone =
    client.phone().or_else(|| {
      phone
        .as_deref()
        .map(normalize_phone)
        .filter(|value| !value.is_empty())
    });

  let push_name = {
    let push_name = client.push_name();

    if push_name.trim().is_empty() {
      None
    } else {
      Some(push_name)
    }
  };

  let Some(authenticated_phone) =
    authenticated_phone
  else {
    client.shutdown().await;

    anyhow::bail!(
      "Native authentication completed, but the \
       authenticated phone number could not be determined."
    );
  };

  // Force the native library to flush its state before
  // persisting the wpp-level session reference.
  client.shutdown().await;

  let final_alias = alias
    .or(existing_alias)
    .unwrap_or_else(|| {
      config.next_session_alias(
        Some(&authenticated_phone),
      )
    });

  config.upsert_session(
    SessionEntry {
      id: session_id.clone(),
      backend: BackendKind::Native,
      aliases: Vec::new(),
      phone: Some(authenticated_phone.clone()),
      push_name: push_name.clone(),
    },
    final_alias.clone(),
  )?;

  config.active_session =
    Some(final_alias.clone());

  config.save()?;

  let who = push_name
    .as_deref()
    .unwrap_or(&authenticated_phone);

  println!(
    " {} Logged in as {}",
    "✓".green().bold(),
    who.bold()
  );

  println!(
    " Phone: {}",
    authenticated_phone
  );

  println!(
    " Alias: {}",
    final_alias
  );

  println!(
    " Backend: native"
  );

  Ok(())
}

/// Wait for native authentication to complete.
///
/// QR and pairing-code payloads arrive through
/// `NativeAuthEvent`, while the final authentication state
/// is confirmed by `wait_for_connected`.
async fn authenticate(
  client: &mut NativeClient,
  pairing: bool,
) -> Result<()> {
  if client.is_logged_in() {
    client
      .wait_for_connected(
        CONNECTED_CHECK_TIMEOUT,
      )
      .await
      .map_err(|error| {
        anyhow::anyhow!(
          "native session is logged in but could \
           not establish a connection: {error}"
        )
      })?;

    return Ok(());
  }

  if pairing {
    println!();
    println!(
      " Enter the pairing code in WhatsApp → \
       Linked Devices → Link with phone number."
    );
    println!();
  } else {
    eprintln!(
      "{}",
      " Waiting for QR code..."
        .dark_grey()
    );
  }

  let deadline =
    Instant::now() + AUTH_TIMEOUT;

  loop {
    if client.is_logged_in() {
      break;
    }

    let now = Instant::now();

    if now >= deadline {
      anyhow::bail!(
        "Timed out waiting for native WhatsApp authentication."
      );
    }

    let remaining =
      deadline.saturating_duration_since(now);

    let poll_timeout =
      remaining.min(Duration::from_millis(500));

    match tokio::time::timeout(
      poll_timeout,
      client.next_auth_event(),
    )
    .await
    {
      Ok(Some(event)) => {
        match event {
          NativeAuthEvent::QrCode(code) => {
            println!();
            render_qr_payload(&code)?;
            println!();
            println!(
              " Scan this QR with WhatsApp → \
               Linked Devices → Link a Device"
            );
            println!();
          }

          NativeAuthEvent::PairCode(code) => {
            println!(
              " {}",
              format_pairing_code(&code)
                .green()
                .bold()
            );
            println!();
          }

          NativeAuthEvent::PairCodeError(error) => {
            anyhow::bail!(
              "Could not obtain native pairing code: {}",
              error
            );
          }
        }
      }

      Ok(None) => {
        anyhow::bail!(
          "Native authentication event channel closed."
        );
      }

      Err(_) => {}
    }
  }

  let remaining = deadline
    .saturating_duration_since(
      Instant::now()
    );

  client
    .wait_for_connected(remaining)
    .await
    .map_err(|error| {
      anyhow::anyhow!(
        "native WhatsApp authentication failed: {error}"
      )
    })?;

  Ok(())
}

/// Find an existing native session by exact normalized phone.
///
/// Returns the existing session ID and its first alias.
fn find_native_session_by_phone(
  config: &Config,
  phone: &str,
) -> Result<Option<(String, Option<String>)>> {
  let normalized_phone =
    normalize_phone(phone);

  if normalized_phone.is_empty() {
    anyhow::bail!(
      "phone number cannot be empty"
    );
  }

  let matches: Vec<&SessionEntry> =
    config
      .sessions
      .values()
      .filter(|entry| {
        entry.backend == BackendKind::Native
          && entry
            .phone
            .as_deref()
            .map(normalize_phone)
            .is_some_and(|entry_phone| {
              entry_phone == normalized_phone
            })
      })
      .collect();

  match matches.len() {
    0 => Ok(None),

    1 => {
      let entry = matches[0];

      Ok(Some((
        entry.id.clone(),
        entry.aliases.first().cloned(),
      )))
    }

    _ => {
      let descriptions = matches
        .iter()
        .map(|entry| {
          let aliases =
            if entry.aliases.is_empty() {
              String::new()
            } else {
              format!(
                " [{}]",
                entry.aliases.join(", ")
              )
            };

          format!(
            "{}{}",
            entry.id,
            aliases
          )
        })
        .collect::<Vec<_>>()
        .join(", ");

      anyhow::bail!(
        "Multiple native sessions match phone '{}': {}",
        phone,
        descriptions
      );
    }
  }
}

/// Generate a unique application-level identifier for a
/// native whatsapp-rust session.
fn generate_native_session_id() -> String {
  let nanos =
    SystemTime::now()
      .duration_since(UNIX_EPOCH)
      .unwrap_or_default()
      .as_nanos();

  format!(
    "native-{nanos}-{}",
    std::process::id()
  )
}

/// Normalize a phone number for exact comparison.
fn normalize_phone(value: &str) -> String {
  value
    .chars()
    .filter(|c| c.is_ascii_digit())
    .collect()
}

/// Format pairing code with a dash in the middle:
/// "ABCD-EFGH"
fn format_pairing_code(code: &str) -> String {
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