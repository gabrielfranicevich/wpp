/// Domain-level chat model used by wpp.
///
/// This deliberately does not mirror OpenWA's DTOs one-to-one.
#[derive(Debug, Clone)]
pub struct Chat {
  pub id: String,
  pub name: String,
  pub is_group: bool,
  pub unread_count: u32,
  pub last_message: Option<String>,
  pub timestamp: i64,
  pub kind: String,
  pub archived: bool,
  pub pinned: bool,
  pub muted: bool,
}

/// WhatsApp session used by application code.
#[derive(Debug, Clone)]
pub struct Session {
  pub id: String,
  pub name: String,
  pub status: String,
  pub phone: Option<String>,
  pub push_name: Option<String>,
  pub engine_loaded: Option<bool>,
}

/// QR response.
#[derive(Debug, Clone)]
pub struct QrCodeResponse {
  pub qr_code: String,
  pub status: String,
}

/// Phone pairing response.
#[derive(Debug, Clone)]
pub struct PairingCodeResponse {
  pub code: String,
}