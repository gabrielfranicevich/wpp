use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

/// A saved OpenWA session.
///
/// A single OpenWA session can have multiple user-facing wpp aliases.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SessionEntry {
  /// OpenWA session UUID
  pub id: String,

  /// User-facing aliases associated with this session
  #[serde(default)]
  pub aliases: Vec<String>,

  /// Phone number (filled once authenticated)
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub phone: Option<String>,

  /// Display name from WhatsApp profile
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub push_name: Option<String>,
}

/// Persistent config stored in ~/.config/wpp/config.toml
/// (or platform equivalent).
#[derive(Debug, Serialize, Deserialize)]
pub struct Config {
  /// OpenWA base URL (default: http://localhost:2785)
  pub base_url: String,

  /// Optional OpenWA API key (can also be supplied via WPP_OPENWA_API_KEY or OPENWA_API_KEY)
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub api_key: Option<String>,

  /// Alias of the last-used session
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub active_session: Option<String>,

  /// All saved OpenWA sessions keyed by session ID
  #[serde(default)]
  pub sessions: HashMap<String, SessionEntry>,
}

impl Default for Config {
  fn default() -> Self {
    Self {
      base_url: "http://localhost:2785".to_string(),
      api_key: None,
      active_session: None,
      sessions: HashMap::new(),
    }
  }
}

impl Config {
  /// Platform-specific config path:
  /// - Windows: %APPDATA%\wpp\config.toml
  /// - Linux: ~/.config/wpp/config.toml
  /// - macOS: ~/Library/Application Support/wpp/config.toml
  pub fn path() -> anyhow::Result<PathBuf> {
    let dir = dirs::config_dir()
      .ok_or_else(|| anyhow::anyhow!("Cannot determine config directory"))?
      .join("wpp");

    Ok(dir.join("config.toml"))
  }

  /// Load config from disk.
  ///
  /// Older versions stored sessions as:
  ///
  ///  alias -> SessionEntry { id, phone, push_name }
  ///
  /// The current format stores:
  ///
  ///  session_id -> SessionEntry { id, aliases, phone, push_name }
  ///
  /// Old files are migrated automatically.
  pub fn load() -> anyhow::Result<Self> {
    let path = Self::path()?;

    if !path.exists() {
      return Ok(Self::default());
    }

    let content = std::fs::read_to_string(&path)?;

    let mut config: Config = toml::from_str(&content)?;

    if config.migrate_legacy_sessions() {
      config.save()?;
    }

    Ok(config)
  }

  /// Persist config to disk, creating parent directories if needed.
  pub fn save(&self) -> anyhow::Result<()> {
    let path = Self::path()?;

    if let Some(parent) = path.parent() {
      std::fs::create_dir_all(parent)?;
    }

    let content = toml::to_string_pretty(self)?;

    std::fs::write(&path, content)?;

    Ok(())
  }

  /// Migrate the old alias-keyed session representation.
  ///
  /// Returns true when the config was changed.
  fn migrate_legacy_sessions(&mut self) -> bool {
    let legacy = self.sessions.iter().any(|(key, entry)| key != &entry.id);

    if !legacy {
      return false;
    }

    let old_sessions = std::mem::take(&mut self.sessions);

    let mut new_sessions: HashMap<String, SessionEntry> = HashMap::new();

    for (alias, entry) in old_sessions {
      let session_id = entry.id.clone();

      let target = new_sessions
        .entry(session_id.clone())
        .or_insert_with(|| SessionEntry {
          id: session_id,
          aliases: Vec::new(),
          phone: entry.phone.clone(),
          push_name: entry.push_name.clone(),
        });

      if !target.aliases.contains(&alias) {
        target.aliases.push(alias);
      }

      if target.phone.is_none() {
        target.phone = entry.phone;
      }

      if target.push_name.is_none() {
        target.push_name = entry.push_name;
      }
    }

    for entry in new_sessions.values_mut() {
      entry.aliases.sort();
    }

    self.sessions = new_sessions;

    true
  }

  /// Return the OpenWA API key.
  ///
  /// Resolution order:
  /// 1. `WPP_OPENWA_API_KEY` environment variable
  /// 2. `OPENWA_API_KEY` environment variable
  /// 3. `api_key` in config file
  /// 4. Auto-discovery from local OpenWA files (`openwa/data/.api-key`)
  pub fn openwa_api_key(&self) -> Option<String> {
    Self::resolve_api_key(self.api_key.as_deref())
  }

