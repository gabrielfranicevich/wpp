use crate::error::WppError;
use crate::whatsapp::models::{
  Chat,
  Message,
  MessagePage,
  PairingCodeResponse,
  QrCodeResponse,
  RealtimeEvent,
  Session,
};
use crate::whatsapp::native::NativeClient;
use crate::whatsapp::openwa::client::{
  OpenWAClient,
  OpenWARealtimeListener,
};

/// Realtime listener exposed by the WhatsApp abstraction layer.
///
/// Concrete transport details remain hidden inside the backend modules.
pub struct RealtimeListener {
  backend: RealtimeBackend,
}

enum RealtimeBackend {
  OpenWA(OpenWARealtimeListener),
}

impl RealtimeListener {
  pub fn try_recv(
    &mut self,
  ) -> Option<RealtimeEvent> {
    match &mut self.backend {
      RealtimeBackend::OpenWA(
        listener
      ) => listener.try_recv(),
    }
  }

  pub async fn recv(
    &mut self,
  ) -> Option<RealtimeEvent> {
    match &mut self.backend {
      RealtimeBackend::OpenWA(
        listener
      ) => listener.recv().await,
    }
  }

  pub async fn disconnect(
    self,
  ) -> Result<(), WppError> {
    match self.backend {
      RealtimeBackend::OpenWA(
        listener
      ) => listener.disconnect().await,
    }
  }
}

/// WhatsApp backend used by the application layer.
///
/// Native Rust is now available as a backend, while existing OpenWA
/// sessions remain supported until the individual application operations
/// are migrated.
pub struct WhatsAppClient {
  backend: Backend,
}

enum Backend {
  OpenWA(OpenWAClient),
  Native(NativeClient),
}

impl WhatsAppClient {
  /// Create a client backed by the existing OpenWA transport.
  pub fn openwa(
    base_url: &str,
    api_key: Option<String>,
  ) -> Self {
    Self {
      backend: Backend::OpenWA(
        OpenWAClient::new(
          base_url,
          api_key,
        ),
      ),
    }
  }

  /// Create a client backed by the native Rust transport.
  pub fn native(
    client: NativeClient,
  ) -> Self {
    Self {
      backend: Backend::Native(
        client
      ),
    }
  }

  pub async fn create_session(
    &self,
    name: &str,
  ) -> Result<Session, WppError> {
    match &self.backend {
      Backend::OpenWA(client) => {
        client.create_session(name).await
      }

      Backend::Native(_) => {
        native_not_implemented(
          "create session"
        )
      }
    }
  }

  pub async fn start_session(
    &self,
    session_id: &str,
  ) -> Result<Session, WppError> {
    match &self.backend {
      Backend::OpenWA(client) => {
        client
          .start_session(session_id)
          .await
      }

      Backend::Native(_) => {
        native_not_implemented(
          "start session"
        )
      }
    }
  }

  pub async fn get_session(
    &self,
    session_id: &str,
  ) -> Result<Session, WppError> {
    match &self.backend {
      Backend::OpenWA(client) => {
        client
          .get_session(session_id)
          .await
      }

      Backend::Native(_) => {
        native_not_implemented(
          "get session"
        )
      }
    }
  }

  pub async fn list_sessions(
    &self,
  ) -> Result<Vec<Session>, WppError> {
    match &self.backend {
      Backend::OpenWA(client) => {
        client.list_sessions().await
      }

      Backend::Native(_) => {
        native_not_implemented(
          "list sessions"
        )
      }
    }
  }

  pub async fn delete_session(
    &self,
    session_id: &str,
  ) -> Result<(), WppError> {
    match &self.backend {
      Backend::OpenWA(client) => {
        client
          .delete_session(session_id)
          .await
      }

      Backend::Native(_) => {
        native_not_implemented(
          "delete session"
        )
      }
    }
  }

  pub async fn logout(
    &self,
    session_id: &str,
  ) -> Result<Session, WppError> {
    match &self.backend {
      Backend::OpenWA(client) => {
        client.logout(session_id).await
      }

      Backend::Native(_) => {
        native_not_implemented(
          "logout"
        )
      }
    }
  }

  pub async fn get_qr(
    &self,
    session_id: &str,
  ) -> Result<QrCodeResponse, WppError> {
    match &self.backend {
      Backend::OpenWA(client) => {
        client.get_qr(session_id).await
      }

      Backend::Native(_) => {
        native_not_implemented(
          "get QR code"
        )
      }
    }
  }

  pub async fn request_pairing_code(
    &self,
    session_id: &str,
    phone: &str,
  ) -> Result<PairingCodeResponse, WppError> {
    match &self.backend {
      Backend::OpenWA(client) => {
        client
          .request_pairing_code(
            session_id,
            phone,
          )
          .await
      }

      Backend::Native(_) => {
        native_not_implemented(
          "request pairing code"
        )
      }
    }
  }

  pub async fn list_chats(
    &self,
    session_id: &str,
    limit: usize,
    offset: usize,
  ) -> Result<Vec<Chat>, WppError> {
    match &self.backend {
      Backend::OpenWA(client) => {
        client
          .list_chats(
            session_id,
            limit,
            offset,
          )
          .await
      }

      Backend::Native(_) => {
        native_not_implemented(
          "list chats"
        )
      }
    }
  }

