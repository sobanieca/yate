//! Drawing: the cell surface, the theme and one module per view. `render`
//! draws a whole frame and returns where the terminal cursor goes.

pub mod diff_view;
pub mod editor_view;
pub mod icons;
pub mod input;
pub mod picker_view;
pub mod sidebar;
pub mod surface;
pub mod theme;

use crate::app::{App, Focus, Layout, Overlay};
use crate::text;
use surface::{Rect, Style, Surface};

pub fn layout(app: &App, width: u16, height: u16) -> Layout {
    let area = Rect::new(0, 0, width, height);
    let (main, status) = area.split_bottom(1);
    let (sidebar, rest) = if app.review.is_some() || app.tree_visible {
        let (sidebar, rest) = main.split_left(sidebar::width(app, width));
        (Some(sidebar), rest)
    } else {
        (None, main)
    };
    let (tabs, body) = rest.split_top(1);
    let (editor, find) = if app.find_open && app.review.is_none() {
        let (editor, find) = body.split_bottom(1);
        (editor, Some(find))
    } else {
        (body, None)
    };
    let (editor, other, separator) = match &app.split {
        Some(split) if app.review.is_none() && app.other_pane_doc().is_some() && editor.w > 20 => {
            let (left, rest) = editor.split_left((editor.w - 1) / 2);
            let (separator, right) = rest.split_left(1);
            if split.focus_right {
                (right, Some(left), Some(separator))
            } else {
                (left, Some(right), Some(separator))
            }
        }
        _ => (editor, None, None),
    };
    Layout {
        sidebar,
        tabs,
        editor,
        other,
        separator,
        find,
        status,
    }
}

pub fn render(app: &mut App, surface: &mut Surface) -> Option<(u16, u16)> {
    let t = app.theme.clone();
    let layout = app.layout;
    let background = t.background(app.terminal_focused);
    surface.fill(surface.area(), Style::new().fg(t.fg).bg(background));
    if let Some(area) = layout.sidebar {
        sidebar::render(app, surface, area);
    }
    let mut cursor = if app.review.is_some() {
        diff_view::render_header(app, surface, layout.tabs);
        diff_view::render(app, surface, layout.editor);
        None
    } else {
        render_tabs(app, surface, layout.tabs);
        if let Some(area) = layout.other {
            editor_view::render_other(app, surface, area);
        }
        if let Some(area) = layout.separator {
            let style = Style::new().fg(t.fg_gutter).bg(background);
            for y in area.y..area.bottom() {
                surface.set_symbol(area.x, y, "│", style);
            }
        }
        editor_view::render(app, surface, layout.editor)
    };
    if app.focus != Focus::Editor {
        cursor = None;
    }
    if let Some(area) = layout.find {
        let find_cursor = render_find(app, surface, area);
        if app.focus == Focus::Find {
            cursor = find_cursor;
        }
    }
    render_status(app, surface, layout.status);
    if app.overlay.is_none() && app.switcher_visible() {
        render_switcher(app, surface, layout);
    }
    match &app.overlay {
        Some(Overlay::Picker(_)) => picker_view::render(app, surface),
        Some(Overlay::Prompt(prompt)) => render_prompt(prompt, surface, &app.theme),
        None => cursor,
    }
}

/// Tab titles with the screen columns they occupy.
fn tab_spans(app: &App, area: Rect) -> Vec<(usize, u16, u16, String)> {
    let mut spans = Vec::new();
    let mut x = area.x;
    for (index, doc) in app.docs.iter().enumerate() {
        let marker = if doc.is_modified() { " ●" } else { "" };
        let title = format!(" {}{marker} ", doc.name());
        let width = text::str_width(&title) as u16;
        spans.push((index, x, width, title));
        x += width + 1;
    }
    let active_end = spans.get(app.active).map(|(_, x, w, _)| x + w).unwrap_or(0);
    if active_end > area.right() {
        let shift = active_end - area.right();
        spans.retain_mut(|(_, x, _, _)| {
            if *x < area.x + shift {
                return false;
            }
            *x -= shift;
            true
        });
    }
    spans
}

pub fn tab_at(app: &App, column: u16) -> Option<usize> {
    tab_spans(app, app.layout.tabs)
        .into_iter()
        .find(|(_, x, w, _)| column >= *x && column < x + w)
        .map(|(index, ..)| index)
}

