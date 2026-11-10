//! Editing commands and cursor motions on a document, VS Code style.

use super::document::{Change, Document};
use super::history::EditKind;
use super::selection::Selection;
use crate::config::Options;
use crate::text;

const PAIRS: &[(char, char)] = &[
    ('(', ')'),
    ('[', ']'),
    ('{', '}'),
    ('"', '"'),
    ('\'', '\''),
    ('`', '`'),
];

fn closing_for(open: char) -> Option<char> {
    PAIRS.iter().find(|(o, _)| *o == open).map(|(_, c)| *c)
}

fn is_closing(c: char) -> bool {
    PAIRS.iter().any(|(o, close)| *close == c && *o != c)
}

pub fn char_at(doc: &Document, pos: usize) -> Option<char> {
    (pos < doc.rope.len_chars()).then(|| doc.rope.char(pos))
}

pub fn indent_unit(options: &Options) -> String {
    if options.insert_spaces {
        " ".repeat(options.tab_size)
    } else {
        "\t".to_string()
    }
}

/// Replaces the selection with `text`.
pub fn insert_text(doc: &mut Document, text: &str, kind: EditKind) {
    let range = doc.selection.range();
    let end = range.start + text.chars().count();
    doc.apply(
        vec![Change::new(range.start, range.end, text)],
        Selection::point(end),
        kind,
    );
}

/// Types one character, with auto-closing of brackets and quotes.
pub fn type_char(doc: &mut Document, c: char, options: &Options) {
    let head = doc.selection.head;
    let next = char_at(doc, head);
    if options.auto_pairs && doc.selection.is_empty() {
        let prev = head.checked_sub(1).and_then(|p| char_at(doc, p));
        if next == Some(c) && (is_closing(c) || closing_for(c) == Some(c)) {
            doc.selection = Selection::point(head + 1);
            doc.history.seal();
            return;
        }
        if let Some(close) = closing_for(c) {
            let next_allows =
                next.is_none_or(|n| n.is_whitespace() || is_closing(n) || n == ',' || n == ';');
            let is_quote = close == c;
            let prev_allows = !is_quote || prev.is_none_or(|p| !text::is_word_char(p) && p != c);
            if next_allows && prev_allows {
                doc.apply(
                    vec![Change::insert(head, format!("{c}{close}"))],
                    Selection::point(head + 1),
                    EditKind::Insert,
                );
                return;
            }
        }
    }
    if options.auto_pairs
        && !doc.selection.is_empty()
        && let Some(close) = closing_for(c)
    {
        let range = doc.selection.range();
        let selected = doc.selected_text();
        let wrapped = format!("{c}{selected}{close}");
        doc.apply(
            vec![Change::new(range.start, range.end, wrapped)],
            Selection::new(range.start + 1, range.end + 1),
            EditKind::Other,
        );
        return;
    }
    insert_text(doc, &c.to_string(), EditKind::Insert);
}

/// Enter: keeps the indentation and indents after an opening bracket.
pub fn newline(doc: &mut Document, options: &Options) {
    let start = doc.selection.start();
    let line = doc.line_of(start);
    let line_text = doc.line(line).into_owned();
    let indent = text::leading_whitespace(&line_text).to_string();
    let before = start.checked_sub(1).and_then(|p| char_at(doc, p));
    let after = char_at(doc, doc.selection.end());
    let opens = matches!(before, Some('{' | '[' | '('))
        || (before == Some(':') && doc.language.is_some_and(|l| l.name == "python"))
        || (before == Some('>')
            && doc.language.is_some_and(|l| {
                matches!(l.name, "html" | "xml" | "svelte" | "tsx" | "javascript")
            })
            && after == Some('<'));
    let unit = indent_unit(options);
    let mut inserted = format!("\n{indent}");
    let mut cursor_offset = inserted.chars().count();
    if opens {
        inserted.push_str(&unit);
        cursor_offset += unit.chars().count();
        let closes = matches!(
            (before, after),
            (Some('{'), Some('}'))
                | (Some('['), Some(']'))
                | (Some('('), Some(')'))
                | (Some('>'), Some('<'))
        );
        if closes {
            inserted.push('\n');
            inserted.push_str(&indent);
        }
    }
    let range = doc.selection.range();
    let mut range_end = range.end;
    if doc.selection.is_empty() {
        while char_at(doc, range_end).is_some_and(|c| c == ' ' || c == '\t') && !opens {
            range_end += 1;
        }
    }
    doc.apply(
        vec![Change::new(range.start, range_end, inserted)],
        Selection::point(range.start + cursor_offset),
        EditKind::Other,
    );
}

