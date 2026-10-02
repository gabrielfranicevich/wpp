use crate::error::WppError;
use crate::whatsapp::client::WhatsAppClient;
use crate::whatsapp::models::Chat;

const PAGE_SIZE: usize = 1000;

/// Resolves a user-provided identifier into exactly one chat.
///
/// Supported identifiers:
///
/// - exact OpenWA chat id
/// - exact phone number
/// - exact chat name, case-insensitive
///
/// Partial matching deliberately does not belong here.
/// That functionality will be provided by `wpp search`.
pub struct ChatResolver<'a> {
  whatsapp: &'a WhatsAppClient,
  session_id: &'a str,
}

impl<'a> ChatResolver<'a> {
  pub fn new(whatsapp: &'a WhatsAppClient, session_id: &'a str) -> Self {
    Self {
      whatsapp,
      session_id,
    }
  }

  /// Resolve a query into a single chat.
  ///
  /// Resolution order:
  ///
  /// 1. Exact chat id
  /// 2. Exact normalized phone number
  /// 3. Exact case-insensitive chat name
  ///
  /// Ambiguous matches return an error instead of choosing
  /// an arbitrary chat.
  pub async fn resolve(&self, query: &str) -> Result<Chat, WppError> {
    let query = query.trim();

    if query.is_empty() {
      return Err(WppError::Other(
        "chat name or phone number cannot be empty".to_string(),
      ));
    }

    let chats = self.list_all().await?;

    // 1. Exact OpenWA chat id.
    if let Some(chat) = chats.iter().find(|chat| chat.id == query) {
      return Ok(chat.clone());
    }

    // 2. Exact phone number.
    let normalized_query = normalize_phone(query);

    if !normalized_query.is_empty() {
      let phone_matches: Vec<&Chat> = chats
        .iter()
        .filter(|chat| normalize_phone(&chat.id) == normalized_query)
        .collect();

      match phone_matches.as_slice() {
        [chat] => {
          return Ok((*chat).clone());
        }

        [] => {}

        _ => {
          return Err(ambiguous_chat_error(query, &phone_matches));
        }
      }
    }

    // 3. Exact name, case-insensitive.
    let query_lower = query.to_lowercase();

    let name_matches: Vec<&Chat> = chats
      .iter()
      .filter(|chat| chat.name.to_lowercase() == query_lower)
      .collect();

    match name_matches.as_slice() {
      [chat] => Ok((*chat).clone()),

      [] => Err(WppError::Other(format!("chat `{query}` not found"))),

      _ => Err(ambiguous_chat_error(query, &name_matches)),
    }
  }

  async fn list_all(&self) -> Result<Vec<Chat>, WppError> {
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

    Ok(chats)
  }
}

fn normalize_phone(value: &str) -> String {
  value.chars().filter(|c| c.is_ascii_digit()).collect()
}

fn ambiguous_chat_error(query: &str, matches: &[&Chat]) -> WppError {
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

  let suffix = if matches.len() > 10 { ", ..." } else { "" };

  WppError::Other(format!("chat `{query}` is ambiguous: {candidates}{suffix}"))
}

#[cfg(test)]
mod tests {
  use super::normalize_phone;

  #[test]
  fn normalize_phone_removes_formatting() {
    assert_eq!(normalize_phone("+54 9 351 123-4567"), "5493511234567");
  }

  #[test]
  fn normalize_phone_keeps_digits_from_chat_id() {
    assert_eq!(normalize_phone("5493511234567@c.us"), "5493511234567");
  }

  #[test]
  fn normalize_phone_returns_empty_for_names() {
    assert_eq!(normalize_phone("Gabriel"), "");
  }
}
