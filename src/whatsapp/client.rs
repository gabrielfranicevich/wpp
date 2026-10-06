use std::path::Path;

use crate::error::WppError;
use crate::whatsapp::models::{
  Chat,
  Message,
  MessagePage,
  RealtimeEvent,
  Session,
};
use crate::whatsapp::native::{
  NativeAuthMode,
  NativeClient,
};
use crate::whatsapp::openwa::client::{
  OpenWAClient,
  OpenWARealtimeListener,
};

const CHAT_LOOKUP_PAGE_SIZE: usize = 1000;

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
      backend:
        Backend::OpenWA(
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
      backend:
        Backend::Native(
          client
        ),
    }
  }

  /// Open a persisted native WhatsApp session without
  /// requesting a new authentication flow.
  pub async fn open_native(
    storage_path: &Path,
  ) -> Result<Self, WppError> {
    let client =
      NativeClient::open(
        storage_path,
        NativeAuthMode::None,
      )
      .await?;

    Ok(
      Self::native(client)
    )
  }

  /// Gracefully shut down the underlying backend.
  pub async fn shutdown(
    self,
  ) {
    match self.backend {
      Backend::OpenWA(_) => {}

      Backend::Native(
        client
      ) => {
        client
          .shutdown()
          .await;
      }
    }
  }

  pub async fn list_sessions(
    &self,
  ) -> Result<Vec<Session>, WppError> {
    match &self.backend {
      Backend::OpenWA(
        client
      ) => {
        client
          .list_sessions()
          .await
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
      Backend::OpenWA(
        client
      ) => {
        client
          .delete_session(
            session_id
          )
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
      Backend::OpenWA(
        client
      ) => {
        client
          .logout(
            session_id
          )
          .await
      }

      Backend::Native(
        client
      ) => {
        client
          .logout()
          .await;

        let push_name =
          client.push_name();

        Ok(Session {
          id:
            session_id.to_string(),

          name:
            if push_name
              .trim()
              .is_empty()
            {
              session_id
                .to_string()
            } else {
              push_name.clone()
            },

          status:
            "logged_out"
              .to_string(),

          phone:
            client.phone(),

          push_name:
            if push_name
              .trim()
              .is_empty()
            {
              None
            } else {
              Some(push_name)
            },
        })
      }
    }
  }

  pub async fn get_chat_by_id(
    &self,
    session_id: &str,
    chat_id: &str,
  ) -> Result<
    Option<Chat>,
    WppError,
  > {
    match &self.backend {
      Backend::OpenWA(
        client
      ) => {
        if !chat_id
          .contains('@')
        {
          return Ok(None);
        }

        let mut offset =
          0usize;

        loop {
          let chats =
            client
              .list_chats(
                session_id,
                CHAT_LOOKUP_PAGE_SIZE,
                offset,
              )
              .await?;

          if chats.is_empty() {
            return Ok(None);
          }

          if let Some(chat) =
            chats
              .into_iter()
              .find(|chat| {
                chat.id == chat_id
              })
          {
            return Ok(Some(chat));
          }

          let page_len =
            CHAT_LOOKUP_PAGE_SIZE;

          offset +=
            page_len;
        }
      }

      Backend::Native(
        client
      ) => {
        client
          .get_chat(chat_id)
          .await
      }
    }
  }

  pub async fn find_chats_by_phone(
    &self,
    session_id: &str,
    phone: &str,
  ) -> Result<
    Vec<Chat>,
    WppError,
  > {
    match &self.backend {
      Backend::OpenWA(
        client
      ) => {
        let mut matches =
          Vec::new();

        let mut offset =
          0usize;

        loop {
          let chats =
            client
              .list_chats(
                session_id,
                CHAT_LOOKUP_PAGE_SIZE,
                offset,
              )
              .await?;

          let page_len =
            chats.len();

          for chat in chats {
            if normalize_phone(
              &chat.id
            ) == normalize_phone(
              phone
            ) {
              matches.push(
                chat
              );
            }
          }

          if page_len
            < CHAT_LOOKUP_PAGE_SIZE
          {
            break;
          }

          offset +=
            page_len;
        }

        Ok(matches)
      }

      Backend::Native(
        client
      ) => {
        client
          .find_chats_by_phone(
            phone
          )
          .await
      }
    }
  }

  pub async fn list_chats(
    &self,
    session_id: &str,
    limit: usize,
    offset: usize,
  ) -> Result<
    Vec<Chat>,
    WppError,
  > {
    match &self.backend {
      Backend::OpenWA(
        client
      ) => {
        client
          .list_chats(
            session_id,
            limit,
            offset,
          )
          .await
      }

      Backend::Native(
        client
      ) => {
        client
          .list_chats(
            limit,
            offset,
          )
          .await
      }
    }
  }

  pub async fn delete_chat(
    &self,
    session_id: &str,
    chat_id: &str,
  ) -> Result<(), WppError> {
    match &self.backend {
      Backend::OpenWA(
        client
      ) => {
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
      Backend::OpenWA(
        client
      ) => {
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
      Backend::OpenWA(
        client
      ) => {
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
      Backend::OpenWA(
        client
      ) => {
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
      Backend::OpenWA(
        client
      ) => {
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
      Backend::OpenWA(
        client
      ) => {
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
      Backend::OpenWA(
        client
      ) => {
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
      Backend::OpenWA(
        client
      ) => {
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
      Backend::OpenWA(
        client
      ) => {
        client
          .send_text(
            session_id,
            chat_id,
            text,
          )
          .await
      }

      Backend::Native(
        client
      ) => {
        client
          .send_text(
            chat_id,
            text,
          )
          .await
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
      Backend::OpenWA(
        client
      ) => {
        client
          .list_messages(
            session_id,
            chat_id,
            limit,
            after,
          )
          .await
      }

      Backend::Native(
        client
      ) => {
        client
          .list_persisted_messages(
            chat_id,
            limit,
            after,
          )
          .await
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
  ) -> Result<
    Vec<Message>,
    WppError,
  > {
    match &self.backend {
      Backend::OpenWA(
        client
      ) => {
        client
          .get_chat_history(
            session_id,
            chat_id,
            limit,
            deep,
          )
          .await
      }

      Backend::Native(
        client
      ) => {
        client
          .get_chat_history(
            chat_id,
            limit,
            deep,
          )
          .await
      }
    }
  }

  /// Open a realtime event listener for one WhatsApp session.
  pub async fn listen(
    &self,
    session_id: &str,
  ) -> Result<
    RealtimeListener,
    WppError,
  > {
    match &self.backend {
      Backend::OpenWA(
        client
      ) => {
        Ok(
          RealtimeListener {
            backend:
              RealtimeBackend::OpenWA(
                client
                  .listen(
                    session_id
                  )
                  .await?,
              ),
          }
        )
      }

      Backend::Native(_) => {
        native_not_implemented(
          "open realtime listener"
        )
      }
    }
  }
}

fn normalize_phone(
  value: &str,
) -> String {
  value
    .chars()
    .filter(|c| c.is_ascii_digit())
    .collect()
}

/// Return a consistent error until a native backend operation is migrated.
fn native_not_implemented<T>(
  operation: &str,
) -> Result<T, WppError> {
  Err(
    WppError::Other(
      format!(
        "native backend does not implement {operation} yet"
      )
    )
  )
}