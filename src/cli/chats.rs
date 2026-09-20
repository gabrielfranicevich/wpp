use crate::app::services::chats::ChatsService;
use crate::app::state::AppContext;
use crate::terminal::render::render_chats;

pub async fn run(unread_only: bool) -> anyhow::Result<()> {
    let context = AppContext::load()?;

    let service = ChatsService::new(
        &context.whatsapp,
        &context.session.entry.id,
    );

    let chats = service.list(unread_only).await?;

    render_chats(&chats);

    Ok(())
}