pub fn backspace(doc: &mut Document, options: &Options) {
    if !doc.selection.is_empty() {
        delete_selection(doc);
        return;
    }
    let head = doc.selection.head;
    if head == 0 {
        return;
    }
    let (line, col) = doc.line_col(head);
    let line_text = doc.line(line).into_owned();
    let prev = char_at(doc, head - 1);
    let next = char_at(doc, head);
    if options.auto_pairs
        && let (Some(p), Some(n)) = (prev, next)
        && closing_for(p) == Some(n)
    {
        doc.apply(
            vec![Change::new(head - 1, head + 1, "")],
            Selection::point(head - 1),
            EditKind::Delete,
        );
        return;
    }
    let leading = text::leading_whitespace(&line_text).chars().count();
    let start = if options.insert_spaces
        && col <= leading
        && col > 0
        && line_text[..col].chars().all(|c| c == ' ')
    {
        let target = (col - 1) / options.tab_size * options.tab_size;
        head - (col - target)
    } else if col == 0 {
        head - 1
    } else {
        doc.line_start(line) + text::prev_grapheme(&line_text, col)
    };
    doc.apply(
        vec![Change::new(start, head, "")],
        Selection::point(start),
        EditKind::Delete,
    );
}

pub fn delete_forward(doc: &mut Document) {
    if !doc.selection.is_empty() {
        delete_selection(doc);
        return;
    }
    let head = doc.selection.head;
    if head >= doc.rope.len_chars() {
        return;
    }
    let (line, col) = doc.line_col(head);
    let end = if col >= doc.line_len(line) {
        head + 1
    } else {
        doc.line_start(line) + text::next_grapheme(&doc.line(line), col)
    };
    doc.apply(
        vec![Change::new(head, end, "")],
        Selection::point(head),
        EditKind::Delete,
    );
}

pub fn delete_selection(doc: &mut Document) {
    let range = doc.selection.range();
    doc.apply(
        vec![Change::new(range.start, range.end, "")],
        Selection::point(range.start),
        EditKind::Other,
    );
}

pub fn selected_lines(doc: &Document) -> (usize, usize) {
    let sel = doc.selection;
    let first = doc.line_of(sel.start());
    let mut last = doc.line_of(sel.end());
    if last > first && doc.line_start(last) == sel.end() {
        last -= 1;
    }
    (first, last)
}

/// Shifts the selection back to the same text after lines changed.
fn shifted(sel: Selection, at: usize, delta: isize) -> Selection {
    let shift = |pos: usize| {
        if pos >= at {
            (pos as isize + delta).max(at as isize) as usize
        } else {
            pos
        }
    };
    Selection::new(shift(sel.anchor), shift(sel.head))
}

/// Tab: inserts an indent, or indents all selected lines.
pub fn tab(doc: &mut Document, options: &Options) {
    let (first, last) = selected_lines(doc);
    if doc.selection.is_empty() || first == last && !spans_line(doc) {
        let unit = if options.insert_spaces {
            let (line, col) = doc.line_col(doc.selection.start());
            let display = text::char_to_display_col(&doc.line(line), col, options.tab_size);
            " ".repeat(options.tab_size - display % options.tab_size)
        } else {
            "\t".to_string()
        };
        insert_text(doc, &unit, EditKind::Insert);
        return;
    }
    indent_lines(doc, first, last, options);
}

fn spans_line(doc: &Document) -> bool {
    let sel = doc.selection;
    let line = doc.line_of(sel.start());
    sel.start() == doc.line_start(line) && sel.end() >= doc.line_end(line)
}

pub fn indent_lines(doc: &mut Document, first: usize, last: usize, options: &Options) {
    let unit = indent_unit(options);
    let unit_len = unit.chars().count() as isize;
    let mut sel = doc.selection;
    let mut changes = Vec::new();
    for line in (first..=last).rev() {
        if doc.line_len(line) == 0 {
            continue;
        }
        let at = doc.line_start(line);
        changes.push(Change::insert(at, unit.clone()));
        sel = shifted(sel, at, unit_len);
    }
    doc.apply(changes, sel, EditKind::Other);
}