  /// Resolve the API key from environment, config or local files.
  pub fn resolve_api_key(from_config: Option<&str>) -> Option<String> {
    if let Ok(key) = std::env::var("WPP_OPENWA_API_KEY") {
      let trimmed = key.trim();

      if !trimmed.is_empty() {
        return Some(trimmed.to_string());
      }
    }

    if let Ok(key) = std::env::var("OPENWA_API_KEY") {
      let trimmed = key.trim();

      if !trimmed.is_empty() {
        return Some(trimmed.to_string());
      }
    }

    if let Some(key) = from_config {
      let trimmed = key.trim();

      if !trimmed.is_empty() {
        return Some(trimmed.to_string());
      }
    }

    // Local development fallback: check for OpenWA bootstrap .api-key file
    let mut candidates = vec![
      PathBuf::from("openwa/data/.api-key"),
      PathBuf::from("data/.api-key"),
      PathBuf::from("../openwa/data/.api-key"),
      PathBuf::from("../../openwa/data/.api-key"),
    ];

    if let Ok(exe) = std::env::current_exe() {
      if let Some(dir) = exe
        .parent()
        .and_then(|p| p.parent())
        .and_then(|p| p.parent())
      {
        candidates.push(dir.join("openwa").join("data").join(".api-key"));

        candidates.push(dir.join("data").join(".api-key"));
      }
    }

    for path in candidates {
      if let Ok(content) = std::fs::read_to_string(&path) {
        let trimmed = content.trim();

        if !trimmed.is_empty() {
          return Some(trimmed.to_string());
        }
      }
    }

    None
  }

  /// Get the active session entry, if any.
  ///
  /// Returns the active alias together with the session it points to.
  pub fn active_entry(&self) -> Option<(&str, &SessionEntry)> {
    let alias = self.active_session.as_deref()?;

    self.find_session(alias)
  }

  /// Get the active session's OpenWA UUID, or error.
  pub fn require_active_session(&self) -> anyhow::Result<&SessionEntry> {
    self.active_entry()
      .map(|(_, entry)| entry)
      .ok_or_else(|| anyhow::anyhow!("No active session. Run `wpp login` first."))
  }

  /// Find a session by alias or phone number.
  ///
  /// Returns the first matching alias and session.
  pub fn find_session<'a>(&'a self, query: &'a str) -> Option<(&'a str, &'a SessionEntry)> {
    if let Some(result) = self.find_by_alias(query) {
      return Some(result);
    }

