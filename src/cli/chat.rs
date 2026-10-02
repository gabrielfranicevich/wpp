use anyhow::Result;

use crate::app::services::chat_resolver::ChatResolver;
use crate::app::services::chats::ChatsService;
use crate::app::services::messages::MessagePager;
use crate::app::state::AppContext;
use crate::terminal::pager::run as run_pager;
use crate::terminal::pager::run_with_listener;
use crate::whatsapp::models::Chat;

pub async fn run(
  who: String,
  unread: bool,
  delete: bool,
  block: bool,
  unblock: bool,
  archive: bool,
  unarchive: bool,
  pin: bool,
  unpin: bool,
  mute: bool,
  unmute: bool,
  mark_read: bool,
  mark_unread: bool,
) -> Result<()> {
  let context = AppContext::load()?;

  let session_id = context.session.entry.id.clone();

  let resolver = ChatResolver::new(&context.whatsapp, &session_id);

  let chat = resolver.resolve(&who).await?;

  let has_block_action = block ^ unblock;
  let has_archive_action = archive ^ unarchive;
  let has_pin_action = pin ^ unpin;
  let has_mute_action = mute ^ unmute;
  let has_read_action = mark_read ^ mark_unread;

  let has_management_action = delete
    || has_block_action
    || has_archive_action
    || has_pin_action
    || has_mute_action
    || has_read_action;

  if !unread && !has_management_action {
    open_realtime_chat(
      &context,
      &session_id,
      &chat,
      false,
    )
    .await?;

    return Ok(());
  }

  if unread {
    open_realtime_chat(
      &context,
      &session_id,
      &chat,
      true,
    )
    .await?;
  }

  let service = ChatsService::new(&context.whatsapp, &session_id);

  if has_block_action {
    if block {
      service.block_chat(&chat).await?;

      println!("Chat blocked: {}.", display_name(&chat));
    } else {
      service.unblock_chat(&chat).await?;

      println!("Chat unblocked: {}.", display_name(&chat));
    }
  }

  if has_archive_action {
    service.archive_chat(&chat, archive).await?;

    if archive {
      println!("Chat archived: {}.", display_name(&chat));
    } else {
      println!("Chat unarchived: {}.", display_name(&chat));
    }
  }

  if has_pin_action {
    service.pin_chat(&chat, pin).await?;

    if pin {
      println!("Chat pinned: {}.", display_name(&chat));
    } else {
      println!("Chat unpinned: {}.", display_name(&chat));
    }
  }

  if has_mute_action {
    service.mute_chat(&chat, mute).await?;

    if mute {
      println!("Chat muted: {}.", display_name(&chat));
    } else {
      println!("Chat unmuted: {}.", display_name(&chat));
    }
  }

  if has_read_action {
    if mark_read {
      service.mark_chat_read(&chat).await?;

      println!("Chat marked as read: {}.", display_name(&chat));
    } else {
      service.mark_chat_unread(&chat).await?;

      println!("Chat marked as unread: {}.", display_name(&chat));
    }
  }

  if delete {
    service.delete_chat(&chat.id).await?;

    println!("Chat deleted: {}.", display_name(&chat));
  }

  Ok(())
}

async fn open_realtime_chat(
  context: &AppContext,
  session_id: &str,
  chat: &Chat,
  unread: bool,
) -> Result<()> {
  let mut listener = context.whatsapp.listen(session_id).await?;

  if unread {
    let mut pager = MessagePager::new_unread(
      &context.whatsapp,
      session_id,
      &chat.id,
      chat.unread_count as usize,
    )
    .await?;

    run_with_listener(
      chat,
      &mut pager,
      &mut listener,
    )
    .await?;
  } else {
    let mut pager =
      MessagePager::new(&context.whatsapp, session_id, &chat.id).await?;

    run_with_listener(
      chat,
      &mut pager,
      &mut listener,
    )
    .await?;
  }

  Ok(())
}

fn display_name<'a>(chat: &'a Chat) -> &'a str {
  if chat.name.trim().is_empty() {
    &chat.id
  } else {
    &chat.name
  }
}