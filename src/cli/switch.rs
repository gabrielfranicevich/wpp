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

  // Collect and sort by alias for stable output
  let mut entries: Vec<_> = config.sessions.iter().collect();
  entries.sort_by_key(|(alias, _)| alias.to_string());

  for (alias, entry) in entries {
    let marker = if alias.as_str() == active {
      "▸".green().bold().to_string()
    } else {
      " ".to_string()
    };

    let phone = entry
      .phone
      .as_deref()
      .unwrap_or("(no phone yet)");

    let name = entry
      .push_name
      .as_deref()
      .map(|n| format!(" — {n}"))
      .unwrap_or_default();

    let active_tag = if alias.as_str() == active {
      " (active)".green().to_string()
    } else {
      String::new()
    };

    println!(
      " {marker} {alias:<16} {phone}{name}{active_tag}",
      alias = alias.clone().bold(),
    );
  }
  println!();

  Ok(())
}

fn switch_session(config: &mut Config, query: &str) -> Result<()> {
  // Find by alias or phone
  let alias = match config.find_session(query) {
    Some((alias, _)) => alias.to_string(),
    None => {
      eprintln!(
        " {} No session matching '{}'",
        "✗".red().bold(),
        query
      );
      eprintln!(" Run `wpp switch` to see available sessions.");
      return Ok(());
    }
  };

  config.active_session = Some(alias.clone());
  config.save()?;

  let entry = &config.sessions[&alias];
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
