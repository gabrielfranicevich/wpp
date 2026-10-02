use futures_util::FutureExt;
use reqwest::{Client, Response};
use rust_socketio::{
  asynchronous::{Client as SocketIoClient, ClientBuilder},
  Payload,
};
use serde_json::{json, Value};
use tokio::sync::mpsc;

use super::models::{
  ChatHistoryMessageRecord,
  ChatSummary,
  MessageListResponse,
  PairingCodeResponse as OpenWAPairingCodeResponse,
  QrCodeResponse as OpenWAQrCodeResponse,
  RealtimeEnvelope,
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
  RealtimeEvent,
  Session,
};

const REALTIME_NAMESPACE: &str = "/events";
const REALTIME_EVENT_NAME: &str = "message";
const REALTIME_MESSAGE_RECEIVED: &str = "message.received";
const REALTIME_CHANNEL_CAPACITY: usize = 256;

/// Thin REST + realtime transport for OpenWA.
///
/// OpenWA-specific URLs, headers, Socket.IO namespaces and wire models
/// stay inside this module.
pub struct OpenWAClient {
  http: Client,
  base_url: String,
  api_key: Option<String>,
}

/// Live realtime listener backed by OpenWA Socket.IO.
pub struct OpenWARealtimeListener {
  socket: SocketIoClient,
  receiver: mpsc::Receiver<RealtimeEvent>,
}

impl OpenWARealtimeListener {
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
    self
      .socket
      .disconnect()
      .await
      .map_err(|error| {
        WppError::Other(format!(
          "failed to disconnect realtime listener: {error}"
        ))
      })
  }
}

impl OpenWAClient {
  pub fn new(base_url: &str, api_key: Option<String>) -> Self {
    Self {
      http: Client::new(),
      base_url: base_url.trim_end_matches('/').to_string(),
      api_key,
    }
  }

  fn url(&self, path: &str) -> String {
    format!("{}/api{}", self.base_url, path)
  }

  fn request(&self, builder: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
    match &self.api_key {
      Some(api_key) => builder.header("X-API-Key", api_key),

      None => builder,
    }
  }

  async fn check(resp: Response) -> Result<Response, WppError> {
    if resp.status().is_success() {
      return Ok(resp);
    }

    let status = resp.status().as_u16();

    let body = resp.text().await.unwrap_or_default();

    let message = if status == 401 {
      format!(
        "OpenWA authentication failed. \
     Set WPP_OPENWA_API_KEY (or OPENWA_API_KEY). \
     Response: {body}"
      )
    } else {
      body
    };

    Err(WppError::Api { status, message })
  }

  /// POST /api/sessions
  pub async fn create_session(&self, name: &str) -> Result<Session, WppError> {
    let builder = self
      .http
      .post(self.url("/sessions"))
      .json(&serde_json::json!({
        "name": name
      }));

    let response = self.request(builder).send().await?;

    let session: OpenWASession = Self::check(response).await?.json().await?;

    Ok(session.into())
  }

  /// POST /api/sessions/:id/start
  pub async fn start_session(&self, session_id: &str) -> Result<Session, WppError> {
    let builder = self
      .http
      .post(self.url(&format!("/sessions/{session_id}/start")));

    let response = self.request(builder).send().await?;

    let session: OpenWASession = Self::check(response).await?.json().await?;

    Ok(session.into())
  }

  /// GET /api/sessions/:id
  pub async fn get_session(&self, session_id: &str) -> Result<Session, WppError> {
    let builder = self
      .http
      .get(self.url(&format!("/sessions/{session_id}")));

    let response = self.request(builder).send().await?;

    let session: OpenWASession = Self::check(response).await?.json().await?;

    Ok(session.into())
  }

  /// GET /api/sessions
  ///
  /// Fetch every session from OpenWA, following the API's
  /// pagination until all sessions have been retrieved.
  pub async fn list_sessions(&self) -> Result<Vec<Session>, WppError> {
    const PAGE_SIZE: usize = 1000;

    let mut offset = 0usize;
    let mut sessions = Vec::new();

    loop {
      let builder = self
        .http
        .get(self.url("/sessions"))
        .query(&[("limit", PAGE_SIZE), ("offset", offset)]);

      let response = self.request(builder).send().await?;

      let page: Vec<OpenWASession> =
        Self::check(response).await?.json().await?;

      let page_len = page.len();

      sessions.extend(page.into_iter().map(Session::from));

      if page_len < PAGE_SIZE {
        break;
      }

      offset += page_len;
    }

    Ok(sessions)
  }

