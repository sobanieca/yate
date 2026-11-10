//! Syntax trees kept in sync with the document by incremental parsing, and
//! highlighting of byte ranges with the language's tree-sitter query.

pub mod definitions;
pub mod languages;

use crate::ui::surface::Style;
use languages::LanguageConfig;
use ropey::{Rope, RopeSlice};
use std::ops::Range;
use std::time::{Duration, Instant};
use tree_sitter::{
    InputEdit, Node, ParseOptions, ParseState, Parser, Point, QueryCursor, StreamingIterator,
    TextProvider, Tree,
};

/// Files larger than this are shown without highlighting.
pub const MAX_HIGHLIGHT_BYTES: usize = 8 * 1024 * 1024;
const PARSE_TIMEOUT: Duration = Duration::from_millis(800);

pub struct Syntax {
    pub language: &'static LanguageConfig,
    parser: Parser,
    tree: Option<Tree>,
    stale: bool,
}

impl Syntax {
    pub fn new(language: &'static LanguageConfig, rope: &Rope) -> Option<Syntax> {
        language.highlighting()?;
        if rope.len_bytes() > MAX_HIGHLIGHT_BYTES {
            return None;
        }
        let mut parser = Parser::new();
        parser.set_language(&language.language()).ok()?;
        let mut syntax = Syntax {
            language,
            parser,
            tree: None,
            stale: true,
        };
        syntax.update(rope);
        Some(syntax)
    }

    pub fn edit(&mut self, edit: &InputEdit) {
        if let Some(tree) = &mut self.tree {
            tree.edit(edit);
        }
        self.stale = true;
    }

    /// Reparses after edits, reusing the unchanged parts of the old tree.
    pub fn update(&mut self, rope: &Rope) {
        if !self.stale {
            return;
        }
        self.stale = false;
        let tree = parse_rope(&mut self.parser, rope, self.tree.as_ref());
        self.tree = tree;
    }

    pub fn tree(&self) -> Option<&Tree> {
        self.tree.as_ref()
    }

    /// Styles for the bytes in `range`: `result[i]` styles byte
    /// `range.start + i`. `capture_styles` comes from
    /// `Theme::capture_styles` for this language.
    pub fn highlight(
        &self,
        rope: &Rope,
        range: Range<usize>,
        capture_styles: &[Option<Style>],
    ) -> Vec<Option<Style>> {
        let mut styles = vec![None; range.len()];
        let (Some(tree), Some(highlighting)) = (&self.tree, self.language.highlighting()) else {
            return styles;
        };
        paint(
            highlighting,
            tree.root_node(),
            RopeProvider(rope.slice(..)),
            range,
            capture_styles,
            &mut styles,
        );
        styles
    }
}

fn parse_rope(parser: &mut Parser, rope: &Rope, old_tree: Option<&Tree>) -> Option<Tree> {
    let started = Instant::now();
    let mut progress = |_: &ParseState| started.elapsed() > PARSE_TIMEOUT;
    let options = ParseOptions::new().progress_callback(&mut progress);
    let mut read = |byte: usize, _: Point| -> &[u8] {
        if byte >= rope.len_bytes() {
            return &[];
        }
        let (chunk, chunk_start, _, _) = rope.chunk_at_byte(byte);
        &chunk.as_bytes()[byte - chunk_start..]
    };
    parser.parse_with_options(&mut read, old_tree, Some(options))
}

struct RopeProvider<'a>(RopeSlice<'a>);

impl<'a> TextProvider<&'a [u8]> for RopeProvider<'a> {
    type I = std::iter::Map<ropey::iter::Chunks<'a>, fn(&'a str) -> &'a [u8]>;

    fn text(&mut self, node: Node) -> Self::I {
        self.0
            .byte_slice(node.start_byte()..node.end_byte())
            .chunks()
            .map(str::as_bytes as fn(&'a str) -> &'a [u8])
    }
}

struct Capture {
    start: usize,
    end: usize,
    pattern: usize,
    style: Style,
}

/// Paints captures so that inner nodes win over outer ones and, for the same
/// node, the earlier pattern wins (the tree-sitter-highlight convention).
fn paint<'a, T: TextProvider<I>, I: AsRef<[u8]>>(
    highlighting: &languages::Highlighting,
    root: Node<'a>,
    provider: T,
    range: Range<usize>,
    capture_styles: &[Option<Style>],
    styles: &mut [Option<Style>],
) {
    let mut cursor = QueryCursor::new();
    cursor.set_byte_range(range.clone());
    cursor.set_match_limit(256);
    let mut captures = Vec::new();
    let mut matches = cursor.matches(&highlighting.query, root, provider);
    while let Some(found) = matches.next() {
        for capture in found.captures {
            let Some(Some(style)) = capture_styles.get(capture.index as usize).copied() else {
                continue;
            };
            captures.push(Capture {
                start: capture.node.start_byte(),
                end: capture.node.end_byte(),
                pattern: found.pattern_index,
                style,
            });
        }
    }
    captures.sort_by(|a, b| {
        (b.end - b.start)
            .cmp(&(a.end - a.start))
            .then(b.pattern.cmp(&a.pattern))
    });
    for capture in captures {
        let start = capture.start.max(range.start) - range.start;
        let end = capture.end.min(range.end).saturating_sub(range.start);
        for slot in styles.iter_mut().take(end).skip(start) {
            *slot = Some(capture.style);
        }
    }
}

