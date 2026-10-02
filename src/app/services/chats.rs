use crate::error::WppError;
use crate::whatsapp::client::WhatsAppClient;
use crate::whatsapp::models::Chat;

const PAGE_SIZE: usize = 1000;

/// Filter that can be applied to the selected chat list.
#[derive(Debug, Clone, Copy)]
pub enum ChatFilter {
  Unread,
}

/// Defines how chats should be selected before applying an optional filter.
#[derive(Debug, Clone, Copy)]
pub enum ChatListMode {
  /// Fetch at most N chats.
  Limit(usize),

  /// Fetch every available chat.
  All,

  /// Fetch chats until N unread chats have been found.
  ///
  /// `None` means fetch every unread chat.
  Unread { limit: Option<usize> },
}

/// Application service for the chat list and chat-level operations.
///
/// It owns pagination and user-facing filtering so the CLI does not know
/// anything about OpenWA's REST API or pagination semantics.
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

  /// Return chats according to the requested listing mode and optional
  /// post-selection filter.
  ///
  /// `Limit(n)` limits the input set before filtering:
  ///
  ///   fetch n -> filter -> return
  ///
  /// `Unread { limit }` limits the final result:
  ///
  ///   fetch page -> filter unread -> accumulate -> next page
  ///
  /// `All` fetches every available chat before applying the optional filter.
  pub async fn list(
    &self,
    mode: ChatListMode,
    filter: Option<ChatFilter>,
  ) -> Result<Vec<Chat>, WppError> {
    match mode {
      ChatListMode::Limit(limit) => self.list_limited(limit, filter).await,

      ChatListMode::All => self.list_all(filter).await,

      ChatListMode::Unread { limit } => self.list_unread(limit).await,
    }
  }

  /// Delete a chat from WhatsApp.
  pub async fn delete_chat(&self, chat_id: &str) -> Result<(), WppError> {
    self.whatsapp.delete_chat(self.session_id, chat_id).await
  }

  /// Search chats by partial name or phone number.
  ///
  /// Name matching is case-insensitive.
  ///
  /// Phone matching ignores formatting characters and searches inside
  /// the numeric part of the chat id, allowing both complete and partial
  /// phone numbers.
  pub async fn search(&self, query: &str) -> Result<Vec<Chat>, WppError> {
    let query = query.trim();

    if query.is_empty() {
      return Err(WppError::Other(
        "search query cannot be empty".to_string(),
      ));
    }

    let query_lower = query.to_lowercase();
    let normalized_phone = normalize_phone(query);

    let mut chats = self.list_all(None).await?;

    chats.retain(|chat| {
      matches_search_query(chat, &query_lower, &normalized_phone)
    });

    Ok(chats)
  }

  async fn list_limited(
    &self,
    limit: usize,
    filter: Option<ChatFilter>,
  ) -> Result<Vec<Chat>, WppError> {
    if limit == 0 {
      return Ok(Vec::new());
    }

    let fetch_limit = limit.min(PAGE_SIZE);

    let mut chats = self
      .whatsapp
      .list_chats(self.session_id, fetch_limit, 0)
      .await?;

    chats.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));

    Self::apply_filter(&mut chats, filter);

    Ok(chats)
  }

  async fn list_all(&self, filter: Option<ChatFilter>) -> Result<Vec<Chat>, WppError> {
    let mut chats = Vec::new();

    let mut offset = 0usize;

    loop {
      let page = self
        .whatsapp
        .list_chats(self.session_id, PAGE_SIZE, offset)
        .await?;

      let page_len = page.len();

      chats.extend(page);

      if page_len < PAGE_SIZE {
        break;
      }

      offset += page_len;
    }

    chats.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));

    Self::apply_filter(&mut chats, filter);

    Ok(chats)
  }

  async fn list_unread(&self, limit: Option<usize>) -> Result<Vec<Chat>, WppError> {
    if matches!(limit, Some(0)) {
      return Ok(Vec::new());
    }

    let mut unread_chats = Vec::new();

    let mut offset = 0usize;

    loop {
      let page = self
        .whatsapp
        .list_chats(self.session_id, PAGE_SIZE, offset)
        .await?;

      let page_len = page.len();

      for chat in page {
        if chat.unread_count > 0 {
          unread_chats.push(chat);

          if let Some(limit) = limit {
            if unread_chats.len() >= limit {
              unread_chats.truncate(limit);

              unread_chats.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));

              return Ok(unread_chats);
            }
          }
        }
      }

      if page_len < PAGE_SIZE {
        break;
      }

      offset += page_len;
    }

    unread_chats.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));

    Ok(unread_chats)
  }

  fn apply_filter(chats: &mut Vec<Chat>, filter: Option<ChatFilter>) {
    match filter {
      Some(ChatFilter::Unread) => {
        chats.retain(|chat| chat.unread_count > 0);
      }

      None => {}
    }
  }
}

fn matches_search_query(
  chat: &Chat,
  query_lower: &str,
  normalized_phone: &str,
) -> bool {
  let name = chat.name.to_lowercase();

  if name.contains(query_lower) {
    return true;
  }

  !normalized_phone.is_empty()
    && normalize_phone(&chat.id).contains(normalized_phone)
}

fn normalize_phone(value: &str) -> String {
  value.chars().filter(|c| c.is_ascii_digit()).collect()
}

#[cfg(test)]
mod tests {
  use super::{matches_search_query, normalize_phone};
  use crate::whatsapp::models::Chat;

  fn chat(name: &str, id: &str, is_group: bool) -> Chat {
    Chat {
      id: id.to_string(),
      name: name.to_string(),
      is_group,
      unread_count: 0,
      last_message: None,
      timestamp: 0,
      kind: String::new(),
      archived: false,
      pinned: false,
      muted: false,
    }
  }

  #[test]
  fn normalize_phone_removes_formatting() {
    assert_eq!(normalize_phone("+54 9 351 123-4567"), "5493511234567");
  }

  #[test]
  fn search_matches_name_case_insensitively() {
    let chat = chat("Gabriel Franicevich", "5493511234567@c.us", false);

    assert!(matches_search_query(
      &chat,
      "gabriel",
      "",
    ));
  }

  #[test]
  fn search_matches_partial_phone() {
    let chat = chat("Gabriel", "5493511234567@c.us", false);

    assert!(matches_search_query(
      &chat,
      "351",
      "351",
    ));
  }

  #[test]
  fn search_accepts_formatted_phone_queries() {
    let chat = chat("Gabriel", "5493511234567@c.us", false);

    assert!(matches_search_query(
      &chat,
      "+54 9 351",
      "549351",
    ));
  }

  #[test]
  fn search_does_not_match_unrelated_chat() {
    let chat = chat("Martina", "5492619876543@c.us", false);

    assert!(!matches_search_query(
      &chat,
      "gabriel",
      "351",
    ));
  }

  #[test]
  fn search_can_match_group_names() {
    let chat = chat("Universidad", "120363000000000000@g.us", true);

    assert!(matches_search_query(
      &chat,
      "universidad",
      "",
    ));
  }
}