use anyhow::Result;
use crossterm::style::Stylize;

use crate::app::config::Config;

/// `wpp switch [target]`
///
/// - No argument: list all sessions, highlight the active one.
/// - With argument: switch the active session by alias or phone number.
pub fn run(target: Option<String>) -> Result<()> {
  let mut config = Config::load()?;

  match target {
    None => list_sessions(&config),

    Some(query) => switch_session(&mut config, &query),
  }
}

fn list_sessions(config: &Config) -> Result<()> {
  if config.sessions.is_empty() {
    println!(
      "{}",
      " No sessions. Run `wpp login` to get started.".dark_grey()
    );

    return Ok(());
  }

  let active = config.active_session.as_deref().unwrap_or("");

  println!();
  println!(" {}", "Sessions:".bold());
  println!();

  let mut entries: Vec<_> = config.sessions.values().collect();

  entries.sort_by_key(|entry| entry.aliases.first().cloned().unwrap_or_default());

  for entry in entries {
    let is_active = entry.aliases.iter().any(|alias| alias == active);

    let marker = if is_active {
      "▸".green().bold().to_string()
    } else {
      " ".to_string()
    };

    let aliases = if entry.aliases.is_empty() {
      "-".to_string()
    } else {
      entry.aliases.join(", ")
    };

    let phone = entry.phone.as_deref().unwrap_or("(no phone yet)");

    let name = entry
      .push_name
      .as_deref()
      .map(|n| format!(" — {n}"))
      .unwrap_or_default();

    let active_tag = if is_active {
      " (active)".green().to_string()
    } else {
      String::new()
    };

    println!(" {marker} {aliases:<28} {phone}{name}{active_tag}");
  }

  println!();

  Ok(())
}

fn switch_session(config: &mut Config, query: &str) -> Result<()> {
  let alias = match config.find_session(query) {
    Some((alias, _)) => alias.to_string(),

    None => {
      eprintln!(" {} No session matching '{}'", "✗".red().bold(), query);

      eprintln!(" Run `wpp switch` to see available sessions.");

      return Ok(());
    }
  };

  config.active_session = Some(alias.clone());

  config.save()?;

  let binding = alias.clone();
  let (_, entry) = config
    .find_session(&binding)
    .ok_or_else(|| anyhow::anyhow!("Session alias '{}' disappeared while switching.", alias))?;

  let phone = entry.phone.as_deref().unwrap_or("");

  let name = entry
    .push_name
    .as_deref()
    .map(|n| format!(" ({n})"))
    .unwrap_or_default();

  println!(
    " {} Switched to {} {phone}{name}",
    "✓".green().bold(),
    alias.bold()
  );

  Ok(())
}
