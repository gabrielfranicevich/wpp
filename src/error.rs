use thiserror::Error;

#[derive(Error, Debug)]
pub enum WppError {
  #[error("Network error: {0}")]
  Network(#[from] reqwest::Error),

  #[error("API error ({status}): {message}")]
  Api { status: u16, message: String },

  #[error("No active session. Run `wpp login` first.")]
  NoSession,

  #[error("Session not found: {0}")]
  SessionNotFound(String),

  #[error("QR code not ready")]
  QrNotReady,

  #[error("OpenWA is not reachable at {0}")]
  Unreachable(String),

  #[error("{0}")]
  Other(String),
}
