use crate::app::services::chats::ChatsService;
use crate::app::state::AppContext;
use crate::terminal::render::render_chats;

pub async fn run(query: String) -> anyhow::Result<()> {
  let context = AppContext::load().await?;

  let service = ChatsService::new(&context.whatsapp, &context.session.entry.id);

  let chats = service.search(&query).await?;

  render_chats(&chats);

  Ok(())
}
