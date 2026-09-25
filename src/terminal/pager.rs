use std::io::{self, Write};
use std::time::{SystemTime, UNIX_EPOCH};

use crossterm::{
  cursor,
  event::{
    self,
    Event,
    KeyCode,
    KeyModifiers,
  },
  execute,
  terminal::{
    self,
    Clear,
    ClearType,
    EnterAlternateScreen,
    LeaveAlternateScreen,
  },
};

use crate::app::services::messages::MessagePager;
use crate::whatsapp::models::{
  Chat,
  Message,
  MessageDirection,
};

pub async fn run(
  chat: &Chat,
  pager: &mut MessagePager<'_>,
) -> anyhow::Result<()> {
  let mut terminal =
    TerminalGuard::enter()?;

  let result = run_loop(
    &mut terminal.stdout,
    chat,
    pager,
  )
  .await;

  drop(terminal);

  result
}

async fn run_loop(
  stdout: &mut io::Stdout,
  chat: &Chat,
  pager: &mut MessagePager<'_>,
) -> anyhow::Result<()> {
  // usize::MAX means "show the newest messages".
  let mut scroll_top = usize::MAX;

  loop {
    let (
      current_scroll,
      max_scroll,
      body_height,
    ) = draw(
      stdout,
      chat,
      pager,
      scroll_top,
    )?;

    scroll_top = current_scroll;

    if !event::poll(
      std::time::Duration::from_millis(250),
    )? {
      continue;
    }

    let event = event::read()?;

    let Event::Key(key) = event else {
      continue;
    };

    match key.code {
      KeyCode::Esc
      | KeyCode::Char('q') => {
        break;
      }

      KeyCode::Char('c')
        if key
          .modifiers
          .contains(KeyModifiers::CONTROL) =>
      {
        break;
      }

      KeyCode::Up
      | KeyCode::Char('k') => {
        if scroll_top > 0 {
          scroll_top -= 1;
        } else if !pager.exhausted() {
          let added =
            pager.load_older().await?;

          if added > 0 {
            // Each message currently occupies two
            // terminal lines. When older messages are
            // prepended, move the viewport down by
            // the number of inserted lines so the
            // current message stays visually fixed.
            scroll_top =
              added.saturating_mul(2);
          }
        }
      }

      KeyCode::Down
      | KeyCode::Char('j') => {
        scroll_top =
          scroll_top
            .saturating_add(1)
            .min(max_scroll);
      }

      KeyCode::PageUp => {
        scroll_top =
          scroll_top
            .saturating_sub(
              body_height.max(1),
            );
      }

      KeyCode::PageDown => {
        scroll_top =
          scroll_top
            .saturating_add(
              body_height.max(1),
            )
            .min(max_scroll);
      }

      KeyCode::Home => {
        pager
          .load_all_older()
          .await?;

        scroll_top = 0;
      }

      KeyCode::End => {
        scroll_top = usize::MAX;
      }

      _ => {}
    }
  }

  Ok(())
}

fn draw(
  stdout: &mut io::Stdout,
  chat: &Chat,
  pager: &MessagePager<'_>,
  requested_scroll: usize,
) -> anyhow::Result<(
  usize,
  usize,
  usize,
)> {
  let (
    terminal_width,
    terminal_height,
  ) = terminal::size()?;

  let width =
    terminal_width.max(1) as usize;

  let height =
    terminal_height as usize;

  // Header: 2 lines.
  // Footer: 2 lines.
  let body_height =
    height.saturating_sub(4);

  let content =
    render_messages(
      chat,
      pager.messages(),
    );

  let max_scroll =
    content
      .len()
      .saturating_sub(body_height);

  let scroll_top =
    requested_scroll.min(max_scroll);

  execute!(
    stdout,
    cursor::MoveTo(0, 0),
    Clear(ClearType::All),
  )?;

  let title =
    if chat.name.trim().is_empty() {
      &chat.id
    } else {
      &chat.name
    };

  let title_line = truncate_line(
    &format!(
      " {}  ({} loaded)",
      title,
      pager.messages().len(),
    ),
    width,
  );

  write!(
    stdout,
    "{title_line}\r\n"
  )?;

  let separator =
    "─".repeat(width);

  write!(
    stdout,
    "{separator}\r\n"
  )?;

  for index in
    scroll_top
      ..scroll_top.saturating_add(body_height)
  {
    if let Some(line) =
      content.get(index)
    {
      write!(
        stdout,
        "{}\r\n",
        truncate_line(
          line,
          width,
        ),
      )?;
    } else {
      write!(stdout, "\r\n")?;
    }
  }

  write!(
    stdout,
    "{separator}\r\n"
  )?;

  let footer =
    " ↑↓ / j/k   PgUp/PgDn   Home/End   q/Esc ";

  write!(
    stdout,
    "{}",
    truncate_line(
      footer,
      width,
    ),
  )?;

  stdout.flush()?;

  Ok((
    scroll_top,
    max_scroll,
    body_height,
  ))
}

