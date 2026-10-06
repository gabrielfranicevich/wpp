use whatsapp_rust::proto_helpers::MessageBuilderExt;
use whatsapp_rust::wacore_binary::JidExt;
use whatsapp_rust_chat_store::{MessageCursor, MessageStatus, StoredMessage};

use crate::error::WppError;
use crate::whatsapp::models::{Message, MessageDirection, MessagePage};

use super::client::NativeClient;

impl NativeClient {
  pub async fn get_chat_history(
    &self,
    chat_id: &str,
    limit: usize,
    deep: bool,
  ) -> Result<Vec<Message>, WppError> {
    let jid = chat_id
      .parse::<whatsapp_rust::Jid>()
      .map_err(|error| WppError::Other(format!("invalid native chat id `{chat_id}`: {error}")))?;

    let max_limit = if deep { 2000 } else { 100 };

    let limit = limit.clamp(1, max_limit);

    let mut stored = self
      .chat_store
      .messages(&jid, None, limit as i64)
      .await
      .map_err(|error| WppError::Other(format!("failed to get native chat history: {error}")))?;

    stored.reverse();

    let own_jid = self
      .client
      .pn()
      .map(|jid| jid.to_string())
      .unwrap_or_default();

    Ok(
      stored
        .into_iter()
        .map(|message| message_from_stored(message, &own_jid))
        .collect(),
    )
  }

  pub async fn list_persisted_messages(
    &self,
    chat_id: &str,
    limit: usize,
    before: Option<&str>,
  ) -> Result<MessagePage, WppError> {
    if limit == 0 {
      return Ok(MessagePage {
        messages: Vec::new(),
        next_cursor: None,
      });
    }

    let jid = chat_id
      .parse::<whatsapp_rust::Jid>()
      .map_err(|error| WppError::Other(format!("invalid native chat id `{chat_id}`: {error}")))?;

    let before = before.map(parse_message_cursor).transpose()?;

    let limit = limit.min(100);

    let mut stored = self
      .chat_store
      .messages(&jid, before, limit as i64)
      .await
      .map_err(|error| {
        WppError::Other(format!("failed to get native persisted messages: {error}"))
      })?;

    let next_cursor = if stored.len() == limit {
      stored.last().map(encode_message_cursor)
    } else {
      None
    };

    stored.reverse();

    let own_jid = self
      .client
      .pn()
      .map(|jid| jid.to_string())
      .unwrap_or_default();

    let messages = stored
      .into_iter()
      .map(|message| message_from_stored(message, &own_jid))
      .collect();

    Ok(MessagePage {
      messages,
      next_cursor,
    })
  }

  pub async fn send_text(&self, chat_id: &str, text: &str) -> Result<(), WppError> {
    let jid = chat_id
      .parse::<whatsapp_rust::Jid>()
      .map_err(|error| WppError::Other(format!("invalid native chat id `{chat_id}`: {error}")))?;

    let message = whatsapp_rust::waproto::whatsapp::Message::text(text);

    let result = self
      .client
      .send_message(jid, message.clone())
      .await
      .map_err(|error| WppError::Other(format!("failed to send native text message: {error}")))?;

    self
      .chat_store
      .record_outgoing(
        &result.to,
        result.message_id,
        &message,
        whatsapp_rust::wacore::time::now_utc(),
      )
      .map_err(|error| {
        WppError::Other(format!("failed to record native outgoing message: {error}"))
      })?;

    self.chat_store.flush().await.map_err(|error| {
      WppError::Other(format!(
        "failed to persist native outgoing message: {error}"
      ))
    })?;

    Ok(())
  }
}

fn message_from_stored(stored: StoredMessage, own_jid: &str) -> Message {
  let chat_id = stored.chat_jid.to_string();

  let sender = stored.sender_jid.to_string();

  let (from, to, author, direction) = if stored.from_me {
    (
      own_jid.to_string(),
      chat_id.clone(),
      None,
      MessageDirection::Outgoing,
    )
  } else {
    let author = if stored.chat_jid.is_group() {
      Some(sender.clone())
    } else {
      None
    };

    (
      sender.clone(),
      own_jid.to_string(),
      author,
      MessageDirection::Incoming,
    )
  };

  Message {
    id: stored.id,
    chat_id,
    from,
    to,
    body: stored.text,
    kind: stored.kind.as_str().to_string(),
    direction,
    author,
    timestamp: Some(stored.timestamp.timestamp()),
    status: message_status_as_str(stored.status).to_string(),
  }
}

fn message_status_as_str(status: MessageStatus) -> &'static str {
  match status {
    MessageStatus::Error => "error",

    MessageStatus::Pending => "pending",

    MessageStatus::ServerAck => "server_ack",

    MessageStatus::Delivered => "delivered",

    MessageStatus::Read => "read",

    MessageStatus::Played => "played",
  }
}

fn encode_message_cursor(message: &StoredMessage) -> String {
  format!("{}:{}", message.timestamp.timestamp_millis(), message.seq,)
}

fn parse_message_cursor(cursor: &str) -> Result<MessageCursor, WppError> {
  let (timestamp_ms, seq) = cursor
    .rsplit_once(':')
    .ok_or_else(|| WppError::Other(format!("invalid native message cursor `{cursor}`")))?;

  let timestamp_ms = timestamp_ms.parse::<i64>().map_err(|error| {
    WppError::Other(format!("invalid native message cursor `{cursor}`: {error}"))
  })?;

  let seq = seq.parse::<i64>().map_err(|error| {
    WppError::Other(format!("invalid native message cursor `{cursor}`: {error}"))
  })?;

  Ok(MessageCursor { timestamp_ms, seq })
}
