use whatsapp_rust::wacore_binary::JidExt;
use whatsapp_rust_chat_store::{ChatCursor, ChatEntry};

use crate::error::WppError;
use crate::whatsapp::models::Chat;

use super::client::NativeClient;

const CHAT_PAGE_SIZE: i64 = 1000;

impl NativeClient {
  pub async fn get_chat(&self, chat_id: &str) -> Result<Option<Chat>, WppError> {
    let Ok(jid) = chat_id.parse::<whatsapp_rust::Jid>() else {
      return Ok(None);
    };

    let entry = self
      .chat_store
      .chat(&jid)
      .await
      .map_err(|error| WppError::Other(format!("failed to find native chat: {error}")))?;

    Ok(entry.map(chat_from_entry))
  }

  pub async fn find_chats_by_phone(&self, phone: &str) -> Result<Vec<Chat>, WppError> {
    let normalized = normalize_phone(phone);

    if normalized.is_empty() {
      return Ok(Vec::new());
    }

    let jid = whatsapp_rust::Jid::pn(normalized);

    let entry =
      self.chat_store.chat(&jid).await.map_err(|error| {
        WppError::Other(format!("failed to find native chat by phone: {error}"))
      })?;

    Ok(entry.into_iter().map(chat_from_entry).collect())
  }

  pub async fn delete_chat(&self, chat_id: &str) -> Result<(), WppError> {
    let jid = chat_id
      .parse::<whatsapp_rust::Jid>()
      .map_err(|error| WppError::Other(format!("invalid native chat id `{chat_id}`: {error}")))?;

    self
      .client
      .chat_actions()
      .delete_chat(&jid, true, None)
      .await
      .map_err(|error| WppError::Other(format!("failed to delete native chat: {error}")))
  }

  pub async fn block_contact(&self, contact_id: &str) -> Result<(), WppError> {
    let jid = contact_id.parse::<whatsapp_rust::Jid>().map_err(|error| {
      WppError::Other(format!("invalid native contact id `{contact_id}`: {error}"))
    })?;

    self
      .client
      .blocking()
      .block(&jid)
      .await
      .map_err(|error| WppError::Other(format!("failed to block native contact: {error}")))
  }

  pub async fn unblock_contact(&self, contact_id: &str) -> Result<(), WppError> {
    let jid = contact_id.parse::<whatsapp_rust::Jid>().map_err(|error| {
      WppError::Other(format!("invalid native contact id `{contact_id}`: {error}"))
    })?;

    self
      .client
      .blocking()
      .unblock(&jid)
      .await
      .map_err(|error| WppError::Other(format!("failed to unblock native contact: {error}")))
  }

  pub async fn archive_chat(&self, chat_id: &str, archive: bool) -> Result<(), WppError> {
    let jid = chat_id
      .parse::<whatsapp_rust::Jid>()
      .map_err(|error| WppError::Other(format!("invalid native chat id `{chat_id}`: {error}")))?;

    let result = if archive {
      self.client.chat_actions().archive_chat(&jid, None).await
    } else {
      self.client.chat_actions().unarchive_chat(&jid, None).await
    };

    result.map_err(|error| {
      WppError::Other(format!(
        "failed to {}native chat: {error}",
        if archive { "archive " } else { "unarchive " }
      ))
    })
  }

  pub async fn pin_chat(&self, chat_id: &str, pin: bool) -> Result<(), WppError> {
    let jid = chat_id
      .parse::<whatsapp_rust::Jid>()
      .map_err(|error| WppError::Other(format!("invalid native chat id `{chat_id}`: {error}")))?;

    let result = if pin {
      self.client.chat_actions().pin_chat(&jid).await
    } else {
      self.client.chat_actions().unpin_chat(&jid).await
    };

    result.map_err(|error| {
      WppError::Other(format!(
        "failed to {}native chat: {error}",
        if pin { "pin " } else { "unpin " }
      ))
    })
  }

