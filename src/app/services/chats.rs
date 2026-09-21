use crate::error::WppError;
use crate::whatsapp::client::WhatsAppClient;
use crate::whatsapp::models::Chat;

const PAGE_SIZE: usize = 1000;

/// Filter that can be applied to the selected chat list.
#[derive(Debug, Clone, Copy)]
pub enum ChatFilter {
  Unread,
}

/// Defines how chats should be selected before applying an optional filter.
#[derive(Debug, Clone, Copy)]
pub enum ChatListMode {
  /// Fetch at most N chats.
  Limit(usize),

  /// Fetch every available chat.
  All,

  /// Fetch chats until N unread chats have been found.
  ///
  /// `None` means fetch every unread chat.
  Unread {
    limit: Option<usize>,
  },
}

/// Application service for the chat list.
///
/// It owns pagination and user-facing filtering so the CLI does not know
/// anything about OpenWA's REST API or pagination semantics.
pub struct ChatsService<'a> {
  whatsapp: &'a WhatsAppClient,
  session_id: &'a str,
}

impl<'a> ChatsService<'a> {
  pub fn new(
    whatsapp: &'a WhatsAppClient,
    session_id: &'a str,
  ) -> Self {
    Self {
      whatsapp,
      session_id,
    }
  }

  /// Return chats according to the requested listing mode and optional
  /// post-selection filter.
  ///
  /// `Limit(n)` limits the input set before filtering:
  ///
  ///   fetch n -> filter -> return
  ///
  /// `Unread { limit }` limits the final result:
  ///
  ///   fetch page -> filter unread -> accumulate -> next page
  ///
  /// `All` fetches every available chat before applying the optional filter.
  pub async fn list(
    &self,
    mode: ChatListMode,
    filter: Option<ChatFilter>,
  ) -> Result<Vec<Chat>, WppError> {
    match mode {
      ChatListMode::Limit(limit) => {
        self.list_limited(limit, filter).await
      }

      ChatListMode::All => {
        self.list_all(filter).await
      }

      ChatListMode::Unread { limit } => {
        self.list_unread(limit).await
      }
    }
  }

  async fn list_limited(
    &self,
    limit: usize,
    filter: Option<ChatFilter>,
  ) -> Result<Vec<Chat>, WppError> {
    if limit == 0 {
      return Ok(Vec::new());
    }

    let fetch_limit = limit.min(PAGE_SIZE);

    let mut chats = self
      .whatsapp
      .list_chats(
        self.session_id,
        fetch_limit,
        0,
      )
      .await?;

    chats.sort_by(|a, b| {
      b.timestamp.cmp(&a.timestamp)
    });

    Self::apply_filter(&mut chats, filter);

    Ok(chats)
  }

  async fn list_all(
    &self,
    filter: Option<ChatFilter>,
  ) -> Result<Vec<Chat>, WppError> {
    let mut chats = Vec::new();
    let mut offset = 0;

    loop {
      let page = self
        .whatsapp
        .list_chats(
          self.session_id,
          PAGE_SIZE,
          offset,
        )
        .await?;

      let page_len = page.len();

      chats.extend(page);

      if page_len < PAGE_SIZE {
        break;
      }

      offset += PAGE_SIZE;
    }

    chats.sort_by(|a, b| {
      b.timestamp.cmp(&a.timestamp)
    });

    Self::apply_filter(&mut chats, filter);

    Ok(chats)
  }

  async fn list_unread(
    &self,
    limit: Option<usize>,
  ) -> Result<Vec<Chat>, WppError> {
    if matches!(limit, Some(0)) {
      return Ok(Vec::new());
    }

    let mut unread_chats = Vec::new();
    let mut offset = 0;

    loop {
      let page = self
        .whatsapp
        .list_chats(
          self.session_id,
          PAGE_SIZE,
          offset,
        )
        .await?;

      let page_len = page.len();

      for chat in page {
        if chat.unread_count > 0 {
          unread_chats.push(chat);

          if let Some(limit) = limit {
            if unread_chats.len() >= limit {
              unread_chats.truncate(limit);
              unread_chats.sort_by(|a, b| {
                b.timestamp.cmp(&a.timestamp)
              });

              return Ok(unread_chats);
            }
          }
        }
      }

      if page_len < PAGE_SIZE {
        break;
      }

      offset += PAGE_SIZE;
    }

    unread_chats.sort_by(|a, b| {
      b.timestamp.cmp(&a.timestamp)
    });

    Ok(unread_chats)
  }

  fn apply_filter(
    chats: &mut Vec<Chat>,
    filter: Option<ChatFilter>,
  ) {
    match filter {
      Some(ChatFilter::Unread) => {
        chats.retain(|chat| chat.unread_count > 0);
      }
      None => {}
    }
  }
}