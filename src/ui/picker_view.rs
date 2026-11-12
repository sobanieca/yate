//! The picker overlay: query input, results with matched characters
//! highlighted, and a syntax-highlighted preview when there is room.

use super::surface::{Rect, Style, Surface};
use super::theme::Theme;
use crate::app::{App, Overlay};
use crate::picker::{Picker, PickerItem, PickerKind};
use crate::text;
use unicode_segmentation::UnicodeSegmentation;

const PREVIEW_MIN_WIDTH: u16 = 110;

fn area(width: u16, height: u16) -> Rect {
    let w = width.saturating_sub(6).min(170).max(width.min(30));
    let h = (height * 4 / 5).max(height.min(12));
    Rect::new(
        width.saturating_sub(w) / 2,
        height.saturating_sub(h) / 2,
        w,
        h,
    )
}

fn split(width: u16, height: u16, kind: PickerKind) -> (Rect, Option<Rect>) {
    let outer = area(width, height);
    let has_preview = !matches!(kind, PickerKind::Commands);
    if has_preview && outer.w >= PREVIEW_MIN_WIDTH {
        let list_width = outer.w * 9 / 20;
        let (list, preview) = outer.split_left(list_width);
        (list, Some(preview))
    } else {
        (outer, None)
    }
}

/// Rows of results that fit.
pub fn list_height(height: u16) -> usize {
    area(200, height).h.saturating_sub(4) as usize
}

/// The screen rows of the results list.
pub fn list_area(width: u16, height: u16, kind: PickerKind) -> Rect {
    let (list, _) = split(width, height, kind);
    Rect::new(
        list.x + 1,
        list.y + 3,
        list.w.saturating_sub(2),
        list.h.saturating_sub(4),
    )
}

pub fn render(app: &mut App, surface: &mut Surface) -> Option<(u16, u16)> {
    let t = app.theme.clone();
    let (width, height) = (surface.width, surface.height);
    let Some(Overlay::Picker(picker)) = &mut app.overlay else {
        return None;
    };
    let (list_box, preview_box) = split(width, height, picker.kind);
    let rows = list_area(width, height, picker.kind);
    picker.ensure_visible(rows.h as usize);
    if picker.kind == PickerKind::Commands {
        scroll_commands(picker, rows);
    }
    let panel = Style::new().fg(t.fg).bg(t.bg_dark);
    let border = Style::new().fg(t.blue).bg(t.bg_dark);
    surface.fill(list_box, panel);
    surface.border(list_box, border, Some(&picker.title));
    let count = if picker.searching {
        format!(" {} … ", picker.shown.len())
    } else {
        format!(" {}/{} ", picker.shown.len(), picker.items.len())
    };
    surface.put_str_right(
        list_box.right() - 2,
        list_box.y,
        &count,
        Style::new().fg(t.dark5).bg(t.bg_dark),
    );

    let input_y = list_box.y + 1;
    let input_x = list_box.x + 2;
    surface.put_str(
        input_x,
        input_y,
        "❯",
        Style::new().fg(t.magenta).bg(t.bg_dark).bold(),
        list_box.right(),
    );
    let input_right = if picker.has_glob() {
        list_box.right().saturating_sub(list_box.w / 3)
    } else {
        list_box.right() - 1
    };
    let input_style = if picker.input.selected {
        panel.bg(t.bg_visual)
    } else {
        panel
    };
    surface.put_str(
        input_x + 2,
        input_y,
        &picker.input.text,
        input_style,
        input_right,
    );
    let mut cursor = Some((input_x + 2 + picker.input.cursor_col() as u16, input_y));
    if picker.has_glob() {
        let label_x = input_right + 1;
        let label_style = Style::new()
            .fg(if picker.glob_focused {
                t.magenta
            } else {
                t.dark5
            })
            .bg(t.bg_dark);
        let x = surface.put_str(label_x, input_y, "glob ", label_style, list_box.right() - 1);
        let glob_text = match (picker.glob.text.is_empty(), picker.glob_focused) {
            (true, false) => "tab to filter",
            (true, true) => "*.ts, !test/**",
            _ => &picker.glob.text,
        };
        let glob_style = if picker.glob.text.is_empty() {
            Style::new().fg(t.dark3).bg(t.bg_dark)
        } else {
            panel
        };
        surface.put_str(x, input_y, glob_text, glob_style, list_box.right() - 1);
        if picker.glob_focused {
            cursor = Some((x + picker.glob.cursor_col() as u16, input_y));
        }
    }
    for x in list_box.x + 1..list_box.right() - 1 {
        surface.set_symbol(x, input_y + 1, "─", Style::new().fg(t.fg_gutter));
    }
    if picker.has_glob() && list_box.h > 2 {
        let hint = if picker.glob_focused {
            " globs: *.rs, src/** · !glob excludes · tab back "
        } else {
            " tab glob filter, e.g. *.ts, !*.test.ts "
        };
        surface.put_str(
            list_box.x + 2,
            list_box.bottom() - 1,
            hint,
            Style::new().fg(t.dark5).bg(t.bg_dark),
            list_box.right() - 1,
        );
    }

    render_rows(picker, surface, rows, &t);

    if let Some(preview_box) = preview_box {
        render_preview(app, surface, preview_box);
    }
    cursor
}