fn render_messages(
  chat: &Chat,
  messages: &[Message],
) -> Vec<String> {
  let mut lines =
    Vec::with_capacity(
      messages.len() * 2,
    );

  for message in messages {
    let sender =
      message_sender(
        chat,
        message,
      );

    let age =
      format_message_age(
        message.timestamp,
      );

    let status =
      message_status(message);

    let body =
      message_body(message);

    lines.push(format!(
      "{age:>4}  {sender}{status}"
    ));

    lines.push(format!(
      "     {body}"
    ));
  }

  lines
}

fn message_sender(
  chat: &Chat,
  message: &Message,
) -> String {
  match message.direction {
    MessageDirection::Outgoing => {
      "You".to_string()
    }

    MessageDirection::Incoming => {
      if chat.is_group {
        message
          .author
          .clone()
          .filter(|author| {
            !author.trim().is_empty()
          })
          .unwrap_or_else(|| {
            message.from.clone()
          })
      } else if !chat.name.trim().is_empty() {
        chat.name.clone()
      } else {
        message.from.clone()
      }
    }
  }
}

fn message_body(
  message: &Message,
) -> String {
  let body = message
    .body
    .as_deref()
    .unwrap_or("")
    .replace('\r', "")
    .replace('\n', " ↵ ")
    .replace('\t', "    ");

  let body = body.trim();

  if message.kind.eq_ignore_ascii_case("text") {
    if body.is_empty() {
      return "[text]".to_string();
    }

    return body.to_string();
  }

  let kind =
    message.kind.trim();

  let label =
    if kind.is_empty() {
      "[message]".to_string()
    } else {
      format!(
        "[{}]",
        kind.to_lowercase(),
      )
    };

  if body.is_empty() {
    label
  } else {
    format!(
      "{label} {body}"
    )
  }
}

fn message_status(
  message: &Message,
) -> &'static str {
  if message.direction
    != MessageDirection::Outgoing
  {
    return "";
  }

  match message
    .status
    .to_ascii_lowercase()
    .as_str()
  {
    "sent" => " ✓",
    "delivered" => " ✓✓",
    "read" => " ✓✓",
    "failed" => " ✗",
    _ => "",
  }
}

fn format_message_age(
  timestamp: Option<i64>,
) -> String {
  let Some(timestamp) =
    timestamp
  else {
    return "?".to_string();
  };

  let Ok(now) =
    SystemTime::now()
      .duration_since(UNIX_EPOCH)
  else {
    return "?".to_string();
  };

  let now =
    now.as_secs() as i64;

  let delta =
    (now - timestamp).max(0);

  match delta {
    0..=59 =>
      "now".to_string(),

    60..=3_599 =>
      format!("{}m", delta / 60),

    3_600..=86_399 =>
      format!("{}h", delta / 3_600),

    86_400..=604_799 =>
      format!("{}d", delta / 86_400),

    _ =>
      format!(
        "{}w",
        delta / 604_800
      ),
  }
}

fn truncate_line(
  value: &str,
  max_chars: usize,
) -> String {
  if max_chars == 0 {
    return String::new();
  }

  let mut chars =
    value.chars();

  let mut output =
    String::new();

  for _ in 0..max_chars {
    let Some(ch) = chars.next()
    else {
      return output;
    };

    output.push(ch);
  }

  if chars.next().is_some() {
    if max_chars == 1 {
      return "…".to_string();
    }

    output.pop();
    output.push('…');
  }

  output
}

struct TerminalGuard {
  stdout: io::Stdout,
}

impl TerminalGuard {
  fn enter() -> anyhow::Result<Self> {
    terminal::enable_raw_mode()?;

    let mut stdout =
      io::stdout();

    if let Err(error) = execute!(
      stdout,
      EnterAlternateScreen,
      cursor::Hide,
    ) {
      let _ =
        terminal::disable_raw_mode();

      return Err(error.into());
    }

    Ok(Self { stdout })
  }
}

impl Drop for TerminalGuard {
  fn drop(&mut self) {
    let _ = execute!(
      self.stdout,
      cursor::Show,
      LeaveAlternateScreen,
    );

    let _ =
      terminal::disable_raw_mode();
  }
}