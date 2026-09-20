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