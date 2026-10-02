use crate::error::WppError;
use crate::storage::LocalMessageStore;
use crate::whatsapp::client::WhatsAppClient;
use crate::whatsapp::models::Chat;

const PAGE_SIZE: usize = 100;

/// Application service responsible for synchronizing remote message history
/// into the local SQLite cache.
pub struct MessageSyncService<'a> {
  whatsapp: &'a WhatsAppClient,
  session_id: &'a str,
  store: &'a mut LocalMessageStore,
}

impl<'a> MessageSyncService<'a> {
  pub fn new(
    whatsapp: &'a WhatsAppClient,
    session_id: &'a str,
    store: &'a mut LocalMessageStore,
  ) -> Self {
    Self {
      whatsapp,
      session_id,
      store,
    }
  }

  /// Synchronize one chat and return the number of messages fetched.
  ///
  /// OpenWA exposes keyset pagination through the `after` cursor. The cursor
  /// is advanced with the last message id from each page so a large chat can
  /// be synchronized without repeatedly downloading the same window.
  pub async fn sync_chat(
    &mut self,
    chat: &Chat,
    max_messages: Option<usize>,
  ) -> Result<usize, WppError> {
    let target = max_messages.unwrap_or(usize::MAX);

    if target == 0 {
      return Ok(0);
    }

    let mut after: Option<String> = None;
    let mut fetched = 0usize;

    loop {
      let remaining = target.saturating_sub(fetched);
      let page_limit = remaining.min(PAGE_SIZE);

      let page = self
        .whatsapp
        .list_messages(
          self.session_id,
          &chat.id,
          page_limit,
          after.as_deref(),
        )
        .await?;

      if page.messages.is_empty() {
        break;
      }

      self
        .store
        .upsert_messages(
          self.session_id,
          &page.messages,
        )
        .map_err(|error| {
          WppError::Other(format!(
            "local message cache error: {error}"
          ))
        })?;

      fetched += page.messages.len();

      if fetched >= target || page.messages.len() < page_limit {
        break;
      }

      let next_after = page
        .messages
        .last()
        .map(|message| message.id.clone())
        .ok_or_else(|| {
          WppError::Other(
            "OpenWA returned an empty message page".to_string(),
          )
        })?;

      if after.as_deref() == Some(next_after.as_str()) {
        return Err(WppError::Other(
          "OpenWA returned a repeated message cursor".to_string(),
        ));
      }

      after = Some(next_after);
    }

    Ok(fetched)
  }
}