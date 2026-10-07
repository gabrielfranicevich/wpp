use crate::app::state::AppContext;
use crate::whatsapp::models::RealtimeEvent;

pub async fn run() -> anyhow::Result<()> {
  let context = AppContext::load().await?;

  let mut listener = context.whatsapp.listen().await?;

  println!("Listening for incoming messages. Press Ctrl+C to stop.");

  let ctrl_c = tokio::signal::ctrl_c();

  tokio::pin!(ctrl_c);

  loop {
    tokio::select! {
      _ = &mut ctrl_c => break,

      event = listener.recv() => {
        match event {
          Some(event) => print_event(&event),
          None => break,
        }
      }
    }
  }

  listener.disconnect().await?;

  Ok(())
}

fn print_event(event: &RealtimeEvent) {
  let sender = event
    .data
    .get("author")
    .and_then(|value| value.as_str())
    .or_else(|| event.data.get("from").and_then(|value| value.as_str()))
    .unwrap_or("unknown");

  let chat_id = event
    .data
    .get("chatId")
    .and_then(|value| value.as_str())
    .unwrap_or("unknown");

  let body = event
    .data
    .get("body")
    .and_then(|value| value.as_str())
    .filter(|body| !body.trim().is_empty())
    .or_else(|| event.data.get("type").and_then(|value| value.as_str()))
    .unwrap_or("message");

  println!("[{}] {} → {}: {}", event.timestamp, sender, chat_id, body);
}
