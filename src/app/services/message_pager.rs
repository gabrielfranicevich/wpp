use crate::error::WppError;
use crate::whatsapp::client::WhatsAppClient;
use crate::whatsapp::models::{Message, MessageDirection};

use super::messages::send_text;

const PAGE_SIZE: usize = 50;
const MAX_DEEP_HISTORY: usize = 2000;
const UNREAD_LOOKBACK_EXTRA: usize = 50;

/// Lazy reader for live WhatsApp chat history.
///
/// The history backend returns messages oldest -> newest and does not expose
/// a keyset cursor through the application abstraction. Therefore, older
/// messages are loaded by requesting a progressively larger history window.
///
/// Example:
///
///   initial: 50
///   older:  100
///   older:  150
///   ...
///
/// This keeps the initial `chat` invocation cheap while allowing the
/// user to keep scrolling backwards.
pub struct MessagePager<'a> {
  whatsapp: &'a WhatsAppClient,
  chat_id: &'a str,

  messages: Vec<Message>,
  loaded_limit: usize,
  exhausted: bool,
  unread_only: bool,
}

impl<'a> MessagePager<'a> {
  pub async fn new(whatsapp: &'a WhatsAppClient, chat_id: &'a str) -> Result<Self, WppError> {
    let messages = whatsapp.get_chat_history(chat_id, PAGE_SIZE, false).await?;

    let loaded_limit = messages.len().min(PAGE_SIZE);
    let exhausted = messages.len() < PAGE_SIZE;

    Ok(Self {
      whatsapp,
      chat_id,
      messages,
      loaded_limit,
      exhausted,
      unread_only: false,
    })
  }

  /// Create a pager containing only the unread messages in a chat.
  ///
  /// The history source does not mark individual messages as unread.
  /// We therefore fetch a recent window and take the last
  /// `unread_count` incoming messages.
  ///
  /// A small lookback is included because the history can also contain
  /// outgoing messages, which do not contribute to the unread count.
  pub async fn new_unread(
    whatsapp: &'a WhatsAppClient,
    chat_id: &'a str,
    unread_count: usize,
  ) -> Result<Self, WppError> {
    if unread_count == 0 {
      return Ok(Self {
        whatsapp,
        chat_id,
        messages: Vec::new(),
        loaded_limit: 0,
        exhausted: true,
        unread_only: true,
      });
    }

    let requested_limit = unread_count
      .saturating_add(UNREAD_LOOKBACK_EXTRA)
      .min(MAX_DEEP_HISTORY);

    let history = whatsapp
      .get_chat_history(chat_id, requested_limit, true)
      .await?;

    let messages = take_unread_messages(&history, unread_count);
    let loaded_limit = messages.len();

    Ok(Self {
      whatsapp,
      chat_id,
      messages,
      loaded_limit,
      exhausted: true,
      unread_only: true,
    })
  }

  /// Send a message from the currently open chat.
  ///
  /// The message is added optimistically to the normal history view after
  /// the backend confirms the request. Unread-only views deliberately do not
  /// add the outgoing message because it is not part of the unread history.
  pub async fn send_text(&mut self, text: &str) -> Result<(), WppError> {
    send_text(self.whatsapp, self.chat_id, text).await?;

    if !self.unread_only {
      self.messages.push(Message {
        id: format!("local-{}", self.messages.len()),
        chat_id: self.chat_id.to_string(),
        from: String::new(),
        to: self.chat_id.to_string(),
        body: Some(text.to_string()),
        kind: "text".to_string(),
        direction: MessageDirection::Outgoing,
        author: None,
        timestamp: None,
        status: "sent".to_string(),
      });
    }

    Ok(())
  }

  /// Load one additional window of older messages.
  ///
  /// Because history access here does not expose a cursor, this requests
  /// the existing messages plus one additional page.
  ///
  /// Returns the number of newly loaded messages.
  pub async fn load_older(&mut self) -> Result<usize, WppError> {
    if self.exhausted {
      return Ok(0);
    }

    let requested_limit = self
      .loaded_limit
      .saturating_add(PAGE_SIZE)
      .min(MAX_DEEP_HISTORY);

    if requested_limit <= self.loaded_limit {
      self.exhausted = true;
      return Ok(0);
    }

    let messages = self
      .whatsapp
      .get_chat_history(self.chat_id, requested_limit, true)
      .await?;

    let previous_len = self.messages.len();
    let new_len = messages.len();

    // The history backend returns oldest -> newest.
    //
    // Replace the currently loaded window with the larger one.
    // The additional messages are therefore implicitly prepended.
    self.messages = messages;

    self.loaded_limit = requested_limit.min(MAX_DEEP_HISTORY);
    self.exhausted = new_len < requested_limit || requested_limit >= MAX_DEEP_HISTORY;

    Ok(new_len.saturating_sub(previous_len))
  }

