use std::io::{self, Write};
use std::time::Duration;

use crossterm::{
  cursor,
  event::{self, Event, KeyCode, KeyModifiers},
  execute,
  style::{Color, ResetColor, SetForegroundColor},
  terminal::{self, Clear, ClearType, EnterAlternateScreen, LeaveAlternateScreen},
};

use crate::app::services::messages::MessagePager;
use crate::whatsapp::client::RealtimeListener;
use crate::whatsapp::models::{Chat, Message, MessageDirection, RealtimeEvent};

const BUBBLE_MAX_WIDTH_RATIO: usize = 60;
const BUBBLE_MIN_WIDTH: usize = 12;
const MESSAGE_PREFIX_WIDTH: usize = 2;

pub async fn run_with_listener(
  chat: &Chat,
  pager: &mut MessagePager<'_>,
  listener: &mut RealtimeListener,
) -> anyhow::Result<()> {
  let mut terminal = TerminalGuard::enter()?;

  let result = run_loop(&mut terminal.stdout, chat, pager, Some(listener)).await;

  drop(terminal);

  result
}

async fn run_loop(
  stdout: &mut io::Stdout,
  chat: &Chat,
  pager: &mut MessagePager<'_>,
  mut listener: Option<&mut RealtimeListener>,
) -> anyhow::Result<()> {
  let mut composer: Option<Composer> = None;

  let (mut scroll_top, mut max_scroll, mut body_height) =
    draw(stdout, chat, pager, usize::MAX, None)?;

  loop {
    let mut redraw = false;
    let mut scroll_to_bottom = false;

    /*
     * Drain any realtime messages already available.
     *
     * `try_recv()` is intentionally non-blocking so the pager
     * remains responsive to keyboard input.
     */
    if let Some(listener) = listener.as_deref_mut() {
      let was_at_bottom = scroll_top == max_scroll;

      while let Some(event) = listener.try_recv() {
        let Some(message) = realtime_message(&event) else {
          continue;
        };

        if pager.push_realtime_message(message) {
          redraw = true;

          if was_at_bottom {
            scroll_to_bottom = true;
          }
        }
      }
    }

    if redraw {
      let requested_scroll = if scroll_to_bottom {
        usize::MAX
      } else {
        scroll_top
      };

      let result = draw(stdout, chat, pager, requested_scroll, composer.as_ref())?;

      scroll_top = result.0;
      max_scroll = result.1;
      body_height = result.2;
    }

    if !event::poll(Duration::from_millis(250))? {
      continue;
    }

    let event = event::read()?;

    redraw = false;
    scroll_to_bottom = false;

    match event {
      Event::Resize(_, _) => {
        redraw = true;
      }

      Event::Key(key) => {
        if let Some(current_composer) = composer.as_mut() {
          match key.code {
            KeyCode::Esc => {
              composer = None;
              redraw = true;
            }

            KeyCode::Enter if key.modifiers.contains(KeyModifiers::SHIFT) => {
              current_composer.insert('\n');
              redraw = true;
            }

            KeyCode::Enter => {
              let text = current_composer.text();

              if !text.trim().is_empty() {
                let was_at_bottom = scroll_top == max_scroll;

                pager.send_text(&text).await?;

                composer = None;

                redraw = true;
                scroll_to_bottom = was_at_bottom;
              }
            }

            KeyCode::Backspace => {
              current_composer.backspace();
              redraw = true;
            }

            KeyCode::Delete => {
              current_composer.delete();
              redraw = true;
            }

            KeyCode::Left => {
              current_composer.move_left();
              redraw = true;
            }

            KeyCode::Right => {
              current_composer.move_right();
              redraw = true;
            }

            KeyCode::Home => {
              current_composer.move_home();
              redraw = true;
            }

            KeyCode::End => {
              current_composer.move_end();
              redraw = true;
            }

            KeyCode::Char(character)
              if !key.modifiers.contains(KeyModifiers::CONTROL)
                && !key.modifiers.contains(KeyModifiers::ALT) =>
            {
              current_composer.insert(character);
              redraw = true;
            }

            _ => {}
          }

          if redraw {
            let requested_scroll = if scroll_to_bottom {
              usize::MAX
            } else {
              scroll_top
            };

            let result = draw(stdout, chat, pager, requested_scroll, composer.as_ref())?;

            scroll_top = result.0;
            max_scroll = result.1;
            body_height = result.2;
          }

          continue;
        }

        match key.code {
          KeyCode::Esc | KeyCode::Char('q') => {
            break;
          }

          KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            break;
          }

          // Ctrl+Space enters message composition mode.
          KeyCode::Char(' ') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            composer = Some(Composer::default());

            redraw = true;
          }

          KeyCode::Up | KeyCode::Char('k') => {
            if scroll_top > 0 {
              scroll_top -= 1;
              redraw = true;
            } else if !pager.exhausted() {
              let width = terminal::size()?.0.max(1) as usize;

              /*
               * Preserve a semantic viewport anchor instead of
               * relying only on the number of newly rendered lines.
               */
              let before_content = render_messages(chat, pager.messages(), width);

              let anchor = capture_viewport_anchor(&before_content, scroll_top);

              pager.load_older().await?;

              let after_content = render_messages(chat, pager.messages(), width);

              if let Some(anchor) = anchor {
                if let Some(restored_scroll) = restore_viewport_anchor(&after_content, &anchor) {
                  scroll_top = restored_scroll;
                  redraw = true;
                } else {
                  let added_lines = after_content.len().saturating_sub(before_content.len());

                  if added_lines > 0 {
                    scroll_top = added_lines;
                    redraw = true;
                  }
                }
              } else {
                let added_lines = after_content.len().saturating_sub(before_content.len());

                if added_lines > 0 {
                  scroll_top = added_lines;
                  redraw = true;
                }
              }
            }
          }

          KeyCode::Down | KeyCode::Char('j') => {
            let next = scroll_top.saturating_add(1).min(max_scroll);

            if next != scroll_top {
              scroll_top = next;
              redraw = true;
            }
          }

          KeyCode::PageUp => {
            let next = scroll_top.saturating_sub(body_height.max(1));

            if next != scroll_top {
              scroll_top = next;
              redraw = true;
            }
          }

          KeyCode::PageDown => {
            let next = scroll_top
              .saturating_add(body_height.max(1))
              .min(max_scroll);

            if next != scroll_top {
              scroll_top = next;
              redraw = true;
            }
          }

          KeyCode::Home => {
            pager.load_all_older().await?;

            scroll_top = 0;
            redraw = true;
          }

          KeyCode::End if scroll_top != max_scroll => {
            scroll_top = max_scroll;
            redraw = true;
          }

          _ => {}
        }
      }

      _ => {}
    }

    if redraw {
      let requested_scroll = if scroll_to_bottom {
        usize::MAX
      } else {
        scroll_top
      };

      let result = draw(stdout, chat, pager, requested_scroll, composer.as_ref())?;

      scroll_top = result.0;
      max_scroll = result.1;
      body_height = result.2;
    }
  }

  Ok(())
}

