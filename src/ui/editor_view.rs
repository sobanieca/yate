//! The text editor view: gutter (git signs, line numbers, bookmarks),
//! syntax-highlighted text, selection, find matches, indent guides,
//! matching brackets and inline color swatches.

use super::surface::{Rect, Rgb, Style, Surface};
use crate::app::{App, Focus, gutter_width};
use crate::editor::document::{Document, Sign, ViewState};
use crate::editor::ops;
use crate::editor::selection::Selection;
use crate::text;
use regex::Regex;
use std::sync::OnceLock;
use unicode_segmentation::UnicodeSegmentation;

const MAX_HIGHLIGHT_RANGE: usize = 2 * 1024 * 1024;

/// Draws the focused pane: the active document. Returns the cursor.
pub fn render(app: &App, surface: &mut Surface, area: Rect) -> Option<(u16, u16)> {
    let doc = app.doc();
    let welcome = app.docs.len() == 1
        && doc.path.is_none()
        && doc.rope.len_chars() == 0
        && app.split.is_none();
    if welcome {
        let t = &app.theme;
        surface.fill(
            area,
            Style::new().fg(t.fg).bg(t.background(app.terminal_focused)),
        );
        render_welcome(app, surface, area);
        return None;
    }
    render_pane(app, surface, area, doc, doc.selection, &doc.view, true)
}

/// Draws the other pane of a split with its own cursor and scroll.
pub fn render_other(app: &App, surface: &mut Surface, area: Rect) {
    let (Some(split), Some(index)) = (&app.split, app.other_pane_doc()) else {
        return;
    };
    let doc = &app.docs[index];
    let selection = split.other.selection.clamp(doc.rope.len_chars());
    let mut view = split.other.view.clone();
    view.scroll_line = view.scroll_line.min(doc.line_count().saturating_sub(1));
    render_pane(app, surface, area, doc, selection, &view, false);
}