const GROUP_COLUMN_MIN_WIDTH: u16 = 44;

/// The width of the column that names the section of each command, or 0
/// when the items have no sections or the list is too narrow for it.
fn group_column_width(picker: &Picker, rows: Rect) -> u16 {
    let widest = picker
        .items
        .iter()
        .map(|item| text::str_width(&item.group))
        .max()
        .unwrap_or(0);
    if widest == 0 || rows.w < GROUP_COLUMN_MIN_WIDTH {
        0
    } else {
        widest as u16 + 2
    }
}

/// Columns a command label gets at least beside its keys; with less, the
/// keys go on a line of their own below the label.
const LABEL_BESIDE_KEYS_MIN_WIDTH: usize = 16;

/// The rows of one command in the palette: its label broken into lines
/// (each with the char index where it starts) and the row of its keys.
struct CommandLayout {
    lines: Vec<(usize, String)>,
    keys_row: usize,
}

impl CommandLayout {
    fn new(item: &PickerItem, width: usize) -> CommandLayout {
        let keys = text::str_width(&item.detail);
        let label = text::str_width(&item.label);
        let (label_width, keys_below) = if keys == 0 || label + 2 + keys <= width {
            (width, false)
        } else if width >= keys + 2 + LABEL_BESIDE_KEYS_MIN_WIDTH {
            (width - keys - 2, false)
        } else {
            (width, true)
        };
        let mut from = 0;
        let lines: Vec<(usize, String)> = text::wrap(&item.label, label_width)
            .into_iter()
            .map(|line| {
                let at = item.label[from..].find(&line).map_or(from, |i| from + i);
                from = at + line.len();
                (item.label[..at].chars().count(), line)
            })
            .collect();
        let last = lines.len().saturating_sub(1);
        let last_width = lines.last().map_or(0, |(_, line)| text::str_width(line));
        let keys_row = if !keys_below {
            0
        } else if last_width + 2 + keys <= width {
            last
        } else {
            lines.len()
        };
        CommandLayout { lines, keys_row }
    }

    fn height(&self) -> usize {
        self.lines.len().max(self.keys_row + 1)
    }
}

/// The columns a command label can take in the list `rows`.
fn command_text_width(picker: &Picker, rows: Rect) -> usize {
    rows.w.saturating_sub(4 + group_column_width(picker, rows)) as usize
}

fn command_height(picker: &Picker, shown: usize, width: usize) -> usize {
    CommandLayout::new(&picker.items[picker.shown[shown].0], width).height()
}

/// Scrolls the palette so the whole selected command shows, now that a
/// command can take more than one row.
fn scroll_commands(picker: &mut Picker, rows: Rect) {
    let width = command_text_width(picker, rows);
    while picker.scroll < picker.selected
        && (picker.scroll..=picker.selected)
            .map(|shown| command_height(picker, shown, width))
            .sum::<usize>()
            > rows.h as usize
    {
        picker.scroll += 1;
    }
}

/// The index in `picker.shown` of the result drawn at a screen cell.
pub fn item_at(picker: &Picker, width: u16, height: u16, column: u16, row: u16) -> Option<usize> {
    let rows = list_area(width, height, picker.kind);
    if !rows.contains(column, row) {
        return None;
    }
    let offset = (row - rows.y) as usize;
    if picker.kind != PickerKind::Commands {
        let index = picker.scroll + offset;
        return (index < picker.shown.len()).then_some(index);
    }
    let text_width = command_text_width(picker, rows);
    let mut bottom = 0;
    for shown in picker.scroll..picker.shown.len() {
        bottom += command_height(picker, shown, text_width);
        if offset < bottom {
            return Some(shown);
        }
    }
    None
}