pub fn outdent(doc: &mut Document, options: &Options) {
    let (first, last) = selected_lines(doc);
    let mut sel = doc.selection;
    let mut changes = Vec::new();
    for line in (first..=last).rev() {
        let line_text = doc.line(line);
        let mut remove = 0;
        let mut width = 0;
        for c in line_text.chars() {
            if width >= options.tab_size {
                break;
            }
            match c {
                ' ' => width += 1,
                '\t' => width = options.tab_size,
                _ => break,
            }
            remove += 1;
        }
        if remove == 0 {
            continue;
        }
        let at = doc.line_start(line);
        changes.push(Change::new(at, at + remove, ""));
        sel = shifted(sel, at, -(remove as isize));
    }
    doc.apply(changes, sel, EditKind::Other);
}

/// Toggles line comments (or block comments per line) on the selected lines.
pub fn toggle_comment(doc: &mut Document) {
    let Some(language) = doc.language else {
        return;
    };
    let (first, last) = selected_lines(doc);
    let (open, close) = match (language.line_comment, language.block_comment) {
        (Some(line), _) => (line.to_string(), String::new()),
        (None, Some((open, close))) => (open.to_string(), close.to_string()),
        (None, None) => return,
    };
    let lines: Vec<String> = (first..=last).map(|l| doc.line(l).into_owned()).collect();
    let non_blank: Vec<&String> = lines.iter().filter(|l| !l.trim().is_empty()).collect();
    if non_blank.is_empty() {
        return;
    }
    let all_commented = non_blank.iter().all(|line| {
        let trimmed = line.trim_start();
        trimmed.starts_with(&open) && trimmed.trim_end().ends_with(&close)
    });
    let min_indent = non_blank
        .iter()
        .map(|line| text::leading_whitespace(line).chars().count())
        .min()
        .unwrap_or(0);
    let mut sel = doc.selection;
    let mut changes = Vec::new();
    for (offset, line_text) in lines.iter().enumerate().rev() {
        if line_text.trim().is_empty() {
            continue;
        }
        let line = first + offset;
        let start = doc.line_start(line);
        if all_commented {
            let indent = text::leading_whitespace(line_text).chars().count();
            let rest: String = line_text.chars().skip(indent).collect();
            let mut inner = rest.strip_prefix(&open).unwrap_or(&rest).to_string();
            if !close.is_empty() {
                let trimmed = inner.trim_end().to_string();
                inner = trimmed
                    .strip_suffix(&close)
                    .unwrap_or(&trimmed)
                    .trim_end()
                    .to_string();
            }
            let inner = inner.strip_prefix(' ').unwrap_or(&inner).to_string();
            let old_len = line_text.chars().count() - indent;
            let delta = inner.chars().count() as isize - old_len as isize;
            changes.push(Change::new(start + indent, start + indent + old_len, inner));
            sel = shifted(sel, start + indent, delta);
        } else {
            let rest: String = line_text.chars().skip(min_indent).collect();
            let commented = if close.is_empty() {
                format!("{open} {rest}")
            } else {
                format!("{open} {rest} {close}")
            };
            let delta = commented.chars().count() as isize - rest.chars().count() as isize;
            changes.push(Change::new(
                start + min_indent,
                start + line_text.chars().count(),
                commented,
            ));
            sel = shifted(sel, start + min_indent, delta);
        }
    }
    doc.apply(changes, sel, EditKind::Other);
}

/// Alt+Up / Alt+Down: moves the selected lines.
pub fn move_lines(doc: &mut Document, down: bool) {
    let (first, last) = selected_lines(doc);
    let count = doc.line_count();
    if (!down && first == 0) || (down && last + 1 >= count) {
        return;
    }
    let block_start = doc.line_start(first);
    let block_end = doc.line_end(last);
    let block = doc.rope.slice(block_start..block_end).to_string();
    let sel = doc.selection;
    if down {
        let other = doc.line(last + 1).into_owned();
        let other_len = other.chars().count() + 1;
        let end = doc.line_end(last + 1);
        doc.apply(
            vec![Change::new(block_start, end, format!("{other}\n{block}"))],
            Selection::new(sel.anchor + other_len, sel.head + other_len),
            EditKind::Other,
        );
    } else {
        let other = doc.line(first - 1).into_owned();
        let other_len = other.chars().count() + 1;
        let start = doc.line_start(first - 1);
        doc.apply(
            vec![Change::new(start, block_end, format!("{block}\n{other}"))],
            Selection::new(sel.anchor - other_len, sel.head - other_len),
            EditKind::Other,
        );
    }
}

