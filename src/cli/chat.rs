use anyhow::Result;

use crate::app::services::chat_resolver::ChatResolver;
use crate::app::services::chats::ChatsService;
use crate::app::services::messages::MessagePager;
use crate::app::state::AppContext;
use crate::terminal::pager::run as run_pager;

pub async fn run(
  who: String,
  unread: bool,
  delete: bool,
  block: bool,
  unblock: bool,
) -> Result<()> {
  let context = AppContext::load()?;

  let session_id = context.session.entry.id.clone();

  let resolver = ChatResolver::new(&context.whatsapp, &session_id);

  let chat = resolver.resolve(&who).await?;

  // `--block --unblock` deliberately cancels itself out.
  let has_block_action = block ^ unblock;

  // With no display mode and no effective management action,
  // `wpp chat <contact>` behaves as the normal pager.
  if !unread && !delete && !has_block_action {
    let mut pager =
      MessagePager::new(&context.whatsapp, &session_id, &chat.id).await?;

    run_pager(&chat, &mut pager).await?;

    return Ok(());
  }

  // `--unread` is a display mode. Any management action is executed
  // only after the pager has been closed.
  if unread {
    let mut pager = MessagePager::new_unread(
      &context.whatsapp,
      &session_id,
      &chat.id,
      chat.unread_count as usize,
    )
    .await?;

    run_pager(&chat, &mut pager).await?;
  }

  let service = ChatsService::new(&context.whatsapp, &session_id);

  if has_block_action {
    if block {
      service.block_chat(&chat).await?;
    } else {
      service.unblock_chat(&chat).await?;
    }

    let name = if chat.name.trim().is_empty() {
      &chat.id
    } else {
      &chat.name
    };

    if block {
      println!("Chat blocked: {name}.");
    } else {
      println!("Chat unblocked: {name}.");
    }
  }

  if delete {
    service.delete_chat(&chat.id).await?;

    let name = if chat.name.trim().is_empty() {
      &chat.id
    } else {
      &chat.name
    };

    println!("Chat deleted: {name}.");
  }

  Ok(())
}