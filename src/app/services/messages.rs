use crate::error::WppError;
use crate::whatsapp::client::WhatsAppClient;
use crate::whatsapp::models::Message;

const PAGE_SIZE: usize = 50;
const MAX_DEEP_HISTORY: usize = 2000;

/// Lazy reader for live WhatsApp chat history.
///
/// OpenWA's live history endpoint returns messages oldest -> newest
/// and does not expose a keyset cursor. Therefore, older messages are
/// loaded by requesting a progressively larger history window.
///
/// Example:
///
///     initial: 50
///     older:   100
///     older:   150
///     ...
///
/// This keeps the initial `chat` invocation cheap while allowing the
/// user to keep scrolling backwards.
pub struct MessagePager<'a> {
    whatsapp: &'a WhatsAppClient,
    session_id: &'a str,
    chat_id: &'a str,

    messages: Vec<Message>,
    loaded_limit: usize,
    exhausted: bool,
}

impl<'a> MessagePager<'a> {
    pub async fn new(
        whatsapp: &'a WhatsAppClient,
        session_id: &'a str,
        chat_id: &'a str,
    ) -> Result<Self, WppError> {
        let messages = whatsapp
            .get_chat_history(session_id, chat_id, PAGE_SIZE, false)
            .await?;

        let loaded_limit = messages.len().min(PAGE_SIZE);

        let exhausted = messages.len() < PAGE_SIZE;

        Ok(Self {
            whatsapp,
            session_id,
            chat_id,
            messages,
            loaded_limit,
            exhausted,
        })
    }

    /// Load one additional window of older messages.
    ///
    /// Because OpenWA's live history endpoint has no cursor, this
    /// requests the existing messages plus one additional page.
    ///
    /// Returns the number of newly loaded messages.
    pub async fn load_older(&mut self) -> Result<usize, WppError> {
        if self.exhausted {
            return Ok(0);
        }

        let requested_limit = self
            .loaded_limit
            .saturating_add(PAGE_SIZE)
            .min(MAX_DEEP_HISTORY);

        if requested_limit <= self.loaded_limit {
            self.exhausted = true;
            return Ok(0);
        }

        let messages = self
            .whatsapp
            .get_chat_history(self.session_id, self.chat_id, requested_limit, true)
            .await?;

        let previous_len = self.messages.len();

        let new_len = messages.len();

        // OpenWA returns history oldest -> newest.
        //
        // Replace the currently loaded window with the larger one.
        // The additional messages are therefore implicitly prepended.
        self.messages = messages;

        self.loaded_limit = requested_limit.min(MAX_DEEP_HISTORY);

        self.exhausted = new_len < requested_limit || requested_limit >= MAX_DEEP_HISTORY;

        Ok(new_len.saturating_sub(previous_len))
    }

    /// Load all remaining available history.
    ///
    /// This is used by the `Home` key.
    pub async fn load_all_older(&mut self) -> Result<usize, WppError> {
        let mut loaded = 0;

        while !self.exhausted {
            let count = self.load_older().await?;

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