fn draw(
  stdout: &mut io::Stdout,
  chat: &Chat,
  pager: &MessagePager<'_>,
  requested_scroll: usize,
  composer: Option<&Composer>,
) -> anyhow::Result<(usize, usize, usize)> {
  let (terminal_width, terminal_height) = terminal::size()?;

  let width = terminal_width.max(1) as usize;
  let height = terminal_height as usize;

  // Header: 2 lines.
  // Normal footer: 2 lines.
  // Composer adds one extra line.
  let reserved_lines = if composer.is_some() { 5 } else { 4 };

  let body_height = height.saturating_sub(reserved_lines);

  let content = render_messages(chat, pager.messages(), width);

  let max_scroll = content.len().saturating_sub(body_height);

  let scroll_top = requested_scroll.min(max_scroll);

  execute!(stdout, cursor::MoveTo(0, 0), Clear(ClearType::All),)?;

  let title = if chat.name.trim().is_empty() {
    &chat.id
  } else {
    &chat.name
  };

  let title_line = truncate_line(
    &format!(" {} ({} loaded)", title, pager.messages().len(),),
    width,
  );

  write!(stdout, "{title_line}\r\n")?;

  let separator = "─".repeat(width);

  write!(stdout, "{separator}\r\n")?;

  for index in scroll_top..scroll_top.saturating_add(body_height) {
    if let Some(line) = content.get(index) {
      draw_line(stdout, line, width)?;
    } else {
      write!(stdout, "\r\n")?;
    }
  }

  write!(stdout, "{separator}\r\n")?;

  if let Some(composer) = composer {
    draw_composer(stdout, composer, width)?;
  }

  let footer = if composer.is_some() {
    " Enter: send   Shift+Enter: newline   Esc: cancel   ←→ / Home/End   Backspace/Delete "
  } else {
    " ↑↓ / j/k  PgUp/PgDn  Home/End  Ctrl+Space: message  q/Esc "
  };

  write!(stdout, "{}", truncate_line(footer, width),)?;

  stdout.flush()?;

  Ok((scroll_top, max_scroll, body_height))
}

