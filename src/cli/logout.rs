use anyhow::Result;
use crossterm::style::Stylize;

use crate::app::config::Config;
use crate::whatsapp::client::WhatsAppClient;

/// `wpp logout [TARGET]`
///
/// Logs out the selected WhatsApp session through OpenWA and removes
/// the corresponding local session entry.
pub async fn run(target: Option<String>) -> Result<()> {
  let mut config = Config::load()?;

  let (alias, session_id) = match target {
    Some(query) => {
      let Some((alias, entry)) =
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

      (alias.to_string(), entry.id.clone())
    }

    None => {
      let Some((alias, entry)) =
        config.active_entry()
      else {
        anyhow::bail!(
          "No active session. Run `wpp switch` to select one \
           or `wpp login` to create one."
        );
      };

      (alias.to_string(), entry.id.clone())
    }
  };

  let client = WhatsAppClient::openwa(
    &config.base_url,
    config.openwa_api_key(),
  );

  eprintln!(
    "{}",
    format!(" Logging out '{alias}'...").dark_grey()
  );

  client.logout(&session_id).await?;

  config.remove_session(&alias);
  config.save()?;

  println!(
    " {} Logged out {}",
    "✓".green().bold(),
    alias.bold()
  );

  if config.active_session.is_none() {
    println!(
      " Run `wpp switch <alias>` to select another session."
    );
  }

  Ok(())
}