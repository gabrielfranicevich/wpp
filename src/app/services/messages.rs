use crate::error::WppError;
use crate::whatsapp::client::WhatsAppClient;

/// Send a plain text message to a chat.
pub async fn send_text(
  whatsapp: &WhatsAppClient,
  session_id: &str,
  chat_id: &str,
  text: &str,
) -> Result<(), WppError> {
  if text.trim().is_empty() {
    return Err(WppError::Other("message cannot be empty".to_string()));
  }

  whatsapp.send_text(session_id, chat_id, text).await
}