  /// DELETE /api/sessions/:id
  pub async fn delete_session(&self, session_id: &str) -> Result<(), WppError> {
    let builder = self
      .http
      .delete(self.url(&format!("/sessions/{session_id}")));

    let response = self.request(builder).send().await?;

    Self::check(response).await?;

    Ok(())
  }

  /// POST /api/sessions/:id/logout
  pub async fn logout(&self, session_id: &str) -> Result<Session, WppError> {
    let builder = self
      .http
      .post(self.url(&format!("/sessions/{session_id}/logout")));

    let response = self.request(builder).send().await?;

    let session: OpenWASession = Self::check(response).await?.json().await?;

    Ok(session.into())
  }

  /// GET /api/sessions/:id/qr
  pub async fn get_qr(&self, session_id: &str) -> Result<QrCodeResponse, WppError> {
    let builder = self
      .http
      .get(self.url(&format!("/sessions/{session_id}/qr")));

    let response = self.request(builder).send().await?;

    let qr: OpenWAQrCodeResponse =
      Self::check(response).await?.json().await?;

    Ok(qr.into())
  }

  /// POST /api/sessions/:id/pairing-code
  pub async fn request_pairing_code(
    &self,
    session_id: &str,
    phone: &str,
  ) -> Result<PairingCodeResponse, WppError> {
    let clean_phone: String =
      phone.chars().filter(|c| c.is_ascii_digit()).collect();

    let builder = self
      .http
      .post(self.url(&format!(
        "/sessions/{session_id}/pairing-code"
      )))
      .json(&serde_json::json!({
        "phoneNumber": clean_phone
      }));

    let response = self.request(builder).send().await?;

    let pairing: OpenWAPairingCodeResponse =
      Self::check(response).await?.json().await?;

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
      .get(self.url(&format!(
        "/sessions/{session_id}/chats"
      )))
      .query(&[("limit", limit), ("offset", offset)]);

    let response = self.request(builder).send().await?;

    let chats: Vec<ChatSummary> =
      Self::check(response).await?.json().await?;

    Ok(chats.into_iter().map(Chat::from).collect())
  }

  /// POST /api/sessions/:id/chats/delete
  ///
  /// Delete a chat from the WhatsApp chat list.
  pub async fn delete_chat(
    &self,
    session_id: &str,
    chat_id: &str,
  ) -> Result<(), WppError> {
    let builder = self
      .http
      .post(self.url(&format!(
        "/sessions/{session_id}/chats/delete"
      )))
      .json(&serde_json::json!({
        "chatId": chat_id,
      }));

    let response = self.request(builder).send().await?;

    Self::check(response).await?;

    Ok(())
  }

  /// POST /api/sessions/:id/contacts/:contactId/block
  ///
  /// Block a contact.
  pub async fn block_contact(
    &self,
    session_id: &str,
    contact_id: &str,
  ) -> Result<(), WppError> {
    let builder = self
      .http
      .post(self.url(&format!(
        "/sessions/{session_id}/contacts/{contact_id}/block"
      )));

    let response = self.request(builder).send().await?;

    Self::check(response).await?;

    Ok(())
  }

  /// DELETE /api/sessions/:id/contacts/:contactId/block
  ///
  /// Unblock a contact.
  pub async fn unblock_contact(
    &self,
    session_id: &str,
    contact_id: &str,
  ) -> Result<(), WppError> {
    let builder = self
      .http
      .delete(self.url(&format!(
        "/sessions/{session_id}/contacts/{contact_id}/block"
      )));

    let response = self.request(builder).send().await?;

    Self::check(response).await?;

    Ok(())
  }

  /// POST /api/sessions/:id/chats/archive
  ///
  /// Archive or unarchive a chat.
  pub async fn archive_chat(
    &self,
    session_id: &str,
    chat_id: &str,
    archive: bool,
  ) -> Result<(), WppError> {
    let builder = self
      .http
      .post(self.url(&format!(
        "/sessions/{session_id}/chats/archive"
      )))
      .json(&serde_json::json!({
        "chatId": chat_id,
        "archive": archive,
      }));

    let response = self.request(builder).send().await?;

    Self::check(response).await?;

    Ok(())
  }

  /// POST /api/sessions/:id/chats/pin
  ///
  /// Pin or unpin a chat.
  pub async fn pin_chat(
    &self,
    session_id: &str,
    chat_id: &str,
    pin: bool,
  ) -> Result<(), WppError> {
    let builder = self
      .http
      .post(self.url(&format!(
        "/sessions/{session_id}/chats/pin"
      )))
      .json(&serde_json::json!({
        "chatId": chat_id,
        "pin": pin,
      }));

    let response = self.request(builder).send().await?;

    Self::check(response).await?;

    Ok(())
  }