  pub async fn delete_chat(
    &self,
    session_id: &str,
    chat_id: &str,
  ) -> Result<(), WppError> {
    match &self.backend {
      Backend::OpenWA(client) => {
        client
          .delete_chat(
            session_id,
            chat_id,
          )
          .await
      }

      Backend::Native(_) => {
        native_not_implemented(
          "delete chat"
        )
      }
    }
  }

  pub async fn block_contact(
    &self,
    session_id: &str,
    contact_id: &str,
  ) -> Result<(), WppError> {
    match &self.backend {
      Backend::OpenWA(client) => {
        client
          .block_contact(
            session_id,
            contact_id,
          )
          .await
      }

      Backend::Native(_) => {
        native_not_implemented(
          "block contact"
        )
      }
    }
  }

  pub async fn unblock_contact(
    &self,
    session_id: &str,
    contact_id: &str,
  ) -> Result<(), WppError> {
    match &self.backend {
      Backend::OpenWA(client) => {
        client
          .unblock_contact(
            session_id,
            contact_id,
          )
          .await
      }

      Backend::Native(_) => {
        native_not_implemented(
          "unblock contact"
        )
      }
    }
  }

  pub async fn archive_chat(
    &self,
    session_id: &str,
    chat_id: &str,
    archive: bool,
  ) -> Result<(), WppError> {
    match &self.backend {
      Backend::OpenWA(client) => {
        client
          .archive_chat(
            session_id,
            chat_id,
            archive,
          )
          .await
      }

      Backend::Native(_) => {
        native_not_implemented(
          "archive chat"
        )
      }
    }
  }

  pub async fn pin_chat(
    &self,
    session_id: &str,
    chat_id: &str,
    pin: bool,
  ) -> Result<(), WppError> {
    match &self.backend {
      Backend::OpenWA(client) => {
        client
          .pin_chat(
            session_id,
            chat_id,
            pin,
          )
          .await
      }

      Backend::Native(_) => {
        native_not_implemented(
          "pin chat"
        )
      }
    }
  }

  pub async fn mute_chat(
    &self,
    session_id: &str,
    chat_id: &str,
    mute_until: Option<i64>,
  ) -> Result<(), WppError> {
    match &self.backend {
      Backend::OpenWA(client) => {
        client
          .mute_chat(
            session_id,
            chat_id,
            mute_until,
          )
          .await
      }

      Backend::Native(_) => {
        native_not_implemented(
          "mute chat"
        )
      }
    }
  }

  pub async fn mark_chat_read(
    &self,
    session_id: &str,
    chat_id: &str,
    message_ids: &[String],
  ) -> Result<(), WppError> {
    match &self.backend {
      Backend::OpenWA(client) => {
        client
          .mark_chat_read(
            session_id,
            chat_id,
            message_ids,
          )
          .await
      }

      Backend::Native(_) => {
        native_not_implemented(
          "mark chat as read"
        )
      }
    }
  }

  pub async fn mark_chat_unread(
    &self,
    session_id: &str,
    chat_id: &str,
  ) -> Result<(), WppError> {
    match &self.backend {
      Backend::OpenWA(client) => {
        client
          .mark_chat_unread(
            session_id,
            chat_id,
          )
          .await
      }

      Backend::Native(_) => {
        native_not_implemented(
          "mark chat as unread"
        )
      }
    }
  }

  pub async fn send_text(
    &self,
    session_id: &str,
    chat_id: &str,
    text: &str,
  ) -> Result<(), WppError> {
    match &self.backend {
      Backend::OpenWA(client) => {
        client
          .send_text(
            session_id,
            chat_id,
            text,
          )
          .await
      }

      Backend::Native(_) => {
        native_not_implemented(
          "send text message"
        )
      }
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
          .list_messages(
            session_id,
            chat_id,
            limit,
            after,
          )
          .await
      }

      Backend::Native(_) => {
        native_not_implemented(
          "list persisted messages"
        )
      }
    }
  }

  /// Get recent chat history from the active WhatsApp backend.
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
          .get_chat_history(
            session_id,
            chat_id,
            limit,
            deep,
          )
          .await
      }

      Backend::Native(_) => {
        native_not_implemented(
          "get chat history"
        )
      }
    }
  }

  /// Open a realtime event listener for one WhatsApp session.
  pub async fn listen(
    &self,
    session_id: &str,
  ) -> Result<RealtimeListener, WppError> {
    match &self.backend {
      Backend::OpenWA(client) => {
        Ok(RealtimeListener {
          backend:
            RealtimeBackend::OpenWA(
              client.listen(
                session_id
              )
              .await?,
            ),
        })
      }

      Backend::Native(_) => {
        native_not_implemented(
          "open realtime listener"
        )
      }
    }
  }
}

/// Return a consistent error until a native backend operation is migrated.
fn native_not_implemented<T>(
  operation: &str,
) -> Result<T, WppError> {
  Err(WppError::Other(format!(
    "native backend does not implement {operation} yet"
  )))
}