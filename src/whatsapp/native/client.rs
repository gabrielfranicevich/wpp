use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::mpsc;
use whatsapp_rust::bot::{
  Bot,
  BotHandle,
};
use whatsapp_rust::pair_code::PairCodeOptions;
use whatsapp_rust::store::SqliteStore;
use whatsapp_rust::wacore::types::events::Subscription;
use whatsapp_rust::wacore_binary::JidExt;
use whatsapp_rust_chat_store::{
  ChatCursor,
  ChatEntry,
  ChatStore,
};

use crate::error::WppError;
use crate::whatsapp::models::Chat;

/// Authentication behavior requested when opening
/// a native WhatsApp session.
pub enum NativeAuthMode {
  /// Open an existing persisted session silently.
  None,

  /// Wait for a QR payload.
  Qr,

  /// Automatically request a phone pairing code.
  PairingCode(String),
}

/// Authentication event exposed to the CLI.
#[derive(Debug)]
pub enum NativeAuthEvent {
  /// Raw WhatsApp QR payload.
  QrCode(String),

  /// Eight-character phone pairing code.
  PairCode(String),

  /// Pairing-code request failed.
  PairCodeError(String),
}

const CHAT_PAGE_SIZE: i64 = 1000;

/// Native WhatsApp transport backed by `whatsapp-rust`.
///
/// This adapter owns both the whatsapp-rust client and the background
/// bot task that keeps the session alive.
pub struct NativeClient {
  client: Arc<whatsapp_rust::Client>,
  handle: BotHandle,
  auth_receiver: mpsc::Receiver<NativeAuthEvent>,
  chat_store: Arc<ChatStore>,
  chat_store_subscription: Subscription,
}

impl NativeClient {
  /// Open a persisted native WhatsApp session.
  ///
  /// The SQLite store contains whatsapp-rust authentication and protocol
  /// state. It is intentionally separate from wpp's own message cache.
  pub async fn open(
    storage_path: &Path,
    auth_mode: NativeAuthMode,
  ) -> Result<Self, WppError> {
    if let Some(parent) =
      storage_path.parent()
    {
      std::fs::create_dir_all(parent)
        .map_err(|error| {
          WppError::Other(
            format!(
              "failed to create native session directory: {error}"
            ),
          )
        })?;
    }

    let database_url =
      storage_path
        .to_string_lossy()
        .into_owned();

    let backend =
      SqliteStore::new(&database_url)
        .await
        .map_err(|error| {
          WppError::Other(
            format!(
              "failed to open native WhatsApp storage: {error}"
            ),
          )
        })?;

    let chat_store =
      ChatStore::new(&backend)
        .await
        .map_err(|error| {
          WppError::Other(
            format!(
              "failed to open native chat store: {error}"
            ),
          )
        })?;

    let (
      sender,
      receiver,
    ) = mpsc::channel(16);

    let mut builder =
      Bot::builder()
        .with_backend(backend);

    match auth_mode {
      NativeAuthMode::None => {}

      NativeAuthMode::Qr => {
        let sender =
          sender.clone();

        builder =
          builder.on_qr_code(
            move |code, _timeout| {
              let sender =
                sender.clone();

              async move {
                let _ = sender
                  .send(
                    NativeAuthEvent::QrCode(
                      code,
                    ),
                  )
                  .await;
              }
            },
          );
      }

      NativeAuthMode::PairingCode(
        phone,
      ) => {
        let phone =
          normalize_phone(&phone);

        if phone.is_empty() {
          return Err(
            WppError::Other(
              "phone number cannot be empty"
                .to_string(),
            ),
          );
        }

        let sender_for_code =
          sender.clone();

        let sender_for_error =
          sender.clone();

        builder =
          builder
            .on_pair_code(
              move |code, _timeout| {
                let sender =
                  sender_for_code
                    .clone();

                async move {
                  let _ = sender
                    .send(
                      NativeAuthEvent::PairCode(
                        code,
                      ),
                    )
                    .await;
                }
              },
            )
            .on_pair_code_error(
              move |error, _client| {
                let sender =
                  sender_for_error
                    .clone();

                async move {
                  let _ = sender
                    .send(
                      NativeAuthEvent::PairCodeError(
                        format!("{error:?}"),
                      ),
                    )
                    .await;
                }
              },
            )
            .with_pair_code(
              PairCodeOptions {
                phone_number: phone,
                show_push_notification: true,
                custom_code: None,
                ..Default::default()
              },
            );
      }
    }

    let bot =
      builder
        .build()
        .await
        .map_err(|error| {
          WppError::Other(
            format!(
              "failed to build native WhatsApp client: {error}"
            ),
          )
        })?;

    let handle =
      bot.spawn();

    let client =
      handle.client();

    let chat_store_subscription =
      client.subscribe_handler(
        chat_store.handler()
      );

    let client =
      handle.client();

    Ok(Self {
      client,
      handle,
      auth_receiver: receiver,
      chat_store,
      chat_store_subscription,
    })
  }

