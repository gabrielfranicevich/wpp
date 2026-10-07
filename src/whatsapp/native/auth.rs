use std::path::Path;
use tokio::sync::mpsc;
use whatsapp_rust::bot::Bot;
use whatsapp_rust::pair_code::PairCodeOptions;
use whatsapp_rust::store::SqliteStore;
use whatsapp_rust_chat_store::ChatStore;
use crate::error::WppError;

use super::chats;
use super::client::NativeClient;

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

impl NativeClient {
  /// Open a persisted native WhatsApp session.
  ///
  /// The SQLite store contains whatsapp-rust authentication and protocol
  /// state. It is intentionally separate from wpp's own message cache.
  pub async fn open(storage_path: &Path, auth_mode: NativeAuthMode) -> Result<Self, WppError> {
    if let Some(parent) = storage_path.parent() {
      std::fs::create_dir_all(parent).map_err(|error| {
        WppError::Other(format!(
          "failed to create native session directory: {error}"
        ))
      })?;
    }

    let database_url = storage_path.to_string_lossy().into_owned();

    let backend = SqliteStore::new(&database_url).await.map_err(|error| {
      WppError::Other(format!("failed to open native WhatsApp storage: {error}"))
    })?;

    let chat_store = ChatStore::new(&backend)
      .await
      .map_err(|error| WppError::Other(format!("failed to open native chat store: {error}")))?;

    let (sender, receiver) = mpsc::channel(16);

    let mut builder = Bot::builder().with_backend(backend);

    match auth_mode {
      NativeAuthMode::None => {}

      NativeAuthMode::Qr => {
        let sender = sender.clone();

        builder = builder.on_qr_code(move |code, _timeout| {
          let sender = sender.clone();

          async move {
            let _ = sender.send(NativeAuthEvent::QrCode(code)).await;
          }
        });
      }

      NativeAuthMode::PairingCode(phone) => {
        let phone = chats::normalize_phone(&phone);

        if phone.is_empty() {
          return Err(WppError::Other("phone number cannot be empty".to_string()));
        }

        let sender_for_code = sender.clone();

        let sender_for_error = sender.clone();

        builder = builder
          .on_pair_code(move |code, _timeout| {
            let sender = sender_for_code.clone();

            async move {
              let _ = sender.send(NativeAuthEvent::PairCode(code)).await;
            }
          })
          .on_pair_code_error(move |error, _client| {
            let sender = sender_for_error.clone();

            async move {
              let _ = sender
                .send(NativeAuthEvent::PairCodeError(format!("{error:?}")))
                .await;
            }
          })
          .with_pair_code(PairCodeOptions {
            phone_number: phone,
            show_push_notification: true,
            custom_code: None,
            ..Default::default()
          });
      }
    }

    let bot = builder.build().await.map_err(|error| {
      WppError::Other(format!("failed to build native WhatsApp client: {error}"))
    })?;

    let handle = bot.spawn();

    let client = handle.client();

    let chat_store_subscription = client.subscribe_handler(chat_store.handler());

    let client = handle.client();

    Ok(Self {
      client,
      handle,
      auth_receiver: receiver,
      chat_store,
      chat_store_subscription,
    })
  }

  pub async fn next_auth_event(&mut self) -> Option<NativeAuthEvent> {
    self.auth_receiver.recv().await
  }
}
