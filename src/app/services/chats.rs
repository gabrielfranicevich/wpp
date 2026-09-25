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
  Unread {
    limit: Option<usize>,
  },
}

/// Application service for the chat list.
///
/// It owns pagination and user-facing filtering so the CLI does not know
/// anything about OpenWA's REST API or pagination semantics.
pub struct ChatsService<'a> {
  whatsapp: &'a WhatsAppClient,
  session_id: &'a str,
}

impl<'a> ChatsService<'a> {
  pub fn new(
    whatsapp: &'a WhatsAppClient,
    session_id: &'a str,
  ) -> Self {
    Self {
      whatsapp,
      session_id,
    }
  }

  /// Resolve a chat by exact id, phone number, exact name,
  /// or a unique partial name/phone match.
  pub async fn resolve(
    &self,
    query: &str,
  ) -> Result<Chat, WppError> {
    let query = query.trim();

    if query.is_empty() {
      return Err(WppError::Other(
        "chat name or phone number cannot be empty"
          .to_string(),
      ));
    }

    let chats = self.list_all(None).await?;

    // 1. Exact chat id.
    if let Some(chat) = chats
      .iter()
      .find(|chat| chat.id == query)
    {
      return Ok((*chat).clone());
    }

    // 2. Exact phone number.
    let normalized_query = normalize_phone(query);

    if !normalized_query.is_empty() {
      let phone_matches: Vec<&Chat> = chats
        .iter()
        .filter(|chat| {
          normalize_phone(&chat.id)
            == normalized_query
        })
        .collect();

      match phone_matches.as_slice() {
        [chat] => {
          return Ok((*chat).clone());
        }

        [] => {}

        _ => {
          return Err(
            ambiguous_chat_error(
              query,
              &phone_matches,
            )
          );
        }
      }
    }

    // 3. Exact name, case-insensitive.
    let query_lower = query.to_lowercase();

    let name_matches: Vec<&Chat> = chats
      .iter()
      .filter(|chat| {
        chat.name.to_lowercase()
          == query_lower
      })
      .collect();

    match name_matches.as_slice() {
      [chat] => {
        return Ok((*chat).clone());
      }

      [] => {}

      _ => {
        return Err(
          ambiguous_chat_error(
            query,
            &name_matches,
          )
        );
      }
    }

    // 4. Unique partial name or phone match.
    let partial_matches: Vec<&Chat> = chats
      .iter()
      .filter(|chat| {
        let name_match = chat
          .name
          .to_lowercase()
          .contains(&query_lower);

        let phone_match =
          !normalized_query.is_empty()
            && normalize_phone(&chat.id)
              .contains(&normalized_query);

        name_match || phone_match
      })
      .collect();

    match partial_matches.as_slice() {
      [chat] => Ok((*chat).clone()),

      [] => Err(WppError::Other(
        format!("chat `{query}` not found"),
      )),

      _ => Err(
        ambiguous_chat_error(
          query,
          &partial_matches,
        )
      ),
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
      ChatListMode::Limit(limit) => {
        self.list_limited(
          limit,
          filter,
        )
        .await
      }

      ChatListMode::All => {
        self.list_all(filter).await
      }

      ChatListMode::Unread { limit } => {
        self.list_unread(limit).await
      }
    }
  }

  async fn list_limited(
    &self,
    limit: usize,
    filter: Option<ChatFilter>,
  ) -> Result<Vec<Chat>, WppError> {
    if limit == 0 {
      return Ok(Vec::new());
    }

    let fetch_limit =
      limit.min(PAGE_SIZE);

    let mut chats = self
      .whatsapp
      .list_chats(
        self.session_id,
        fetch_limit,
        0,
      )
      .await?;

    chats.sort_by(|a, b| {
      b.timestamp.cmp(&a.timestamp)
    });

    Self::apply_filter(
      &mut chats,
      filter,
    );

    Ok(chats)
  }

  async fn list_all(
    &self,
    filter: Option<ChatFilter>,
  ) -> Result<Vec<Chat>, WppError> {
    let mut chats = Vec::new();
    let mut offset = 0;

    loop {
      let page = self
        .whatsapp
        .list_chats(
          self.session_id,
          PAGE_SIZE,
          offset,
        )
        .await?;

      let page_len = page.len();

      chats.extend(page);

      if page_len < PAGE_SIZE {
        break;
      }

      offset += page_len;
    }

    chats.sort_by(|a, b| {
      b.timestamp.cmp(&a.timestamp)
    });

    Self::apply_filter(
      &mut chats,
      filter,
    );

    Ok(chats)
  }

  async fn list_unread(
    &self,
    limit: Option<usize>,
  ) -> Result<Vec<Chat>, WppError> {
    if matches!(limit, Some(0)) {
      return Ok(Vec::new());
    }

    let mut unread_chats = Vec::new();
    let mut offset = 0;

    loop {
      let page = self
        .whatsapp
        .list_chats(
          self.session_id,
          PAGE_SIZE,
          offset,
        )
        .await?;

      let page_len = page.len();

      for chat in page {
        if chat.unread_count > 0 {
          unread_chats.push(chat);

          if let Some(limit) = limit {
            if unread_chats.len() >= limit {
              unread_chats.truncate(limit);

              unread_chats.sort_by(
                |a, b| {
                  b.timestamp
                    .cmp(&a.timestamp)
                },
              );

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

    unread_chats.sort_by(
      |a, b| {
        b.timestamp.cmp(&a.timestamp)
      },
    );

    Ok(unread_chats)
  }

  fn apply_filter(
    chats: &mut Vec<Chat>,
    filter: Option<ChatFilter>,
  ) {
    match filter {
      Some(ChatFilter::Unread) => {
        chats.retain(|chat| {
          chat.unread_count > 0
        });
      }

      None => {}
    }
  }
}

fn normalize_phone(value: &str) -> String {
  value
    .chars()
    .filter(|c| c.is_ascii_digit())
    .collect()
}

fn ambiguous_chat_error(
  query: &str,
  matches: &[&Chat],
) -> WppError {
  let candidates = matches
    .iter()
    .take(10)
    .map(|chat| {
      let name = if chat.name.trim().is_empty() {
        &chat.id
      } else {
        &chat.name
      };

      format!("{name} ({})", chat.id)
    })
    .collect::<Vec<_>>()
    .join(", ");

  let suffix = if matches.len() > 10 {
    ", ..."
  } else {
    ""
  };

  WppError::Other(format!(
    "chat `{query}` is ambiguous: {candidates}{suffix}"
  ))
}

#[cfg(test)]
mod tests {
  use super::normalize_phone;

  #[test]
  fn normalize_phone_removes_formatting() {
    assert_eq!(
      normalize_phone("+54 9 351 123-4567"),
      "5493511234567"
    );
  }

  #[test]
  fn normalize_phone_keeps_digits() {
    assert_eq!(
      normalize_phone("5493511234567@c.us"),
      "5493511234567"
    );
  }

  #[test]
  fn normalize_phone_returns_empty_for_names() {
    assert_eq!(
      normalize_phone("Gabriel"),
      ""
    );
  }
}