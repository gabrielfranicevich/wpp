use thiserror::Error;

#[derive(Error, Debug)]
pub enum WppError {
  #[error("Network error: {0}")]
  Network(#[from] reqwest::Error),

  #[error("API error ({status}): {message}")]
  Api {
    status: u16,
    message: String,
  },

  #[error("{0}")]
  Other(String),
}