#[derive(Debug, Clone)]
struct RenderedLine {
  alignment: Alignment,
  text: String,
  color: Option<Color>,

  // Present only on the first body line of each message.
  message_id: Option<String>,
}

#[derive(Debug, Clone, Copy)]
enum Alignment {
  Left,
  Right,
}

#[derive(Debug, Clone)]
struct ViewportAnchor {
  message_id: String,
  screen_row: usize,
}

fn capture_viewport_anchor(content: &[RenderedLine], scroll_top: usize) -> Option<ViewportAnchor> {
  for (line_index, line) in content.iter().enumerate().skip(scroll_top) {
    let Some(message_id) = line.message_id.as_ref() else {
      continue;
    };

    return Some(ViewportAnchor {
      message_id: message_id.clone(),
      screen_row: line_index.saturating_sub(scroll_top),
    });
  }

  None
}

fn restore_viewport_anchor(content: &[RenderedLine], anchor: &ViewportAnchor) -> Option<usize> {
  for (line_index, line) in content.iter().enumerate() {
    if line.message_id.as_deref() != Some(anchor.message_id.as_str()) {
      continue;
    }

    return Some(line_index.saturating_sub(anchor.screen_row));
  }

  None
}

#[derive(Debug, Default)]
struct Composer {
  chars: Vec<char>,
  cursor: usize,
}

impl Composer {
  fn text(&self) -> String {
    self.chars.iter().collect()
  }

  fn insert(&mut self, character: char) {
    self.chars.insert(self.cursor, character);
    self.cursor += 1;
  }

  fn backspace(&mut self) {
    if self.cursor == 0 {
      return;
    }

    self.cursor -= 1;
    self.chars.remove(self.cursor);
  }

  fn delete(&mut self) {
    if self.cursor >= self.chars.len() {
      return;
    }

    self.chars.remove(self.cursor);
  }

  fn move_left(&mut self) {
    self.cursor = self.cursor.saturating_sub(1);
  }

  fn move_right(&mut self) {
    self.cursor = self.cursor.saturating_add(1).min(self.chars.len());
  }

  fn move_home(&mut self) {
    self.cursor = 0;
  }

  fn move_end(&mut self) {
    self.cursor = self.chars.len();
  }
}

fn draw_composer(
  stdout: &mut io::Stdout,
  composer: &Composer,
  terminal_width: usize,
) -> anyhow::Result<()> {
  if terminal_width == 0 {
    return Ok(());
  }

  let prefix = "> ";

  let available = terminal_width
    .saturating_sub(prefix.chars().count())
    .saturating_sub(1);

  let cursor = composer.cursor;

  let start = cursor.saturating_sub(available);

  let end = (start + available).min(composer.chars.len());

  let visible = composer.chars[start..end].iter().collect::<String>();

  let cursor_offset = cursor.saturating_sub(start);

  let mut line = String::with_capacity(prefix.len() + visible.len() + 1);

  line.push_str(prefix);

  for (index, character) in visible.chars().enumerate() {
    if index == cursor_offset {
      line.push('▌');
    }

    if character == '\n' {
      line.push('↵');
    } else {
      line.push(character);
    }
  }

  if cursor_offset == visible.chars().count() {
    line.push('▌');
  }

  write!(stdout, "{line}")?;

  let padding = terminal_width.saturating_sub(line.chars().count());

  write!(stdout, "{}", " ".repeat(padding),)?;

  write!(stdout, "\r\n")?;

  Ok(())
}

