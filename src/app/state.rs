use crate::app::config::{Config, SessionEntry};
use crate::whatsapp::client::WhatsAppClient;

/// The session selected by `wpp switch` or created by `wpp login`.
#[derive(Debug, Clone)]
pub struct ActiveSession {
  pub entry: SessionEntry,
}

/// Runtime dependencies shared by CLI commands.
pub struct AppContext {
  pub session: ActiveSession,
  pub whatsapp: WhatsAppClient,
}

impl AppContext {
  /// Load configuration and create a WhatsApp client bound
  /// to the active session.
  pub async fn load() -> anyhow::Result<Self> {
    let config = Config::load()?;

    let (_, entry) = config
      .active_entry()
      .ok_or_else(|| anyhow::anyhow!("No active session. Run `wpp login` first."))?;

    let session = ActiveSession {
      entry: entry.clone(),
    };

    let path = config.native_session_path(&entry.id)?;

    let whatsapp = WhatsAppClient::open_native(&path).await?;

    Ok(Self { session, whatsapp })
  }
}