fn render_rows(picker: &Picker, surface: &mut Surface, rows: Rect, t: &Theme) {
    if picker.kind == PickerKind::Commands {
        render_commands(picker, surface, rows, t);
    } else {
        render_results(picker, surface, rows, t);
    }
    if picker.shown.is_empty() {
        let message = if picker.searching {
            "Searching…"
        } else if picker.kind == PickerKind::Grep && picker.input.text.is_empty() {
            "Type to search the project"
        } else {
            "No results"
        };
        surface.put_str(
            rows.x + 2,
            rows.y,
            message,
            Style::new().fg(t.dark5).bg(t.bg_dark),
            rows.right(),
        );
    }
}

fn render_commands(picker: &Picker, surface: &mut Surface, rows: Rect, t: &Theme) {
    let group_width = group_column_width(picker, rows);
    let text_width = command_text_width(picker, rows);
    let filtered = !picker.input.text.trim().is_empty();
    let right = rows.right().saturating_sub(1);
    let mut y = rows.y;
    for (row, (index, matched)) in picker.shown.iter().enumerate().skip(picker.scroll) {
        if y >= rows.bottom() {
            break;
        }
        let item = &picker.items[*index];
        let layout = CommandLayout::new(item, text_width);
        let height = (layout.height() as u16).min(rows.bottom() - y);
        let selected = row == picker.selected;
        let bg = if selected { t.bg_highlight } else { t.bg_dark };
        surface.fill(Rect::new(rows.x, y, rows.w, height), Style::new().bg(bg));
        if selected {
            for marker_y in y..y + height {
                surface.set_symbol(rows.x, marker_y, "▌", Style::new().fg(t.magenta));
            }
        }
        let x = rows.x + 2;
        if group_width > 0 {
            let starts_group =
                row == picker.scroll || picker.items[picker.shown[row - 1].0].group != item.group;
            if filtered || starts_group {
                let mut group_x = x;
                for (i, c) in item.group.chars().enumerate() {
                    let style = if matched.binary_search(&(i as u32)).is_ok() {
                        Style::new().fg(t.orange).bg(bg).bold()
                    } else {
                        Style::new().fg(t.blue).bg(bg)
                    };
                    group_x = surface.put_str(group_x, y, &c.to_string(), style, right);
                }
            }
        }
        let label_x = x + group_width;
        let label_offset = item.label_offset();
        for (line_y, (start, line)) in (y..y + height).zip(&layout.lines) {
            let mut line_x = label_x;
            let mut char_index = *start;
            for grapheme in line.graphemes(true) {
                let len = grapheme.chars().count();
                let is_match = (char_index..char_index + len)
                    .any(|c| matched.binary_search(&((c + label_offset) as u32)).is_ok());
                let style = if is_match {
                    Style::new().fg(t.orange).bg(bg).bold()
                } else {
                    Style::new().fg(t.fg).bg(bg)
                };
                line_x = surface.put_str(line_x, line_y, grapheme, style, right);
                char_index += len;
            }
        }
        let keys_y = y + layout.keys_row as u16;
        if !item.detail.is_empty() && keys_y < y + height {
            surface.put_str_right(right, keys_y, &item.detail, Style::new().fg(t.dark5).bg(bg));
        }
        y += height;
    }
}

