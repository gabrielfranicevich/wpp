use std::path::Path;

use crate::error::WppError;
use crate::whatsapp::models::{Chat, Message, MessagePage, RealtimeEvent};
use crate::whatsapp::native::{NativeAuthMode, NativeClient, NativeRealtimeListener};

/// Realtime listener exposed by the WhatsApp abstraction layer.
pub struct RealtimeListener {
  listener: NativeRealtimeListener,
}

impl RealtimeListener {
  pub fn try_recv(&mut self) -> Option<RealtimeEvent> {
    self.listener.try_recv()
  }

  pub async fn recv(&mut self) -> Option<RealtimeEvent> {
    self.listener.recv().await
  }

  pub async fn disconnect(self) -> Result<(), WppError> {
    self.listener.disconnect().await
  }
}

/// WhatsApp client used by the application layer.
pub struct WhatsAppClient {
  client: NativeClient,
}

impl WhatsAppClient {
  /// Create a client backed by the native Rust transport.
  pub fn native(client: NativeClient) -> Self {
    Self { client }
  }

  /// Open a persisted native WhatsApp session without
  /// requesting a new authentication flow.
  pub async fn open_native(storage_path: &Path) -> Result<Self, WppError> {
    let client = NativeClient::open(storage_path, NativeAuthMode::None).await?;

    Ok(Self::native(client))
  }

  /// Gracefully shut down the underlying client.
  pub async fn shutdown(self) {
    self.client.shutdown().await;
  }

  pub async fn logout(&self, _session_id: &str) -> Result<(), WppError> {
    self.client.logout().await;

    Ok(())
  }

  pub async fn get_chat_by_id(
    &self,
    _session_id: &str,
    chat_id: &str,
  ) -> Result<Option<Chat>, WppError> {
    self.client.get_chat(chat_id).await
  }

  pub async fn find_chats_by_phone(
    &self,
    _session_id: &str,
    phone: &str,
  ) -> Result<Vec<Chat>, WppError> {
    self.client.find_chats_by_phone(phone).await
  }

  pub async fn list_chats(
    &self,
    _session_id: &str,
    limit: usize,
    offset: usize,
  ) -> Result<Vec<Chat>, WppError> {
    self.client.list_chats(limit, offset).await
  }

  pub async fn delete_chat(&self, _session_id: &str, chat_id: &str) -> Result<(), WppError> {
    self.client.delete_chat(chat_id).await
  }

  pub async fn block_contact(&self, _session_id: &str, contact_id: &str) -> Result<(), WppError> {
    self.client.block_contact(contact_id).await
  }

  pub async fn unblock_contact(&self, _session_id: &str, contact_id: &str) -> Result<(), WppError> {
    self.client.unblock_contact(contact_id).await
  }

  pub async fn archive_chat(
    &self,
    _session_id: &str,
    chat_id: &str,
    archive: bool,
  ) -> Result<(), WppError> {
    self.client.archive_chat(chat_id, archive).await
  }

  pub async fn pin_chat(
    &self,
    _session_id: &str,
    chat_id: &str,
    pin: bool,
  ) -> Result<(), WppError> {
    self.client.pin_chat(chat_id, pin).await
  }

  pub async fn mute_chat(
    &self,
    _session_id: &str,
    chat_id: &str,
    mute_until: Option<i64>,
  ) -> Result<(), WppError> {
    self.client.mute_chat(chat_id, mute_until).await
  }

  pub async fn mark_chat_read(
    &self,
    _session_id: &str,
    chat_id: &str,
    message_ids: &[String],
  ) -> Result<(), WppError> {
    self.client.mark_chat_read(chat_id, message_ids).await
  }

  pub async fn mark_chat_unread(&self, _session_id: &str, chat_id: &str) -> Result<(), WppError> {
    self.client.mark_chat_unread(chat_id).await
  }

  pub async fn send_text(
    &self,
    _session_id: &str,
    chat_id: &str,
    text: &str,
  ) -> Result<(), WppError> {
    self.client.send_text(chat_id, text).await
  }

  pub async fn list_messages(
    &self,
    _session_id: &str,
    chat_id: &str,
    limit: usize,
    after: Option<&str>,
  ) -> Result<MessagePage, WppError> {
    self
      .client
      .list_persisted_messages(chat_id, limit, after)
      .await
  }

  /// Get recent chat history from WhatsApp.
  pub async fn get_chat_history(
    &self,
    _session_id: &str,
    chat_id: &str,
    limit: usize,
    deep: bool,
  ) -> Result<Vec<Message>, WppError> {
    self.client.get_chat_history(chat_id, limit, deep).await
  }

  /// Open a realtime event listener for the active WhatsApp session.
  pub async fn listen(&self, _session_id: &str) -> Result<RealtimeListener, WppError> {
    Ok(RealtimeListener {
      listener: self.client.listen().await?,
    })
  }
}
