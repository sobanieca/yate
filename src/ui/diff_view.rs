//! The diff of the review view: old and new line numbers, added and removed
//! lines with the changed words emphasized, syntax highlighted.

use super::surface::{Rect, Style, Surface};
use crate::app::{App, Focus, ReviewSource};
use crate::text;
use crate::workspace::diff::LineKind;
use crate::workspace::git::FileStatus;
use unicode_segmentation::UnicodeSegmentation;

pub fn render_header(app: &App, surface: &mut Surface, area: Rect) {
    let t = &app.theme;
    surface.fill(area, Style::new().fg(t.fg).bg(t.bg_dark));
    let Some(review) = &app.review else {
        return;
    };
    let label = match review.source {
        ReviewSource::Git => " REVIEW ",
        ReviewSource::Compare { .. } => " COMPARE ",
    };
    let x = surface.put_str(
        area.x,
        area.y,
        label,
        Style::new()
            .fg(t.bg_dark)
            .bg(super::focus_color(t, Focus::Review))
            .bold(),
        area.right(),
    );
    let Some(path) = review.selected_path() else {
        return;
    };
    let modified = review
        .files
        .get(review.selected)
        .is_some_and(|(_, status)| *status == FileStatus::Modified);
    let shown = match review.old_path(path) {
        Some(old) if modified => format!("{} → {}", app.relative(&old), app.relative(path)),
        _ => app.relative(path),
    };
    let added = review
        .lines
        .iter()
        .filter(|l| l.kind == LineKind::Added)
        .count();
    let removed = review
        .lines
        .iter()
        .filter(|l| l.kind == LineKind::Removed)
        .count();
    let x = surface.put_str(
        x + 1,
        area.y,
        &shown,
        Style::new().fg(t.fg).bg(t.bg_dark).bold(),
        area.right(),
    );
    let x = surface.put_str(
        x + 2,
        area.y,
        &format!("+{added}"),
        Style::new().fg(t.green).bg(t.bg_dark),
        area.right(),
    );
    let x = surface.put_str(
        x + 1,
        area.y,
        &format!("-{removed}"),
        Style::new().fg(t.red).bg(t.bg_dark),
        area.right(),
    );
    let next = app.keymap.hint("review.nextChange").unwrap_or_default();
    let previous = app.keymap.hint("review.previousChange").unwrap_or_default();
    let hint =
        format!("  {next}/{previous} next/prev change · enter open · tab switch · esc close");
    surface.put_str(
        x,
        area.y,
        &hint,
        Style::new().fg(t.dark3).bg(t.bg_dark),
        area.right(),
    );
}

pub fn render(app: &mut App, surface: &mut Surface, area: Rect) {
    let t = app.theme.clone();
    let focused = app.focus == Focus::Review && app.review.as_ref().is_some_and(|r| r.diff_focused);
    let background = t.background(app.terminal_focused);
    let tab_size = app.options.tab_size;
    let Some(review) = &mut app.review else {
        return;
    };
    surface.fill(area, Style::new().fg(t.fg).bg(background));
    if review.lines.is_empty() {
        let message = match (&review.source, review.files.is_empty()) {
            (ReviewSource::Git, true) => "Nothing to review: no changes against HEAD",
            (ReviewSource::Compare { .. }, true) => "The folders have the same files",
            (ReviewSource::Git, false) => "No textual changes (binary file or mode change)",
            (ReviewSource::Compare { .. }, false) => "No textual differences",
        };
        surface.put_str(
            area.x + 2,
            area.y + 1,
            message,
            Style::new().fg(t.dark5),
            area.right(),
        );
        return;
    }
    review.ensure_visible(area.h as usize, 0);
    let number_width = review
        .lines
        .iter()
        .filter_map(|l| l.old_line.max(l.new_line))
        .max()
        .unwrap_or(0)
        .saturating_add(1)
        .to_string()
        .len()
        .max(3);
    let gutter = (number_width * 2 + 5) as u16;
    for (i, line) in review
        .lines
        .iter()
        .enumerate()
        .skip(review.scroll)
        .take(area.h as usize)
    {
        let y = area.y + (i - review.scroll) as u16;
        let is_cursor = i == review.cursor && focused;
        let (bg, emphasis_bg, sign, sign_color) = match line.kind {
            LineKind::Added => (t.diff_add_bg, t.diff_add_emphasis, "+", t.git_add),
            LineKind::Removed => (t.diff_delete_bg, t.diff_delete_emphasis, "-", t.red1),
            LineKind::Context => (background, background, " ", t.fg_gutter),
            LineKind::Hunk => (t.bg_dark, t.bg_dark, " ", t.blue),
        };
        let bg = if is_cursor && line.kind == LineKind::Context {
            t.bg_highlight
        } else {
            bg
        };
        surface.fill(
            Rect::new(area.x, y, area.w, 1),
            Style::new().fg(t.fg).bg(bg),
        );
        if is_cursor {
            surface.set_symbol(area.x, y, "▌", Style::new().fg(t.blue).bg(bg));
        }
        if line.kind == LineKind::Hunk {
            surface.put_str(
                area.x + gutter,
                y,
                &line.text,
                Style::new().fg(t.blue).bg(bg).italic(),
                area.right(),
            );
            continue;
        }
        let number = |n: Option<usize>| {
            n.map(|n| format!("{:>number_width$}", n + 1))
                .unwrap_or_else(|| " ".repeat(number_width))
        };
        let numbers = format!("{} {}", number(line.old_line), number(line.new_line));
        surface.put_str(
            area.x + 1,
            y,
            &numbers,
            Style::new()
                .fg(if is_cursor { t.orange } else { t.fg_gutter })
                .bg(bg),
            area.right(),
        );
        surface.put_str(
            area.x + gutter - 2,
            y,
            sign,
            Style::new().fg(sign_color).bg(bg).bold(),
            area.right(),
        );

        let styles = review.styles.get(i);
        let mut x = area.x + gutter;
        let mut col = 0usize;
        let mut byte = 0usize;
        for grapheme in line.text.graphemes(true) {
            let width = text::display_width(grapheme, col, tab_size);
            let mut style = Style::new().fg(t.fg).bg(bg);
            if let Some(Some(syntax)) = styles.and_then(|s| s.get(byte)) {
                style = style.patch(Style {
                    bg: None,
                    ..*syntax
                });
            }
            if line.emphasis.iter().any(|(s, e)| byte >= *s && byte < *e) {
                style = style.bg(emphasis_bg);
            }
            if x + width as u16 > area.right() {
                break;
            }
            if grapheme == "\t" {
                for offset in 0..width {
                    surface.put_grapheme(x + offset as u16, y, " ", 1, style);
                }
            } else {
                surface.put_grapheme(x, y, grapheme, width as u16, style);
            }
            x += width as u16;
            col += width;
            byte += grapheme.len();
        }
    }
}
