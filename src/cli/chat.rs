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
) -> Result<()> {
  let context = AppContext::load()?;

  let session_id = context.session.entry.id.clone();

  let resolver = ChatResolver::new(&context.whatsapp, &session_id);

  let chat = resolver.resolve(&who).await?;

  if delete {
    if unread {
      anyhow::bail!("--delete and --unread cannot be used together");
    }

    let service = ChatsService::new(&context.whatsapp, &session_id);

    service.delete_chat(&chat.id).await?;

    let name = if chat.name.trim().is_empty() {
      &chat.id
    } else {
      &chat.name
    };

    println!("Chat deleted: {name}.");

    return Ok(());
  }

  let mut pager = if unread {
    MessagePager::new_unread(
      &context.whatsapp,
      &session_id,
      &chat.id,
      chat.unread_count as usize,
    )
    .await?
  } else {
    MessagePager::new(&context.whatsapp, &session_id, &chat.id).await?
  };

  run_pager(&chat, &mut pager).await?;

  Ok(())
}