pub fn duplicate_lines(doc: &mut Document) {
    let (first, last) = selected_lines(doc);
    let start = doc.line_start(first);
    let end = doc.line_end(last);
    let block = doc.rope.slice(start..end).to_string();
    let len = block.chars().count() + 1;
    let sel = doc.selection;
    doc.apply(
        vec![Change::insert(end, format!("\n{block}"))],
        Selection::new(sel.anchor + len, sel.head + len),
        EditKind::Other,
    );
}

pub fn delete_lines(doc: &mut Document) {
    let (first, last) = selected_lines(doc);
    let col = doc.line_col(doc.selection.head).1;
    let start = doc.line_start(first);
    let end = doc.line_end(last);
    let len = doc.rope.len_chars();
    let (from, to, target) = if end < len {
        (start, end + 1, first)
    } else if start > 0 {
        (start - 1, end, first - 1)
    } else {
        (start, end, first)
    };
    doc.apply(
        vec![Change::new(from, to, "")],
        Selection::point(from),
        EditKind::Other,
    );
    doc.selection = Selection::point(doc.pos(target, col));
}

/// Ctrl+D: selects the word under the cursor, then its next occurrence.
pub fn select_next_occurrence(doc: &mut Document) {
    if doc.selection.is_empty() {
        let (line, col) = doc.line_col(doc.selection.head);
        if let Some((start, end)) = text::word_at(&doc.line_chars(line), col) {
            let base = doc.line_start(line);
            doc.selection = Selection::new(base + start, base + end);
        }
        return;
    }
    let needle = doc.selected_text();
    let haystack = doc.text();
    let from = doc.rope.char_to_byte(doc.selection.end());
    let found = haystack[from..]
        .find(&needle)
        .map(|i| i + from)
        .or_else(|| haystack[..from].find(&needle));
    if let Some(byte) = found {
        let start = doc.rope.byte_to_char(byte);
        doc.selection = Selection::new(start, start + needle.chars().count());
    }
}

pub fn left(doc: &Document, pos: usize) -> usize {
    if pos == 0 {
        return 0;
    }
    let (line, col) = doc.line_col(pos);
    if col == 0 {
        return pos - 1;
    }
    doc.line_start(line) + text::prev_grapheme(&doc.line(line), col)
}

pub fn right(doc: &Document, pos: usize) -> usize {
    let (line, col) = doc.line_col(pos);
    if col >= doc.line_len(line) {
        return (pos + 1).min(doc.rope.len_chars());
    }
    doc.line_start(line) + text::next_grapheme(&doc.line(line), col)
}

/// Ctrl+Left: `count` characters left, stopping at the line start; at the
/// line start, to the end of the previous line.
pub fn jump_left(doc: &Document, pos: usize, count: usize) -> usize {
    let (line, col) = doc.line_col(pos);
    if col == 0 {
        return left(doc, pos);
    }
    let start = doc.line_start(line);
    let mut target = pos;
    for _ in 0..count.max(1) {
        if target <= start {
            break;
        }
        target = left(doc, target);
    }
    target
}

/// Ctrl+Right: `count` characters right, stopping at the line end; at the
/// line end, to the start of the next line.
pub fn jump_right(doc: &Document, pos: usize, count: usize) -> usize {
    let (line, col) = doc.line_col(pos);
    if col >= doc.line_len(line) {
        return right(doc, pos);
    }
    let end = doc.line_end(line);
    let mut target = pos;
    for _ in 0..count.max(1) {
        if target >= end {
            break;
        }
        target = right(doc, target);
    }
    target
}

/// Home: toggles between the first non-blank character and the line start.
pub fn smart_home(doc: &Document, pos: usize) -> usize {
    let (line, col) = doc.line_col(pos);
    let indent = text::leading_whitespace(&doc.line(line)).chars().count();
    doc.line_start(line) + if col == indent { 0 } else { indent }
}

