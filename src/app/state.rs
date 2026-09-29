use crate::app::config::{Config, SessionEntry};
use crate::whatsapp::client::WhatsAppClient;

/// The session selected by `wpp switch` or created by `wpp login`.
#[derive(Debug, Clone)]
pub struct ActiveSession {
    pub alias: String,
    pub entry: SessionEntry,
}

/// Runtime dependencies shared by CLI commands.
pub struct AppContext {
    pub config: Config,
    pub session: ActiveSession,
    pub whatsapp: WhatsAppClient,
}

impl AppContext {
    /// Load configuration and create a WhatsApp client bound to the active session.
    pub fn load() -> anyhow::Result<Self> {
        let config = Config::load()?;

        let (alias, entry) = config
            .active_entry()
            .ok_or_else(|| anyhow::anyhow!("No active session. Run `wpp login` first."))?;

        let session = ActiveSession {
            alias: alias.to_string(),
            entry: entry.clone(),
        };

        let whatsapp = WhatsAppClient::openwa(&config.base_url, config.openwa_api_key());

        Ok(Self {
            config,
            session,
            whatsapp,
        })
    }
}
