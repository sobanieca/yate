//! The left panel: the file tree.

use super::icons;
use super::surface::{Rect, Rgb, Style, Surface};
use super::theme::Theme;
use crate::app::{App, Focus};
use crate::text;
use crate::workspace::git::FileStatus;
use crate::workspace::tree::{FileTree, INDENT, Row};
use unicode_segmentation::UnicodeSegmentation;

const MIN_WIDTH: u16 = 12;
const MIN_EDITOR_WIDTH: u16 = 20;

/// A file tree width set by hand, kept between a readable tree and a
/// usable editor.
pub fn clamp_width(width: u16, screen_width: u16) -> u16 {
    let max = screen_width.saturating_sub(MIN_EDITOR_WIDTH).max(MIN_WIDTH);
    width.clamp(MIN_WIDTH, max)
}

pub fn width(app: &App, screen_width: u16) -> u16 {
    if app.options.tree_width > 0 {
        return clamp_width(app.options.tree_width, screen_width);
    }
    let max = (screen_width / 3).clamp(20, 50);
    let wanted = app
        .tree
        .rows
        .iter()
        .map(|row| row.depth * INDENT + text::str_width(&row.name) + 7)
        .max()
        .unwrap_or(20);
    (wanted as u16).clamp(24.min(max), max)
}

pub fn status_color(t: &Theme, status: FileStatus) -> Rgb {
    match status {
        FileStatus::Modified => t.yellow,
        FileStatus::Added | FileStatus::Untracked => t.green,
        FileStatus::Deleted => t.red,
        FileStatus::Renamed => t.cyan,
        FileStatus::Conflict => t.red1,
    }
}

pub fn render(app: &mut App, surface: &mut Surface, area: Rect) {
    let t = app.theme.clone();
    let base = Style::new().fg(t.fg_dark).bg(t.bg_dark);
    surface.fill(area, base);
    let (header, list) = area.split_top(1);
    render_tree(app, surface, header, list);
}

fn render_tree(app: &mut App, surface: &mut Surface, header: Rect, list: Rect) {
    let t = app.theme.clone();
    let focused = app.focus == Focus::Tree;
    let root_name = app
        .root
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "/".to_string());
    let title_style = Style::new()
        .fg(if focused {
            super::focus_color(&t, Focus::Tree)
        } else {
            t.dark5
        })
        .bg(t.bg_dark)
        .bold();
    let x = surface.put_str(
        header.x + 1,
        header.y,
        &root_name.to_uppercase(),
        title_style,
        header.right(),
    );
    let mut flags = Vec::new();
    if app.tree.changed_only {
        flags.push("changed");
    }
    if app.tree.show_ignored {
        flags.push("+ignored");
    }
    if app.tree.locked {
        flags.push("locked");
    }
    if !flags.is_empty() {
        let text = format!(" {}", flags.join(" "));
        surface.put_str(
            x,
            header.y,
            &text,
            Style::new().fg(t.magenta).bg(t.bg_dark),
            header.right(),
        );
    }

    app.tree.ensure_visible(list.h as usize);
    let nerd = app.options.nerd_icons;
    if let Some(row) = app.tree.selected_row() {
        let icon = row_icon(&t, &app.tree, row, nerd).0;
        let label_width = text::str_width(icon) + 1 + text::str_width(&row.name);
        app.tree.fit_selected(
            list.w.saturating_sub(1 + MARKER_SPACE) as usize,
            label_width,
        );
    }
    let current = app.doc().path.clone();
    let tree = &app.tree;
    for (i, row) in tree
        .rows
        .iter()
        .enumerate()
        .skip(tree.scroll)
        .take(list.h as usize)
    {
        let y = list.y + (i - tree.scroll) as u16;
        let selected = i == tree.selected;
        let bg = if selected && focused {
            t.bg_highlight
        } else if selected {
            t.bg
        } else {
            t.bg_dark
        };
        surface.fill(Rect::new(list.x, y, list.w, 1), Style::new().bg(bg));
        let status = if row.is_dir {
            None
        } else {
            app.git.get(&row.path)
        };
        let dir_changed = row.is_dir && app.git.dir_has_changes(&row.path);
        let indent = (row.depth * INDENT) as isize - tree.h_scroll as isize;
        let left = list.x + 1;
        let mut x = left + indent.max(0) as u16;
        let mut hidden = (-indent).max(0) as usize;
        let name_color = if row.ignored {
            t.dark3
        } else if let Some(status) = status {
            status_color(&t, status)
        } else if row.is_dir {
            t.blue
        } else {
            t.fg_dark
        };
        let (icon, icon_color) = row_icon(&t, tree, row, nerd);
        let icon_label = format!("{icon} ");
        let (shown_icon, skipped) = clip_left(&icon_label, hidden);
        hidden -= skipped;
        x = surface.put_str(
            x,
            y,
            &shown_icon,
            Style::new().fg(icon_color).bg(bg),
            list.right(),
        );
        let mut style = Style::new().fg(name_color).bg(bg);
        if current.as_ref() == Some(&row.path) {
            style = style
                .bold()
                .fg(if status.is_some() { name_color } else { t.fg });
        }
        let (shown_name, _) = clip_left(&row.name, hidden);
        let end = surface.put_str(
            x,
            y,
            &shown_name,
            style,
            list.right().saturating_sub(MARKER_SPACE),
        );
        if let Some(status) = status {
            surface.put_str_right(
                list.right().saturating_sub(1),
                y,
                status.letter(),
                Style::new().fg(status_color(&t, status)).bg(bg),
            );
        } else if dir_changed && !row.ignored {
            surface.put_str(
                end + 1,
                y,
                "•",
                Style::new().fg(t.yellow).bg(bg),
                list.right(),
            );
        }
    }
    if tree.rows.is_empty() {
        let message = if tree.changed_only {
            "No changed files"
        } else {
            "Empty folder"
        };
        surface.put_str(
            list.x + 2,
            list.y,
            message,
            Style::new().fg(t.dark5).bg(t.bg_dark),
            list.right(),
        );
    }
}

const MARKER_SPACE: u16 = 2;

fn row_icon(t: &Theme, tree: &FileTree, row: &Row, nerd: bool) -> (&'static str, Rgb) {
    if row.is_dir {
        let open = tree.expanded.contains(&row.path);
        let color = if row.ignored { t.dark3 } else { t.blue };
        (icons::folder(open, nerd), color)
    } else {
        icons::file(t, &row.path, nerd)
    }
}

/// Drops the first `columns` columns of `text`; a wide character cut in
/// half becomes a space. Returns the rest and the columns it dropped.
fn clip_left(text: &str, columns: usize) -> (String, usize) {
    let mut skipped = 0;
    let mut rest = String::new();
    for grapheme in text.graphemes(true) {
        let width = text::grapheme_width(grapheme);
        if skipped >= columns {
            rest.push_str(grapheme);
        } else if skipped + width > columns {
            rest.push_str(&" ".repeat(skipped + width - columns));
            skipped = columns;
        } else {
            skipped += width;
        }
    }
    (rest, skipped)
}