fn render_tabs(app: &App, surface: &mut Surface, area: Rect) {
    let t = &app.theme;
    surface.fill(area, Style::new().bg(t.bg_dark));
    let other = app.other_pane_doc();
    for (index, x, _, title) in tab_spans(app, area) {
        let style = if index == app.active {
            Style::new()
                .fg(t.blue)
                .bg(t.background(app.terminal_focused))
                .bold()
        } else if Some(index) == other {
            Style::new()
                .fg(t.fg_dark)
                .bg(t.background(app.terminal_focused))
        } else {
            Style::new().fg(t.dark5).bg(t.bg_dark)
        };
        let end = surface.put_str(x, area.y, &title, style, area.right());
        if app.docs[index].is_modified() {
            let dot_x = end.saturating_sub(2);
            surface.put_str(dot_x, area.y, "●", Style::new().fg(t.orange), area.right());
        }
    }
}

/// The color of a focus area: its status label, panel title and bar label.
pub fn focus_color(t: &theme::Theme, focus: Focus) -> surface::Rgb {
    match focus {
        Focus::Editor => t.blue,
        Focus::Find => t.yellow,
        Focus::Tree => t.green,
        Focus::Review => t.magenta,
    }
}

fn status_label(app: &App) -> (&'static str, surface::Rgb) {
    let t = &app.theme;
    match (&app.overlay, app.focus) {
        (Some(Overlay::Picker(_)), _) => (" PICK ", t.cyan),
        (None, _) if app.switcher.is_some() => (" SWITCH ", t.cyan),
        (Some(Overlay::Prompt(_)), _) => (" INPUT ", t.orange),
        (_, Focus::Editor) => (" EDIT ", focus_color(t, Focus::Editor)),
        (_, Focus::Find) => (" FIND ", focus_color(t, Focus::Find)),
        (_, Focus::Tree) => (" TREE ", focus_color(t, Focus::Tree)),
        (_, Focus::Review) => (" REVIEW ", focus_color(t, Focus::Review)),
    }
}

fn render_status(app: &App, surface: &mut Surface, area: Rect) {
    let t = &app.theme;
    let (label, accent) = status_label(app);
    surface.fill(area, Style::new().fg(accent).bg(t.bg_dark));
    let mut x = surface.put_str(
        area.x,
        area.y,
        label,
        Style::new().fg(t.bg_dark).bg(accent).bold(),
        area.right(),
    );
    if let Some(branch) = &app.git.branch {
        let branch = if app.options.nerd_icons {
            format!(" \u{e0a0} {branch} ")
        } else {
            format!(" {branch} ")
        };
        x = surface.put_str(
            x,
            area.y,
            &branch,
            Style::new().fg(accent).bg(t.fg_gutter),
            area.right(),
        );
    }
    let doc = app.doc();
    let (line, col) = doc.line_col(doc.selection.head);
    let percent = if doc.line_count() <= 1 {
        100
    } else {
        (line * 100 / (doc.line_count() - 1)).min(100)
    };
    let language = doc.language.map(|l| l.name).unwrap_or("text");
    let selection = if doc.selection.is_empty() {
        String::new()
    } else {
        let count = doc.selection.end() - doc.selection.start();
        format!("({count} selected) ")
    };
    let sync = if app.split.as_ref().is_some_and(|s| s.sync_scroll) {
        "scroll sync  "
    } else {
        ""
    };
    let right_info = format!(
        " {sync}{selection}{language}  utf-8  {}  {percent}% ",
        if doc.crlf { "CRLF" } else { "LF" }
    );
    let position = format!(" {}:{} ", line + 1, col + 1);
    let position_x = area
        .right()
        .saturating_sub(text::str_width(&position) as u16);
    surface.put_str(
        position_x,
        area.y,
        &position,
        Style::new().fg(t.bg_dark).bg(accent).bold(),
        area.right(),
    );
    let info_x = position_x.saturating_sub(text::str_width(&right_info) as u16);
    surface.put_str(
        info_x,
        area.y,
        &right_info,
        Style::new().fg(t.fg_dark).bg(t.bg_dark),
        position_x,
    );

    let middle_width = info_x.saturating_sub(x + 1) as usize;
    if let Some(message) = &app.message {
        let style = if message.error {
            Style::new().fg(t.red).bg(t.bg_dark).bold()
        } else {
            Style::new().fg(t.green1).bg(t.bg_dark)
        };
        let text = text::truncate_to_width(&format!(" {}", message.text), middle_width);
        surface.put_str(x, area.y, &text, style, info_x);
        return;
    }
    let name = doc
        .path
        .as_ref()
        .map(|p| app.relative(p))
        .unwrap_or_else(|| "untitled".to_string());
    let mut flags = String::new();
    if doc.is_modified() {
        flags.push_str(" ●");
    }
    if doc.readonly {
        flags.push_str(" [read-only]");
    }
    let path_width = text::str_width(&format!(" {name}{flags}"));
    let hint = fit_hint(
        &status_hints(app),
        middle_width.saturating_sub(path_width + 2),
    );
    let hint_width = text::str_width(&hint);
    if !hint.is_empty() {
        surface.put_str(
            info_x.saturating_sub(hint_width as u16),
            area.y,
            &hint,
            Style::new().fg(t.dark5).bg(t.bg_dark),
            info_x,
        );
    }
    let path = text::truncate_left(
        &format!(" {name}"),
        middle_width.saturating_sub(flags.len() + hint_width),
    );
    let end = surface.put_str(
        x,
        area.y,
        &path,
        Style::new().fg(t.fg).bg(t.bg_dark),
        info_x,
    );
    surface.put_str(
        end,
        area.y,
        &flags,
        Style::new().fg(t.orange).bg(t.bg_dark),
        info_x,
    );
}

