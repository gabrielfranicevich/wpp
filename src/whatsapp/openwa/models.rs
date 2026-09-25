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

  #[serde(default)]
  pub engine_loaded: Option<bool>,
}

/// Persisted message returned by OpenWA.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MessageRecord {
  pub id: String,

  #[serde(default)]
  pub session_id: String,

  #[serde(default)]
  pub wa_message_id: Option<String>,

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

/// Response returned by the persisted message-list endpoint.
#[derive(Debug, Clone, Deserialize)]
pub struct MessageListResponse {
  pub messages: Vec<MessageRecord>,
  pub total: usize,
}

/// QR code response from OpenWA.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QrCodeResponse {
  pub qr_code: String,
  pub status: String,
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

  #[serde(default)]
  pub kind: String,

  #[serde(default)]
  pub archived: bool,

  #[serde(default)]
  pub pinned: bool,

  #[serde(default)]
  pub muted: bool,
}