    self.sessions.values().find_map(|entry| {
      entry.phone.as_deref().and_then(|phone| {
        if phone.contains(query) || query.contains(phone) {
          entry.aliases.first().map(|alias| (alias.as_str(), entry))
        } else {
          None
        }
      })
    })
  }

  /// Find a session by an exact normalized phone number.
  ///
  /// Formatting characters such as `+`, spaces and `-` are ignored.
  ///
  /// This is intentionally stricter than `find_session`, which also
  /// supports partial phone queries for interactive commands.
  pub fn find_session_by_phone(&self, phone: &str) -> Option<(&str, &SessionEntry)> {
    let normalized = normalize_phone(phone);

    if normalized.is_empty() {
      return None;
    }

    self.sessions.values().find_map(|entry| {
      let entry_phone = entry.phone.as_deref()?;

      if normalize_phone(entry_phone) == normalized {
        entry.aliases.first().map(|alias| (alias.as_str(), entry))
      } else {
        None
      }
    })
  }

  fn find_by_alias<'a>(&'a self, query: &str) -> Option<(&'a str, &'a SessionEntry)> {
    self.sessions.values().find_map(|entry| {
      entry
        .aliases
        .iter()
        .find(|alias| alias.as_str() == query)
        .map(|alias| (alias.as_str(), entry))
    })
  }

  /// Return true when an alias is already in use.
  pub fn alias_exists(&self, alias: &str) -> bool {
    self.sessions
      .values()
      .any(|entry| entry.aliases.iter().any(|existing| existing == alias))
  }

  /// Add an alias to an existing session.
  ///
  /// Returns true if the alias was added.
  pub fn add_alias(&mut self, session_id: &str, alias: String) -> anyhow::Result<bool> {
    if self.alias_exists(&alias) {
      return Ok(false);
    }

    let entry = self.sessions.get_mut(session_id).ok_or_else(|| {
      anyhow::anyhow!("OpenWA session '{}' is not known by wpp.", session_id)
    })?;

    entry.aliases.push(alias);
    entry.aliases.sort();

    Ok(true)
  }

  /// Insert a new session or update its metadata.
  ///
  /// The alias is associated with the session instead of
  /// becoming the map key.
  pub fn upsert_session(&mut self, session: SessionEntry, alias: String) -> anyhow::Result<()> {
    if self.alias_exists(&alias) {
      let existing = self.find_by_alias(&alias);

      if existing.map(|(_, entry)| entry.id.as_str()) != Some(session.id.as_str()) {
        anyhow::bail!("Session alias '{}' already exists.", alias);
      }
    }

    let session_id = session.id.clone();

    let session_phone = session.phone.clone();

    let session_push_name = session.push_name.clone();

    let entry = self.sessions.entry(session_id).or_insert(session);

    // Reusing a session can refresh its metadata.
    if session_phone.is_some() {
      entry.phone = session_phone;
    }

    if session_push_name.is_some() {
      entry.push_name = session_push_name;
    }

    if !entry.aliases.contains(&alias) {
      entry.aliases.push(alias);
      entry.aliases.sort();
    }

    Ok(())
  }

  /// Generate the next available wpp alias for a newly
  /// authenticated session.
  ///
  /// A phone number becomes the preferred alias. If no phone
  /// is available, `default` is used.
  ///
  /// Existing aliases are preserved by adding `-2`, `-3`, ...
  pub fn next_session_alias(&self, phone: Option<&str>) -> String {
    let phone_digits = phone
      .map(|value| {
        value
          .chars()
          .filter(|c| c.is_ascii_digit())
          .collect::<String>()
      })
      .filter(|value| !value.is_empty());

    let base = phone_digits.as_deref().unwrap_or("default");

    self.next_available_alias(base)
  }

  /// Return `base` when unused, otherwise the first available
  /// `base-N` alias.
  pub fn next_available_alias(&self, base: &str) -> String {
    if !self.alias_exists(base) {
      return base.to_string();
    }

    let mut suffix = 2usize;

    loop {
      let candidate = format!("{base}-{suffix}");

      if !self.alias_exists(&candidate) {
        return candidate;
      }

      suffix += 1;
    }
  }

  /// Remove a local session by alias.
  ///
  /// Because aliases belong to a session, removing one alias through
  /// this operation removes the entire local session representation.
  pub fn remove_session(&mut self, alias: &str) -> Option<SessionEntry> {
    let session_id = self
      .find_by_alias(alias)
      .map(|(_, entry)| entry.id.clone())?;

    self.remove_session_by_id(&session_id)
  }

  /// Remove one OpenWA session and all of its aliases.
  pub fn remove_session_by_id(&mut self, session_id: &str) -> Option<SessionEntry> {
    let removed = self.sessions.remove(session_id);

    if let Some(entry) = &removed {
      if self
        .active_session
        .as_deref()
        .is_some_and(|active| entry.aliases.iter().any(|alias| alias == active))
      {
        self.active_session = None;
      }
    }

    removed
  }

  /// Remove every local alias pointing to the same
  /// OpenWA session ID.
  ///
  /// Kept as a convenience for session deletion code.
  pub fn remove_sessions_by_id(&mut self, session_id: &str) -> Vec<(String, SessionEntry)> {
    let Some(entry) = self.remove_session_by_id(session_id) else {
      return Vec::new();
    };

    entry
      .aliases
      .iter()
      .cloned()
      .map(|alias| (alias, entry.clone()))
      .collect()
  }
}

/// Normalize a phone number for exact comparison.
///
/// WhatsApp/OpenWA may expose the number with different formatting,
/// so only ASCII digits are kept.
fn normalize_phone(value: &str) -> String {
  value.chars().filter(|c| c.is_ascii_digit()).collect()
}

#[cfg(test)]
mod tests {
  use super::{Config, SessionEntry};

  fn session(id: &str, aliases: &[&str]) -> SessionEntry {
    SessionEntry {
      id: id.to_string(),
      aliases: aliases.iter().map(|alias| alias.to_string()).collect(),
      phone: None,
      push_name: None,
    }
  }

  #[test]
  fn next_session_alias_uses_phone() {
    let config = Config::default();

    assert_eq!(
      config.next_session_alias(Some("+5493511234567")),
      "5493511234567"
    );
  }

  #[test]
  fn next_session_alias_falls_back_to_default() {
    let config = Config::default();

    assert_eq!(config.next_session_alias(None), "default");
  }

  #[test]
  fn next_session_alias_avoids_existing_aliases() {
    let mut config = Config::default();

    config.sessions.insert(
      "session-id".to_string(),
      session("session-id", &["5493511234567", "5493511234567-2"]),
    );

    assert_eq!(
      config.next_session_alias(Some("+5493511234567")),
      "5493511234567-3"
    );
  }

