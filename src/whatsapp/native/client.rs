use std::path::Path;
use std::sync::Arc;
use std::time::Duration;
use whatsapp_rust::prelude::MessageExt;

use tokio::sync::mpsc;
use whatsapp_rust::bot::{
  Bot,
  BotHandle,
};
use whatsapp_rust::pair_code::PairCodeOptions;
use whatsapp_rust::proto_helpers::MessageBuilderExt;
use whatsapp_rust::store::SqliteStore;
use whatsapp_rust::wacore::types::events::{
  Event as NativeEvent,
  EventHandler,
  EventInterest,
  EventKind,
  InboundMessage,
  Subscription,
};
use whatsapp_rust::wacore_binary::JidExt;
use whatsapp_rust_chat_store::{
  ChatCursor,
  ChatEntry,
  ChatStore,
  MessageCursor,
  MessageStatus,
  StoredMessage,
};

use crate::error::WppError;
use crate::whatsapp::models::{
  Chat,
  Message,
  MessageDirection,
  MessagePage,
  RealtimeEvent,
};

/// Authentication behavior requested when opening
/// a native WhatsApp session.
pub enum NativeAuthMode {
  /// Open an existing persisted session silently.
  None,

  /// Wait for a QR payload.
  Qr,

  /// Automatically request a phone pairing code.
  PairingCode(String),
}

/// Authentication event exposed to the CLI.
#[derive(Debug)]
pub enum NativeAuthEvent {
  /// Raw WhatsApp QR payload.
  QrCode(String),

  /// Eight-character phone pairing code.
  PairCode(String),

  /// Pairing-code request failed.
  PairCodeError(String),
}

const CHAT_PAGE_SIZE: i64 = 1000;
const REALTIME_CHANNEL_CAPACITY: usize = 256;

pub struct NativeClient {
  client: Arc<whatsapp_rust::Client>,
  handle: BotHandle,
  auth_receiver: mpsc::Receiver<NativeAuthEvent>,
  chat_store: Arc<ChatStore>,
  chat_store_subscription: Subscription,
}

pub struct NativeRealtimeListener {
  receiver: mpsc::Receiver<RealtimeEvent>,
  subscription: Subscription,
}

impl NativeRealtimeListener {
  pub fn try_recv(
    &mut self,
  ) -> Option<RealtimeEvent> {
    match self.receiver.try_recv() {
      Ok(event) => Some(event),

      Err(mpsc::error::TryRecvError::Empty) => None,

      Err(mpsc::error::TryRecvError::Disconnected) => None,
    }
  }

  pub async fn recv(
    &mut self,
  ) -> Option<RealtimeEvent> {
    self.receiver.recv().await
  }

  pub async fn disconnect(
    self,
  ) -> Result<(), WppError> {
    drop(self.subscription);

    Ok(())
  }
}

struct NativeRealtimeHandler {
  sender: mpsc::Sender<RealtimeEvent>,
  own_jid: String,
}

impl EventHandler for NativeRealtimeHandler {
  fn handle_event(
    &self,
    event: Arc<NativeEvent>,
  ) {
    let NativeEvent::Messages(
      batch
    ) = &*event
    else {
      return;
    };

    for message in batch {
      if message.info.is_offline {
        continue;
      }

      let event =
        realtime_event_from_message(
          message,
          &self.own_jid,
        );

      let _ =
        self.sender.try_send(event);
    }
  }

  fn interest(
    &self,
  ) -> EventInterest {
    EventInterest::of(
      &[EventKind::Messages]
    )
  }
}

