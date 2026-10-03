use crate::app::services::chats::{
  ChatFilter as ServiceChatFilter,
  ChatListMode,
  ChatsService,
};
use crate::app::state::AppContext;
use crate::cli::ChatFilter;
use crate::terminal::render::render_chats;

pub async fn run(
  limit: Option<usize>,
  all: bool,
  unread: bool,
  filter: Option<ChatFilter>,
) -> anyhow::Result<()> {
  if let Some(limit) = limit {
    if limit == 0 {
      anyhow::bail!(
        "--limit must be greater than 0"
      );
    }
  }

  let mode = if unread {
    ChatListMode::Unread { limit }
  } else if all {
    ChatListMode::All
  } else {
    ChatListMode::Limit(
      limit.unwrap_or(50)
    )
  };

  let filter = filter.map(|filter| {
    match filter {
      ChatFilter::Unread =>
        ServiceChatFilter::Unread,
    }
  });

  let context =
    AppContext::load().await?;

  let service = ChatsService::new(
    &context.whatsapp,
    &context.session.entry.id,
  );

  let chats =
    service.list(mode, filter).await?;

  render_chats(&chats);

  Ok(())
}