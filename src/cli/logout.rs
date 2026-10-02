use anyhow::Result;
use crossterm::style::Stylize;

use crate::app::config::Config;
use crate::whatsapp::client::WhatsAppClient;

/// `wpp logout [TARGET]`
///
/// Logs out the selected WhatsApp session through OpenWA and removes
/// the corresponding local session entry.
///
/// All aliases associated with that OpenWA session are removed because
/// logout operates on the session itself, not on an individual alias.
pub async fn run(target: Option<String>) -> Result<()> {
  let mut config = Config::load()?;

  let session_id = match target {
    Some(query) => {
      let Some((_, entry)) = config.find_session(&query) else {
        eprintln!(" {} No session matching '{}'", "✗".red().bold(), query);

        eprintln!(" Run `wpp switch` to see available sessions.");

        return Ok(());
      };

      entry.id.clone()
    }

    None => {
      let Some((_, entry)) = config.active_entry() else {
        anyhow::bail!(
          "No active session. Run `wpp switch` to select one \
       or `wpp login` to create one."
        );
      };

      entry.id.clone()
    }
  };

  let client = WhatsAppClient::openwa(&config.base_url, config.openwa_api_key());

  eprintln!(
    "{}",
    format!(" Logging out session '{session_id}'...").dark_grey()
  );

  client.logout(&session_id).await?;

  let removed = config.remove_session_by_id(&session_id);

  config.save()?;

  println!(
    " {} Logged out session {}",
    "✓".green().bold(),
    session_id.bold()
  );

  if let Some(entry) = removed {
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
    println!(" Run `wpp switch <alias>` to select another session.");
  }

  Ok(())
}
