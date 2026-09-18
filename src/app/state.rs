use crate::app::config::Config;
use crate::whatsapp::client::OpenWAClient;

/// Convenience: load config and build a client pointed at the active session.
pub fn load_context() -> anyhow::Result<(Config, OpenWAClient, String)> {
    let config = Config::load()?;
    let entry = config.require_active_session()?;
    let session_id = entry.id.clone();
    let client = OpenWAClient::new(&config.base_url);
    Ok((config, client, session_id))
}
