use std::sync::Arc;

use tokio::sync::mpsc;
use whatsapp_rust::prelude::MessageExt;
use whatsapp_rust::wacore::types::events::{
  Event as NativeEvent, EventHandler, EventInterest, EventKind, InboundMessage, Subscription,
};
use whatsapp_rust::wacore_binary::JidExt;

use crate::error::WppError;
use crate::whatsapp::models::RealtimeEvent;

const REALTIME_CHANNEL_CAPACITY: usize = 256;

pub struct NativeRealtimeListener {
  receiver: mpsc::Receiver<RealtimeEvent>,
  subscription: Subscription,
}

impl NativeRealtimeListener {
  pub fn try_recv(&mut self) -> Option<RealtimeEvent> {
    match self.receiver.try_recv() {
      Ok(event) => Some(event),

      Err(mpsc::error::TryRecvError::Empty) => None,

      Err(mpsc::error::TryRecvError::Disconnected) => None,
    }
  }

  pub async fn recv(&mut self) -> Option<RealtimeEvent> {
    self.receiver.recv().await
  }

  pub async fn disconnect(self) -> Result<(), WppError> {
    drop(self.subscription);

    Ok(())
  }
}

struct NativeRealtimeHandler {
  sender: mpsc::Sender<RealtimeEvent>,
  own_jid: String,
}

impl EventHandler for NativeRealtimeHandler {
  fn handle_event(&self, event: Arc<NativeEvent>) {
    let NativeEvent::Messages(batch) = &*event else {
      return;
    };

    for message in batch {
      if message.info.is_offline {
        continue;
      }

      let event = realtime_event_from_message(message, &self.own_jid);

      let _ = self.sender.try_send(event);
    }
  }

  fn interest(&self) -> EventInterest {
    EventInterest::of(&[EventKind::Messages])
  }
}

pub(super) async fn listen(
  client: &Arc<whatsapp_rust::Client>,
) -> Result<NativeRealtimeListener, WppError> {
  let (sender, receiver) = mpsc::channel(REALTIME_CHANNEL_CAPACITY);

  let own_jid = client.pn().map(|jid| jid.to_string()).unwrap_or_default();

  let handler = Arc::new(NativeRealtimeHandler { sender, own_jid });

  let subscription = client.subscribe_handler(handler);

  Ok(NativeRealtimeListener {
    receiver,
    subscription,
  })
}

fn realtime_event_from_message(message: &InboundMessage, own_jid: &str) -> RealtimeEvent {
  let chat_id = message.info.source.chat.to_string();

  let sender = message.info.source.sender.to_string();

  let from_me = message.info.source.is_from_me;

  let from = if from_me {
    own_jid.to_string()
  } else {
    sender.clone()
  };

  let to = if from_me {
    chat_id.clone()
  } else {
    own_jid.to_string()
  };

  let text = message.message.text_content().map(str::to_string);

  let body = text
    .clone()
    .or_else(|| message.message.get_caption().map(str::to_string));

  let kind = if text.is_some() {
    "text".to_string()
  } else if !message.info.media_type.is_empty() {
    message.info.media_type.clone()
  } else if !message.info.r#type.is_empty() {
    message.info.r#type.clone()
  } else {
    "message".to_string()
  };

  let author = if !from_me && message.info.source.chat.is_group() {
    Some(sender)
  } else {
    None
  };

  RealtimeEvent {
    event: "message.received".to_string(),

    timestamp: message.info.timestamp.to_rfc3339(),

    data: serde_json::json!({
      "id":
        message
          .info
          .id,
      "chatId":
        chat_id,
      "from":
        from,
      "to":
        to,
      "body":
        body,
      "type":
        kind,
      "timestamp":
        message
          .info
          .timestamp
          .timestamp(),
      "fromMe":
        from_me,
      "author":
        author,
    }),
  }
}