  /// Return whether whatsapp-rust considers this session authenticated.
  pub fn is_logged_in(&self) -> bool {
    self.client.is_logged_in()
  }

  /// Wait until the native session is connected and authenticated.
  pub async fn wait_for_connected(
    &self,
    timeout: Duration,
  ) -> Result<(), WppError> {
    self
      .client
      .wait_for_connected(timeout)
      .await
      .map_err(|error| {
        WppError::Other(
          format!(
            "native WhatsApp connection failed: {error}"
          ),
        )
      })
  }

  /// Receive the next authentication event without blocking the caller
  /// indefinitely beyond what the caller's own timeout allows.
  pub async fn next_auth_event(
    &mut self,
  ) -> Option<NativeAuthEvent> {
    self.auth_receiver.recv().await
  }

  /// Return the authenticated phone number, when available.
  pub fn phone(&self) -> Option<String> {
    let jid =
      self.client.pn()?;

    let raw =
      jid.to_string();

    let user =
      raw.split('@')
        .next()
        .unwrap_or("");

    let user =
      user.split(':')
        .next()
        .unwrap_or(user);

    let phone =
      user
        .chars()
        .filter(|c| c.is_ascii_digit())
        .collect::<String>();

    if phone.is_empty() {
      None
    } else {
      Some(phone)
    }
  }

  /// Return this device's WhatsApp push name.
  pub fn push_name(&self) -> String {
    self.client.push_name()
  }

  /// Log out the native WhatsApp device.
  pub async fn logout(&self) {
    self.client.logout().await;
  }

  /// List chats materialized by the native chat store.
  ///
  /// The application abstraction still exposes offset pagination, so this
  /// adapter translates it into the chat store's cursor-based pagination.
  pub async fn list_chats(
    &self,
    limit: usize,
    offset: usize,
  ) -> Result<Vec<Chat>, WppError> {
    if limit == 0 {
      return Ok(Vec::new());
    }

    let mut cursor =
      None;

    let mut skipped =
      0usize;

    let mut chats =
      Vec::with_capacity(limit);

    loop {
      let page =
        self
          .chat_store
          .chats_page(
            true,
            cursor,
            CHAT_PAGE_SIZE,
          )
          .await
          .map_err(|error| {
            WppError::Other(
              format!(
                "failed to list native chats: {error}"
              ),
            )
          })?;

      if page.is_empty() {
        break;
      }

      let page_len =
        page.len();

      let Some(last_chat) =
        page.last()
      else {
        break;
      };

      let next_cursor =
        ChatCursor::from(
          last_chat
        );

      let start =
        offset.saturating_sub(
          skipped
        );

      if start < page_len {
        for entry in
          page.into_iter().skip(start)
        {
          chats.push(
            chat_from_entry(entry)
          );

          if chats.len() >= limit {
            return Ok(chats);
          }
        }

        skipped +=
          page_len;

        cursor =
          Some(next_cursor);
      } else {
        skipped +=
          page_len;

        cursor =
          Some(next_cursor);
      }

      if page_len
        < CHAT_PAGE_SIZE as usize
      {
        break;
      }
    }

    Ok(chats)
  }

  /// Gracefully stop the background bot and flush native state.
  pub async fn shutdown(self) {
    let NativeClient {
      handle,
      chat_store_subscription,
      ..
    } = self;

    drop(
      chat_store_subscription
    );

    handle.shutdown().await;
  }
}

fn chat_from_entry(
  entry: ChatEntry,
) -> Chat {
  let jid =
    entry.jid.to_string();

  let name =
    entry
      .name
      .filter(|name| {
        !name.trim().is_empty()
      })
      .unwrap_or_else(
        || jid.clone()
      );

  let unread_count =
    if entry.unread_count < 0 {
      1
    } else {
      entry.unread_count
        .min(u32::MAX as i32)
    } as u32;

  Chat {
    id: jid.clone(),
    name,
    is_group: entry.jid.is_group(),
    unread_count,
    last_message:
      entry.last_message_preview,
    timestamp:
      entry
        .last_message_at
        .map(|timestamp| {
          timestamp.timestamp()
        })
        .unwrap_or(0),
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