impl NativeClient {
  /// Open a persisted native WhatsApp session.
  ///
  /// The SQLite store contains whatsapp-rust authentication and protocol
  /// state. It is intentionally separate from wpp's own message cache.
  pub async fn open(
    storage_path: &Path,
    auth_mode: NativeAuthMode,
  ) -> Result<Self, WppError> {
    if let Some(parent) =
      storage_path.parent()
    {
      std::fs::create_dir_all(parent)
        .map_err(|error| {
          WppError::Other(
            format!(
              "failed to create native session directory: {error}"
            )
          )
        })?;
    }

    let database_url =
      storage_path
        .to_string_lossy()
        .into_owned();

    let backend =
      SqliteStore::new(
        &database_url
      )
      .await
      .map_err(|error| {
        WppError::Other(
          format!(
            "failed to open native WhatsApp storage: {error}"
          )
        )
      })?;

    let chat_store =
      ChatStore::new(&backend)
        .await
        .map_err(|error| {
          WppError::Other(
            format!(
              "failed to open native chat store: {error}"
            )
          )
        })?;

    let (
      sender,
      receiver,
    ) =
      mpsc::channel(16);

    let mut builder =
      Bot::builder()
        .with_backend(backend);

    match auth_mode {
      NativeAuthMode::None => {}

      NativeAuthMode::Qr => {
        let sender =
          sender.clone();

        builder =
          builder.on_qr_code(
            move |code, _timeout| {
              let sender =
                sender.clone();

              async move {
                let _ =
                  sender
                    .send(
                      NativeAuthEvent::QrCode(
                        code
                      )
                    )
                    .await;
              }
            },
          );
      }

      NativeAuthMode::PairingCode(
        phone
      ) => {
        let phone =
          normalize_phone(
            &phone
          );

        if phone.is_empty() {
          return Err(
            WppError::Other(
              "phone number cannot be empty"
                .to_string()
            )
          );
        }

        let sender_for_code =
          sender.clone();

        let sender_for_error =
          sender.clone();

        builder =
          builder
            .on_pair_code(
              move |code, _timeout| {
                let sender =
                  sender_for_code
                    .clone();

                async move {
                  let _ =
                    sender
                      .send(
                        NativeAuthEvent::PairCode(
                          code
                        )
                      )
                      .await;
                }
              },
            )
            .on_pair_code_error(
              move |error, _client| {
                let sender =
                  sender_for_error
                    .clone();

                async move {
                  let _ =
                    sender
                      .send(
                        NativeAuthEvent::PairCodeError(
                          format!(
                            "{error:?}"
                          )
                        )
                      )
                      .await;
                }
              },
            )
            .with_pair_code(
              PairCodeOptions {
                phone_number:
                  phone,
                show_push_notification:
                  true,
                custom_code:
                  None,
                ..Default::default()
              }
            );
      }
    }

    let bot =
      builder
        .build()
        .await
        .map_err(|error| {
          WppError::Other(
            format!(
              "failed to build native WhatsApp client: {error}"
            )
          )
        })?;

    let handle =
      bot.spawn();

    let client =
      handle.client();

    let chat_store_subscription =
      client
        .subscribe_handler(
          chat_store.handler()
        );

    let client =
      handle.client();

    Ok(Self {
      client,
      handle,
      auth_receiver:
        receiver,
      chat_store,
      chat_store_subscription,
    })
  }

  pub fn is_logged_in(
    &self,
  ) -> bool {
    self.client.is_logged_in()
  }

  pub async fn wait_for_connected(
    &self,
    timeout: Duration,
  ) -> Result<(), WppError> {
    self
      .client
      .wait_for_connected(
        timeout
      )
      .await
      .map_err(|error| {
        WppError::Other(
          format!(
            "native WhatsApp connection failed: {error}"
          )
        )
      })
  }

  pub async fn next_auth_event(
    &mut self,
  ) -> Option<NativeAuthEvent> {
    self.auth_receiver.recv().await
  }

  pub fn phone(
    &self,
  ) -> Option<String> {
    let jid =
      self.client.pn()?;

    let raw =
      jid.to_string();

    let user =
      raw
        .split('@')
        .next()
        .unwrap_or("");

    let user =
      user
        .split(':')
        .next()
        .unwrap_or(user);

    let phone =
      user
        .chars()
        .filter(|c| {
          c.is_ascii_digit()
        })
        .collect::<String>();

    if phone.is_empty() {
      None
    } else {
      Some(phone)
    }
  }

  pub fn push_name(
    &self,
  ) -> String {
    self.client.push_name()
  }

  pub async fn logout(
    &self,
  ) {
    self.client.logout().await;
  }

  pub async fn get_chat(
    &self,
    chat_id: &str,
  ) -> Result<Option<Chat>, WppError> {
    let Ok(jid) =
      chat_id
        .parse::<whatsapp_rust::Jid>()
    else {
      return Ok(None);
    };

    let entry =
      self
        .chat_store
        .chat(&jid)
        .await
        .map_err(|error| {
          WppError::Other(
            format!(
              "failed to find native chat: {error}"
            )
          )
        })?;

    Ok(
      entry
        .map(chat_from_entry)
    )
  }