fn realtime_message(event: &RealtimeEvent) -> Option<Message> {
  if event.event != "message.received" {
    return None;
  }

  let data = &event.data;

  let id = data.get("id").and_then(|value| value.as_str())?.to_string();

  let chat_id = data
    .get("chatId")
    .and_then(|value| value.as_str())?
    .to_string();

  let from = data
    .get("from")
    .and_then(|value| value.as_str())
    .unwrap_or("")
    .to_string();

  let to = data
    .get("to")
    .and_then(|value| value.as_str())
    .unwrap_or("")
    .to_string();

  let body = data
    .get("body")
    .and_then(|value| value.as_str())
    .map(str::to_string);

  let kind = data
    .get("type")
    .and_then(|value| value.as_str())
    .unwrap_or("text")
    .to_string();

  let timestamp = data.get("timestamp").and_then(|value| value.as_i64());

  let from_me = data
    .get("fromMe")
    .and_then(|value| value.as_bool())
    .unwrap_or(false);

  let author = data
    .get("author")
    .and_then(|value| value.as_str())
    .map(str::to_string);

  Some(Message {
    id,
    chat_id,
    from,
    to,
    body,
    kind,
    direction: if from_me {
      MessageDirection::Outgoing
    } else {
      MessageDirection::Incoming
    },
    author,
    timestamp,
    status: String::new(),
  })
}

fn render_messages(chat: &Chat, messages: &[Message], terminal_width: usize) -> Vec<RenderedLine> {
  let mut lines = Vec::new();

  let max_bubble_width = ((terminal_width * BUBBLE_MAX_WIDTH_RATIO) / 100).max(BUBBLE_MIN_WIDTH);

  let mut index = 0;

  while index < messages.len() {
    let first = &messages[index];

    let direction = first.direction;
    let sender = message_group_key(chat, first);

    let color = if direction == MessageDirection::Incoming {
      Some(sender_color(&sender))
    } else {
      None
    };

    let start = index;

    index += 1;

    // Group consecutive messages from
    // the same sender and direction.
    while index < messages.len() {
      let current = &messages[index];

      if current.direction != direction {
        break;
      }

      if message_group_key(chat, current) != sender {
        break;
      }

      index += 1;
    }

    let group = &messages[start..index];

    render_group(
      group,
      direction,
      &sender,
      color,
      max_bubble_width,
      &mut lines,
    );
  }

  lines
}

fn render_group(
  messages: &[Message],
  direction: MessageDirection,
  sender: &str,
  color: Option<Color>,
  max_bubble_width: usize,
  lines: &mut Vec<RenderedLine>,
) {
  let incoming = direction == MessageDirection::Incoming;

  let alignment = if incoming {
    Alignment::Left
  } else {
    Alignment::Right
  };

  /*
   * Bubble layout:
   *
   *  ╭─ Martina ─────────╮
   *  │• Hola             │
   *  │  Segunda línea    │
   *  │• Otro mensaje     │
   *  ╰───────────────────╯
   */

  let body_width = max_bubble_width
    .saturating_sub(2)
    .saturating_sub(MESSAGE_PREFIX_WIDTH)
    .max(1);

  let mut messages_lines = Vec::<Vec<String>>::new();

  for message in messages {
    let body = message_body(message);
    let wrapped = wrap_text(&body, body_width);

    messages_lines.push(wrapped);
  }

  let longest_message_line = messages_lines
    .iter()
    .flat_map(|group| group.iter())
    .map(|line| line.chars().count() + MESSAGE_PREFIX_WIDTH)
    .max()
    .unwrap_or(MESSAGE_PREFIX_WIDTH);

  let sender_line_width = if incoming {
    sender.chars().count() + 3
  } else {
    0
  };

  let inner_width = longest_message_line
    .max(sender_line_width)
    .max(4)
    .min(max_bubble_width.saturating_sub(2));

  let bubble_width = inner_width + 2;

  // Top border.
  if incoming {
    let prefix = if sender.is_empty() {
      "╭─ ".to_string()
    } else {
      format!("╭─ {sender} ")
    };

    let prefix_width = prefix.chars().count();

    let remaining = inner_width.saturating_add(2).saturating_sub(prefix_width);

    let mut top = format!("{prefix}{}╮", "─".repeat(remaining),);

    top = fit_line(&top, bubble_width);

    lines.push(RenderedLine {
      alignment,
      text: top,
      color,
      message_id: None,
    });
  } else {
    lines.push(RenderedLine {
      alignment,
      text: format!("╭{}╮", "─".repeat(inner_width),),
      color: None,
      message_id: None,
    });
  }

  // Individual messages.
  //
  // A new `•` means a new WhatsApp message.
  // Two spaces indicate a continuation line.
  for (message_index, message_lines) in messages_lines.into_iter().enumerate() {
    let message_id = messages
      .get(message_index)
      .map(|message| message.id.clone());

    for (line_index, line) in message_lines.into_iter().enumerate() {
      let prefix = if line_index == 0 { "• " } else { "  " };

      let text_width = line.chars().count();

      let used_width = prefix.chars().count() + text_width;

      let padding = inner_width.saturating_sub(used_width);

      let rendered = format!("│{prefix}{line}{}│", " ".repeat(padding),);

      lines.push(RenderedLine {
        alignment,
        text: rendered,
        color: None,
        message_id: if line_index == 0 {
          message_id.clone()
        } else {
          None
        },
      });
    }
  }

  // Bottom border.
  lines.push(RenderedLine {
    alignment,
    text: format!("╰{}╯", "─".repeat(inner_width),),
    color: None,
    message_id: None,
  });
}