  pub async fn mute_chat(&self, chat_id: &str, mute_until: Option<i64>) -> Result<(), WppError> {
    let jid = chat_id
      .parse::<whatsapp_rust::Jid>()
      .map_err(|error| WppError::Other(format!("invalid native chat id `{chat_id}`: {error}")))?;

    let result = match mute_until {
      Some(0) => self.client.chat_actions().mute_chat(&jid).await,

      Some(mute_end_timestamp_ms) => {
        self
          .client
          .chat_actions()
          .mute_chat_until(&jid, mute_end_timestamp_ms)
          .await
      }

      None => self.client.chat_actions().unmute_chat(&jid).await,
    };

    result.map_err(|error| {
      WppError::Other(format!(
        "failed to {}native chat: {error}",
        match mute_until {
          Some(_) => "mute ",
          None => "unmute ",
        }
      ))
    })
  }

  pub async fn mark_chat_read(
    &self,
    chat_id: &str,
    _message_ids: &[String],
  ) -> Result<(), WppError> {
    let jid = chat_id
      .parse::<whatsapp_rust::Jid>()
      .map_err(|error| WppError::Other(format!("invalid native chat id `{chat_id}`: {error}")))?;

    self
      .client
      .chat_actions()
      .mark_chat_as_read(&jid, true, None)
      .await
      .map_err(|error| WppError::Other(format!("failed to mark native chat as read: {error}")))
  }

  pub async fn mark_chat_unread(&self, chat_id: &str) -> Result<(), WppError> {
    let jid = chat_id
      .parse::<whatsapp_rust::Jid>()
      .map_err(|error| WppError::Other(format!("invalid native chat id `{chat_id}`: {error}")))?;

    self
      .client
      .chat_actions()
      .mark_chat_as_read(&jid, false, None)
      .await
      .map_err(|error| WppError::Other(format!("failed to mark native chat as unread: {error}")))
  }

  pub async fn list_chats(&self, limit: usize, offset: usize) -> Result<Vec<Chat>, WppError> {
    if limit == 0 {
      return Ok(Vec::new());
    }

    let mut cursor = None;

    let mut skipped = 0usize;

    let mut chats = Vec::with_capacity(limit);

    loop {
      let page = self
        .chat_store
        .chats_page(true, cursor, CHAT_PAGE_SIZE)
        .await
        .map_err(|error| WppError::Other(format!("failed to list native chats: {error}")))?;

      if page.is_empty() {
        break;
      }

      let page_len = page.len();

      let Some(last_chat) = page.last() else {
        break;
      };

      let next_cursor = ChatCursor::from(last_chat);

      let start = offset.saturating_sub(skipped);

      if start < page_len {
        for entry in page.into_iter().skip(start) {
          chats.push(chat_from_entry(entry));

          if chats.len() >= limit {
            return Ok(chats);
          }
        }

        skipped += page_len;

        cursor = Some(next_cursor);
      } else {
        skipped += page_len;

        cursor = Some(next_cursor);
      }

      if page_len < CHAT_PAGE_SIZE as usize {
        break;
      }
    }

    Ok(chats)
  }
}

fn chat_from_entry(entry: ChatEntry) -> Chat {
  let jid = entry.jid.to_string();

  let name = entry
    .name
    .filter(|name| !name.trim().is_empty())
    .unwrap_or_else(|| jid.clone());

  let unread_count = if entry.unread_count < 0 {
    1
  } else {
    entry.unread_count.min(u32::MAX as i32)
  } as u32;

  Chat {
    id: jid.clone(),
    name,
    is_group: entry.jid.is_group(),
    unread_count,
    last_message: entry.last_message_preview,
    timestamp: entry
      .last_message_at
      .map(|timestamp| timestamp.timestamp())
      .unwrap_or(0),
  }
}

pub(super) fn normalize_phone(value: &str) -> String {
  value.chars().filter(|c| c.is_ascii_digit()).collect()
}