  #[test]
  fn add_alias_associates_multiple_aliases_with_one_session() {
    let mut config = Config::default();

    config.sessions.insert(
      "session-id".to_string(),
      session("session-id", &["personal"]),
    );

    let added = config.add_alias("session-id", "phone".to_string());

    assert!(added.is_ok());
    assert!(added.unwrap());

    let entry = &config.sessions["session-id"];

    assert_eq!(
      entry.aliases,
      vec!["personal".to_string(), "phone".to_string(),]
    );
  }

  #[test]
  fn add_alias_rejects_duplicate_alias() {
    let mut config = Config::default();

    config.sessions.insert(
      "session-id".to_string(),
      session("session-id", &["personal"]),
    );

    let added = config.add_alias("session-id", "personal".to_string());

    assert!(added.is_ok());
    assert!(!added.unwrap());
  }

  #[test]
  fn find_session_finds_alias() {
    let mut config = Config::default();

    config.sessions.insert(
      "session-id".to_string(),
      session("session-id", &["personal", "phone"]),
    );

    let result = config.find_session("phone");

    assert!(result.is_some());

    let (alias, entry) = result.unwrap();

    assert_eq!(alias, "phone");
    assert_eq!(entry.id, "session-id");
  }

  #[test]
  fn find_session_by_phone_normalizes_formatting() {
    let mut config = Config::default();

    let mut entry = session("session-id", &["personal"]);

    entry.phone = Some("+54 9 351-123-4567".to_string());

    config.sessions.insert("session-id".to_string(), entry);

    let result = config.find_session_by_phone("5493511234567");

    assert!(result.is_some());

    let (alias, entry) = result.unwrap();

    assert_eq!(alias, "personal");

    assert_eq!(entry.id, "session-id");
  }

  #[test]
  fn find_session_by_phone_does_not_match_partial_numbers() {
    let mut config = Config::default();

    let mut entry = session("session-id", &["personal"]);

    entry.phone = Some("5493511234567".to_string());

    config.sessions.insert("session-id".to_string(), entry);

    assert!(config.find_session_by_phone("351").is_none());
  }

  #[test]
  fn remove_active_session_clears_active_alias() {
    let mut config = Config::default();

    config.sessions.insert(
      "session-id".to_string(),
      session("session-id", &["personal", "phone"]),
    );

    config.active_session = Some("phone".to_string());

    let removed = config.remove_session_by_id("session-id");

    assert!(removed.is_some());
    assert!(config.sessions.is_empty());
    assert!(config.active_session.is_none());
  }

  #[test]
  fn remove_session_by_alias_removes_entire_session() {
    let mut config = Config::default();

    config.sessions.insert(
      "session-id".to_string(),
      session("session-id", &["personal", "phone"]),
    );

    let removed = config.remove_session("phone");

    assert!(removed.is_some());
    assert!(config.sessions.is_empty());
  }

  #[test]
  fn remove_sessions_by_id_returns_all_aliases() {
    let mut config = Config::default();

    config.sessions.insert(
      "shared-session".to_string(),
      session("shared-session", &["personal", "phone"]),
    );

    config.sessions.insert(
      "other-session".to_string(),
      session("other-session", &["business"]),
    );

    let removed = config.remove_sessions_by_id("shared-session");

    assert_eq!(removed.len(), 2);

    assert!(!config.sessions.contains_key("shared-session"));

    assert!(config.sessions.contains_key("other-session"));
  }

  #[test]
  fn upsert_session_creates_one_session_with_alias() {
    let mut config = Config::default();

    let result = config.upsert_session(session("session-id", &[]), "personal".to_string());

    assert!(result.is_ok());

    let entry = &config.sessions["session-id"];

    assert_eq!(entry.aliases, vec!["personal".to_string()]);
  }

  #[test]
  fn upsert_session_adds_alias_to_existing_session() {
    let mut config = Config::default();

    config.sessions.insert(
      "session-id".to_string(),
      session("session-id", &["personal"]),
    );

    let result = config.upsert_session(session("session-id", &[]), "phone".to_string());

    assert!(result.is_ok());

    let entry = &config.sessions["session-id"];

    assert_eq!(
      entry.aliases,
      vec!["personal".to_string(), "phone".to_string(),]
    );
  }

  #[test]
  fn upsert_session_updates_metadata() {
    let mut config = Config::default();

    config.sessions.insert(
      "session-id".to_string(),
      session("session-id", &["personal"]),
    );

    let mut updated = session("session-id", &[]);

    updated.phone = Some("5493511234567".to_string());

    updated.push_name = Some("Gabriel".to_string());

    let result = config.upsert_session(updated, "personal".to_string());

    assert!(result.is_ok());

    let entry = &config.sessions["session-id"];

    assert_eq!(entry.phone.as_deref(), Some("5493511234567"));

    assert_eq!(entry.push_name.as_deref(), Some("Gabriel"));
  }
}
