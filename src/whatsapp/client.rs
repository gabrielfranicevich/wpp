use reqwest::Client;

use super::models::*;
use crate::error::WppError;

/// HTTP client for the OpenWA REST API.
///
/// Session ID is passed per-call so the same client can operate on
/// any session (needed for `wpp switch`, `wpp login`, etc.).
pub struct OpenWAClient {
    http: Client,
    base_url: String,
}

impl OpenWAClient {
    pub fn new(base_url: &str) -> Self {
        Self {
            http: Client::new(),
            base_url: base_url.trim_end_matches('/').to_string(),
        }
    }

    // ── helpers ──────────────────────────────────────────────────

    fn url(&self, path: &str) -> String {
        format!("{}/api{}", self.base_url, path)
    }

    fn session_url(&self, id: &str, path: &str) -> String {
        self.url(&format!("/sessions/{}{}", id, path))
    }

    /// Turn a non-2xx response into a WppError::Api.
    async fn check(resp: reqwest::Response) -> Result<reqwest::Response, WppError> {
        if resp.status().is_success() {
            return Ok(resp);
        }
        let status = resp.status().as_u16();
        let body = resp.text().await.unwrap_or_default();
        Err(WppError::Api {
            status,
            message: body,
        })
    }

    // ── session lifecycle ───────────────────────────────────────

    /// POST /api/sessions  → create a new session.
    pub async fn create_session(&self, name: &str) -> Result<Session, WppError> {
        let resp = self
            .http
            .post(&self.url("/sessions"))
            .json(&serde_json::json!({ "name": name }))
            .send()
            .await?;
        Ok(Self::check(resp).await?.json().await?)
    }

    /// POST /api/sessions/:id/start
    pub async fn start_session(&self, id: &str) -> Result<Session, WppError> {
        let resp = self
            .http
            .post(&self.session_url(id, "/start"))
            .send()
            .await?;
        Ok(Self::check(resp).await?.json().await?)
    }

    /// GET /api/sessions/:id
    pub async fn get_session(&self, id: &str) -> Result<Session, WppError> {
        let resp = self
            .http
            .get(&self.session_url(id, ""))
            .send()
            .await?;
        Ok(Self::check(resp).await?.json().await?)
    }

    /// GET /api/sessions/:id/qr  → raw QR string to encode locally.
    pub async fn get_qr(&self, id: &str) -> Result<QrCodeResponse, WppError> {
        let resp = self
            .http
            .get(&self.session_url(id, "/qr"))
            .send()
            .await?;
        Ok(Self::check(resp).await?.json().await?)
    }

    /// POST /api/sessions/:id/pairing-code
    pub async fn request_pairing_code(
        &self,
        id: &str,
        phone: &str,
    ) -> Result<PairingCodeResponse, WppError> {
        let resp = self
            .http
            .post(&self.session_url(id, "/pairing-code"))
            .json(&serde_json::json!({ "phoneNumber": phone }))
            .send()
            .await?;
        Ok(Self::check(resp).await?.json().await?)
    }
}