/// Highlights a whole text once (previews, diffs): styles per line, indexed
/// by byte within the line.
pub fn highlight_text(
    language: &'static LanguageConfig,
    text: &str,
    theme: &crate::ui::theme::Theme,
) -> Vec<Vec<Option<Style>>> {
    let lines: Vec<&str> = text.split('\n').collect();
    let mut result: Vec<Vec<Option<Style>>> =
        lines.iter().map(|line| vec![None; line.len()]).collect();
    if text.len() > MAX_HIGHLIGHT_BYTES / 4 {
        return result;
    }
    let Some(highlighting) = language.highlighting() else {
        return result;
    };
    let Some(tree) = parse_text(language, text) else {
        return result;
    };
    let mut styles = vec![None; text.len()];
    paint(
        highlighting,
        tree.root_node(),
        text.as_bytes(),
        0..text.len(),
        &theme.capture_styles(language),
        &mut styles,
    );
    let mut offset = 0;
    for (line, line_styles) in lines.iter().zip(result.iter_mut()) {
        line_styles.copy_from_slice(&styles[offset..offset + line.len()]);
        offset += line.len() + 1;
    }
    result
}

/// Parses a whole text once, giving up after `PARSE_TIMEOUT`.
pub fn parse_text(language: &LanguageConfig, text: &str) -> Option<Tree> {
    let mut parser = Parser::new();
    parser.set_language(&language.language()).ok()?;
    let started = Instant::now();
    let mut progress = |_: &ParseState| started.elapsed() > PARSE_TIMEOUT;
    let options = ParseOptions::new().progress_callback(&mut progress);
    let bytes = text.as_bytes();
    let mut read = |byte: usize, _: Point| -> &[u8] { bytes.get(byte..).unwrap_or(&[]) };
    parser.parse_with_options(&mut read, None, Some(options))
}

/// The tree-sitter edit for replacing chars `start..end` of `rope` with
/// `text`. Call before applying the change to the rope; `finish_edit` fills
/// in the new end once it has been applied.
pub fn begin_edit(rope: &Rope, start: usize, end: usize) -> InputEdit {
    let start_byte = rope.char_to_byte(start);
    let old_end_byte = rope.char_to_byte(end);
    InputEdit {
        start_byte,
        old_end_byte,
        new_end_byte: start_byte,
        start_position: point(rope, start_byte),
        old_end_position: point(rope, old_end_byte),
        new_end_position: point(rope, start_byte),
    }
}

pub fn finish_edit(rope: &Rope, edit: &mut InputEdit, inserted_bytes: usize) {
    edit.new_end_byte = edit.start_byte + inserted_bytes;
    edit.new_end_position = point(rope, edit.new_end_byte);
}

fn point(rope: &Rope, byte: usize) -> Point {
    let row = rope.byte_to_line(byte);
    Point::new(row, byte - rope.line_to_byte(row))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn incremental_edits_keep_highlighting_right() {
        let language = languages::by_name("javascript").unwrap();
        let mut rope = Rope::from_str("let a = 1;\n");
        let mut syntax = Syntax::new(language, &rope).unwrap();
        let mut edit = begin_edit(&rope, 0, 0);
        rope.insert(0, "const s = \"x\";\n");
        finish_edit(&rope, &mut edit, "const s = \"x\";\n".len());
        syntax.edit(&edit);
        syntax.update(&rope);
        let theme = crate::ui::theme::Theme::default();
        let styles = syntax.highlight(&rope, 0..rope.len_bytes(), &theme.capture_styles(language));
        let string_style = theme.capture_style("string");
        assert_eq!(styles[10], string_style);
        assert_eq!(styles[15 + 4], theme.capture_style("variable"));
    }

    #[test]
    fn whole_text_highlighting_splits_lines() {
        let language = languages::by_name("rust").unwrap();
        let theme = crate::ui::theme::Theme::default();
        let lines = highlight_text(language, "fn main() {\n    // hi\n}", &theme);
        assert_eq!(lines.len(), 3);
        assert_eq!(lines[1][4], theme.capture_style("comment"));
    }
}
