use std::collections::HashSet;

use anyhow::Result;

use crate::app::services::chat_resolver::ChatResolver;
use crate::app::services::chats::{
  ChatListMode,
  ChatsService,
};
use crate::app::services::sync::MessageSyncService;
use crate::app::state::AppContext;
use crate::storage::LocalMessageStore;
use crate::whatsapp::models::Chat;

pub async fn run(
  limit: Option<usize>,
  who: Option<String>,
) -> Result<()> {
  let context =
    AppContext::load().await?;

  let session_id =
    context.session.entry.id.clone();

  let chats =
    select_chats(
      &context,
      &session_id,
      who,
    )
    .await?;

  if chats.is_empty() {
    println!(
      "No chats to synchronize."
    );

    return Ok(());
  }

  let mut store =
    LocalMessageStore::open()?;

  let database_path =
    store.path().display().to_string();

  let mut service =
    MessageSyncService::new(
      &context.whatsapp,
      &session_id,
      &mut store,
    );

  println!(
    "Synchronizing {} chat{} into {}",
    chats.len(),
    if chats.len() == 1 {
      ""
    } else {
      "s"
    },
    database_path,
  );

  let mut total_messages = 0usize;

  for (index, chat) in
    chats.iter().enumerate()
  {
    let count =
      service.sync_chat(
        chat,
        limit,
      )
      .await?;

    total_messages += count;

    println!(
      "[{}/{}] {}: {} message{}",
      index + 1,
      chats.len(),
      display_name(chat),
      count,
      if count == 1 {
        ""
      } else {
        "s"
      },
    );
  }

  println!(
    "Synchronized {} message{} across {} chat{}.",
    total_messages,
    if total_messages == 1 {
      ""
    } else {
      "s"
    },
    chats.len(),
    if chats.len() == 1 {
      ""
    } else {
      "s"
    },
  );

  Ok(())
}

async fn select_chats(
  context: &AppContext,
  session_id: &str,
  who: Option<String>,
) -> Result<Vec<Chat>> {
  let Some(who) = who else {
    let service =
      ChatsService::new(
        &context.whatsapp,
        session_id,
      );

    return Ok(
      service
        .list(
          ChatListMode::All,
          None,
        )
        .await?,
    );
  };

  let resolver =
    ChatResolver::new(
      &context.whatsapp,
      session_id,
    );

  let mut chats = Vec::new();
  let mut seen = HashSet::new();

  for query in who.split(',') {
    let query = query.trim();

    if query.is_empty() {
      continue;
    }

    let chat =
      resolver.resolve(query).await?;

    if seen.insert(chat.id.clone()) {
      chats.push(chat);
    }
  }

  if chats.is_empty() {
    anyhow::bail!(
      "--who did not contain any chat identifier"
    );
  }

  Ok(chats)
}

fn display_name<'a>(
  chat: &'a Chat,
) -> &'a str {
  if chat.name.trim().is_empty() {
    &chat.id
  } else {
    &chat.name
  }
}