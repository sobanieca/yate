//! Text helpers: display widths, grapheme boundaries and words on UTF-8
//! strings.

use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

pub fn grapheme_width(grapheme: &str) -> usize {
    if grapheme.starts_with(|c: char| c.is_control()) {
        return 1;
    }
    grapheme.width()
}

pub fn str_width(text: &str) -> usize {
    text.graphemes(true).map(grapheme_width).sum()
}

/// Width of a grapheme at display column `col`, with tabs expanded.
pub fn display_width(grapheme: &str, col: usize, tab_size: usize) -> usize {
    if grapheme == "\t" {
        tab_size - col % tab_size
    } else {
        grapheme_width(grapheme)
    }
}

/// Display column of char index `char_col` in `line`.
pub fn char_to_display_col(line: &str, char_col: usize, tab_size: usize) -> usize {
    let mut col = 0;
    let mut chars = 0;
    for grapheme in line.graphemes(true) {
        if chars >= char_col {
            break;
        }
        col += display_width(grapheme, col, tab_size);
        chars += grapheme.chars().count();
    }
    col
}

/// The char index in `line` whose grapheme covers display column `target`.
pub fn display_col_to_char(line: &str, target: usize, tab_size: usize) -> usize {
    let mut col = 0;
    let mut chars = 0;
    for grapheme in line.graphemes(true) {
        let width = display_width(grapheme, col, tab_size);
        if col + width > target {
            return chars;
        }
        col += width;
        chars += grapheme.chars().count();
    }
    chars
}

/// Char index of the grapheme boundary before `char_col`.
pub fn prev_grapheme(line: &str, char_col: usize) -> usize {
    let mut prev = 0;
    let mut chars = 0;
    for grapheme in line.graphemes(true) {
        if chars >= char_col {
            break;
        }
        prev = chars;
        chars += grapheme.chars().count();
    }
    prev
}

/// Char index of the grapheme boundary after `char_col`.
pub fn next_grapheme(line: &str, char_col: usize) -> usize {
    let mut chars = 0;
    for grapheme in line.graphemes(true) {
        chars += grapheme.chars().count();
        if chars > char_col {
            return chars;
        }
    }
    chars
}

#[derive(PartialEq, Eq, Clone, Copy)]
enum CharClass {
    Space,
    Word,
    Punct,
}

fn class(c: char) -> CharClass {
    if c.is_whitespace() {
        CharClass::Space
    } else if c.is_alphanumeric() || c == '_' {
        CharClass::Word
    } else {
        CharClass::Punct
    }
}

pub fn is_word_char(c: char) -> bool {
    class(c) == CharClass::Word
}

/// The word around `col`, as a char range.
pub fn word_at(line: &[char], col: usize) -> Option<(usize, usize)> {
    let at = if col < line.len() && is_word_char(line[col]) {
        col
    } else if col > 0 && col <= line.len() && is_word_char(line[col - 1]) {
        col - 1
    } else {
        return None;
    };
    let mut start = at;
    while start > 0 && is_word_char(line[start - 1]) {
        start -= 1;
    }
    let mut end = at;
    while end < line.len() && is_word_char(line[end]) {
        end += 1;
    }
    Some((start, end))
}

pub fn leading_whitespace(line: &str) -> &str {
    let end = line
        .find(|c: char| c != ' ' && c != '\t')
        .unwrap_or(line.len());
    &line[..end]
}

/// Display width of the indentation of `line`.
pub fn indent_width(line: &str, tab_size: usize) -> usize {
    let mut col = 0;
    for c in leading_whitespace(line).chars() {
        col += if c == '\t' {
            tab_size - col % tab_size
        } else {
            1
        };
    }
    col
}

pub fn truncate_to_width(text: &str, width: usize) -> String {
    let mut out = String::new();
    let mut used = 0;
    for grapheme in text.graphemes(true) {
        let w = grapheme_width(grapheme);
        if used + w > width {
            break;
        }
        used += w;
        out.push_str(grapheme);
    }
    out
}

/// Shortens `text` from the left with an ellipsis so it fits `width`.
pub fn truncate_left(text: &str, width: usize) -> String {
    if str_width(text) <= width {
        return text.to_string();
    }
    if width == 0 {
        return String::new();
    }
    let graphemes: Vec<&str> = text.graphemes(true).collect();
    let mut used = 1;
    let mut start = graphemes.len();
    while start > 0 {
        let w = grapheme_width(graphemes[start - 1]);
        if used + w > width {
            break;
        }
        used += w;
        start -= 1;
    }
    format!("…{}", graphemes[start..].concat())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_columns_expand_tabs() {
        assert_eq!(char_to_display_col("\tab", 1, 4), 4);
        assert_eq!(char_to_display_col("a\tb", 2, 4), 4);
        assert_eq!(display_col_to_char("\tab", 5, 4), 2);
        assert_eq!(display_col_to_char("\tab", 2, 4), 0);
    }

    #[test]
    fn graphemes_are_stepped_as_a_whole() {
        let line = "ae\u{301}b";
        assert_eq!(next_grapheme(line, 1), 3);
        assert_eq!(prev_grapheme(line, 3), 1);
    }

    #[test]
    fn word_under_the_cursor() {
        let line: Vec<char> = "foo.bar  baz".chars().collect();
        assert_eq!(word_at(&line, 5), Some((4, 7)));
    }

    #[test]
    fn truncation_keeps_the_end() {
        assert_eq!(truncate_left("src/editor/view.rs", 10), "…r/view.rs");
        assert_eq!(truncate_to_width("abcdef", 3), "abc");
    }
}
