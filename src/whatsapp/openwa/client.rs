use reqwest::{Client, Response};

use super::models::{
  ChatSummary,
  MessageListResponse,
  PairingCodeResponse as OpenWAPairingCodeResponse,
  QrCodeResponse as OpenWAQrCodeResponse,
  Session as OpenWASession,
};

use crate::error::WppError;
use crate::whatsapp::models::{
  Chat,
  Message,
  MessageDirection,
  MessagePage,
  PairingCodeResponse,
  QrCodeResponse,
  Session,
};

/// Thin REST transport for OpenWA.
///
/// OpenWA-specific URLs, headers and wire models stay inside this module.
pub struct OpenWAClient {
  http: Client,
  base_url: String,
  api_key: Option<String>,
}

impl OpenWAClient {
  pub fn new(
    base_url: &str,
    api_key: Option<String>,
  ) -> Self {
    Self {
      http: Client::new(),
      base_url: base_url
        .trim_end_matches('/')
        .to_string(),
      api_key,
    }
  }

  fn url(&self, path: &str) -> String {
    format!("{}{}",
      self.base_url,
      format!("/api{path}")
    )
  }

  fn request(
    &self,
    builder: reqwest::RequestBuilder,
  ) -> reqwest::RequestBuilder {
    match &self.api_key {
      Some(api_key) => {
        builder.header(
          "X-API-Key",
          api_key,
        )
      }

      None => builder,
    }
  }

  async fn check(
    resp: Response,
  ) -> Result<Response, WppError> {
    if resp.status().is_success() {
      return Ok(resp);
    }

    let status =
      resp.status().as_u16();

    let body = resp
      .text()
      .await
      .unwrap_or_default();

    let message = if status == 401 {
      format!(
        "OpenWA authentication failed. \
         Set WPP_OPENWA_API_KEY (or OPENWA_API_KEY). \
         Response: {body}"
      )
    } else {
      body
    };

    Err(WppError::Api {
      status,
      message,
    })
  }

  /// POST /api/sessions
  pub async fn create_session(
    &self,
    name: &str,
  ) -> Result<Session, WppError> {
    let builder = self
      .http
      .post(self.url("/sessions"))
      .json(&serde_json::json!({
        "name": name
      }));

    let response = self
      .request(builder)
      .send()
      .await?;

    let session: OpenWASession =
      Self::check(response)
        .await?
        .json()
        .await?;

    Ok(session.into())
  }

  /// POST /api/sessions/:id/start
  pub async fn start_session(
    &self,
    session_id: &str,
  ) -> Result<Session, WppError> {
    let builder = self
      .http
      .post(self.url(
        &format!(
          "/sessions/{session_id}/start"
        ),
      ));

    let response = self
      .request(builder)
      .send()
      .await?;

    let session: OpenWASession =
      Self::check(response)
        .await?
        .json()
        .await?;

    Ok(session.into())
  }

  /// GET /api/sessions/:id
  pub async fn get_session(
    &self,
    session_id: &str,
  ) -> Result<Session, WppError> {
    let builder = self
      .http
      .get(self.url(
        &format!(
          "/sessions/{session_id}"
        ),
      ));

    let response = self
      .request(builder)
      .send()
      .await?;

    let session: OpenWASession =
      Self::check(response)
        .await?
        .json()
        .await?;

    Ok(session.into())
  }

  /// GET /api/sessions
  ///
  /// Fetch every session from OpenWA, following the API's
  /// pagination until all sessions have been retrieved.
  pub async fn list_sessions(
    &self,
  ) -> Result<Vec<Session>, WppError> {
    const PAGE_SIZE: usize = 1000;

    let mut offset = 0usize;
    let mut sessions = Vec::new();

    loop {
      let builder = self
        .http
        .get(self.url("/sessions"))
        .query(&[
          ("limit", PAGE_SIZE),
          ("offset", offset),
        ]);

      let response = self
        .request(builder)
        .send()
        .await?;

      let page: Vec<OpenWASession> =
        Self::check(response)
          .await?
          .json()
          .await?;

      let page_len = page.len();

      sessions.extend(
        page.into_iter()
          .map(Session::from),
      );

      if page_len < PAGE_SIZE {
        break;
      }

      offset += page_len;
    }

    Ok(sessions)
  }

  /// POST /api/sessions/:id/logout
  ///
  /// Logs the WhatsApp device out and tears down the OpenWA session.
  pub async fn logout(
    &self,
    session_id: &str,
  ) -> Result<Session, WppError> {
    let builder = self
      .http
      .post(self.url(
        &format!(
          "/sessions/{session_id}/logout"
        ),
      ));

    let response = self
      .request(builder)
      .send()
      .await?;

    let session: OpenWASession =
      Self::check(response)
        .await?
        .json()
        .await?;

    Ok(session.into())
  }

