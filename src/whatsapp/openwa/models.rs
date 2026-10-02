use serde::Deserialize;

/// Session info returned by OpenWA REST API.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Session {
  pub id: String,
  pub name: String,
  pub status: String,

  #[serde(default)]
  pub phone: Option<String>,

  #[serde(default)]
  pub push_name: Option<String>,
}

/// Persisted message returned by OpenWA.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MessageRecord {
  pub id: String,

  pub chat_id: String,

  pub from: String,

  pub to: String,

  #[serde(default)]
  pub body: Option<String>,

  #[serde(rename = "type")]
  pub kind: String,

  pub direction: String,

  #[serde(default)]
  pub author: Option<String>,

  #[serde(default)]
  pub timestamp: Option<i64>,

  pub status: String,
}

/// Message returned by OpenWA's live chat-history endpoint.
///
/// This is the engine payload and is intentionally separate from
/// the persisted `MessageRecord`.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatHistoryMessageRecord {
  pub id: String,

  pub from: String,

  pub to: String,

  pub chat_id: String,

  #[serde(default)]
  pub body: String,

  #[serde(rename = "type")]
  pub kind: String,

  pub timestamp: i64,

  pub from_me: bool,

  #[serde(default)]
  pub author: Option<String>,
}

/// Top-level envelope sent through the OpenWA realtime `message` event.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RealtimeEnvelope {
  #[serde(rename = "type")]
  pub kind: String,

  pub timestamp: String,

  #[serde(default)]
  pub payload: Option<RealtimeEnvelopePayload>,
}

/// Payload contained inside a realtime event envelope.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RealtimeEnvelopePayload {
  pub event: String,

  pub session_id: String,

  pub data: serde_json::Value,
}

/// Response returned by the persisted message-list endpoint.
#[derive(Debug, Clone, Deserialize)]
pub struct MessageListResponse {
  pub messages: Vec<MessageRecord>,
}

/// QR code response from OpenWA.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QrCodeResponse {
  pub qr_code: String,
}

/// Pairing code response from OpenWA.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PairingCodeResponse {
  #[serde(alias = "code", alias = "pairingCode")]
  pub pairing_code: String,
}

/// Chat summary returned by OpenWA.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatSummary {
  pub id: String,
  pub name: String,

  #[serde(default)]
  pub is_group: bool,

  #[serde(default)]
  pub unread_count: u32,

  #[serde(default)]
  pub last_message: Option<String>,

  #[serde(default)]
  pub timestamp: i64,
}

#[cfg(test)]
mod tests {
  use super::RealtimeEnvelope;

  #[test]
  fn realtime_envelope_deserializes_message_event() {
    let value = serde_json::json!({
      "type": "event",
      "timestamp": "2026-10-02T17:30:00.000Z",
      "payload": {
        "event": "message.received",
        "sessionId": "session-1",
        "data": {
          "id": "message-1",
          "from": "5493511234567@c.us",
          "to": "5493517654321@c.us",
          "chatId": "5493511234567@c.us",
          "body": "hello",
          "type": "text",
          "timestamp": 1790962200,
          "fromMe": false,
          "isGroup": false
        }
      }
    });

    let envelope: RealtimeEnvelope =
      serde_json::from_value(value).expect("expected valid realtime envelope");

    assert_eq!(envelope.kind, "event");
    assert_eq!(envelope.timestamp, "2026-10-02T17:30:00.000Z");

    let payload = envelope
      .payload
      .expect("expected realtime payload");

    assert_eq!(payload.event, "message.received");
    assert_eq!(payload.session_id, "session-1");
    assert_eq!(
      payload.data["body"].as_str(),
      Some("hello")
    );
  }
}