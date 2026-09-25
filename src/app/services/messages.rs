use crate::error::WppError;
use crate::whatsapp::client::WhatsAppClient;
use crate::whatsapp::models::Message;

const PAGE_SIZE: usize = 50;

/// Lazy reader for persisted chat history.
///
/// Messages are kept oldest -> newest even though OpenWA returns
/// pages newest -> oldest.
pub struct MessagePager<'a> {
  whatsapp: &'a WhatsAppClient,
  session_id: &'a str,
  chat_id: &'a str,

  messages: Vec<Message>,
  next_cursor: Option<String>,
  exhausted: bool,
}

impl<'a> MessagePager<'a> {
  pub async fn new(
    whatsapp: &'a WhatsAppClient,
    session_id: &'a str,
    chat_id: &'a str,
  ) -> Result<Self, WppError> {
    let page = whatsapp
      .list_messages(
        session_id,
        chat_id,
        PAGE_SIZE,
        None,
      )
      .await?;

    let page_len = page.messages.len();

    let mut messages = page.messages;

    // OpenWA returns newest -> oldest.
    // The last message in the wire page is therefore
    // the oldest message we currently have.
    let next_cursor = messages
      .last()
      .map(|message| message.id.clone());

    messages.reverse();

    let exhausted =
      page_len < PAGE_SIZE
        || next_cursor.is_none();

    Ok(Self {
      whatsapp,
      session_id,
      chat_id,
      messages,
      next_cursor,
      exhausted,
    })
  }

  /// Load one older page.
  ///
  /// Returns the number of newly loaded messages.
  pub async fn load_older(
    &mut self,
  ) -> Result<usize, WppError> {
    if self.exhausted {
      return Ok(0);
    }

    let Some(cursor) =
      self.next_cursor.clone()
    else {
      self.exhausted = true;
      return Ok(0);
    };

    let page = self
      .whatsapp
      .list_messages(
        self.session_id,
        self.chat_id,
        PAGE_SIZE,
        Some(&cursor),
      )
      .await?;

    let page_len = page.messages.len();

    if page_len == 0 {
      self.exhausted = true;
      return Ok(0);
    }

    let mut older = page.messages;

    let next_cursor = older
      .last()
      .map(|message| message.id.clone());

    // Defensive guard against a broken/non-advancing cursor.
    if next_cursor.as_deref()
      == Some(cursor.as_str())
    {
      self.exhausted = true;
      return Ok(0);
    }

    older.reverse();

    let count = older.len();

    older.extend(
      self.messages.drain(..)
    );

    self.messages = older;
    self.next_cursor = next_cursor;

    if page_len < PAGE_SIZE {
      self.exhausted = true;
    }

    Ok(count)
  }

  /// Load all remaining older pages.
  ///
  /// This is used by the `Home` key. Normal scrolling remains lazy
  /// and requests only one page at a time.
  pub async fn load_all_older(
    &mut self,
  ) -> Result<usize, WppError> {
    let mut loaded = 0;

    while !self.exhausted {
      let count =
        self.load_older().await?;

      if count == 0 {
        break;
      }

      loaded += count;
    }

    Ok(loaded)
  }

  pub fn messages(&self) -> &[Message] {
    &self.messages
  }

  pub fn exhausted(&self) -> bool {
    self.exhausted
  }
}