/// Key hints for the status line, most important last: the ones that do
/// not fit are dropped from the front.
fn status_hints(app: &App) -> Vec<String> {
    let key = |command: &str| app.keymap.hint(command);
    let hint = |command: &str, what: &str| key(command).map(|k| format!("{k} {what}"));
    let mut hints: Vec<Option<String>> = Vec::new();
    if app.overlay.is_some() {
        return Vec::new();
    }
    if app.switcher.is_some() {
        hints.push(Some("esc cancel".to_string()));
        hints.push(hint("switch.next", "next file"));
        hints.push(Some("1 tree · 2-9 open file".to_string()));
        return hints.into_iter().flatten().collect();
    }
    match app.focus {
        Focus::Editor => {
            if app.split.is_some() {
                hints.push(hint("view.toggleScrollSync", "sync scroll"));
                hints.push(hint("view.focusOtherPane", "other pane"));
            } else {
                hints.push(hint("view.split", "split"));
            }
            hints.push(hint("picker.commands", "commands"));
            hints.push(hint("picker.files", "files"));
            hints.push(hint("switch.next", "switch"));
        }
        Focus::Tree => {
            hints.push(hint("file.compare", "compare"));
            hints.push(hint("file.delete", "delete"));
            hints.push(hint("picker.grepInFolder", "grep here"));
            hints.push(hint("picker.filesInFolder", "files here"));
            hints.push(hint("file.rename", "rename"));
            hints.push(hint("file.new", "new"));
            hints.push(hint("switch.next", "switch"));
        }
        Focus::Review => {
            hints.push(hint("switch.next", "switch"));
        }
        Focus::Find => {}
    }
    hints.push(hint("help.keys", "keys"));
    hints.into_iter().flatten().collect()
}

fn fit_hint(hints: &[String], width: usize) -> String {
    for skip in 0..hints.len() {
        let text = format!("{}  ", hints[skip..].join(" · "));
        if text::str_width(&text) <= width {
            return text;
        }
    }
    String::new()
}

/// The numbered targets of Alt+W, after `keyHintDelay`.
fn render_switcher(app: &App, surface: &mut Surface, layout: Layout) {
    let t = &app.theme;
    let targets = app.switch_targets();
    let screen = surface.area();
    let width = targets
        .iter()
        .map(|target| text::str_width(target) + 8)
        .max()
        .unwrap_or(20)
        .max(24)
        .min(screen.w.saturating_sub(2) as usize) as u16;
    let height = (targets.len() as u16 + 2).min(screen.h.saturating_sub(2));
    let area = Rect::new(
        screen.x + screen.w.saturating_sub(width) / 2,
        layout.tabs.y + 1,
        width,
        height,
    );
    surface.fill(area, Style::new().fg(t.fg).bg(t.bg_dark));
    surface.border(
        area,
        Style::new().fg(t.blue).bg(t.bg_dark),
        Some("Switch to"),
    );
    let current = match app.focus {
        Focus::Tree | Focus::Review => 0,
        _ => app.active + 1,
    };
    for (i, target) in targets.iter().enumerate() {
        let y = area.y + 1 + i as u16;
        if y + 1 >= area.bottom() {
            break;
        }
        let bg = if i == current {
            t.bg_highlight
        } else {
            t.bg_dark
        };
        let row = Rect::new(area.x + 1, y, area.w.saturating_sub(2), 1);
        surface.fill(row, Style::new().bg(bg));
        let x = surface.put_str(
            row.x + 1,
            y,
            &(i + 1).to_string(),
            Style::new().fg(t.orange).bg(bg).bold(),
            row.right(),
        );
        surface.put_str(x + 2, y, target, Style::new().fg(t.fg).bg(bg), row.right());
    }
}

