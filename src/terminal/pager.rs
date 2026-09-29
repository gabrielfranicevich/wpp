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
use crate::whatsapp::models::{Chat, Message, MessageDirection};

const BUBBLE_MAX_WIDTH_RATIO: usize = 60;
const BUBBLE_MIN_WIDTH: usize = 12;
const MESSAGE_PREFIX_WIDTH: usize = 2;

pub async fn run(chat: &Chat, pager: &mut MessagePager<'_>) -> anyhow::Result<()> {
    let mut terminal = TerminalGuard::enter()?;

    let result = run_loop(&mut terminal.stdout, chat, pager).await;

    drop(terminal);

    result
}

async fn run_loop(
    stdout: &mut io::Stdout,
    chat: &Chat,
    pager: &mut MessagePager<'_>,
) -> anyhow::Result<()> {
    // Draw once when entering the pager.
    let (mut scroll_top, mut max_scroll, mut body_height) = draw(stdout, chat, pager, usize::MAX)?;

    loop {
        if !event::poll(Duration::from_millis(250))? {
            // Nothing changed. Do NOT redraw.
            continue;
        }

        let event = event::read()?;

        let mut redraw = false;

        match event {
            Event::Resize(_, _) => {
                redraw = true;
            }

            Event::Key(key) => {
                match key.code {
                    KeyCode::Esc | KeyCode::Char('q') => {
                        break;
                    }

                    KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                        break;
                    }

                    KeyCode::Up | KeyCode::Char('k') => {
                        if scroll_top > 0 {
                            scroll_top -= 1;
                            redraw = true;
                        } else if !pager.exhausted() {
                            // Measure the rendered height before loading older
                            // messages so the viewport can stay anchored.
                            let before_lines = render_messages(
                                chat,
                                pager.messages(),
                                terminal::size()?.0.max(1) as usize,
                            )
                            .len();

                            pager.load_older().await?;

                            let after_lines = render_messages(
                                chat,
                                pager.messages(),
                                terminal::size()?.0.max(1) as usize,
                            )
                            .len();

                            let added_lines = after_lines.saturating_sub(before_lines);

                            if added_lines > 0 {
                                scroll_top = added_lines;
                                redraw = true;
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
                        let loaded = pager.load_all_older().await?;

                        scroll_top = 0;

                        if loaded > 0 || scroll_top != 0 {
                            redraw = true;
                        } else {
                            redraw = true;
                        }
                    }

                    KeyCode::End => {
                        if scroll_top != max_scroll {
                            scroll_top = max_scroll;
                            redraw = true;
                        }
                    }

                    _ => {}
                }
            }

            _ => {}
        }

        if redraw {
            let result = draw(stdout, chat, pager, scroll_top)?;

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
) -> anyhow::Result<(usize, usize, usize)> {
    let (terminal_width, terminal_height) = terminal::size()?;

    let width = terminal_width.max(1) as usize;

    let height = terminal_height as usize;

    // Header: 2 lines.
    // Footer: 2 lines.
    let body_height = height.saturating_sub(4);

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
        &format!(" {}  ({} loaded)", title, pager.messages().len(),),
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

    let footer = " ↑↓ / j/k   PgUp/PgDn   Home/End   q/Esc ";

    write!(stdout, "{}", truncate_line(footer, width,),)?;

    stdout.flush()?;

    Ok((scroll_top, max_scroll, body_height))
}

#[derive(Debug, Clone)]
struct RenderedLine {
    alignment: Alignment,
    text: String,
    color: Option<Color>,
}

#[derive(Debug, Clone, Copy)]
enum Alignment {
    Left,
    Right,
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
     *   ╭─ Martina ─────────╮
     *   │• Hola             │
     *   │  Segunda línea    │
     *   │• Otro mensaje     │
     *   ╰───────────────────╯
     *
     * The body width accounts for:
     *
     *   2 columns -> border characters
     *   2 columns -> "• " / "  "
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

        let mut top = format!("{prefix}{}╮", "─".repeat(remaining,),);

        // Sender names are allowed to be long,
        // but the bubble itself must remain inside
        // the terminal.
        top = fit_line(&top, bubble_width);

        lines.push(RenderedLine {
            alignment,
            text: top,
            color,
        });
    } else {
        lines.push(RenderedLine {
            alignment,
            text: format!("╭{}╮", "─".repeat(inner_width,),),
            color: None,
        });
    }

    // Individual messages.
    //
    // A new `•` means a new WhatsApp message.
    // A line starting with two spaces is just
    // a continuation of that same message.
    for message in messages_lines {
        for (line_index, line) in message.into_iter().enumerate() {
            let prefix = if line_index == 0 { "• " } else { "  " };

            let text_width = line.chars().count();

            let used_width = prefix.chars().count() + text_width;

            let padding = inner_width.saturating_sub(used_width);

            let rendered = format!("│{prefix}{line}{}│", " ".repeat(padding,),);

            lines.push(RenderedLine {
                alignment,
                text: rendered,
                color: None,
            });
        }
    }

    // Bottom border.
    lines.push(RenderedLine {
        alignment,
        text: format!("╰{}╯", "─".repeat(inner_width,),),
        color: None,
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

    write!(stdout, "{}", " ".repeat(left_padding,),)?;

    if let Some(color) = line.color {
        execute!(stdout, SetForegroundColor(color),)?;
    }

    // IMPORTANT:
    //
    // Do not truncate message lines here.
    //
    // `wrap_text()` has already made sure the message
    // fits inside the bubble. Truncating here would turn
    // a perfectly valid long message into:
    //
    //   │• Q tengas un dia maravill…│
    //
    // which loses actual message content.
    write!(stdout, "{}", line.text,)?;

    if line.color.is_some() {
        execute!(stdout, ResetColor,)?;
    }

    write!(stdout, "\r\n",)?;

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
        .replace('\n', "\n")
        .replace('\t', "    ");

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
        format!("[{}]", kind.to_lowercase(),)
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

            // Break extremely long words instead
            // of truncating them.
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
        return format!("{value}{}", " ".repeat(width - value_width,),);
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
