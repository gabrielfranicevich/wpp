use crate::error::WppError;
use crate::whatsapp::models::{
  Chat, Message, MessagePage, PairingCodeResponse, QrCodeResponse, Session,
};
use crate::whatsapp::openwa::client::OpenWAClient;

/// WhatsApp backend used by the application layer.
///
/// The rest of wpp depends on this type instead of OpenWA-specific clients.
/// A future native WhatsApp backend can be added here without changing the
/// command/service layer.
pub struct WhatsAppClient {
  backend: Backend,
}

enum Backend {
  OpenWA(OpenWAClient),
}

impl WhatsAppClient {
  pub fn openwa(base_url: &str, api_key: Option<String>) -> Self {
    Self {
      backend: Backend::OpenWA(OpenWAClient::new(base_url, api_key)),
    }
  }

  pub async fn create_session(&self, name: &str) -> Result<Session, WppError> {
    match &self.backend {
      Backend::OpenWA(client) => client.create_session(name).await,
    }
  }

  pub async fn start_session(&self, session_id: &str) -> Result<Session, WppError> {
    match &self.backend {
      Backend::OpenWA(client) => client.start_session(session_id).await,
    }
  }

  pub async fn get_session(&self, session_id: &str) -> Result<Session, WppError> {
    match &self.backend {
      Backend::OpenWA(client) => client.get_session(session_id).await,
    }
  }

  pub async fn list_sessions(&self) -> Result<Vec<Session>, WppError> {
    match &self.backend {
      Backend::OpenWA(client) => client.list_sessions().await,
    }
  }

  pub async fn delete_session(&self, session_id: &str) -> Result<(), WppError> {
    match &self.backend {
      Backend::OpenWA(client) => client.delete_session(session_id).await,
    }
  }

  pub async fn logout(&self, session_id: &str) -> Result<Session, WppError> {
    match &self.backend {
      Backend::OpenWA(client) => client.logout(session_id).await,
    }
  }

  pub async fn get_qr(&self, session_id: &str) -> Result<QrCodeResponse, WppError> {
    match &self.backend {
      Backend::OpenWA(client) => client.get_qr(session_id).await,
    }
  }

  pub async fn request_pairing_code(
    &self,
    session_id: &str,
    phone: &str,
  ) -> Result<PairingCodeResponse, WppError> {
    match &self.backend {
      Backend::OpenWA(client) => client.request_pairing_code(session_id, phone).await,
    }
  }

  pub async fn list_chats(
    &self,
    session_id: &str,
    limit: usize,
    offset: usize,
  ) -> Result<Vec<Chat>, WppError> {
    match &self.backend {
      Backend::OpenWA(client) => client.list_chats(session_id, limit, offset).await,
    }
  }

  pub async fn list_messages(
    &self,
    session_id: &str,
    chat_id: &str,
    limit: usize,
    after: Option<&str>,
  ) -> Result<MessagePage, WppError> {
    match &self.backend {
      Backend::OpenWA(client) => {
        client
          .list_messages(session_id, chat_id, limit, after)
          .await
      }
    }
  }

  /// Get recent chat history directly from the WhatsApp engine.
  ///
  /// `deep = false` uses OpenWA's normal history limit.
  /// `deep = true` allows the larger history ceiling.
  pub async fn get_chat_history(
    &self,
    session_id: &str,
    chat_id: &str,
    limit: usize,
    deep: bool,
  ) -> Result<Vec<Message>, WppError> {
    match &self.backend {
      Backend::OpenWA(client) => {
        client
          .get_chat_history(session_id, chat_id, limit, deep)
          .await
      }
    }
  }
}