  /// POST /api/sessions/:id/chats/mute
  ///
  /// Mute until an epoch-milliseconds timestamp.
  /// `null` un-mutes the chat. `0` means indefinite mute.
  pub async fn mute_chat(
    &self,
    session_id: &str,
    chat_id: &str,
    mute_until: Option<i64>,
  ) -> Result<(), WppError> {
    let builder = self
      .http
      .post(self.url(&format!(
        "/sessions/{session_id}/chats/mute"
      )))
      .json(&serde_json::json!({
        "chatId": chat_id,
        "muteUntil": mute_until,
      }));

    let response = self.request(builder).send().await?;

    Self::check(response).await?;

    Ok(())
  }

  /// POST /api/sessions/:id/chats/read
  ///
  /// Mark a chat as read. OpenWA accepts up to 100 message ids.
  pub async fn mark_chat_read(
    &self,
    session_id: &str,
    chat_id: &str,
    message_ids: &[String],
  ) -> Result<(), WppError> {
    let body = if message_ids.is_empty() {
      serde_json::json!({
        "chatId": chat_id,
      })
    } else {
      serde_json::json!({
        "chatId": chat_id,
        "messageIds": message_ids,
      })
    };

    let builder = self
      .http
      .post(self.url(&format!(
        "/sessions/{session_id}/chats/read"
      )))
      .json(&body);

    let response = self.request(builder).send().await?;

    Self::check(response).await?;

    Ok(())
  }

  /// POST /api/sessions/:id/chats/unread
  ///
  /// Mark a chat as unread.
  pub async fn mark_chat_unread(
    &self,
    session_id: &str,
    chat_id: &str,
  ) -> Result<(), WppError> {
    let builder = self
      .http
      .post(self.url(&format!(
        "/sessions/{session_id}/chats/unread"
      )))
      .json(&serde_json::json!({
        "chatId": chat_id,
      }));

    let response = self.request(builder).send().await?;

    Self::check(response).await?;

    Ok(())
  }

  /// POST /api/sessions/:id/messages/send-text
  ///
  /// Send a plain text message to a chat.
  pub async fn send_text(
    &self,
    session_id: &str,
    chat_id: &str,
    text: &str,
  ) -> Result<(), WppError> {
    let builder = self
      .http
      .post(self.url(&format!(
        "/sessions/{session_id}/messages/send-text"
      )))
      .json(&serde_json::json!({
        "chatId": chat_id,
        "text": text,
      }));

    let response = self.request(builder).send().await?;

    Self::check(response).await?;

    Ok(())
  }

