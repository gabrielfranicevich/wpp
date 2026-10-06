use std::sync::Arc;
use std::time::Duration;

use tokio::sync::mpsc;
use whatsapp_rust::bot::BotHandle;
use whatsapp_rust::wacore::types::events::Subscription;
use whatsapp_rust_chat_store::ChatStore;

use crate::error::WppError;

use super::realtime::{self, NativeRealtimeListener};

pub struct NativeClient {
  pub(super) client: Arc<whatsapp_rust::Client>,
  pub(super) handle: BotHandle,
  pub(super) auth_receiver: mpsc::Receiver<super::auth::NativeAuthEvent>,
  pub(super) chat_store: Arc<ChatStore>,
  pub(super) chat_store_subscription: Subscription,
}

impl NativeClient {
  pub fn is_logged_in(&self) -> bool {
    self.client.is_logged_in()
  }

  pub async fn wait_for_connected(&self, timeout: Duration) -> Result<(), WppError> {
    self
      .client
      .wait_for_connected(timeout)
      .await
      .map_err(|error| WppError::Other(format!("native WhatsApp connection failed: {error}")))
  }

  pub fn phone(&self) -> Option<String> {
    let jid = self.client.pn()?;

    let raw = jid.to_string();

    let user = raw.split('@').next().unwrap_or("");

    let user = user.split(':').next().unwrap_or(user);

    let phone = user
      .chars()
      .filter(|c| c.is_ascii_digit())
      .collect::<String>();

    if phone.is_empty() {
      None
    } else {
      Some(phone)
    }
  }

  pub fn push_name(&self) -> String {
    self.client.push_name()
  }

  pub async fn logout(&self) {
    self.client.logout().await;
  }

  pub async fn listen(&self) -> Result<NativeRealtimeListener, WppError> {
    realtime::listen(&self.client).await
  }

  pub async fn shutdown(self) {
    let NativeClient {
      client,
      handle,
      auth_receiver,
      chat_store,
      chat_store_subscription,
    } = self;

    drop(chat_store_subscription);
    drop(chat_store);
    drop(auth_receiver);
    drop(client);

    handle.shutdown().await;
  }
}
