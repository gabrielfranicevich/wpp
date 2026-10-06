use std::path::Path;

use anyhow::{bail, Result};
use crossterm::style::Stylize;

use crate::app::config::{
  Config,
  SessionEntry,
};
use crate::whatsapp::client::WhatsAppClient;

pub async fn run(
  delete: Option<String>,
) -> Result<()> {
  let mut config =
    Config::load()?;

  match delete {
    Some(target) =>
      delete_session(
        &mut config,
        &target,
      )
      .await,

    None =>
      list_sessions(&config)
        .await,
  }
}

async fn list_sessions(
  config: &Config,
) -> Result<()> {
  if config.sessions.is_empty() {
    println!(
      " {}",
      "No sessions.".dark_grey()
    );

    return Ok(());
  }

  let mut entries =
    config
      .sessions
      .values()
      .collect::<Vec<_>>();

  entries.sort_by_key(
    |entry| {
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
    "Sessions:".bold()
  );
  println!();

  println!(
    "  {:<20} {:<16} {:<36} {:<17} {}",
    "NAME".bold(),
    "PHONE".bold(),
    "ID".bold(),
    "STATUS".bold(),
    "WPP".bold(),
  );

  for entry in entries {
    print_saved_session(
      entry,
      config,
    );
  }

  println!();

  Ok(())
}

async fn delete_session(
  config: &mut Config,
  target: &str,
) -> Result<()> {
  let entry =
    if let Some((_, entry)) =
      config.find_session(target)
    {
      entry.clone()
    } else if let Some(entry) =
      config.sessions.get(target).cloned()
    {
      entry
    } else {
      bail!(
        "No session or local reference matching '{}'. \
         Run `wpp session` to see available sessions.",
        target
      );
    };

  delete_native_session(
    config,
    &entry,
  )
  .await
}

async fn delete_native_session(
  config: &mut Config,
  entry: &SessionEntry,
) -> Result<()> {
  let session_id =
    entry.id.clone();

  let storage_path =
    config.native_session_path(
      &session_id
    )?;

  println!();

  println!(
    " {} Deleting session {}...",
    "→".cyan().bold(),
    session_id.clone().bold(),
  );

  println!();

  if storage_path.exists() {
    let client =
      WhatsAppClient::open_native(
        &storage_path
      )
      .await?;

    let result =
      client
        .logout(&session_id)
        .await;

    client.shutdown().await;

    result?;
  }

  remove_native_storage(
    &storage_path
  )?;

  let removed =
    config.remove_sessions_by_id(
      &session_id
    );

  if !removed.is_empty() {
    config.save()?;
  }

  println!(
    " {} Deleted session {}",
    "✓".green().bold(),
    session_id.bold(),
  );

  println!(
    " {} Removed native session storage.",
    "✓".green().bold()
  );

  if removed.is_empty() {
    println!(
      " {} No local wpp aliases were associated with this session.",
      "i".dark_grey().to_string()
    );
  } else {
    let aliases =
      removed
        .into_iter()
        .map(|(alias, _)| alias)
        .collect::<Vec<_>>();

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

fn print_saved_session(
  entry: &SessionEntry,
  config: &Config,
) {
  let mut aliases =
    entry.aliases.clone();

  aliases.sort();

  let is_active =
    aliases.iter().any(
      |alias| {
        config
          .active_session
          .as_deref()
          == Some(alias.as_str())
      }
    );

  let marker =
    if is_active {
      "▸"
        .green()
        .bold()
        .to_string()
    } else {
      " ".to_string()
    };

  let name =
    entry
      .push_name
      .as_deref()
      .filter(
        |name| !name.trim().is_empty()
      )
      .or_else(
        || aliases.first().map(String::as_str)
      )
      .unwrap_or(entry.id.as_str());

  let phone =
    entry
      .phone
      .as_deref()
      .unwrap_or("-");

  let status =
    match config.native_session_path(
      &entry.id
    ) {
      Ok(path) if path.exists() =>
        "saved"
          .green()
          .to_string(),

      Ok(_) =>
        "missing"
          .yellow()
          .to_string(),

      Err(_) =>
        "unknown"
          .dark_grey()
          .to_string(),
    };

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
    name,
    phone,
    entry.id,
    status,
    wpp,
  );
}

fn remove_native_storage(
  path: &Path,
) -> Result<()> {
  match std::fs::remove_file(path) {
    Ok(_) => Ok(()),

    Err(error)
      if error.kind()
        == std::io::ErrorKind::NotFound =>
    {
      Ok(())
    }

    Err(error) =>
      Err(error.into()),
  }
}