fn render_pane(
    app: &App,
    surface: &mut Surface,
    area: Rect,
    doc: &Document,
    selection: Selection,
    view: &ViewState,
    focused: bool,
) -> Option<(u16, u16)> {
    let t = &app.theme;
    let options = &app.options;
    let background = t.background(app.terminal_focused);
    surface.fill(area, Style::new().fg(t.fg).bg(background));
    if area.w == 0 || area.h == 0 {
        return None;
    }
    let gutter = gutter_width(doc, options);
    let text_x = area.x + gutter;
    let text_width = area.w.saturating_sub(gutter) as usize;
    let first = view.scroll_line;
    let line_count = doc.line_count();
    let last = (first + area.h as usize).min(line_count);
    let tab_size = options.tab_size;

    let range_start = doc.rope.line_to_byte(first.min(doc.rope.len_lines()));
    let range_end = doc.rope.line_to_byte(last.min(doc.rope.len_lines()));
    let syntax_styles = match &doc.syntax {
        Some(syntax) if range_end - range_start <= MAX_HIGHLIGHT_RANGE => syntax.highlight(
            &doc.rope,
            range_start..range_end,
            &t.capture_styles(syntax.language),
        ),
        _ => Vec::new(),
    };

    let (cursor_line, cursor_col) = doc.line_col(selection.head);
    let bracket = if focused && (app.focus == Focus::Editor || app.focus == Focus::Find) {
        ops::matching_bracket(doc, selection.head)
    } else {
        None
    };
    let bookmarks = doc
        .path
        .as_ref()
        .map(|p| app.bookmarks.lines_in(p))
        .unwrap_or_default();
    let matches = visible_matches(app, doc, first, last);
    let guides = if options.indent_guides {
        indent_levels(doc, first, last, tab_size)
    } else {
        Vec::new()
    };
    let digits = (gutter as usize).saturating_sub(3);
    let mut cursor = None;

    for line in first..last {
        let y = area.y + (line - first) as u16;
        let is_cursor_line = line == cursor_line;
        let line_bg = if focused && is_cursor_line && options.cursor_line && selection.is_empty() {
            t.bg_highlight
        } else {
            background
        };
        surface.fill(
            Rect::new(area.x, y, area.w, 1),
            Style::new().fg(t.fg).bg(line_bg),
        );

        match doc.git.signs.get(line).copied().flatten() {
            Some(Sign::Added) => surface.set_symbol(area.x, y, "▎", Style::new().fg(t.git_add)),
            Some(Sign::Changed) => {
                surface.set_symbol(area.x, y, "▎", Style::new().fg(t.git_change))
            }
            Some(Sign::Deleted) => {
                surface.set_symbol(area.x, y, "▁", Style::new().fg(t.git_delete))
            }
            None => {}
        }
        if options.line_numbers {
            let number = format!("{:>digits$}", line + 1);
            let style = if is_cursor_line {
                Style::new().fg(t.orange).bold()
            } else {
                Style::new().fg(t.fg_gutter)
            };
            surface.put_str(area.x + 1, y, &number, style, text_x);
        }
        if bookmarks.contains(&line) {
            surface.set_symbol(text_x - 2, y, "◆", Style::new().fg(t.yellow));
        }

        let line_text = doc.line(line);
        let line_start_char = doc.line_start(line);
        let line_start_byte = doc.rope.line_to_byte(line) - range_start;
        let colors = if options.colorize {
            color_ranges(&line_text)
        } else {
            Vec::new()
        };
        let line_matches: Vec<(usize, usize, bool)> = matches
            .iter()
            .filter(|(start, end, _)| {
                *end > line_start_char && *start <= line_start_char + line_text.len()
            })
            .copied()
            .collect();
        let indent_guides = guides.get(line - first).copied().unwrap_or(0);
        let leading = text::indent_width(&line_text, tab_size);

        let mut col = 0usize;
        let mut byte = 0usize;
        let mut char_index = 0usize;
        for grapheme in line_text.graphemes(true) {
            let width = text::display_width(grapheme, col, tab_size);
            let pos = line_start_char + char_index;
            let mut style = Style::new().fg(t.fg).bg(line_bg);
            if let Some(Some(syntax)) = syntax_styles.get(line_start_byte + byte) {
                style = style.patch(Style {
                    bg: None,
                    ..*syntax
                });
            }
            if let Some(color) = colors
                .iter()
                .find(|(s, e, _)| byte >= *s && byte < *e)
                .map(|c| c.2)
            {
                let fg = if color.luminance() > 140.0 {
                    t.bg_dark
                } else {
                    t.fg
                };
                style = style.fg(fg).bg(color);
            }
            if pos >= selection.start() && pos < selection.end() {
                style = style.bg(t.bg_visual);
            }
            if let Some((_, _, current)) =
                line_matches.iter().find(|(s, e, _)| pos >= *s && pos < *e)
            {
                style = if *current {
                    style.fg(t.bg_dark).bg(t.orange)
                } else {
                    style.bg(t.bg_search)
                };
            }
            if bracket.is_some_and(|(a, b)| pos == a || pos == b) {
                style = style.bg(t.fg_gutter).bold();
            }
            let is_whitespace = grapheme == " " || grapheme == "\t";
            let visible_start = view.scroll_col;
            let visible_end = visible_start + text_width;
            if col >= visible_end {
                break;
            }
            if col + width > visible_start {
                let partial = col < visible_start || col + width > visible_end;
                if is_whitespace || partial {
                    for display in col.max(visible_start)..(col + width).min(visible_end) {
                        let x = text_x + (display - visible_start) as u16;
                        let guide = is_whitespace
                            && display < leading
                            && display % tab_size == 0
                            && display / tab_size < indent_guides;
                        if guide && style.bg == Some(line_bg) {
                            surface.put_grapheme(x, y, "▏", 1, style.fg(t.fg_gutter));
                        } else {
                            surface.put_grapheme(x, y, " ", 1, style);
                        }
                    }
                } else {
                    let symbol = if grapheme.starts_with(|c: char| c.is_control()) {
                        "�"
                    } else {
                        grapheme
                    };
                    let x = text_x + (col - visible_start) as u16;
                    surface.put_grapheme(x, y, symbol, width as u16, style);
                }
            }
            col += width;
            byte += grapheme.len();
            char_index += grapheme.chars().count();
        }
        let line_end = line_start_char + char_index;
        if line_end >= selection.start() && line_end < selection.end() && col >= view.scroll_col {
            let x = text_x as usize + col - view.scroll_col;
            if x < (area.x + area.w) as usize {
                surface.put_grapheme(x as u16, y, " ", 1, Style::new().bg(t.bg_visual));
            }
        }
        if leading == 0 && line_text.trim().is_empty() && indent_guides > 0 {
            for level in 0..indent_guides {
                let display = level * tab_size;
                if display < view.scroll_col {
                    continue;
                }
                let x = text_x as usize + display - view.scroll_col;
                if x < (area.x + area.w) as usize {
                    surface.put_grapheme(x as u16, y, "▏", 1, Style::new().fg(t.fg_gutter));
                }
            }
        }
        if focused && is_cursor_line {
            let display = text::char_to_display_col(&line_text, cursor_col, tab_size);
            if display >= view.scroll_col && display - view.scroll_col < text_width {
                cursor = Some((text_x + (display - view.scroll_col) as u16, y));
            }
        }
    }
    cursor
}

const WELCOME: &[(&str, &str)] = &[
    ("picker.files", "find file"),
    ("picker.grep", "live grep"),
    ("tree.toggle", "file tree"),
    ("switch.next", "switch: tree, files"),
    ("git.review", "review git changes"),
    ("picker.commands", "command palette"),
    ("help.keys", "all key bindings"),
    ("app.quit", "quit"),
];