  pub async fn find_chats_by_phone(
    &self,
    phone: &str,
  ) -> Result<Vec<Chat>, WppError> {
    let normalized =
      normalize_phone(phone);

    if normalized.is_empty() {
      return Ok(Vec::new());
    }

    let jid =
      whatsapp_rust::Jid::pn(
        normalized
      );

    let entry =
      self
        .chat_store
        .chat(&jid)
        .await
        .map_err(|error| {
          WppError::Other(
            format!(
              "failed to find native chat by phone: {error}"
            )
          )
        })?;

    Ok(
      entry
        .into_iter()
        .map(chat_from_entry)
        .collect()
    )
  }

  pub async fn get_chat_history(
    &self,
    chat_id: &str,
    limit: usize,
    deep: bool,
  ) -> Result<Vec<Message>, WppError> {
    let jid =
      chat_id
        .parse::<whatsapp_rust::Jid>()
        .map_err(|error| {
          WppError::Other(
            format!(
              "invalid native chat id `{chat_id}`: {error}"
            )
          )
        })?;

    let max_limit =
      if deep {
        2000
      } else {
        100
      };

    let limit =
      limit.clamp(
        1,
        max_limit,
      );

    let mut stored =
      self
        .chat_store
        .messages(
          &jid,
          None,
          limit as i64,
        )
        .await
        .map_err(|error| {
          WppError::Other(
            format!(
              "failed to get native chat history: {error}"
            )
          )
        })?;

    stored.reverse();

    let own_jid =
      self
        .client
        .pn()
        .map(|jid| {
          jid.to_string()
        })
        .unwrap_or_default();

    Ok(
      stored
        .into_iter()
        .map(|message| {
          message_from_stored(
            message,
            &own_jid,
          )
        })
        .collect()
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

    let jid =
      chat_id
        .parse::<whatsapp_rust::Jid>()
        .map_err(|error| {
          WppError::Other(
            format!(
              "invalid native chat id `{chat_id}`: {error}"
            )
          )
        })?;

    let before =
      before
        .map(parse_message_cursor)
        .transpose()?;

    let limit =
      limit.min(100);

    let mut stored =
      self
        .chat_store
        .messages(
          &jid,
          before,
          limit as i64,
        )
        .await
        .map_err(|error| {
          WppError::Other(
            format!(
              "failed to get native persisted messages: {error}"
            )
          )
        })?;

    let next_cursor =
      if stored.len() == limit {
        stored
          .last()
          .map(encode_message_cursor)
      } else {
        None
      };

    stored.reverse();

    let own_jid =
      self
        .client
        .pn()
        .map(|jid| {
          jid.to_string()
        })
        .unwrap_or_default();

    let messages =
      stored
        .into_iter()
        .map(|message| {
          message_from_stored(
            message,
            &own_jid,
          )
        })
        .collect();

    Ok(MessagePage {
      messages,
      next_cursor,
    })
  }

  pub async fn send_text(
    &self,
    chat_id: &str,
    text: &str,
  ) -> Result<(), WppError> {
    let jid =
      chat_id
        .parse::<whatsapp_rust::Jid>()
        .map_err(|error| {
          WppError::Other(
            format!(
              "invalid native chat id `{chat_id}`: {error}"
            )
          )
        })?;

    let message =
      whatsapp_rust::waproto::whatsapp::Message::text(
        text
      );

    let result =
      self
        .client
        .send_message(
          jid,
          message.clone(),
        )
        .await
        .map_err(|error| {
          WppError::Other(
            format!(
              "failed to send native text message: {error}"
            )
          )
        })?;

    self
      .chat_store
      .record_outgoing(
        &result.to,
        result.message_id,
        &message,
        whatsapp_rust::wacore::time::now_utc(),
      )
      .map_err(|error| {
        WppError::Other(
          format!(
            "failed to record native outgoing message: {error}"
          )
        )
      })?;

    self
      .chat_store
      .flush()
      .await
      .map_err(|error| {
        WppError::Other(
          format!(
            "failed to persist native outgoing message: {error}"
          )
        )
      })?;

    Ok(())
  }

  pub async fn delete_chat(
    &self,
    chat_id: &str,
  ) -> Result<(), WppError> {
    let jid =
      chat_id
        .parse::<whatsapp_rust::Jid>()
        .map_err(|error| {
          WppError::Other(
            format!(
              "invalid native chat id `{chat_id}`: {error}"
            )
          )
        })?;

    self
      .client
      .chat_actions()
      .delete_chat(
        &jid,
        true,
        None,
      )
      .await
      .map_err(|error| {
        WppError::Other(
          format!(
            "failed to delete native chat: {error}"
          )
        )
      })
  }

  pub async fn block_contact(
    &self,
    contact_id: &str,
  ) -> Result<(), WppError> {
    let jid =
      contact_id
        .parse::<whatsapp_rust::Jid>()
        .map_err(|error| {
          WppError::Other(
            format!(
              "invalid native contact id `{contact_id}`: {error}"
            )
          )
        })?;

    self
      .client
      .blocking()
      .block(&jid)
      .await
      .map_err(|error| {
        WppError::Other(
          format!(
            "failed to block native contact: {error}"
          )
        )
      })
  }

  pub async fn unblock_contact(
    &self,
    contact_id: &str,
  ) -> Result<(), WppError> {
    let jid =
      contact_id
        .parse::<whatsapp_rust::Jid>()
        .map_err(|error| {
          WppError::Other(
            format!(
              "invalid native contact id `{contact_id}`: {error}"
            )
          )
        })?;

    self
      .client
      .blocking()
      .unblock(&jid)
      .await
      .map_err(|error| {
        WppError::Other(
          format!(
            "failed to unblock native contact: {error}"
          )
        )
      })
  }

  pub async fn archive_chat(
    &self,
    chat_id: &str,
    archive: bool,
  ) -> Result<(), WppError> {
    let jid =
      chat_id
        .parse::<whatsapp_rust::Jid>()
        .map_err(|error| {
          WppError::Other(
            format!(
              "invalid native chat id `{chat_id}`: {error}"
            )
          )
        })?;

    let result =
      if archive {
        self
          .client
          .chat_actions()
          .archive_chat(
            &jid,
            None,
          )
          .await
      } else {
        self
          .client
          .chat_actions()
          .unarchive_chat(
            &jid,
            None,
          )
          .await
      };

    result
      .map_err(|error| {
        WppError::Other(
          format!(
            "failed to {}native chat: {error}",
            if archive {
              "archive "
            } else {
              "unarchive "
            }
          )
        )
      })
  }

  pub async fn pin_chat(
    &self,
    chat_id: &str,
    pin: bool,
  ) -> Result<(), WppError> {
    let jid =
      chat_id
        .parse::<whatsapp_rust::Jid>()
        .map_err(|error| {
          WppError::Other(
            format!(
              "invalid native chat id `{chat_id}`: {error}"
            )
          )
        })?;

    let result =
      if pin {
        self
          .client
          .chat_actions()
          .pin_chat(
            &jid
          )
          .await
      } else {
        self
          .client
          .chat_actions()
          .unpin_chat(
            &jid
          )
          .await
      };

    result
      .map_err(|error| {
        WppError::Other(
          format!(
            "failed to {}native chat: {error}",
            if pin {
              "pin "
            } else {
              "unpin "
            }
          )
        )
      })
  }

  pub async fn mute_chat(
    &self,
    chat_id: &str,
    mute_until: Option<i64>,
  ) -> Result<(), WppError> {
    let jid =
      chat_id
        .parse::<whatsapp_rust::Jid>()
        .map_err(|error| {
          WppError::Other(
            format!(
              "invalid native chat id `{chat_id}`: {error}"
            )
          )
        })?;

    let result =
      match mute_until {
        Some(0) => {
          self
            .client
            .chat_actions()
            .mute_chat(
              &jid
            )
            .await
        }

        Some(mute_end_timestamp_ms) => {
          self
            .client
            .chat_actions()
            .mute_chat_until(
              &jid,
              mute_end_timestamp_ms,
            )
            .await
        }

        None => {
          self
            .client
            .chat_actions()
            .unmute_chat(
              &jid
            )
            .await
        }
      };

    result
      .map_err(|error| {
        WppError::Other(
          format!(
            "failed to {}native chat: {error}",
            match mute_until {
              Some(_) => "mute ",
              None => "unmute ",
            }
          )
        )
      })
  }

  pub async fn mark_chat_read(
    &self,
    chat_id: &str,
    _message_ids: &[String],
  ) -> Result<(), WppError> {
    let jid =
      chat_id
        .parse::<whatsapp_rust::Jid>()
        .map_err(|error| {
          WppError::Other(
            format!(
              "invalid native chat id `{chat_id}`: {error}"
            )
          )
        })?;

    self
      .client
      .chat_actions()
      .mark_chat_as_read(
        &jid,
        true,
        None,
      )
      .await
      .map_err(|error| {
        WppError::Other(
          format!(
            "failed to mark native chat as read: {error}"
          )
        )
      })
  }

  pub async fn mark_chat_unread(
    &self,
    chat_id: &str,
  ) -> Result<(), WppError> {
    let jid =
      chat_id
        .parse::<whatsapp_rust::Jid>()
        .map_err(|error| {
          WppError::Other(
            format!(
              "invalid native chat id `{chat_id}`: {error}"
            )
          )
        })?;

    self
      .client
      .chat_actions()
      .mark_chat_as_read(
        &jid,
        false,
        None,
      )
      .await
      .map_err(|error| {
        WppError::Other(
          format!(
            "failed to mark native chat as unread: {error}"
          )
        )
      })
  }

  pub async fn list_chats(
    &self,
    limit: usize,
    offset: usize,
  ) -> Result<Vec<Chat>, WppError> {
    if limit == 0 {
      return Ok(Vec::new());
    }

    let mut cursor =
      None;

    let mut skipped =
      0usize;

    let mut chats =
      Vec::with_capacity(
        limit
      );

    loop {
      let page =
        self
          .chat_store
          .chats_page(
            true,
            cursor,
            CHAT_PAGE_SIZE,
          )
          .await
          .map_err(|error| {
            WppError::Other(
              format!(
                "failed to list native chats: {error}"
              )
            )
          })?;

      if page.is_empty() {
        break;
      }

      let page_len =
        page.len();

      let Some(last_chat) =
        page.last()
      else {
        break;
      };

      let next_cursor =
        ChatCursor::from(
          last_chat
        );

      let start =
        offset.saturating_sub(
          skipped
        );

      if start < page_len {
        for entry in
          page
            .into_iter()
            .skip(start)
        {
          chats.push(
            chat_from_entry(
              entry
            )
          );

          if chats.len() >= limit {
            return Ok(chats);
          }
        }

        skipped +=
          page_len;

        cursor =
          Some(next_cursor);
      } else {
        skipped +=
          page_len;

        cursor =
          Some(next_cursor);
      }

      if page_len
        < CHAT_PAGE_SIZE as usize
      {
        break;
      }
    }

    Ok(chats)
  }

  pub async fn listen(
    &self,
  ) -> Result<NativeRealtimeListener, WppError> {
    let (
      sender,
      receiver,
    ) =
      mpsc::channel(
        REALTIME_CHANNEL_CAPACITY
      );

    let own_jid =
      self
        .client
        .pn()
        .map(|jid| jid.to_string())
        .unwrap_or_default();

    let handler =
      Arc::new(
        NativeRealtimeHandler {
          sender,
          own_jid,
        }
      );

    let subscription =
      self
        .client
        .subscribe_handler(
          handler
        );

    Ok(
      NativeRealtimeListener {
        receiver,
        subscription,
      }
    )
  }

  pub async fn shutdown(
    self,
  ) {
    let NativeClient {
      client,
      handle,
      auth_receiver,
      chat_store,
      chat_store_subscription,
    } = self;

    drop(
      chat_store_subscription
    );
    drop(chat_store);
    drop(auth_receiver);
    drop(client);

    handle.shutdown().await;
  }
}

fn realtime_event_from_message(
  message: &InboundMessage,
  own_jid: &str,
) -> RealtimeEvent {
  let chat_id =
    message
      .info
      .source
      .chat
      .to_string();

  let sender =
    message
      .info
      .source
      .sender
      .to_string();

  let from_me =
    message
      .info
      .source
      .is_from_me;

  let from =
    if from_me {
      own_jid.to_string()
    } else {
      sender.clone()
    };

  let to =
    if from_me {
      chat_id.clone()
    } else {
      own_jid.to_string()
    };

  let text =
    message
      .message
      .text_content()
      .map(str::to_string);

  let body =
    text
      .clone()
      .or_else(|| {
        message
          .message
          .get_caption()
          .map(str::to_string)
      });

  let kind =
    if text.is_some() {
      "text".to_string()
    } else if !message
      .info
      .media_type
      .is_empty()
    {
      message
        .info
        .media_type
        .clone()
    } else if !message
      .info
      .r#type
      .is_empty()
    {
      message
        .info
        .r#type
        .clone()
    } else {
      "message".to_string()
    };

  let author =
    if !from_me
      && message
        .info
        .source
        .chat
        .is_group()
    {
      Some(sender)
    } else {
      None
    };

  RealtimeEvent {
    event:
      "message.received"
        .to_string(),

    timestamp:
      message
        .info
        .timestamp
        .to_rfc3339(),

    data:
      serde_json::json!({
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

fn message_from_stored(
  stored: StoredMessage,
  own_jid: &str,
) -> Message {
  let chat_id =
    stored.chat_jid
      .to_string();

  let sender =
    stored.sender_jid
      .to_string();

  let (
    from,
    to,
    author,
    direction,
  ) =
    if stored.from_me {
      (
        own_jid.to_string(),
        chat_id.clone(),
        None,
        MessageDirection::Outgoing,
      )
    } else {
      let author =
        if stored.chat_jid.is_group() {
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
    kind:
      stored.kind
        .as_str()
        .to_string(),
    direction,
    author,
    timestamp:
      Some(
        stored
          .timestamp
          .timestamp()
      ),
    status:
      message_status_as_str(
        stored.status
      )
      .to_string(),
  }
}

fn message_status_as_str(
  status: MessageStatus,
) -> &'static str {
  match status {
    MessageStatus::Error =>
      "error",

    MessageStatus::Pending =>
      "pending",

    MessageStatus::ServerAck =>
      "server_ack",

    MessageStatus::Delivered =>
      "delivered",

    MessageStatus::Read =>
      "read",

    MessageStatus::Played =>
      "played",
  }
}

fn chat_from_entry(
  entry: ChatEntry,
) -> Chat {
  let jid =
    entry.jid.to_string();

  let name =
    entry
      .name
      .filter(|name| {
        !name.trim().is_empty()
      })
      .unwrap_or_else(
        || jid.clone()
      );

  let unread_count =
    if entry.unread_count < 0 {
      1
    } else {
      entry
        .unread_count
        .min(
          u32::MAX as i32
        )
    } as u32;

  Chat {
    id: jid.clone(),
    name,
    is_group:
      entry.jid.is_group(),
    unread_count,
    last_message:
      entry.last_message_preview,
    timestamp:
      entry
        .last_message_at
        .map(|timestamp| {
          timestamp.timestamp()
        })
        .unwrap_or(0),
  }
}

fn encode_message_cursor(
  message: &StoredMessage,
) -> String {
  format!(
    "{}:{}",
    message
      .timestamp
      .timestamp_millis(),
    message.seq,
  )
}

fn parse_message_cursor(
  cursor: &str,
) -> Result<
  MessageCursor,
  WppError,
> {
  let (
    timestamp_ms,
    seq
  ) =
    cursor
      .rsplit_once(':')
      .ok_or_else(|| {
        WppError::Other(
          format!(
            "invalid native message cursor `{cursor}`"
          )
        )
      })?;

  let timestamp_ms =
    timestamp_ms
      .parse::<i64>()
      .map_err(|error| {
        WppError::Other(
          format!(
            "invalid native message cursor `{cursor}`: {error}"
          )
        )
      })?;

  let seq =
    seq
      .parse::<i64>()
      .map_err(|error| {
        WppError::Other(
          format!(
            "invalid native message cursor `{cursor}`: {error}"
          )
        )
      })?;

  Ok(MessageCursor {
    timestamp_ms,
    seq,
  })
}

fn normalize_phone(
  value: &str,
) -> String {
  value
    .chars()
    .filter(|c| c.is_ascii_digit())
    .collect()
}