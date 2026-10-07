/// Domain-level chat model used by wpp.
#[derive(Debug, Clone)]
pub struct Chat {
  pub id: String,
  pub name: String,
  pub is_group: bool,
  pub is_pinned: bool,
  pub unread_count: u32,
  pub last_message: Option<String>,
  pub timestamp: i64,
}

#[derive(Debug, Clone)]
pub struct Message {
  pub id: String,
  pub chat_id: String,
  pub from: String,
  pub to: String,
  pub body: Option<String>,
  pub kind: String,
  pub direction: MessageDirection,
  pub author: Option<String>,
  pub timestamp: Option<i64>,
  pub status: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MessageDirection {
  Incoming,
  Outgoing,
}

#[derive(Debug, Clone)]
pub struct MessagePage {
  pub messages: Vec<Message>,
  pub next_cursor: Option<String>,
}

/// A realtime event received from WhatsApp.
#[derive(Debug, Clone)]
pub struct RealtimeEvent {
  pub event: String,
  pub timestamp: String,
  pub data: serde_json::Value,
}
