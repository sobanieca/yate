//! A single-line text input, used by pickers, prompts and the find bar.

use crate::text;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use unicode_segmentation::UnicodeSegmentation;

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
                let end = self.byte(text::next_grapheme(&self.text, self.cursor));
                self.text.replace_range(start..end, "");
                Some(true)
            }
            KeyCode::Left => {
                self.cursor = text::prev_grapheme(&self.text, self.cursor);
                Some(false)
            }
            KeyCode::Right => {
                self.cursor = text::next_grapheme(&self.text, self.cursor);
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
        let previous = text::prev_grapheme(&self.text, self.cursor);
        let start = self.byte(previous);
        let end = self.byte(self.cursor);
        self.text.replace_range(start..end, "");
        self.cursor = previous;
        Some(true)
    }

    /// Display column of the cursor.
    pub fn cursor_col(&self) -> usize {
        let prefix: String = self.text.chars().take(self.cursor).collect();
        text::str_width(&prefix)
    }

    /// The chars scrolled out on the left so that the cursor fits in
    /// `width` columns, and their display width.
    fn scrolled(&self, width: usize) -> (usize, usize) {
        let mut col = self.cursor_col();
        let mut chars = 0;
        let mut columns = 0;
        for grapheme in self.text.graphemes(true) {
            if col < width || chars >= self.cursor {
                break;
            }
            let grapheme_width = text::grapheme_width(grapheme);
            col -= grapheme_width;
            columns += grapheme_width;
            chars += grapheme.chars().count();
        }
        (chars, columns)
    }

    /// The text that fits in `width` columns with the cursor in view, and
    /// the column of the cursor in it.
    pub fn visible(&self, width: usize) -> (String, usize) {
        let (skipped, columns) = self.scrolled(width);
        let shown = self.text.chars().skip(skipped).collect();
        (shown, self.cursor_col() - columns)
    }

    /// Puts the cursor at a clicked column of a field `width` columns wide.
    pub fn click(&mut self, col: usize, width: usize) {
        let (skipped, _) = self.scrolled(width);
        let shown: String = self.text.chars().skip(skipped).collect();
        self.cursor = skipped + text::display_col_to_char(&shown, col, 1);
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
    fn editing_keys_work_on_whole_emoji() {
        let mut input = LineInput::with_text("a👍🏽👨‍👩‍👧b");
        input.handle(key(KeyCode::Left));
        input.handle(key(KeyCode::Backspace));
        assert_eq!(input.text, "a👍🏽b");
        input.handle(key(KeyCode::Left));
        input.handle(key(KeyCode::Delete));
        assert_eq!(input.text, "ab");
        assert_eq!(input.cursor, 1);
    }

    #[test]
    fn wide_text_scrolls_by_columns() {
        let mut input = LineInput::with_text("🎉🎉🎉🎉");
        assert_eq!(input.visible(6), ("🎉🎉".to_string(), 4));
        input.click(2, 6);
        assert_eq!(input.cursor, 3);
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
