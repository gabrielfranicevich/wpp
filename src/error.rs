use thiserror::Error;

#[derive(Error, Debug)]
pub enum WppError {
  #[error("{0}")]
  Other(String),
}