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

/// Native WhatsApp transport backed by `whatsapp-rust`.
///
/// This adapter owns both the whatsapp-rust client and the background
/// bot task that keeps the session alive.
pub struct NativeClient {
  client: Arc<whatsapp_rust::Client>,
  handle: BotHandle,
  auth_receiver: mpsc::Receiver<NativeAuthEvent>,
}

impl NativeClient {
  /// Open a persisted native WhatsApp session.
  ///
  /// The SQLite store contains whatsapp-rust authentication and protocol
  /// state. It is intentionally separate from wpp's own message cache.
  pub async fn open(
    storage_path: &Path,
    auth_mode: NativeAuthMode,
  ) -> Result<Self, crate::error::WppError> {
    if let Some(parent) =
      storage_path.parent()
    {
      std::fs::create_dir_all(parent)
        .map_err(|error| {
          crate::error::WppError::Other(
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
          crate::error::WppError::Other(
            format!(
              "failed to open native WhatsApp storage: {error}"
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
            crate::error::WppError::Other(
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
          crate::error::WppError::Other(
            format!(
              "failed to build native WhatsApp client: {error}"
            ),
          )
        })?;

    let handle =
      bot.spawn();

    let client =
      handle.client();

    Ok(Self {
      client,
      handle,
      auth_receiver: receiver,
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
  ) -> Result<(), crate::error::WppError> {
    self
      .client
      .wait_for_connected(timeout)
      .await
      .map_err(|error| {
        crate::error::WppError::Other(
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

  /// Gracefully stop the background bot and flush native state.
  pub async fn shutdown(self) {
    self.handle.shutdown().await;
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