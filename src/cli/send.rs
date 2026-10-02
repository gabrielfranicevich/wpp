use crate::app::services::chat_resolver::ChatResolver;
use crate::app::services::messages::send_text;
use crate::app::state::AppContext;

pub async fn run(who: String, message: Vec<String>) -> anyhow::Result<()> {
  if message.is_empty() {
    anyhow::bail!("message cannot be empty");
  }

  let text = message.join(" ");

  if text.trim().is_empty() {
    anyhow::bail!("message cannot be empty");
  }

  let context = AppContext::load()?;

  let session_id = context.session.entry.id.clone();

  let resolver = ChatResolver::new(&context.whatsapp, &session_id);

  let chat = resolver.resolve(&who).await?;

  send_text(&context.whatsapp, &session_id, &chat.id, &text).await?;

  let name = if chat.name.trim().is_empty() {
    &chat.id
  } else {
    &chat.name
  };

  println!("Message sent to {name}.");

  Ok(())
}