fn draw_line(
  stdout: &mut io::Stdout,
  line: &RenderedLine,
  terminal_width: usize,
) -> anyhow::Result<()> {
  let text_width = line.text.chars().count();

  let left_padding = match line.alignment {
    Alignment::Left => 0,

    Alignment::Right => terminal_width.saturating_sub(text_width),
  };

  write!(stdout, "{}", " ".repeat(left_padding),)?;

  if let Some(color) = line.color {
    execute!(stdout, SetForegroundColor(color),)?;
  }

  // Message lines are already wrapped to fit.
  write!(stdout, "{}", line.text)?;

  if line.color.is_some() {
    execute!(stdout, ResetColor)?;
  }

  write!(stdout, "\r\n")?;

  Ok(())
}

fn message_group_key(chat: &Chat, message: &Message) -> String {
  match message.direction {
    MessageDirection::Outgoing => "outgoing".to_string(),

    MessageDirection::Incoming => {
      if chat.is_group {
        message
          .author
          .clone()
          .filter(|author| !author.trim().is_empty())
          .unwrap_or_else(|| message.from.clone())
      } else if !chat.name.trim().is_empty() {
        chat.name.clone()
      } else {
        message.from.clone()
      }
    }
  }
}

fn sender_color(sender: &str) -> Color {
  let colors = [
    Color::Cyan,
    Color::Green,
    Color::Yellow,
    Color::Magenta,
    Color::Blue,
    Color::DarkCyan,
    Color::DarkGreen,
    Color::DarkYellow,
    Color::DarkMagenta,
    Color::DarkBlue,
  ];

  let mut hash = 0usize;

  for byte in sender.as_bytes() {
    hash = hash.wrapping_mul(31).wrapping_add(*byte as usize);
  }

  colors[hash % colors.len()]
}

fn message_body(message: &Message) -> String {
  let body = message
    .body
    .as_deref()
    .unwrap_or("")
    .replace('\r', "")
    .replace('\t', "  ");

  let body = body.trim();

  if message.kind.eq_ignore_ascii_case("text") {
    if body.is_empty() {
      return "[text]".to_string();
    }

    return body.to_string();
  }

  let kind = message.kind.trim();

  let label = if kind.is_empty() {
    "[message]".to_string()
  } else {
    format!("[{}]", kind.to_lowercase())
  };

  if body.is_empty() {
    label
  } else {
    format!("{label} {body}")
  }
}

fn wrap_text(text: &str, width: usize) -> Vec<String> {
  if width == 0 {
    return vec![String::new()];
  }

  let mut result = Vec::new();

  for paragraph in text.split('\n') {
    if paragraph.is_empty() {
      result.push(String::new());
      continue;
    }

    let words: Vec<&str> = paragraph.split_whitespace().collect();

    if words.is_empty() {
      result.push(String::new());
      continue;
    }

    let mut current = String::new();

    for word in words {
      let word_width = word.chars().count();

      if word_width > width {
        if !current.is_empty() {
          result.push(current.clone());
          current.clear();
        }

        let mut chunk = String::new();

        for ch in word.chars() {
          if chunk.chars().count() >= width {
            result.push(chunk.clone());
            chunk.clear();
          }

          chunk.push(ch);
        }

        current = chunk;
        continue;
      }

      let separator = if current.is_empty() { 0 } else { 1 };

      if current.chars().count() + separator + word_width <= width {
        if !current.is_empty() {
          current.push(' ');
        }

        current.push_str(word);
      } else {
        result.push(current.clone());
        current = word.to_string();
      }
    }

    if !current.is_empty() {
      result.push(current);
    }
  }

  result
}

