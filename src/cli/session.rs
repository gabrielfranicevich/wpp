use std::path::Path;

use anyhow::{bail, Result};
use crossterm::style::Stylize;

use crate::app::config::{
  BackendKind,
  Config,
  SessionEntry,
};
use crate::whatsapp::client::WhatsAppClient;
use crate::whatsapp::models::Session;

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
  let has_openwa =
    config.sessions.values().any(
      |entry| {
        entry.backend
          == BackendKind::OpenWA
      }
    );

  let native_entries =
    config
      .sessions
      .values()
      .filter(|entry| {
        entry.backend
          == BackendKind::Native
      })
      .collect::<Vec<_>>();

  let openwa_sessions =
    if has_openwa {
      let client =
        WhatsAppClient::openwa(
          &config.base_url,
          config.openwa_api_key(),
        );

      let mut sessions =
        client.list_sessions()
          .await?;

      sessions.sort_by(|a, b| {
        a.name
          .cmp(&b.name)
          .then_with(|| a.id.cmp(&b.id))
      });

      Some(sessions)
    } else {
      None
    };

  let has_openwa_sessions =
    openwa_sessions
      .as_ref()
      .is_some_and(
        |sessions| !sessions.is_empty()
      );

  if !has_openwa_sessions
    && native_entries.is_empty()
  {
    println!(
      " {}",
      "No sessions.".dark_grey()
    );

    return Ok(());
  }

  if let Some(sessions) =
    openwa_sessions
  {
    if !sessions.is_empty() {
      println!();
      println!(
        " {}",
        "OpenWA sessions:".bold()
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

      for session in
        &sessions
      {
        print_session(
          session,
          config,
        );
      }
    }
  }

  if !native_entries.is_empty() {
    let mut entries =
      native_entries;

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
      "Native sessions:".bold()
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

    for entry in
      entries
    {
      print_native_session(
        entry,
        config,
      );
    }
  }

  println!();

  Ok(())
}

async fn delete_session(
  config: &mut Config,
  target: &str,
) -> Result<()> {
  if let Some((_, entry)) =
    config.find_session(target)
  {
    let entry =
      entry.clone();

    if entry.backend
      == BackendKind::Native
    {
      return delete_native_session(
        config,
        &entry,
      )
      .await;
    }

    return delete_local_openwa_session(
      config,
      target,
      &entry,
    )
    .await;
  }

  if let Some(entry) =
    config.sessions.get(target).cloned()
  {
    if entry.backend
      == BackendKind::Native
    {
      return delete_native_session(
        config,
        &entry,
      )
      .await;
    }

    return delete_local_openwa_session(
      config,
      target,
      &entry,
    )
    .await;
  }

  let client =
    WhatsAppClient::openwa(
      &config.base_url,
      config.openwa_api_key(),
    );

  let sessions =
    client.list_sessions().await?;

  if let Some(session) =
    sessions
      .iter()
      .find(|session| {
        session.id == target
      })
  {
    return delete_remote_session(
      config,
      &client,
      session,
    )
    .await;
  }

  let session =
    resolve_remote_target(
      &sessions,
      target,
    )?;

  let Some(session) =
    session
  else {
    bail!(
      "No OpenWA session or local reference matching '{}'. \
       Run `wpp session` to see available sessions.",
      target
    );
  };

  delete_remote_session(
    config,
    &client,
    session,
  )
  .await
}

async fn delete_local_openwa_session(
  config: &mut Config,
  target: &str,
  entry: &SessionEntry,
) -> Result<()> {
  let client =
    WhatsAppClient::openwa(
      &config.base_url,
      config.openwa_api_key(),
    );

  let sessions =
    client.list_sessions().await?;

  let Some(session) =
    sessions
      .iter()
      .find(|session| {
        session.id == entry.id
      })
  else {
    return remove_stale_local_openwa_session(
      config,
      &entry.id,
      target,
    );
  };

  delete_remote_session(
    config,
    &client,
    session,
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
    " {} Deleting native session {}...",
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
    " {} Deleted native session {}",
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
      "i".dark_grey()
    );
  } else {
    let aliases =
      removed
        .into_iter()
        .map(
          |(alias, _)| alias
        )
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

  match client.logout(&session_id).await {
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
      &session_id
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
    let aliases =
      removed
        .into_iter()
        .map(
          |(alias, _)| alias
        )
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

fn remove_stale_local_openwa_session(
  config: &mut Config,
  session_id: &str,
  target: &str,
) -> Result<()> {
  let removed =
    config.remove_sessions_by_id(
      session_id
    );

  if removed.is_empty() {
    bail!(
      "No local session reference matching '{}'. \
       Run `wpp session` to see available sessions.",
      target
    );
  }

  config.save()?;

  let aliases =
    removed
      .into_iter()
      .map(
        |(alias, _)| alias
      )
      .collect::<Vec<_>>();

  println!();

  println!(
    " {} Local reference '{}' points to an OpenWA session that no longer exists.",
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
  if let Some(session) =
    sessions
      .iter()
      .find(|session| {
        session.name == target
      })
  {
    return Ok(Some(session));
  }

  let normalized_target =
    normalize_phone(target);

  if !normalized_target.is_empty() {
    let phone_matches =
      sessions
        .iter()
        .filter(|session| {
          session
            .phone
            .as_deref()
            .map(normalize_phone)
            .is_some_and(
              |phone| {
                phone == normalized_target
              }
            )
        })
        .collect::<Vec<_>>();

    match phone_matches.len() {
      1 => {
        return Ok(
          Some(phone_matches[0])
        );
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
      &session.id
    );

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

  let phone =
    session
      .phone
      .as_deref()
      .unwrap_or("-");

  let status =
    format_status(
      &session.status
    );

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

fn print_native_session(
  entry: &SessionEntry,
  config: &Config,
) {
  let aliases =
    {
      let mut aliases =
        entry.aliases.clone();

      aliases.sort();

      aliases
    };

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

  let storage =
    config.native_session_path(
      &entry.id
    );

  let status =
    match storage {
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

fn aliases_for_session(
  config: &Config,
  session_id: &str,
) -> Vec<String> {
  let mut aliases =
    config
      .sessions
      .get(session_id)
      .map(
        |entry| entry.aliases.clone()
      )
      .unwrap_or_default();

  aliases.sort();

  aliases
}

fn format_status(
  status: &str,
) -> String {
  match status {
    "ready" =>
      status
        .green()
        .to_string(),

    "failed" =>
      status
        .red()
        .to_string(),

    "disconnected" =>
      status
        .dark_grey()
        .to_string(),

    "initializing"
    | "qr_ready"
    | "authenticating"
    | "action_required" =>
      status
        .yellow()
        .to_string(),

    "created" =>
      status
        .cyan()
        .to_string(),

    _ =>
      status.to_string(),
  }
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