fn render_welcome(app: &App, surface: &mut Surface, area: Rect) {
    let t = &app.theme;
    let width = 44.min(area.w);
    let height = WELCOME.len() as u16 + 4;
    let x = area.x + area.w.saturating_sub(width) / 2;
    let y = area.y + area.h.saturating_sub(height) / 3;
    let title = "Y A T E";
    let title_x = x + width.saturating_sub(title.len() as u16) / 2;
    surface.put_str(
        title_x,
        y,
        title,
        Style::new().fg(t.blue).bold(),
        area.right(),
    );
    let subtitle = "yet another text editor";
    let subtitle_x = x + width.saturating_sub(subtitle.len() as u16) / 2;
    surface.put_str(
        subtitle_x,
        y + 1,
        subtitle,
        Style::new().fg(t.dark5).italic(),
        area.right(),
    );
    for (i, (command, action)) in WELCOME.iter().enumerate() {
        let row = y + 3 + i as u16;
        let keys = app.keymap.hint(command).unwrap_or_default();
        surface.put_str(x + 2, row, action, Style::new().fg(t.fg_dark), area.right());
        surface.put_str_right(x + width - 2, row, &keys, Style::new().fg(t.orange));
    }
    let root = app
        .root
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    let root = text::truncate_left(&root, width as usize);
    let root_x = x + width.saturating_sub(text::str_width(&root) as u16) / 2;
    surface.put_str(
        root_x,
        y + height + 1,
        &root,
        Style::new().fg(t.dark3),
        area.right(),
    );
}

/// Find matches overlapping the visible lines: char range and whether it
/// is the current match.
fn visible_matches(
    app: &App,
    doc: &Document,
    first: usize,
    last: usize,
) -> Vec<(usize, usize, bool)> {
    if !app.find_open
        || app.find.input.text.is_empty()
        || app.find.key.as_ref().is_none_or(|k| k.0 != doc.id)
    {
        return Vec::new();
    }
    let start = doc.line_start(first);
    let end = if last >= doc.rope.len_lines() {
        doc.rope.len_chars()
    } else {
        doc.line_start(last)
    };
    let matches = &app.find.matches;
    let from = matches.partition_point(|(_, e)| *e <= start);
    matches[from..]
        .iter()
        .enumerate()
        .take_while(|(_, (s, _))| *s < end)
        .map(|(i, (s, e))| (*s, *e, app.find.current == Some(from + i)))
        .collect()
}

/// Number of indent guides per visible line. Blank lines take the larger
/// indent of the lines around them, so guides run through blank lines.
fn indent_levels(doc: &Document, first: usize, last: usize, tab_size: usize) -> Vec<usize> {
    const LOOK: usize = 200;
    let indent_of = |line: usize| -> Option<usize> {
        let text = doc.line(line);
        if text.trim().is_empty() {
            None
        } else {
            Some(text::indent_width(&text, tab_size))
        }
    };
    let mut levels = Vec::with_capacity(last - first);
    let mut previous: Option<usize> = (first.saturating_sub(LOOK)..first)
        .rev()
        .find_map(indent_of);
    let mut next_cache: Option<(usize, usize)> = None;
    for line in first..last {
        let indent = match indent_of(line) {
            Some(indent) => {
                previous = Some(indent);
                indent
            }
            None => {
                let next = match next_cache {
                    Some((at, indent)) if at > line => indent,
                    _ => {
                        let found = (line + 1..(line + LOOK).min(doc.line_count()))
                            .find_map(|l| indent_of(l).map(|i| (l, i)))
                            .unwrap_or((usize::MAX, 0));
                        next_cache = Some(found);
                        found.1
                    }
                };
                previous.unwrap_or(0).max(next)
            }
        };
        levels.push(indent.div_ceil(tab_size.max(1)));
    }
    levels
}

fn color_regex() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    REGEX.get_or_init(|| {
        Regex::new(r"#(?:[0-9a-fA-F]{8}|[0-9a-fA-F]{6}|[0-9a-fA-F]{3})\b").expect("valid regex")
    })
}

/// `#rgb`, `#rrggbb` and `#rrggbbaa` colors in a line: byte range and color.
fn color_ranges(line: &str) -> Vec<(usize, usize, Rgb)> {
    if !line.contains('#') {
        return Vec::new();
    }
    color_regex()
        .find_iter(line)
        .filter_map(|found| {
            let hex = &found.as_str()[1..];
            let value = |s: &str| u8::from_str_radix(s, 16).ok();
            let rgb = if hex.len() == 3 {
                let digit = |i: usize| value(&hex[i..i + 1]).map(|v| v * 17);
                Rgb(digit(0)?, digit(1)?, digit(2)?)
            } else {
                Rgb(value(&hex[0..2])?, value(&hex[2..4])?, value(&hex[4..6])?)
            };
            Some((found.start(), found.end(), rgb))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_colors_are_found() {
        let ranges = color_ranges("a: #fff; b: #24283b; c: #12345");
        assert_eq!(ranges.len(), 2);
        assert_eq!(ranges[0].2, Rgb(255, 255, 255));
        assert_eq!(ranges[1].2, Rgb(0x24, 0x28, 0x3b));
    }

    #[test]
    fn blank_lines_keep_the_guides_of_their_block() {
        let mut doc = Document::scratch(1);
        doc.rope = ropey::Rope::from_str("{\n    a\n\n    b\n}\n\nx\n");
        let levels = indent_levels(&doc, 0, 7, 2);
        assert_eq!(levels, vec![0, 2, 2, 2, 0, 0, 0]);
    }
}
