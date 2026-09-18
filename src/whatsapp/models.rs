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

/// QR code response from GET /api/sessions/:id/qr
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QrCodeResponse {
    pub qr_code: String,
    pub status: String,
}

/// Pairing code response from POST /api/sessions/:id/pairing-code
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PairingCodeResponse {
    pub code: String,
}
