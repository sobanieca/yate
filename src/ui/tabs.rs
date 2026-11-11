//! The tab bar above the editor: one tab per file you opened, the tab of
//! the focused pane highlighted. While Ctrl+O is armed the tabs show the
//! digit that picks them.

use super::surface::{Rect, Style, Surface};
use crate::app::{App, MAX_SWITCH_TARGETS};
use crate::text;
use std::path::Path;

/// Where a tab is drawn, for drawing and for clicks.
pub struct TabCell {
    pub index: usize,
    pub area: Rect,
    /// The `×` (or `●` for unsaved changes) that closes the tab.
    pub close: Rect,
}

struct Label {
    number: String,
    name: String,
    mark: &'static str,
}

impl Label {
    fn width(&self) -> u16 {
        let number = text::str_width(&self.number);
        (number + text::str_width(&self.name) + 4) as u16
    }
}

/// The name of a tab: the file name, with its folder when another tab has
/// the same file name.
fn tab_name(app: &App, path: &Path) -> String {
    let file_name = |path: &Path| {
        path.file_name()
            .map(|name| name.to_string_lossy().into_owned())
    };
    let name = file_name(path).unwrap_or_else(|| app.relative(path));
    let same_name = app
        .tabs
        .iter()
        .filter(|tab| file_name(tab).as_deref() == Some(name.as_str()))
        .count();
    let folder = path.parent().and_then(file_name);
    match folder {
        Some(folder) if same_name > 1 => format!("{folder}/{name}"),
        _ => name,
    }
}

fn labels(app: &App) -> Vec<Label> {
    let numbered = app.switcher.is_some();
    app.tabs
        .iter()
        .enumerate()
        .map(|(index, path)| {
            let modified = app
                .docs
                .iter()
                .any(|doc| doc.path.as_deref() == Some(path) && doc.is_modified());
            let number = if numbered && index + 1 < MAX_SWITCH_TARGETS {
                format!("{} ", index + 2)
            } else {
                String::new()
            };
            Label {
                number,
                name: tab_name(app, path),
                mark: if modified { "●" } else { "×" },
            }
        })
        .collect()
}

/// The first tab drawn: the first one, or a later one so the current tab
/// fits in `room` columns.
fn first_shown(widths: &[u16], current: usize, room: u16) -> usize {
    let mut first = 0;
    while first < current && widths[first..=current].iter().sum::<u16>() > room {
        first += 1;
    }
    first
}

/// The tabs that fit in `area`, and whether more are left and right of
/// them.
fn layout(app: &App, area: Rect) -> (Vec<TabCell>, bool, bool) {
    let widths: Vec<u16> = labels(app).iter().map(Label::width).collect();
    let total: u16 = widths.iter().sum();
    let overflows = total > area.w;
    let (start_x, room) = if overflows {
        (area.x + 1, area.w.saturating_sub(2))
    } else {
        (area.x, area.w)
    };
    let current = app.current_tab().unwrap_or(0);
    let first = first_shown(&widths, current, room);
    let mut cells = Vec::new();
    let mut x = start_x;
    for (index, &width) in widths.iter().enumerate().skip(first) {
        let fits = x + width <= start_x + room;
        if !fits && !cells.is_empty() {
            break;
        }
        let shown = width.min(start_x + room - x);
        cells.push(TabCell {
            index,
            area: Rect::new(x, area.y, shown, 1),
            close: Rect::new(x + width - 2, area.y, u16::from(fits), 1),
        });
        x += shown;
    }
    let more_right = cells
        .last()
        .is_some_and(|cell| cell.index + 1 < widths.len());
    (cells, first > 0, more_right)
}

/// The tabs on screen in `area`, for mouse clicks.
pub fn cells(app: &App, area: Rect) -> Vec<TabCell> {
    layout(app, area).0
}

pub fn render(app: &App, surface: &mut Surface, area: Rect) {
    let t = &app.theme;
    let bar = Style::new().fg(t.dark5).bg(t.bg_dark);
    surface.fill(area, bar);
    let labels = labels(app);
    let (cells, more_left, more_right) = layout(app, area);
    let current = app.current_tab();
    let background = t.background(app.terminal_focused);
    for cell in &cells {
        let label = &labels[cell.index];
        let path = &app.tabs[cell.index];
        let on_screen = app.docs.iter().any(|doc| doc.path.as_ref() == Some(path));
        let (fg, bg) = if current == Some(cell.index) {
            (t.fg, background)
        } else if on_screen {
            (t.fg_dark, t.bg_dark)
        } else {
            (t.dark5, t.bg_dark)
        };
        let base = Style::new().fg(fg).bg(bg);
        surface.fill(cell.area, base);
        let right = cell.area.right();
        let mut x = surface.put_str(
            cell.area.x + 1,
            area.y,
            &label.number,
            base.fg(t.cyan).bold(),
            right,
        );
        let name_style = if current == Some(cell.index) {
            base.bold()
        } else {
            base
        };
        x = surface.put_str(x, area.y, &label.name, name_style, right);
        let mark_color = if label.mark == "●" {
            t.orange
        } else {
            t.dark5
        };
        surface.put_str(x + 1, area.y, label.mark, base.fg(mark_color), right);
    }
    if more_left {
        surface.put_str(area.x, area.y, "‹", bar, area.right());
    }
    if more_right {
        surface.put_str(area.right() - 1, area.y, "›", bar, area.right());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_current_tab_stays_in_view() {
        assert_eq!(first_shown(&[10, 10, 10], 2, 30), 0);
        assert_eq!(first_shown(&[10, 10, 10], 2, 25), 1);
        assert_eq!(first_shown(&[10, 10, 10], 2, 10), 2);
        assert_eq!(first_shown(&[10, 10, 10], 0, 5), 0);
    }
}