  /// Load all remaining available history.
  ///
  /// This is used by the `Home` key.
  pub async fn load_all_older(&mut self) -> Result<usize, WppError> {
    let mut loaded = 0;

    while !self.exhausted {
      let count = self.load_older().await?;

      if count == 0 {
        break;
      }

      loaded += count;
    }

    Ok(loaded)
  }

  /// Add a message received through the realtime listener.
  ///
  /// Messages belonging to another chat are ignored.
  /// Duplicate message ids are ignored as well.
  ///
  /// In unread-only mode only incoming messages are accepted.
  pub fn push_realtime_message(&mut self, message: Message) -> bool {
    if message.chat_id != self.chat_id {
      return false;
    }

    if self.unread_only && message.direction != MessageDirection::Incoming {
      return false;
    }

    if self
      .messages
      .iter()
      .any(|existing| existing.id == message.id)
    {
      return false;
    }

    self.messages.push(message);

    true
  }

  pub fn messages(&self) -> &[Message] {
    &self.messages
  }

  pub fn exhausted(&self) -> bool {
    self.exhausted
  }
}

/// Select the last `count` incoming messages while preserving
/// their original chronological order.
fn take_unread_messages(messages: &[Message], count: usize) -> Vec<Message> {
  if count == 0 {
    return Vec::new();
  }

  let mut unread = Vec::with_capacity(count.min(messages.len()));

  for message in messages.iter().rev() {
    if message.direction != MessageDirection::Incoming {
      continue;
    }

    unread.push(message.clone());

    if unread.len() >= count {
      break;
    }
  }

  unread.reverse();

  unread
}

#[cfg(test)]
mod tests {
  use super::take_unread_messages;
  use crate::whatsapp::models::{Message, MessageDirection};

  fn message(id: &str, direction: MessageDirection) -> Message {
    Message {
      id: id.to_string(),
      chat_id: "chat".to_string(),
      from: "from".to_string(),
      to: "to".to_string(),
      body: Some(id.to_string()),
      kind: "text".to_string(),
      direction,
      author: None,
      timestamp: None,
      status: String::new(),
    }
  }

  #[test]
  fn selects_last_unread_messages_in_original_order() {
    let messages = vec![
      message("1", MessageDirection::Incoming),
      message("2", MessageDirection::Outgoing),
      message("3", MessageDirection::Incoming),
      message("4", MessageDirection::Incoming),
      message("5", MessageDirection::Outgoing),
    ];

    let unread = take_unread_messages(&messages, 2);

    let ids = unread
      .iter()
      .map(|message| message.id.as_str())
      .collect::<Vec<_>>();

    assert_eq!(ids, vec!["3", "4"]);
  }

  #[test]
  fn ignores_outgoing_messages() {
    let messages = vec![
      message("1", MessageDirection::Outgoing),
      message("2", MessageDirection::Incoming),
      message("3", MessageDirection::Outgoing),
    ];

    let unread = take_unread_messages(&messages, 1);

    assert_eq!(unread.len(), 1);
    assert_eq!(unread[0].id, "2");
  }

  #[test]
  fn zero_count_returns_empty() {
    let messages = vec![
      message("1", MessageDirection::Incoming),
      message("2", MessageDirection::Incoming),
    ];

    let unread = take_unread_messages(&messages, 0);

    assert!(unread.is_empty());
  }

  #[test]
  fn returns_available_messages_when_history_is_shorter() {
    let messages = vec![
      message("1", MessageDirection::Incoming),
      message("2", MessageDirection::Incoming),
    ];

    let unread = take_unread_messages(&messages, 10);

    let ids = unread
      .iter()
      .map(|message| message.id.as_str())
      .collect::<Vec<_>>();

    assert_eq!(ids, vec!["1", "2"]);
  }
}
