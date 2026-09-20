use crate::error::WppError;
use crate::whatsapp::client::WhatsAppClient;
use crate::whatsapp::models::Chat;

const PAGE_SIZE: usize = 1000;

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

    /// Return every visible chat, ordered newest to oldest.
    pub async fn list(
        &self,
        unread_only: bool,
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

        if unread_only {
            chats.retain(|chat| chat.unread_count > 0);
        }

        Ok(chats)
    }
}