fn fit_line(value: &str, width: usize) -> String {
  let value_width = value.chars().count();

  if value_width == width {
    return value.to_string();
  }

  if value_width < width {
    return format!("{}{}", value, " ".repeat(width - value_width),);
  }

  truncate_line(value, width)
}

fn truncate_line(value: &str, max_chars: usize) -> String {
  if max_chars == 0 {
    return String::new();
  }

  let mut chars = value.chars();

  let mut output = String::new();

  for _ in 0..max_chars {
    let Some(ch) = chars.next() else {
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

    let mut stdout = io::stdout();

    if let Err(error) = execute!(stdout, EnterAlternateScreen, cursor::Hide,) {
      let _ = terminal::disable_raw_mode();

      return Err(error.into());
    }

    Ok(Self { stdout })
  }
}

impl Drop for TerminalGuard {
  fn drop(&mut self) {
    let _ = execute!(self.stdout, cursor::Show, LeaveAlternateScreen,);

    let _ = terminal::disable_raw_mode();
  }
}

#[cfg(test)]
mod tests {
  use super::{
    capture_viewport_anchor, restore_viewport_anchor, Alignment, Composer, RenderedLine,
  };

  #[test]
  fn composer_inserts_and_moves_cursor() {
    let mut composer = Composer::default();

    composer.insert('a');
    composer.insert('b');
    composer.move_left();
    composer.insert('x');

    assert_eq!(composer.text(), "axb");
  }

  #[test]
  fn composer_backspace_deletes_before_cursor() {
    let mut composer = Composer::default();

    composer.insert('a');
    composer.insert('b');
    composer.backspace();

    assert_eq!(composer.text(), "a");
  }

  #[test]
  fn composer_delete_deletes_after_cursor() {
    let mut composer = Composer::default();

    composer.insert('a');
    composer.insert('b');
    composer.move_left();
    composer.delete();

    assert_eq!(composer.text(), "a");
  }

  #[test]
  fn composer_supports_unicode() {
    let mut composer = Composer::default();

    composer.insert('á');
    composer.insert('🙂');

    assert_eq!(composer.text(), "á🙂");
  }

  #[test]
  fn composer_preserves_newlines() {
    let mut composer = Composer::default();

    composer.insert('a');
    composer.insert('\n');
    composer.insert('b');

    assert_eq!(composer.text(), "a\nb");
  }

  fn line(message_id: Option<&str>) -> RenderedLine {
    RenderedLine {
      alignment: Alignment::Left,
      text: String::new(),
      color: None,
      message_id: message_id.map(str::to_string),
    }
  }

  #[test]
  fn viewport_anchor_preserves_message_screen_row() {
    let before = vec![
      line(None),
      line(Some("message-1")),
      line(None),
      line(Some("message-2")),
    ];

    let anchor = capture_viewport_anchor(&before, 0).expect("expected a viewport anchor");

    assert_eq!(anchor.message_id, "message-1");

    assert_eq!(anchor.screen_row, 1);

    let after = vec![
      line(None),
      line(None),
      line(None),
      line(Some("message-1")),
      line(None),
      line(Some("message-2")),
    ];

    let restored =
      restore_viewport_anchor(&after, &anchor).expect("expected the anchor to be restored");

    assert_eq!(restored, 2);

    assert_eq!(restored + anchor.screen_row, 3);
  }

  #[test]
  fn viewport_anchor_returns_none_when_no_messages_are_visible() {
    let content = vec![line(None), line(None), line(None)];

    assert!(capture_viewport_anchor(&content, 0).is_none());
  }

  #[test]
  fn viewport_anchor_returns_none_when_message_disappears() {
    let before = vec![line(None), line(Some("message-1"))];

    let anchor = capture_viewport_anchor(&before, 0).expect("expected a viewport anchor");

    let after = vec![line(None), line(Some("message-2"))];

    assert!(restore_viewport_anchor(&after, &anchor,).is_none());
  }
}
