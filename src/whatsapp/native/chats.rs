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

    match entry {
      Some(entry) => Ok(Some(self.chat_from_entry(entry).await?)),
      None => Ok(None),
    }
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

    match entry {
      Some(entry) => Ok(vec![self.chat_from_entry(entry).await?]),
      None => Ok(Vec::new()),
    }
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
          chats.push(self.chat_from_entry(entry).await?);

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

impl NativeClient {
  async fn chat_from_entry(&self, entry: ChatEntry) -> Result<Chat, WppError> {
    let jid = entry.jid.to_string();
    let is_group = entry.jid.is_group();
    let chat_name = entry.name.filter(|name| !name.trim().is_empty());
    let contact_name = if is_group {
      None
    } else {
      self
        .chat_store
        .contact(&entry.jid)
        .await
        .map_err(|error| WppError::Other(format!("failed to find native contact: {error}")))?
        .and_then(|contact| contact.display_name().map(str::to_owned))
        .filter(|name| !name.trim().is_empty())
    };

    let name = resolve_chat_name(&jid, is_group, chat_name, contact_name);

    Ok(Chat {
      id: jid.clone(),
      name,
      is_group,
      unread_count: map_unread_count(entry.unread_count),
      last_message: entry.last_message_preview,
      timestamp: entry
        .last_message_at
        .map(|timestamp| timestamp.timestamp())
        .unwrap_or(0),
    })
  }
}

fn resolve_chat_name(
  jid: &str,
  is_group: bool,
  chat_name: Option<String>,
  contact_name: Option<String>,
) -> String {
  let name = if is_group {
    chat_name
  } else {
    contact_name.or(chat_name)
  };

  name.unwrap_or_else(|| jid.to_owned())
}

fn map_unread_count(count: i32) -> u32 {
  if count < 0 {
    1
  } else {
    count as u32
  }
}

pub(super) fn normalize_phone(value: &str) -> String {
  value.chars().filter(|c| c.is_ascii_digit()).collect()
}

#[cfg(test)]
mod tests {
  use super::{map_unread_count, resolve_chat_name};

  #[test]
  fn direct_chat_prefers_contact_name() {
    assert_eq!(
      resolve_chat_name(
        "123@s.whatsapp.net",
        false,
        Some("Chat row".to_owned()),
        Some("Contact".to_owned()),
      ),
      "Contact"
    );
  }

  #[test]
  fn group_chat_uses_chat_name() {
    assert_eq!(
      resolve_chat_name(
        "123@g.us",
        true,
        Some("Group".to_owned()),
        Some("Contact".to_owned()),
      ),
      "Group"
    );
  }

  #[test]
  fn chat_name_falls_back_to_jid() {
    assert_eq!(resolve_chat_name("123@lid", false, None, None), "123@lid");
  }

  #[test]
  fn unread_count_maps_negative_sentinel_without_overflow() {
    assert_eq!(map_unread_count(-1), 1);
    assert_eq!(map_unread_count(-12), 1);
    assert_eq!(map_unread_count(0), 0);
    assert_eq!(map_unread_count(42), 42);
  }
}
