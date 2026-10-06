use std::path::Path;

use anyhow::Result;
use crossterm::style::Stylize;

use crate::app::config::Config;
use crate::whatsapp::client::WhatsAppClient;

pub async fn run(
  target: Option<String>,
) -> Result<()> {
  let mut config =
    Config::load()?;

  let session_id =
    match target {
      Some(query) => {
        let Some((_, entry)) =
          config.find_session(&query)
        else {
          eprintln!(
            " {} No session matching '{}'",
            "✗".red().bold(),
            query
          );

          eprintln!(
            " Run `wpp switch` to see available sessions."
          );

          return Ok(());
        };

        entry.id.clone()
      }

      None => {
        let Some((_, entry)) =
          config.active_entry()
        else {
          anyhow::bail!(
            "No active session. Run `wpp switch` to select one \
             or `wpp login` to create one."
          );
        };

        entry.id.clone()
      }
    };

  let native_path =
    config.native_session_path(
      &session_id
    )?;

  eprintln!(
    "{}",
    format!(
      " Logging out session '{session_id}'..."
    )
    .dark_grey()
  );

  if native_path.exists() {
    let client =
      WhatsAppClient::open_native(
        &native_path
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
    &native_path
  )?;

  let removed =
    config.remove_session_by_id(
      &session_id
    );

  config.save()?;

  println!(
    " {} Logged out session {}",
    "✓".green().bold(),
    session_id.bold()
  );

  println!(
    " {} Removed native session storage.",
    "✓".green().bold()
  );

  if let Some(entry) =
    removed
  {
    if entry.aliases.len() == 1 {
      println!(
        " {} Removed local alias: {}",
        "✓".green().bold(),
        entry.aliases[0]
      );
    } else if !entry.aliases.is_empty() {
      println!(
        " {} Removed local aliases: {}",
        "✓".green().bold(),
        entry.aliases.join(", ")
      );
    }
  }

  if config.active_session.is_none() {
    println!(
      " Run `wpp switch <alias>` to select another session."
    );
  }

  Ok(())
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