  /// GET /api/sessions/:id/messages
  ///
  /// Read messages persisted by OpenWA.
  ///
  /// This endpoint is used for persistent message storage and
  /// keyset pagination.
  pub async fn list_messages(
    &self,
    session_id: &str,
    chat_id: &str,
    limit: usize,
    after: Option<&str>,
  ) -> Result<MessagePage, WppError> {
    let limit = limit.clamp(1, 100);

    let mut params = vec![
      ("chatId", chat_id.to_string()),
      ("limit", limit.to_string()),
      ("inlineMedia", "false".to_string()),
    ];

    if let Some(after) = after {
      params.push(("after", after.to_string()));
    }

    let builder = self
      .http
      .get(self.url(&format!(
        "/sessions/{session_id}/messages"
      )))
      .query(&params);

    let response = self.request(builder).send().await?;

    let response: MessageListResponse =
      Self::check(response).await?.json().await?;

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
        direction: if message.direction.eq_ignore_ascii_case("outgoing") {
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

  /// GET /api/sessions/:id/messages/:chatId/history
  ///
  /// Read chat history directly from the WhatsApp engine.
  ///
  /// Unlike `list_messages`, this does not depend on OpenWA's
  /// persisted message database.
  pub async fn get_chat_history(
    &self,
    session_id: &str,
    chat_id: &str,
    limit: usize,
    deep: bool,
  ) -> Result<Vec<Message>, WppError> {
    let max_limit = if deep { 2000 } else { 100 };

    let limit = limit.clamp(1, max_limit);

    let mut params = vec![("limit", limit.to_string())];

    if deep {
      params.push(("deep", "true".to_string()));
    }

    let builder = self
      .http
      .get(self.url(&format!(
        "/sessions/{session_id}/messages/{chat_id}/history"
      )))
      .query(&params);

    let response = self.request(builder).send().await?;

    let messages: Vec<ChatHistoryMessageRecord> =
      Self::check(response).await?.json().await?;

    Ok(messages
      .into_iter()
      .map(|message| Message {
        id: message.id,
        chat_id: message.chat_id,
        from: message.from,
        to: message.to,
        body: Some(message.body),
        kind: message.kind,
        direction: if message.from_me {
          MessageDirection::Outgoing
        } else {
          MessageDirection::Incoming
        },
        author: message.author,
        timestamp: Some(message.timestamp),
        // Live history does not expose the persisted delivery
        // status field used by MessageRecord.
        status: String::new(),
      })
      .collect())
  }

  /// Connect to OpenWA's realtime Socket.IO namespace and subscribe
  /// to incoming messages for one session.
  ///
  /// OpenWA exposes realtime events under `/events`. Messages are
  /// delivered through the Socket.IO `message` event as an envelope
  /// containing the actual event name, session id and event data.
  pub async fn listen(
    &self,
    session_id: &str,
  ) -> Result<OpenWARealtimeListener, WppError> {
    let (sender, receiver) =
      mpsc::channel(REALTIME_CHANNEL_CAPACITY);

    let subscription = json!({
      "type": "subscribe",
      "sessionId": session_id,
      "events": [REALTIME_MESSAGE_RECEIVED],
      "requestId": format!("wpp-listen-{session_id}"),
    });

    let subscription_for_open = subscription.clone();
    let expected_session_id = session_id.to_string();

    let sender_for_message = sender.clone();

    let mut builder = ClientBuilder::new(&self.base_url)
      .namespace(REALTIME_NAMESPACE)
      .reconnect(true)
      .reconnect_on_disconnect(true)
      .reconnect_delay(1000, 5000)
      .on("open", move |_, socket| {
        let subscription = subscription_for_open.clone();

        async move {
          if let Err(error) =
            socket.emit(REALTIME_EVENT_NAME, subscription).await
          {
            eprintln!(
              "wpp: realtime subscription failed: {error}"
            );
          }
        }
        .boxed()
      })
      .on("message", move |payload, _| {
        let sender = sender_for_message.clone();
        let expected_session_id = expected_session_id.clone();

        async move {
          let Some(value) = payload_to_json(payload) else {
            return;
          };

          let Ok(envelope) =
            serde_json::from_value::<RealtimeEnvelope>(value)
          else {
            return;
          };

          if envelope.kind != "event" {
            return;
          }

          let Some(payload) = envelope.payload else {
            return;
          };

          if payload.event != REALTIME_MESSAGE_RECEIVED {
            return;
          }

          if payload.session_id != expected_session_id {
            return;
          }

          let event = RealtimeEvent {
            event: payload.event,
            timestamp: envelope.timestamp,
            session_id: payload.session_id,
            data: payload.data,
          };

          let _ = sender.send(event).await;
        }
        .boxed()
      })
      .on("error", |payload, _| {
        async move {
          eprintln!(
            "wpp: realtime listener error: {payload:?}"
          );
        }
        .boxed()
      });

    if let Some(api_key) = &self.api_key {
      builder = builder.auth(json!({
        "apiKey": api_key,
      }));
    }

    let socket = builder
      .connect()
      .await
      .map_err(|error| {
        WppError::Other(format!(
          "failed to connect to OpenWA realtime events: {error}"
        ))
      })?;

    Ok(OpenWARealtimeListener {
      socket,
      receiver,
    })
  }
}

fn payload_to_json(payload: Payload) -> Option<Value> {
  match payload {
    Payload::Text(values) => values.into_iter().next(),

    Payload::String(value) => serde_json::from_str(&value).ok(),

    Payload::Binary(_) => None,
  }
}

impl From<OpenWASession> for Session {
  fn from(session: OpenWASession) -> Self {
    Self {
      id: session.id,
      name: session.name,
      status: session.status,
      phone: session.phone,
      push_name: session.push_name,
      engine_loaded: session.engine_loaded,
    }
  }
}

impl From<OpenWAQrCodeResponse> for QrCodeResponse {
  fn from(response: OpenWAQrCodeResponse) -> Self {
    Self {
      qr_code: response.qr_code,
      status: response.status,
    }
  }
}

impl From<OpenWAPairingCodeResponse> for PairingCodeResponse {
  fn from(response: OpenWAPairingCodeResponse) -> Self {
    Self {
      code: response.pairing_code,
    }
  }
}

impl From<ChatSummary> for Chat {
  fn from(chat: ChatSummary) -> Self {
    Self {
      id: chat.id,
      name: chat.name,
      is_group: chat.is_group,
      unread_count: chat.unread_count,
      last_message: chat.last_message,
      timestamp: chat.timestamp,
      kind: chat.kind,
      archived: chat.archived,
      pinned: chat.pinned,
      muted: chat.muted,
    }
  }
}