fn render_results(picker: &Picker, surface: &mut Surface, rows: Rect, t: &Theme) {
    for (row, (index, matched)) in picker
        .shown
        .iter()
        .enumerate()
        .skip(picker.scroll)
        .take(rows.h as usize)
    {
        let y = rows.y + (row - picker.scroll) as u16;
        let item = &picker.items[*index];
        let selected = row == picker.selected;
        let bg = if selected { t.bg_highlight } else { t.bg_dark };
        surface.fill(Rect::new(rows.x, y, rows.w, 1), Style::new().bg(bg));
        if selected {
            surface.set_symbol(rows.x, y, "▌", Style::new().fg(t.magenta));
        }
        let mut x = rows.x + 2;
        let right = rows.right().saturating_sub(1);
        let label_offset = item.label_offset();
        let max_label = match picker.kind {
            PickerKind::Grep | PickerKind::Bookmarks | PickerKind::Definitions
                if !item.detail.is_empty() =>
            {
                rows.w.saturating_sub(4) as usize / 2
            }
            _ => rows.w.saturating_sub(4) as usize,
        };
        let mut graphemes = Vec::new();
        let mut char_index = 0;
        for grapheme in item.label.graphemes(true) {
            let len = grapheme.chars().count();
            graphemes.push((char_index, len, grapheme, text::grapheme_width(grapheme)));
            char_index += len;
        }
        let total: usize = graphemes.iter().map(|g| g.3).sum();
        let mut skip = 0;
        if total > max_label {
            let mut used = 1;
            skip = graphemes.len();
            while skip > 0 && used + graphemes[skip - 1].3 <= max_label {
                used += graphemes[skip - 1].3;
                skip -= 1;
            }
            x = surface.put_str(x, y, "…", Style::new().fg(t.dark5).bg(bg), right);
        }
        let name_start = if picker.kind == PickerKind::Files {
            item.label
                .rfind('/')
                .map(|i| item.label[..=i].chars().count())
                .unwrap_or(0)
        } else {
            usize::MAX
        };
        for &(i, len, grapheme, _) in graphemes.iter().skip(skip) {
            let is_match =
                (i..i + len).any(|c| matched.binary_search(&((c + label_offset) as u32)).is_ok());
            let base_fg = match picker.kind {
                PickerKind::Grep | PickerKind::Bookmarks | PickerKind::Definitions => t.cyan,
                _ if i >= name_start => t.fg,
                PickerKind::Files => t.dark5,
                _ => t.fg,
            };
            let style = if is_match {
                Style::new().fg(t.orange).bg(bg).bold()
            } else {
                Style::new().fg(base_fg).bg(bg)
            };
            x = surface.put_str(x, y, grapheme, style, right);
        }
        if item.detail.is_empty() {
            continue;
        }
        let detail = item.detail.trim_start();
        let trimmed = item.detail.len() - detail.len();
        let mut dx = x + 2;
        let mut byte = trimmed;
        for grapheme in detail.graphemes(true) {
            let emphasized = item
                .detail_match
                .is_some_and(|(s, e)| byte >= s && byte < e);
            let style = if emphasized {
                Style::new().fg(t.orange).bg(bg).bold()
            } else {
                Style::new().fg(t.fg_dark).bg(bg)
            };
            let shown = if grapheme == "\t" { " " } else { grapheme };
            dx = surface.put_str(dx, y, shown, style, right);
            byte += grapheme.len();
            if dx >= right {
                break;
            }
        }
    }
}

fn render_preview(app: &App, surface: &mut Surface, area: Rect) {
    let t = &app.theme;
    let panel = Style::new().fg(t.fg).bg(t.bg_dark);
    surface.fill(area, panel);
    let Some(Overlay::Picker(picker)) = &app.overlay else {
        return;
    };
    let Some(preview) = &app.preview else {
        surface.border(
            area,
            Style::new().fg(t.fg_gutter).bg(t.bg_dark),
            Some("Preview"),
        );
        return;
    };
    let title = app.relative(&preview.path);
    surface.border(
        area,
        Style::new().fg(t.fg_gutter).bg(t.bg_dark),
        Some(&text::truncate_left(
            &title,
            area.w.saturating_sub(6) as usize,
        )),
    );
    let inner = area.inner(1);
    let item = picker.selected_item();
    let target = item.and_then(|i| i.line);
    let first = target
        .map(|t| t.saturating_sub(inner.h as usize / 3))
        .unwrap_or(0);
    let tab_size = app.options.tab_size;
    let number_width = (first + inner.h as usize).to_string().len();
    for (row, line_index) in (first..preview.lines.len())
        .take(inner.h as usize)
        .enumerate()
    {
        let y = inner.y + row as u16;
        let is_target = Some(line_index) == target;
        let bg = if is_target { t.bg_highlight } else { t.bg_dark };
        surface.fill(Rect::new(inner.x, y, inner.w, 1), Style::new().bg(bg));
        let number = format!("{:>number_width$} ", line_index + 1);
        let mut x = surface.put_str(
            inner.x,
            y,
            &number,
            Style::new()
                .fg(if is_target { t.orange } else { t.fg_gutter })
                .bg(bg),
            inner.right(),
        );
        let line = &preview.lines[line_index];
        let styles = preview.styles.get(line_index);
        let mut byte = 0;
        let mut col = 0;
        for grapheme in line.graphemes(true) {
            let width = text::display_width(grapheme, col, tab_size);
            let mut style = Style::new().fg(t.fg).bg(bg);
            if let Some(Some(syntax)) = styles.and_then(|s| s.get(byte)) {
                style = style.patch(Style {
                    bg: None,
                    ..*syntax
                });
            }
            if is_target
                && let Some((s, e)) = item.and_then(|i| i.detail_match)
                && byte >= s
                && byte < e
            {
                style = style.bg(t.bg_search);
            }
            if x as usize + width > inner.right() as usize {
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
