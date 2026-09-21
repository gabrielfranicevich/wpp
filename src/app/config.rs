use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

/// A saved session: maps an alias to an OpenWA session UUID + phone.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SessionEntry {
  /// OpenWA session UUID
  pub id: String,

  /// Phone number (filled once authenticated)
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub phone: Option<String>,

  /// Display name from WhatsApp profile
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub push_name: Option<String>,
}

/// Persistent config stored in ~/.config/wpp/config.toml (or platform equivalent).
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

  /// All saved sessions keyed by alias
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
  /// - Linux:  ~/.config/wpp/config.toml
  /// - macOS:  ~/Library/Application Support/wpp/config.toml
  pub fn path() -> anyhow::Result<PathBuf> {
    let dir = dirs::config_dir()
      .ok_or_else(|| {
        anyhow::anyhow!(
          "Cannot determine config directory"
        )
      })?
      .join("wpp");

    Ok(dir.join("config.toml"))
  }

  /// Load config from disk, or return defaults if file doesn't exist.
  pub fn load() -> anyhow::Result<Self> {
    let path = Self::path()?;

    if !path.exists() {
      return Ok(Self::default());
    }

    let content = std::fs::read_to_string(&path)?;
    let config: Config = toml::from_str(&content)?;

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
  pub fn resolve_api_key(
    from_config: Option<&str>,
  ) -> Option<String> {
    if let Ok(key) =
      std::env::var("WPP_OPENWA_API_KEY")
    {
      let trimmed = key.trim();

      if !trimmed.is_empty() {
        return Some(trimmed.to_string());
      }
    }

    if let Ok(key) =
      std::env::var("OPENWA_API_KEY")
    {
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
        candidates.push(
          dir.join("openwa")
            .join("data")
            .join(".api-key"),
        );

        candidates.push(
          dir.join("data")
            .join(".api-key"),
        );
      }
    }

    for path in candidates {
      if let Ok(content) =
        std::fs::read_to_string(&path)
      {
        let trimmed = content.trim();

        if !trimmed.is_empty() {
          return Some(trimmed.to_string());
        }
      }
    }

    None
  }

  /// Get the active session entry, if any.
  pub fn active_entry(
    &self,
  ) -> Option<(&str, &SessionEntry)> {
    let alias = self.active_session.as_deref()?;

    self.sessions
      .get(alias)
      .map(|entry| (alias, entry))
  }

  /// Get the active session's OpenWA UUID, or error.
  pub fn require_active_session(
    &self,
  ) -> anyhow::Result<&SessionEntry> {
    self.active_entry()
      .map(|(_, entry)| entry)
      .ok_or_else(|| {
        anyhow::anyhow!(
          "No active session. Run `wpp login` first."
        )
      })
  }

  /// Find a session by alias or phone number.
  pub fn find_session<'a>(
    &'a self,
    query: &'a str,
  ) -> Option<(&'a str, &'a SessionEntry)> {
    if let Some(entry) = self.sessions.get(query) {
      return Some((query, entry));
    }

    self.sessions.iter().find_map(
      |(alias, entry)| {
        entry.phone.as_deref().and_then(|phone| {
          if phone.contains(query)
            || query.contains(phone)
          {
            Some((alias.as_str(), entry))
          } else {
            None
          }
        })
      },
    )
  }

  /// Generate the next available wpp alias for a newly
  /// authenticated session.
  ///
  /// A phone number becomes the preferred alias. If no phone
  /// is available, `default` is used.
  ///
  /// Existing aliases are preserved by adding `-2`, `-3`, ...
  pub fn next_session_alias(
    &self,
    phone: Option<&str>,
  ) -> String {
    let phone_digits = phone
      .map(|value| {
        value
          .chars()
          .filter(|c| c.is_ascii_digit())
          .collect::<String>()
      })
      .filter(|value| !value.is_empty());

    let base = phone_digits
      .as_deref()
      .unwrap_or("default");

    self.next_available_alias(base)
  }

  /// Return `base` when unused, otherwise the first available
  /// `base-N` alias.
  pub fn next_available_alias(
    &self,
    base: &str,
  ) -> String {
    if !self.sessions.contains_key(base) {
      return base.to_string();
    }

    let mut suffix = 2usize;

    loop {
      let candidate = format!("{base}-{suffix}");

      if !self.sessions.contains_key(&candidate) {
        return candidate;
      }

      suffix += 1;
    }
  }

  /// Remove a local session entry.
  ///
  /// When removing the active session, the active session is cleared as well.
  pub fn remove_session(
    &mut self,
    alias: &str,
  ) -> Option<SessionEntry> {
    let removed = self.sessions.remove(alias);

    if self.active_session.as_deref() == Some(alias) {
      self.active_session = None;
    }

    removed
  }
}

#[cfg(test)]
mod tests {
  use super::{
    Config,
    SessionEntry,
  };

  fn session() -> SessionEntry {
    SessionEntry {
      id: "session-id".to_string(),
      phone: None,
      push_name: None,
    }
  }

  #[test]
  fn next_session_alias_uses_phone() {
    let config = Config::default();

    assert_eq!(
      config.next_session_alias(
        Some("+5493511234567")
      ),
      "5493511234567"
    );
  }

  #[test]
  fn next_session_alias_falls_back_to_default() {
    let config = Config::default();

    assert_eq!(
      config.next_session_alias(None),
      "default"
    );
  }

  #[test]
  fn next_session_alias_avoids_phone_collision() {
    let mut config = Config::default();

    config.sessions.insert(
      "5493511234567".to_string(),
      session(),
    );

    config.sessions.insert(
      "5493511234567-2".to_string(),
      session(),
    );

    assert_eq!(
      config.next_session_alias(
        Some("+5493511234567")
      ),
      "5493511234567-3"
    );
  }

  #[test]
  fn next_available_alias_avoids_default_collision() {
    let mut config = Config::default();

    config.sessions.insert(
      "default".to_string(),
      session(),
    );

    config.sessions.insert(
      "default-2".to_string(),
      session(),
    );

    assert_eq!(
      config.next_available_alias("default"),
      "default-3"
    );
  }

  #[test]
  fn remove_active_session_clears_active_alias() {
    let mut config = Config::default();

    config.sessions.insert(
      "personal".to_string(),
      session(),
    );

    config.active_session =
      Some("personal".to_string());

    let removed =
      config.remove_session("personal");

    assert!(removed.is_some());
    assert!(config.sessions.is_empty());
    assert!(config.active_session.is_none());
  }

  #[test]
  fn remove_non_active_session_keeps_active_alias() {
    let mut config = Config::default();

    config.sessions.insert(
      "personal".to_string(),
      session(),
    );

    config.sessions.insert(
      "business".to_string(),
      session(),
    );

    config.active_session =
      Some("personal".to_string());

    let removed =
      config.remove_session("business");

    assert!(removed.is_some());
    assert_eq!(
      config.active_session.as_deref(),
      Some("personal")
    );
    assert!(
      config.sessions.contains_key("personal")
    );
  }
}