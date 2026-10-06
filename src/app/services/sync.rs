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
  /// Message pagination is handled by the active WhatsApp backend so the
  /// application service remains independent of transport-specific cursors.
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
        .list_messages(self.session_id, &chat.id, page_limit, after.as_deref())
        .await?;

      if page.messages.is_empty() {
        break;
      }

      self
        .store
        .upsert_messages(self.session_id, &page.messages)
        .map_err(|error| WppError::Other(format!("local message cache error: {error}")))?;

      fetched += page.messages.len();

      if fetched >= target || page.messages.len() < page_limit {
        break;
      }

      let next_after = page
        .next_cursor
        .ok_or_else(|| WppError::Other("message backend returned no next cursor".to_string()))?;

      if after.as_deref() == Some(next_after.as_str()) {
        return Err(WppError::Other(
          "message backend returned a repeated cursor".to_string(),
        ));
      }

      after = Some(next_after);
    }

    Ok(fetched)
  }
}