/// Moves `lines` up (negative) or down, keeping the preferred display column.
pub fn vertical(doc: &mut Document, lines: isize, extend: bool, tab_size: usize) {
    let head = doc.selection.head;
    let (line, col) = doc.line_col(head);
    let current_display = text::char_to_display_col(&doc.line(line), col, tab_size);
    let preferred = *doc.view.preferred_col.get_or_insert(current_display);
    let max_line = doc.line_count().saturating_sub(1) as isize;
    let target = (line as isize + lines).clamp(0, max_line) as usize;
    let new_pos = if target == line && lines < 0 {
        0
    } else if target == line && lines > 0 {
        doc.line_end(line)
    } else {
        let target_col = text::display_col_to_char(&doc.line(target), preferred, tab_size);
        doc.line_start(target) + target_col
    };
    doc.selection = doc.selection.move_to(new_pos, extend);
}

pub fn matching_bracket(doc: &Document, pos: usize) -> Option<(usize, usize)> {
    const BRACKETS: [(char, char); 3] = [('(', ')'), ('[', ']'), ('{', '}')];
    const LIMIT: usize = 20_000;
    let candidates = [Some(pos), pos.checked_sub(1)];
    for at in candidates.into_iter().flatten() {
        let Some(c) = char_at(doc, at) else { continue };
        if let Some((open, close)) = BRACKETS.iter().find(|(o, _)| *o == c) {
            let mut depth = 0;
            for (i, ch) in doc.rope.chars_at(at).enumerate().take(LIMIT) {
                if ch == *open {
                    depth += 1;
                } else if ch == *close {
                    depth -= 1;
                    if depth == 0 {
                        return Some((at, at + i));
                    }
                }
            }
            return None;
        }
        if let Some((open, close)) = BRACKETS.iter().find(|(_, cl)| *cl == c) {
            let mut depth = 0;
            let mut chars = doc.rope.chars_at(at + 1);
            let mut i = at + 1;
            while let Some(ch) = chars.prev() {
                i -= 1;
                if at - i > LIMIT {
                    break;
                }
                if ch == *close {
                    depth += 1;
                } else if ch == *open {
                    depth -= 1;
                    if depth == 0 {
                        return Some((i, at));
                    }
                }
            }
            return None;
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use ropey::Rope;

    fn doc(text: &str, cursor: usize) -> Document {
        let mut doc = Document::scratch(1);
        doc.rope = Rope::from_str(text);
        doc.selection = Selection::point(cursor);
        doc.language = crate::syntax::languages::by_name("javascript");
        doc
    }

    fn options() -> Options {
        Options::default()
    }

    #[test]
    fn brackets_auto_close_and_skip() {
        let mut d = doc("", 0);
        type_char(&mut d, '(', &options());
        assert_eq!(d.text(), "()");
        type_char(&mut d, ')', &options());
        assert_eq!(d.text(), "()");
        assert_eq!(d.selection.head, 2);
    }

    #[test]
    fn enter_between_braces_opens_a_block() {
        let mut d = doc("  if (x) {}", 10);
        newline(&mut d, &options());
        assert_eq!(d.text(), "  if (x) {\n    \n  }");
        assert_eq!(d.selection.head, 15);
    }

    #[test]
    fn backspace_removes_one_indent_level() {
        let mut d = doc("    x", 4);
        backspace(&mut d, &options());
        assert_eq!(d.text(), "  x");
    }

    #[test]
    fn comments_toggle_on_and_off() {
        let mut d = doc("  a\n  b\n", 0);
        d.selection = Selection::new(0, 6);
        toggle_comment(&mut d);
        assert_eq!(d.text(), "  // a\n  // b\n");
        toggle_comment(&mut d);
        assert_eq!(d.text(), "  a\n  b\n");
    }

    #[test]
    fn lines_move_and_duplicate() {
        let mut d = doc("a\nb\nc", 0);
        move_lines(&mut d, true);
        assert_eq!(d.text(), "b\na\nc");
        assert_eq!(d.selection.head, 2);
        duplicate_lines(&mut d);
        assert_eq!(d.text(), "b\na\na\nc");
    }

    #[test]
    fn indent_and_outdent_selected_lines() {
        let mut d = doc("a\nb\n", 0);
        d.selection = Selection::new(0, 3);
        tab(&mut d, &options());
        assert_eq!(d.text(), "  a\n  b\n");
        outdent(&mut d, &options());
        assert_eq!(d.text(), "a\nb\n");
    }

    #[test]
    fn brackets_match_both_ways() {
        let d = doc("f(a[1])", 1);
        assert_eq!(matching_bracket(&d, 1), Some((1, 6)));
        assert_eq!(matching_bracket(&d, 7), Some((1, 6)));
    }
}