fn render_find(app: &App, surface: &mut Surface, area: Rect) -> Option<(u16, u16)> {
    let t = &app.theme;
    let find = &app.find;
    surface.fill(area, Style::new().fg(t.fg).bg(t.bg_dark));
    let label_style = Style::new()
        .fg(t.bg_dark)
        .bg(focus_color(t, Focus::Find))
        .bold();
    let mut x = surface.put_str(area.x, area.y, " FIND ", label_style, area.right());
    x += 1;
    let input_width = (area.w / 3).max(12);
    let input_style = Style::new().fg(t.fg).bg(t.bg_highlight);
    surface.fill(Rect::new(x, area.y, input_width, 1), input_style);
    let query_style = match (find.error, find.input.selected) {
        (true, _) => input_style.fg(t.red),
        (false, true) => input_style.bg(t.bg_visual),
        (false, false) => input_style,
    };
    surface.put_str(
        x + 1,
        area.y,
        &find.input.text,
        query_style,
        x + input_width,
    );
    let mut cursor = Some((x + 1 + find.input.cursor_col() as u16, area.y));
    x += input_width + 1;
    let toggle = |on: bool| {
        if on {
            Style::new().fg(t.bg_dark).bg(t.cyan).bold()
        } else {
            Style::new().fg(t.dark5).bg(t.bg_dark)
        }
    };
    x = surface.put_str(x, area.y, " Aa ", toggle(find.case_sensitive), area.right());
    x = surface.put_str(x, area.y, " .* ", toggle(find.regex), area.right());
    let count = match (find.current, find.matches.len()) {
        (_, 0) if !find.input.text.is_empty() => " no matches".to_string(),
        (Some(current), total) => format!(" {} of {total}", current + 1),
        (None, total) if total > 0 => format!(" {total}"),
        _ => String::new(),
    };
    x = surface.put_str(
        x,
        area.y,
        &count,
        Style::new().fg(t.fg_dark).bg(t.bg_dark),
        area.right(),
    );
    if find.replace_open {
        x += 2;
        x = surface.put_str(
            x,
            area.y,
            "REPLACE ",
            Style::new().fg(t.magenta).bg(t.bg_dark).bold(),
            area.right(),
        );
        surface.fill(
            Rect::new(
                x,
                area.y,
                input_width.min(area.right().saturating_sub(x)),
                1,
            ),
            input_style,
        );
        surface.put_str(
            x + 1,
            area.y,
            &find.replace.text,
            input_style,
            x + input_width,
        );
        if find.replace_focused {
            cursor = Some((x + 1 + find.replace.cursor_col() as u16, area.y));
        }
        x += input_width;
    }
    let hint = if find.replace_open {
        "  enter replace · alt+a all · tab switch · alt+c case · alt+r regex"
    } else {
        "  enter/↓ next · ↑ prev · alt+c case · alt+r regex"
    };
    surface.put_str(
        x,
        area.y,
        hint,
        Style::new().fg(t.dark3).bg(t.bg_dark),
        area.right(),
    );
    cursor
}

fn render_prompt(
    prompt: &crate::app::Prompt,
    surface: &mut Surface,
    t: &theme::Theme,
) -> Option<(u16, u16)> {
    let screen = surface.area();
    let width = (text::str_width(&prompt.title) as u16 + 6)
        .clamp(50.min(screen.w), screen.w.saturating_sub(4).max(10));
    let area = Rect::new(
        screen.x + (screen.w.saturating_sub(width)) / 2,
        screen.h / 4,
        width,
        3,
    );
    surface.fill(area, Style::new().fg(t.fg).bg(t.bg_dark));
    surface.border(
        area,
        Style::new().fg(t.blue).bg(t.bg_dark),
        Some(&prompt.title),
    );
    if prompt.confirm {
        surface.put_str(
            area.x + 2,
            area.y + 1,
            "y / n",
            Style::new().fg(t.dark5).bg(t.bg_dark),
            area.right() - 1,
        );
        return None;
    }
    let inner_width = area.w.saturating_sub(4) as usize;
    let cursor = prompt.input.cursor_col();
    let shown = if cursor >= inner_width {
        let skip = cursor + 1 - inner_width;
        prompt.input.text.chars().skip(skip).collect::<String>()
    } else {
        prompt.input.text.clone()
    };
    surface.put_str(
        area.x + 2,
        area.y + 1,
        &shown,
        Style::new().fg(t.fg).bg(t.bg_dark),
        area.right() - 2,
    );
    Some((
        area.x + 2 + cursor.min(inner_width.saturating_sub(1)) as u16,
        area.y + 1,
    ))
}
