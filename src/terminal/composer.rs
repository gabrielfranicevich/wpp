use std::io::{self, Write};

#[derive(Debug, Default)]
pub(super) struct Composer {
  chars: Vec<char>,
  cursor: usize,
}

impl Composer {
  pub(super) fn text(&self) -> String {
    self.chars.iter().collect()
  }

  pub(super) fn insert(&mut self, character: char) {
    self.chars.insert(self.cursor, character);
    self.cursor += 1;
  }

  pub(super) fn backspace(&mut self) {
    if self.cursor == 0 {
      return;
    }

    self.cursor -= 1;
    self.chars.remove(self.cursor);
  }

  pub(super) fn delete(&mut self) {
    if self.cursor >= self.chars.len() {
      return;
    }

    self.chars.remove(self.cursor);
  }

  pub(super) fn move_left(&mut self) {
    self.cursor = self.cursor.saturating_sub(1);
  }

  pub(super) fn move_right(&mut self) {
    self.cursor = self.cursor.saturating_add(1).min(self.chars.len());
  }

  pub(super) fn move_home(&mut self) {
    self.cursor = 0;
  }

  pub(super) fn move_end(&mut self) {
    self.cursor = self.chars.len();
  }
}

pub(super) fn draw_composer(
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

#[cfg(test)]
mod tests {
  use super::Composer;

  #[test]
  fn inserts_and_moves_cursor() {
    let mut composer = Composer::default();

    composer.insert('a');
    composer.insert('b');
    composer.move_left();
    composer.insert('x');

    assert_eq!(composer.text(), "axb");
  }

  #[test]
  fn backspace_deletes_before_cursor() {
    let mut composer = Composer::default();

    composer.insert('a');
    composer.insert('b');
    composer.backspace();

    assert_eq!(composer.text(), "a");
  }

  #[test]
  fn delete_deletes_after_cursor() {
    let mut composer = Composer::default();

    composer.insert('a');
    composer.insert('b');
    composer.move_left();
    composer.delete();

    assert_eq!(composer.text(), "a");
  }

  #[test]
  fn supports_unicode() {
    let mut composer = Composer::default();

    composer.insert('á');
    composer.insert('🙂');

    assert_eq!(composer.text(), "á🙂");
  }

  #[test]
  fn preserves_newlines() {
    let mut composer = Composer::default();

    composer.insert('a');
    composer.insert('\n');
    composer.insert('b');

    assert_eq!(composer.text(), "a\nb");
  }
}
