use std::collections::HashSet;

use anyhow::{bail, Result};
use crossterm::style::Stylize;

use crate::app::config::Config;
use crate::whatsapp::client::WhatsAppClient;
use crate::whatsapp::models::Session;

/// `wpp session`
///
/// - No argument: list every OpenWA session and mark which
///   ones are known locally by wpp.
/// - `-D TARGET`: attempt logout and then delete the
///   selected OpenWA session.
///
/// If TARGET refers to a local alias whose OpenWA session no
/// longer exists, only the stale local reference is removed.
pub async fn run(
  delete: Option<String>,
) -> Result<()> {
  let mut config =
    Config::load()?;

  let client =
    WhatsAppClient::openwa(
      &config.base_url,
      config.openwa_api_key(),
    );

  match delete {
    Some(target) =>
      delete_session(
        &mut config,
        &client,
        &target,
      )
      .await,

    None =>
      list_sessions(
        &config,
        &client,
      )
      .await,
  }
}

async fn list_sessions(
  config: &Config,
  client: &WhatsAppClient,
) -> Result<()> {
  let mut sessions =
    client.list_sessions().await?;

  sessions.sort_by(|a, b| {
    a.name
      .cmp(&b.name)
      .then_with(|| {
        a.id.cmp(&b.id)
      })
  });

  println!();

  if sessions.is_empty() {
    println!(
      " {}",
      "No OpenWA sessions."
        .dark_grey()
    );
  } else {
    println!(
      " {}",
      "OpenWA sessions:"
        .bold()
    );

    println!();

    println!(
      "   {:<20} {:<16} {:<36} {:<17} {}",
      "NAME".bold(),
      "PHONE".bold(),
      "ID".bold(),
      "STATUS".bold(),
      "WPP".bold(),
    );

    for session in &sessions {
      print_session(
        session,
        config,
      );
    }
  }

  println!();

  Ok(())
}

async fn delete_session(
  config: &mut Config,
  client: &WhatsAppClient,
  target: &str,
) -> Result<()> {
  let sessions =
    client.list_sessions().await?;

  /*
   * First resolve TARGET through wpp's local aliases.
   *
   * This also lets us clean stale local references when the
   * corresponding OpenWA session has already disappeared.
   */
  if let Some((_, entry)) =
    config.find_session(target)
  {
    let session_id =
      entry.id.clone();

    if let Some(session) =
      sessions.iter().find(|session| {
        session.id == session_id
      })
    {
      return delete_remote_session(
        config,
        client,
        session,
      )
      .await;
    }

    return remove_stale_local_session(
      config,
      &session_id,
      target,
    );
  }

  /*
   * TARGET may also be a raw OpenWA session ID.
   */
  if let Some(session) =
    sessions.iter().find(|session| {
      session.id == target
    })
  {
    return delete_remote_session(
      config,
      client,
      session,
    )
    .await;
  }

  /*
   * TARGET may be a stale session ID still referenced locally.
   */
  if config.sessions.contains_key(target) {
    return remove_stale_local_session(
      config,
      target,
      target,
    );
  }

  /*
   * Try OpenWA session name and exact phone match.
   */
  let session =
    resolve_remote_target(
      &sessions,
      target,
    )?;

  /*
   * No remote session exists. TARGET may still refer to
   * another form of local stale reference.
   */
  if session.is_none() {
    if let Some((_, entry)) =
      config.find_session(target)
    {
      let session_id =
        entry.id.clone();

      return remove_stale_local_session(
        config,
        &session_id,
        target,
      );
    }

    bail!(
      "No OpenWA session or local reference matching '{}'. Run `wpp session` to see available sessions.",
      target
    );
  }

  delete_remote_session(
    config,
    client,
    session.unwrap(),
  )
  .await
}

async fn delete_remote_session(
  config: &mut Config,
  client: &WhatsAppClient,
  session: &Session,
) -> Result<()> {
  let session_id =
    session.id.clone();

  let session_name =
    session.name.clone();

  println!();

  println!(
    " {} Deleting session {} ({})...",
    "→".cyan().bold(),
    session_name.clone().bold(),
    session_id,
  );

  println!();

  // Logout is intentionally best-effort here.
  //
  // An inactive OpenWA session may reject logout,
  // but we still want DELETE to clean up the session.
  match client
    .logout(&session_id)
    .await
  {
    Ok(_) => {
      println!(
        " {} Logged out {}",
        "✓".green().bold(),
        session_name
      );
    }

    Err(error) => {
      eprintln!(
        " {} Logout skipped: {}",
        "!".yellow().bold(),
        error
      );
    }
  }

  client
    .delete_session(&session_id)
    .await?;

  let removed =
    config.remove_sessions_by_id(
      &session_id,
    );

  if !removed.is_empty() {
    config.save()?;
  }

  println!(
    " {} Deleted OpenWA session {}",
    "✓".green().bold(),
    session_name.bold(),
  );

  if removed.is_empty() {
    println!(
      " {} No local wpp aliases were associated with this session.",
      "i".dark_grey()
    );
  } else {
    let aliases: Vec<String> =
      removed
        .into_iter()
        .map(|(alias, _)| alias)
        .collect();

    println!(
      " {} Removed local alias{}: {}",
      "✓".green().bold(),
      if aliases.len() == 1 {
        ""
      } else {
        "es"
      },
      aliases.join(", "),
    );
  }

  Ok(())
}

