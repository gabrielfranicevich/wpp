use std::collections::HashSet;

use anyhow::Result;
use crossterm::style::Stylize;

use crate::app::config::Config;
use crate::whatsapp::client::WhatsAppClient;
use crate::whatsapp::models::Session;

/// `wpp session`
///
/// Lists every session currently known by OpenWA and marks
/// which ones are also known locally by wpp.
pub async fn run() -> Result<()> {
  let config = Config::load()?;

  let client = WhatsAppClient::openwa(
    &config.base_url,
    config.openwa_api_key(),
  );

  let mut sessions = client.list_sessions().await?;

  sessions.sort_by(|a, b| {
    a.name
      .cmp(&b.name)
      .then_with(|| a.id.cmp(&b.id))
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
      "OpenWA sessions:".bold()
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
        &config,
      );
    }
  }

  print_local_only_sessions(
    &config,
    &sessions,
  );

  println!();

  Ok(())
}

fn print_session(
  session: &Session,
  config: &Config,
) {
  let aliases = aliases_for_session(
    config,
    &session.id,
  );

  let is_active = aliases.iter().any(
    |alias| {
      config.active_session.as_deref()
        == Some(alias.as_str())
    },
  );

  let marker = if is_active {
    "▸".green().bold().to_string()
  } else {
    " ".to_string()
  };

  let phone = session
    .phone
    .as_deref()
    .unwrap_or("-");

  let status =
    format_status(&session.status);

  let wpp = if aliases.is_empty() {
    "unmanaged".dark_grey().to_string()
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
  let mut aliases: Vec<String> = config
    .sessions
    .iter()
    .filter_map(|(alias, entry)| {
      if entry.id == session_id {
        Some(alias.clone())
      } else {
        None
      }
    })
    .collect();

  aliases.sort();

  aliases
}

fn format_status(
  status: &str,
) -> String {
  match status {
    "ready" => status.green().to_string(),

    "failed" => status.red().to_string(),

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

fn print_local_only_sessions(
  config: &Config,
  openwa_sessions: &[Session],
) {
  let openwa_ids: HashSet<&str> =
    openwa_sessions
      .iter()
      .map(|session| session.id.as_str())
      .collect();

  let mut local_only: Vec<_> = config
    .sessions
    .iter()
    .filter(|(_, entry)| {
      !openwa_ids.contains(entry.id.as_str())
    })
    .collect();

  if local_only.is_empty() {
    return;
  }

  local_only.sort_by_key(|(alias, _)| *alias);

  println!();
  println!(
    " {}",
    "Local sessions not found in OpenWA:"
      .yellow()
      .bold()
  );
  println!();

  for (alias, entry) in local_only {
    let active = if config.active_session
      .as_deref()
      == Some(alias.as_str())
    {
      " (active)"
    } else {
      ""
    };

    let phone = entry
      .phone
      .as_deref()
      .unwrap_or("-");

    println!(
      "   {:<20} {:<16} {}{}",
      alias.clone().bold(),
      phone,
      entry.id,
      active
    );
  }
}