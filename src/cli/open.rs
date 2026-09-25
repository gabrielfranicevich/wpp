use anyhow::Result;

use crate::app::services::chats::ChatsService;
use crate::app::services::messages::MessagePager;
use crate::app::state::AppContext;
use crate::terminal::pager::run as run_pager;

pub async fn run(
  who: String,
  unread: bool,
) -> Result<()> {
  if unread {
    anyhow::bail!(
      "`wpp open --unread` is not implemented yet"
    );
  }

  let context = AppContext::load()?;

  let session_id =
    context.session.entry.id.clone();

  let chats = ChatsService::new(
    &context.whatsapp,
    &session_id,
  );

  let chat =
    chats.resolve(&who).await?;

  let mut pager =
    MessagePager::new(
      &context.whatsapp,
      &session_id,
      &chat.id,
    )
    .await?;

  run_pager(
    &chat,
    &mut pager,
  )
  .await?;

  Ok(())
}