fn remove_stale_local_session(
  config: &mut Config,
  session_id: &str,
  target: &str,
) -> Result<()> {
  let removed =
    config.remove_sessions_by_id(
      session_id,
    );

  if removed.is_empty() {
    bail!(
      "No local session reference matching '{}'. Run `wpp session` to see available sessions.",
      target
    );
  }

  config.save()?;

  let aliases: Vec<String> =
    removed
      .into_iter()
      .map(|(alias, _)| alias)
      .collect();

  println!();

  println!(
    " {} Local reference '{}' points to a session that no longer exists in OpenWA.",
    "→".yellow().bold(),
    target
  );

  println!(
    " {} Removed local alias{}: {}",
    "✓".green().bold(),
    if aliases.len() == 1 {
      ""
    } else {
      "es"
    },
    aliases.join(", "),
  );

  println!();

  Ok(())
}

fn resolve_remote_target<'a>(
  sessions: &'a [Session],
  target: &str,
) -> Result<Option<&'a Session>> {
  /*
   * OpenWA session name.
   */
  if let Some(session) =
    sessions.iter().find(|session| {
      session.name == target
    })
  {
    return Ok(Some(session));
  }

  /*
   * Exact phone-number match.
   */
  let normalized_target =
    normalize_phone(target);

  if !normalized_target.is_empty() {
    let phone_matches:
      Vec<&Session> =
      sessions
        .iter()
        .filter(|session| {
          session
            .phone
            .as_deref()
            .map(normalize_phone)
            .is_some_and(|phone| {
              phone == normalized_target
            })
        })
        .collect();

    match phone_matches.len() {
      1 => {
        return Ok(Some(
          phone_matches[0]
        ));
      }

      0 => {}

      _ => {
        let matches =
          phone_matches
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

        bail!(
          "Multiple OpenWA sessions match phone '{}': {}",
          target,
          matches
        );
      }
    }
  }

  Ok(None)
}

fn normalize_phone(
  value: &str,
) -> String {
  value
    .chars()
    .filter(|c| c.is_ascii_digit())
    .collect()
}

fn print_session(
  session: &Session,
  config: &Config,
) {
  let aliases =
    aliases_for_session(
      config,
      &session.id,
    );

  let is_active =
    aliases.iter().any(|alias| {
      config
        .active_session
        .as_deref()
        == Some(alias.as_str())
    });

  let marker = if is_active {
    "▸".green().bold().to_string()
  } else {
    " ".to_string()
  };

  let phone =
    session
      .phone
      .as_deref()
      .unwrap_or("-");

  let status =
    format_status(&session.status);

  let wpp =
    if aliases.is_empty() {
      "unmanaged"
        .dark_grey()
        .to_string()
    } else {
      format!(
        "{} {}",
        "wpp:".dark_grey(),
        aliases.join(", ")
      )
    };

  println!(
    " {marker} {:<20} {:<16} {:<36} {:<17} {}",
    session.name,
    phone,
    session.id,
    status,
    wpp,
  );
}

fn aliases_for_session(
  config: &Config,
  session_id: &str,
) -> Vec<String> {
  let mut aliases =
    config
      .sessions
      .get(session_id)
      .map(|entry| {
        entry.aliases.clone()
      })
      .unwrap_or_default();

  aliases.sort();

  aliases
}

fn format_status(
  status: &str,
) -> String {
  match status {
    "ready" => {
      status.green().to_string()
    }

    "failed" => {
      status.red().to_string()
    }

    "disconnected" => {
      status.dark_grey().to_string()
    }

    "initializing"
    | "qr_ready"
    | "authenticating"
    | "action_required" => {
      status.yellow().to_string()
    }

    "created" => {
      status.cyan().to_string()
    }

    _ => status.to_string(),
  }
}

#[allow(dead_code)]
fn print_local_only_sessions(
  config: &Config,
  openwa_sessions: &[Session],
) {
  let openwa_ids:
    HashSet<&str> =
    openwa_sessions
      .iter()
      .map(|session| {
        session.id.as_str()
      })
      .collect();

  let mut local_only: Vec<_> =
    config
      .sessions
      .iter()
      .filter(|(session_id, _)| {
        !openwa_ids.contains(
          session_id.as_str()
        )
      })
      .collect();

  if local_only.is_empty() {
    return;
  }

  local_only.sort_by_key(
    |(_, entry)| {
      entry
        .aliases
        .first()
        .cloned()
        .unwrap_or_default()
    }
  );

  println!();

  println!(
    " {}",
    "Local sessions not found in OpenWA:"
      .yellow()
      .bold()
  );

  println!();

  for (session_id, entry) in local_only {
    let active =
      if entry.aliases.iter().any(
        |alias| {
          config
            .active_session
            .as_deref()
            == Some(alias.as_str())
        }
      ) {
        " (active)"
      } else {
        ""
      };

    let aliases =
      if entry.aliases.is_empty() {
        "-".to_string()
      } else {
        entry.aliases.join(", ")
      };

    let phone =
      entry
        .phone
        .as_deref()
        .unwrap_or("-");

    println!(
      "   {:<28} {:<16} {}{}",
      aliases,
      phone,
      session_id,
      active
    );
  }
}