  /// GET /api/sessions/:id/qr
  pub async fn get_qr(
    &self,
    session_id: &str,
  ) -> Result<QrCodeResponse, WppError> {
    let builder = self
      .http
      .get(self.url(
        &format!(
          "/sessions/{session_id}/qr"
        ),
      ));

    let response = self
      .request(builder)
      .send()
      .await?;

    let qr: OpenWAQrCodeResponse =
      Self::check(response)
        .await?
        .json()
        .await?;

    Ok(qr.into())
  }

  /// POST /api/sessions/:id/pairing-code
  pub async fn request_pairing_code(
    &self,
    session_id: &str,
    phone: &str,
  ) -> Result<PairingCodeResponse, WppError> {
    let clean_phone: String = phone
      .chars()
      .filter(|c| c.is_ascii_digit())
      .collect();

    let builder = self
      .http
      .post(self.url(
        &format!(
          "/sessions/{session_id}/pairing-code"
        ),
      ))
      .json(&serde_json::json!({
        "phoneNumber": clean_phone
      }));

    let response = self
      .request(builder)
      .send()
      .await?;

    let pairing:
      OpenWAPairingCodeResponse =
      Self::check(response)
        .await?
        .json()
        .await?;

    Ok(pairing.into())
  }

  /// GET /api/sessions/:id/chats
  pub async fn list_chats(
    &self,
    session_id: &str,
    limit: usize,
    offset: usize,
  ) -> Result<Vec<Chat>, WppError> {
    let builder = self
      .http
      .get(self.url(
        &format!(
          "/sessions/{session_id}/chats"
        ),
      ))
      .query(&[
        ("limit", limit),
        ("offset", offset),
      ]);

    let response = self
      .request(builder)
      .send()
      .await?;

    let chats: Vec<ChatSummary> =
      Self::check(response)
        .await?
        .json()
        .await?;

    Ok(chats
      .into_iter()
      .map(Chat::from)
      .collect())
  }

  /// GET /api/sessions/:id/messages
  ///
  /// OpenWA returns messages newest -> oldest.
  /// `after` is the id of the oldest message in the
  /// previously loaded page, so it is used as a keyset cursor.
  pub async fn list_messages(
    &self,
    session_id: &str,
    chat_id: &str,
    limit: usize,
    after: Option<&str>,
  ) -> Result<MessagePage, WppError> {
    let limit = limit.clamp(1, 100);

    let mut params = vec![
      (
        "chatId",
        chat_id.to_string(),
      ),
      (
        "limit",
        limit.to_string(),
      ),
      (
        "inlineMedia",
        "false".to_string(),
      ),
    ];

    if let Some(after) = after {
      params.push((
        "after",
        after.to_string(),
      ));
    }

    let builder = self
      .http
      .get(self.url(
        &format!(
          "/sessions/{session_id}/messages"
        ),
      ))
      .query(&params);

    let response = self
      .request(builder)
      .send()
      .await?;

    let response:
      MessageListResponse =
      Self::check(response)
        .await?
        .json()
        .await?;

    let messages = response
      .messages
      .into_iter()
      .map(|message| Message {
        id: message.id,
        chat_id: message.chat_id,
        from: message.from,
        to: message.to,
        body: message.body,
        kind: message.kind,
        direction:
          if message
            .direction
            .eq_ignore_ascii_case(
              "outgoing",
            )
          {
            MessageDirection::Outgoing
          } else {
            MessageDirection::Incoming
          },
        author: message.author,
        timestamp: message.timestamp,
        status: message.status,
      })
      .collect();

    Ok(MessagePage {
      messages,
      total: response.total,
    })
  }
}

impl From<OpenWASession> for Session {
  fn from(
    session: OpenWASession,
  ) -> Self {
    Self {
      id: session.id,
      name: session.name,
      status: session.status,
      phone: session.phone,
      push_name: session.push_name,
      engine_loaded:
        session.engine_loaded,
    }
  }
}

impl From<OpenWAQrCodeResponse>
  for QrCodeResponse
{
  fn from(
    response: OpenWAQrCodeResponse,
  ) -> Self {
    Self {
      qr_code: response.qr_code,
      status: response.status,
    }
  }
}

impl From<OpenWAPairingCodeResponse>
  for PairingCodeResponse
{
  fn from(
    response: OpenWAPairingCodeResponse,
  ) -> Self {
    Self {
      code: response.pairing_code,
    }
  }
}

impl From<ChatSummary> for Chat {
  fn from(
    chat: ChatSummary,
  ) -> Self {
    Self {
      id: chat.id,
      name: chat.name,
      is_group: chat.is_group,
      unread_count:
        chat.unread_count,
      last_message:
        chat.last_message,
      timestamp:
        chat.timestamp,
      kind:
        chat.kind,
      archived:
        chat.archived,
      pinned:
        chat.pinned,
      muted:
        chat.muted,
    }
  }
}