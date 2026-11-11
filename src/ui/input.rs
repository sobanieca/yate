//! A single-line text input, used by pickers, prompts and the find bar.

use crate::text;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

#[derive(Clone, Default, Debug)]
pub struct LineInput {
    pub text: String,
    /// Cursor as a char index.
    pub cursor: usize,
    /// The whole text is selected (prefilled), typing replaces it.
    pub selected: bool,
}

impl LineInput {
    pub fn with_text(value: &str) -> Self {
        LineInput {
            text: value.to_string(),
            cursor: value.chars().count(),
            selected: false,
        }
    }

    pub fn set(&mut self, value: &str) {
        self.text = value.to_string();
        self.cursor = value.chars().count();
        self.selected = false;
    }

    /// Prefills the input with `value` selected, so typing replaces it.
    pub fn prefill(&mut self, value: &str) {
        self.set(value);
        self.selected = !value.is_empty();
    }

    fn byte(&self, char_index: usize) -> usize {
        self.text
            .char_indices()
            .nth(char_index)
            .map(|(i, _)| i)
            .unwrap_or(self.text.len())
    }

    pub fn insert(&mut self, value: &str) {
        if self.selected {
            self.text.clear();
            self.cursor = 0;
            self.selected = false;
        }
        let value: String = value.chars().filter(|c| *c != '\n' && *c != '\r').collect();
        let at = self.byte(self.cursor);
        self.text.insert_str(at, &value);
        self.cursor += value.chars().count();
    }

    /// Handles editing keys. Returns true when the text changed; keys it does
    /// not know are left to the caller.
    pub fn handle(&mut self, key: KeyEvent) -> Option<bool> {
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        if self.selected {
            match key.code {
                KeyCode::Backspace | KeyCode::Delete => {
                    self.set("");
                    return Some(true);
                }
                KeyCode::Char(_) if !ctrl => {}
                _ => self.selected = false,
            }
        }
        let chars: Vec<char> = self.text.chars().collect();
        match key.code {
            KeyCode::Char('u') if ctrl => {
                let changed = !self.text.is_empty();
                self.text.clear();
                self.cursor = 0;
                Some(changed)
            }
            KeyCode::Char('a') if ctrl => {
                self.cursor = 0;
                Some(false)
            }
            KeyCode::Char('e') if ctrl => {
                self.cursor = chars.len();
                Some(false)
            }
            KeyCode::Char(c) if !ctrl => {
                self.insert(&c.to_string());
                Some(true)
            }
            KeyCode::Backspace => self.backspace(),
            KeyCode::Char('h') if ctrl => self.backspace(),
            KeyCode::Delete => {
                if self.cursor >= chars.len() {
                    return Some(false);
                }
                let start = self.byte(self.cursor);
                let end = self.byte(self.cursor + 1);
                self.text.replace_range(start..end, "");
                Some(true)
            }
            KeyCode::Left => {
                self.cursor = self.cursor.saturating_sub(1);
                Some(false)
            }
            KeyCode::Right => {
                self.cursor = (self.cursor + 1).min(chars.len());
                Some(false)
            }
            KeyCode::Home => {
                self.cursor = 0;
                Some(false)
            }
            KeyCode::End => {
                self.cursor = chars.len();
                Some(false)
            }
            _ => None,
        }
    }

    fn backspace(&mut self) -> Option<bool> {
        if self.cursor == 0 {
            return Some(false);
        }
        let start = self.byte(self.cursor - 1);
        let end = self.byte(self.cursor);
        self.text.replace_range(start..end, "");
        self.cursor -= 1;
        Some(true)
    }

    /// Display column of the cursor.
    pub fn cursor_col(&self) -> usize {
        let prefix: String = self.text.chars().take(self.cursor).collect();
        text::str_width(&prefix)
    }

    fn chars_scrolled(&self, width: usize) -> usize {
        (self.cursor_col() + 1).saturating_sub(width)
    }

    /// The text that fits in `width` columns with the cursor in view, and
    /// the column of the cursor in it.
    pub fn visible(&self, width: usize) -> (String, usize) {
        let skipped = self.chars_scrolled(width);
        let shown = self.text.chars().skip(skipped).collect();
        let cursor = self.cursor_col().min(width.saturating_sub(1));
        (shown, cursor)
    }

    /// Puts the cursor at a clicked column of a field `width` columns wide.
    pub fn click(&mut self, col: usize, width: usize) {
        let at = self.chars_scrolled(width) + col;
        self.cursor = at.min(self.text.chars().count());
        self.selected = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    #[test]
    fn editing_keys_work_on_chars() {
        let mut input = LineInput::default();
        for c in "zaż".chars() {
            input.handle(key(KeyCode::Char(c)));
        }
        input.handle(key(KeyCode::Left));
        input.handle(key(KeyCode::Backspace));
        assert_eq!(input.text, "zż");
        assert_eq!(input.cursor, 1);
        input.handle(KeyEvent::new(KeyCode::Char('u'), KeyModifiers::CONTROL));
        assert_eq!(input.text, "");
    }

    #[test]
    fn long_text_scrolls_to_keep_the_cursor_in_view() {
        let mut input = LineInput::with_text("abcdefghij");
        assert_eq!(input.visible(6), ("fghij".to_string(), 5));
        input.click(1, 6);
        assert_eq!(input.cursor, 6);
        assert_eq!(input.visible(20), ("abcdefghij".to_string(), 6));
    }

    #[test]
    fn prefilled_text_is_replaced_by_typing() {
        let mut input = LineInput::default();
        input.prefill("old");
        input.handle(key(KeyCode::Char('n')));
        assert_eq!(input.text, "n");
        input.prefill("old");
        input.handle(key(KeyCode::End));
        input.handle(key(KeyCode::Char('!')));
        assert_eq!(input.text, "old!");
    }
}
