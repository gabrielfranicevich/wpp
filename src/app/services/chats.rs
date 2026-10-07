use crate::error::WppError;
use crate::whatsapp::client::WhatsAppClient;
use crate::whatsapp::models::{Chat, MessageDirection};

const MARK_READ_HISTORY_LIMIT: usize = 2000;
const MARK_READ_LOOKBACK_EXTRA: usize = 50;
const MARK_READ_BATCH_SIZE: usize = 100;

/// Application service for chat-level operations.
pub struct ChatsService<'a> {
  whatsapp: &'a WhatsAppClient,
  session_id: &'a str,
}

impl<'a> ChatsService<'a> {
  pub fn new(whatsapp: &'a WhatsAppClient, session_id: &'a str) -> Self {
    Self {
      whatsapp,
      session_id,
    }
  }

  /// Delete a chat from WhatsApp.
  pub async fn delete_chat(&self, chat_id: &str) -> Result<(), WppError> {
    self.whatsapp.delete_chat(self.session_id, chat_id).await
  }

  /// Block a contact represented by a direct chat.
  pub async fn block_chat(&self, chat: &Chat) -> Result<(), WppError> {
    ensure_contact_chat(chat)?;

    self.whatsapp.block_contact(self.session_id, &chat.id).await
  }

  /// Unblock a contact represented by a direct chat.
  pub async fn unblock_chat(&self, chat: &Chat) -> Result<(), WppError> {
    ensure_contact_chat(chat)?;

    self
      .whatsapp
      .unblock_contact(self.session_id, &chat.id)
      .await
  }

  /// Archive or unarchive a chat.
  pub async fn archive_chat(&self, chat: &Chat, archive: bool) -> Result<(), WppError> {
    self
      .whatsapp
      .archive_chat(self.session_id, &chat.id, archive)
      .await
  }

  /// Pin or unpin a chat.
  pub async fn pin_chat(&self, chat: &Chat, pin: bool) -> Result<(), WppError> {
    self.whatsapp.pin_chat(self.session_id, &chat.id, pin).await
  }

  /// Mute or unmute a chat.
  pub async fn mute_chat(&self, chat: &Chat, mute: bool) -> Result<(), WppError> {
    let mute_until = if mute { Some(0) } else { None };

    self
      .whatsapp
      .mute_chat(self.session_id, &chat.id, mute_until)
      .await
  }

  /// Mark a chat as read.
  ///
  /// The unread count is a chat-level value, so individual unread message
  /// ids are recovered from a recent history window before marking them read.
  pub async fn mark_chat_read(&self, chat: &Chat) -> Result<(), WppError> {
    let unread_count = chat.unread_count as usize;

    if unread_count == 0 {
      return Ok(());
    }

    let requested_limit = unread_count
      .saturating_add(MARK_READ_LOOKBACK_EXTRA)
      .min(MARK_READ_HISTORY_LIMIT);

    let history = self
      .whatsapp
      .get_chat_history(self.session_id, &chat.id, requested_limit, true)
      .await?;

    let mut message_ids = Vec::with_capacity(unread_count);

    for message in history.iter().rev() {
      if message.direction != MessageDirection::Incoming {
        continue;
      }

      message_ids.push(message.id.clone());

      if message_ids.len() >= unread_count {
        break;
      }
    }

    message_ids.reverse();

    if message_ids.is_empty() {
      return self
        .whatsapp
        .mark_chat_read(self.session_id, &chat.id, &[])
        .await;
    }

    for chunk in message_ids.chunks(MARK_READ_BATCH_SIZE) {
      self
        .whatsapp
        .mark_chat_read(self.session_id, &chat.id, chunk)
        .await?;
    }

    Ok(())
  }

  /// Mark a chat as unread.
  pub async fn mark_chat_unread(&self, chat: &Chat) -> Result<(), WppError> {
    self
      .whatsapp
      .mark_chat_unread(self.session_id, &chat.id)
      .await
  }
}

fn ensure_contact_chat(chat: &Chat) -> Result<(), WppError> {
  if chat.is_group {
    return Err(WppError::Other(format!(
      "cannot block or unblock group chat `{}`",
      chat.name
    )));
  }

  Ok(())
}
