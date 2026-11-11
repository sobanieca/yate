//! Drawing: the cell surface, the theme and one module per view. `render`
//! draws a whole frame and returns where the terminal cursor goes.

pub mod editor_view;
pub mod icons;
pub mod input;
pub mod picker_view;
pub mod sidebar;
pub mod surface;
pub mod theme;

use crate::app::{
    App, Dialog, FindField, FindState, Focus, Layout, MenuItem, MenuSpec, Overlay, PaneArea,
    Prompt, menu_key_label,
};
use crate::text;
use surface::{Rect, Style, Surface};

pub fn layout(app: &App, width: u16, height: u16) -> Layout {
    let area = Rect::new(0, 0, width, height);
    let (main, status) = area.split_bottom(1);
    let (sidebar, rest) = if app.tree_visible {
        let (sidebar, rest) = main.split_left(sidebar::width(app, width));
        (Some(sidebar), rest)
    } else {
        (None, main)
    };
    let (editor_area, find) = if app.find_open {
        let (editor_area, find) = rest.split_bottom(app.find.rows().len() as u16);
        (editor_area, Some(find))
    } else {
        (rest, None)
    };
    let placement = app.panes.place(editor_area);
    let focused = placement
        .panes
        .iter()
        .find(|pane| pane.id == app.panes.focused);
    Layout {
        sidebar,
        editor: focused.map_or(editor_area, |pane| pane.body),
        editor_area,
        panes: placement.panes,
        separators: placement.separators,
        find,
        status,
    }
}

pub fn render(app: &mut App, surface: &mut Surface) -> Option<(u16, u16)> {
    let t = app.theme.clone();
    let layout = app.layout.clone();
    let background = t.background(app.terminal_focused);
    surface.fill(surface.area(), Style::new().fg(t.fg).bg(background));
    let tree_cursor = layout
        .sidebar
        .and_then(|area| sidebar::render(app, surface, area));
    for pane in &layout.panes {
        render_pane_title(app, surface, pane);
        if pane.id != app.panes.focused {
            editor_view::render_unfocused(app, surface, pane.id, pane.body);
        }
    }
    render_separators(surface, &t, &layout, background);
    let mut cursor = editor_view::render(app, surface, layout.editor);
    if app.focus != Focus::Editor {
        cursor = None;
    }
    if app.focus == Focus::Tree {
        cursor = tree_cursor;
    }
    if let Some(area) = layout.find {
        let find_cursor = render_find(app, surface, area);
        if app.focus == Focus::Find {
            cursor = find_cursor;
        }
    }
    render_status(app, surface, layout.status);
    if app.overlay.is_none() && app.switcher_visible() {
        render_switcher(app, surface, &layout);
    }
    let cursor = match &app.overlay {
        Some(Overlay::Picker(_)) => picker_view::render(app, surface),
        Some(Overlay::Prompt(prompt)) => render_prompt(prompt, surface, &app.theme, &layout),
        Some(Overlay::Menu(menu)) if app.menu_visible() => {
            let rows = menu_rows(menu.spec);
            let area = key_popup_area(&layout, &rows);
            render_key_popup(surface, &app.theme, area, menu.spec.title, &rows, None);
            None
        }
        Some(Overlay::Menu(_)) => cursor,
        None => cursor,
    };
    match &app.dialog {
        Some(dialog) => render_dialog(dialog, surface, &app.theme, &layout),
        None => cursor,
    }
}

/// The file of a pane of a split, above the pane.
fn render_pane_title(app: &App, surface: &mut Surface, pane: &PaneArea) {
    let t = &app.theme;
    let area = pane.title;
    if area.h == 0 {
        return;
    }
    let focused = pane.id == app.panes.focused;
    let doc_id = match app.panes.pane(pane.id) {
        Some(_) if focused => app.doc().id,
        Some(shown) => shown.doc_id,
        None => return,
    };
    let Some(doc) = app.docs.iter().find(|doc| doc.id == doc_id) else {
        return;
    };
    surface.fill(area, Style::new().bg(t.bg_dark));
    let name = match &doc.path {
        Some(path) => app.relative(path),
        None => doc.name(),
    };
    let marker = if doc.is_modified() { " ●" } else { "" };
    let width = (area.w as usize).saturating_sub(text::str_width(marker) + 1);
    let title = text::truncate_left(&format!(" {name}"), width);
    let style = if focused {
        Style::new().fg(t.blue).bg(t.bg_dark).bold()
    } else {
        Style::new().fg(t.dark5).bg(t.bg_dark)
    };
    let end = surface.put_str(area.x, area.y, &title, style, area.right());
    let marker_style = Style::new().fg(t.orange).bg(t.bg_dark);
    surface.put_str(end, area.y, marker, marker_style, area.right());
}

/// The columns between side by side panes: a line, or a part of the title
/// bar where two titles meet.
fn render_separators(
    surface: &mut Surface,
    t: &theme::Theme,
    layout: &Layout,
    background: surface::Rgb,
) {
    let in_title = |x: u16, y: u16| layout.panes.iter().any(|pane| pane.title.contains(x, y));
    for area in &layout.separators {
        for y in area.y..area.bottom() {
            if in_title(area.x.saturating_sub(1), y) && in_title(area.x + 1, y) {
                surface.fill(Rect::new(area.x, y, 1, 1), Style::new().bg(t.bg_dark));
            } else {
                let style = Style::new().fg(t.fg_gutter).bg(background);
                surface.set_symbol(area.x, y, "│", style);
            }
        }
    }
}

/// The color of a focus area: its status label, panel title and bar label.
pub fn focus_color(t: &theme::Theme, focus: Focus) -> surface::Rgb {
    match focus {
        Focus::Editor => t.blue,
        Focus::Find => t.yellow,
        Focus::Tree => t.green,
    }
}

fn status_label(app: &App) -> (&'static str, surface::Rgb) {
    let t = &app.theme;
    match &app.dialog {
        Some(dialog) if dialog.error => return (" ERROR ", t.red),
        Some(_) => return (" INFO ", t.cyan),
        None => {}
    }
    match (&app.overlay, app.focus) {
        (Some(Overlay::Picker(_)), _) => (" PICK ", t.cyan),
        (None, _) if app.switcher.is_some() => (" SWITCH ", t.cyan),
        (Some(Overlay::Prompt(_)), _) => (" INPUT ", t.orange),
        (Some(Overlay::Menu(_)), _) => (" MENU ", t.cyan),
        (_, Focus::Editor) => (" EDIT ", focus_color(t, Focus::Editor)),
        (_, Focus::Find) => (" FIND ", focus_color(t, Focus::Find)),
        (_, Focus::Tree) => (" TREE ", focus_color(t, Focus::Tree)),
    }
}

const WIDEST_STATUS_LABEL: usize = 8;

fn status_branch(app: &App) -> Option<String> {
    let branch = app.git.branch.as_ref()?;
    Some(if app.options.nerd_icons {
        format!(" \u{e0a0} {branch} ")
    } else {
        format!(" {branch} ")
    })
}

fn status_position(app: &App) -> String {
    let doc = app.doc();
    let (line, col) = doc.line_col(doc.selection.head);
    format!(" {}:{} ", line + 1, col + 1)
}

/// The width of the longest message the status line shows whole: the
/// file info on its right makes room for it, the label, the branch and
/// the cursor position stay.
pub fn status_message_room(app: &App) -> usize {
    let branch = status_branch(app).map_or(0, |branch| text::str_width(&branch));
    let taken = WIDEST_STATUS_LABEL + branch + text::str_width(&status_position(app)) + 1;
    (app.layout.status.w as usize).saturating_sub(taken)
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
    if let Some(branch) = status_branch(app) {
        x = surface.put_str(
            x,
            area.y,
            &branch,
            Style::new().fg(accent).bg(t.fg_gutter),
            area.right(),
        );
    }
    let doc = app.doc();
    let (line, _) = doc.line_col(doc.selection.head);
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
    let sync = if app.panes.sync_scroll {
        "scroll sync  "
    } else {
        ""
    };
    let right_info = format!(
        " {sync}{selection}{language}  utf-8  {}  {percent}% ",
        if doc.crlf { "CRLF" } else { "LF" }
    );
    let position = status_position(app);
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
    let message = app.message.as_ref();
    let message_text = message.map(|message| format!(" {}", message.text.replace('\n', " ")));
    let message_width = message_text.as_deref().map_or(0, text::str_width);
    let beside_info = position_x.saturating_sub(text::str_width(&right_info) as u16);
    let info_x = if message_width > beside_info.saturating_sub(x + 1) as usize {
        position_x
    } else {
        beside_info
    };
    surface.put_str(
        info_x,
        area.y,
        &right_info,
        Style::new().fg(t.fg_dark).bg(t.bg_dark),
        position_x,
    );

    let middle_width = info_x.saturating_sub(x + 1) as usize;
    if let (Some(message), Some(message_text)) = (message, &message_text) {
        let style = if message.error {
            Style::new().fg(t.red).bg(t.bg_dark).bold()
        } else {
            Style::new().fg(t.green1).bg(t.bg_dark)
        };
        let text = text::truncate_to_width(message_text, middle_width);
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
    if let Some(dialog) = &app.dialog {
        let view = dialog_view(dialog, &app.layout);
        let scrolls = view.lines.len() > view.rows;
        hints.push(scrolls.then(|| "↑↓ scroll".to_string()));
        hints.push(Some("esc close".to_string()));
        return hints.into_iter().flatten().collect();
    }
    if let Some(Overlay::Menu(menu)) = &app.overlay {
        let keys: Vec<String> = menu_rows(menu.spec).into_iter().map(|row| row.0).collect();
        return vec![
            "esc cancel".to_string(),
            format!("{}: {}", menu.spec.title, keys.join(" ")),
        ];
    }
    if app.overlay.is_some() {
        return Vec::new();
    }
    if app.switcher.is_some() {
        hints.push(Some("esc cancel".to_string()));
        hints.push(hint("switch.next", "next file"));
        hints.push(Some("1 tree · 2-9 recent file".to_string()));
        return hints.into_iter().flatten().collect();
    }
    match app.focus {
        Focus::Editor => {
            if app.panes.is_split() {
                hints.push(hint("view.toggleScrollSync", "sync scroll"));
                hints.push(hint("view.focusNextPane", "next pane"));
            }
            hints.push(hint("view.paneMenu", "panes"));
            hints.push(hint("picker.files", "files"));
            hints.push(hint("switch.next", "switch"));
        }
        Focus::Tree => {
            hints.push(hint("file.delete", "delete"));
            hints.push(hint("picker.grepInFolder", "grep here"));
            hints.push(hint("picker.filesInFolder", "files here"));
            hints.push(hint("file.rename", "rename"));
            hints.push(hint("file.new", "new"));
            hints.push(hint("switch.next", "switch"));
        }
        Focus::Find => {}
    }
    hints.push(hint("picker.commands", "commands"));
    if let Some(spec) = app.repeating_menu() {
        let repeating = spec.items.iter().filter(|item| item.repeats);
        let keys: String = repeating
            .filter_map(|item| item.keys.first())
            .map(|key| menu_key_label(key))
            .collect();
        hints.push(Some(format!("ctrl+{keys} again")));
    }
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

/// The numbered targets of Ctrl+O, after `keyHintDelay`.
fn render_switcher(app: &App, surface: &mut Surface, layout: &Layout) {
    let targets = app.switch_targets().into_iter().enumerate();
    let rows: Vec<(String, String)> = targets
        .map(|(i, target)| ((i + 1).to_string(), target))
        .collect();
    let current = app.switch_target_focused();
    let area = key_popup_area(layout, &rows);
    render_key_popup(surface, &app.theme, area, "Switch to", &rows, current);
}

/// The rows of a menu: the keys of each item and its label.
fn menu_rows(spec: &MenuSpec) -> Vec<(String, String)> {
    let keys = |item: &MenuItem| {
        let labels: Vec<&str> = item.keys.iter().map(|key| menu_key_label(key)).collect();
        labels.join(" ")
    };
    let items = spec.items.iter();
    items
        .map(|item| (keys(item), item.label.to_string()))
        .collect()
}

/// Where the popup of a menu is drawn, for drawing and for mouse clicks.
pub fn menu_area(spec: &MenuSpec, layout: &Layout) -> Rect {
    key_popup_area(layout, &menu_rows(spec))
}

fn key_column_width(rows: &[(String, String)]) -> usize {
    let widths = rows.iter().map(|(keys, _)| text::str_width(keys));
    widths.max().unwrap_or(1)
}

fn key_popup_area(layout: &Layout, rows: &[(String, String)]) -> Rect {
    let screen = Rect::new(0, 0, layout.status.w, layout.status.y + 1);
    let keys = key_column_width(rows);
    let width = rows
        .iter()
        .map(|(_, label)| text::str_width(label) + keys + 7)
        .max()
        .unwrap_or(20)
        .max(24)
        .min(screen.w.saturating_sub(2) as usize) as u16;
    let height = (rows.len() as u16 + 2).min(screen.h.saturating_sub(2));
    Rect::new(
        screen.x + screen.w.saturating_sub(width) / 2,
        screen.y + 1,
        width,
        height,
    )
}

/// A bordered popup of rows chosen with the key in front of each.
fn render_key_popup(
    surface: &mut Surface,
    t: &theme::Theme,
    area: Rect,
    title: &str,
    rows: &[(String, String)],
    current: Option<usize>,
) {
    surface.fill(area, Style::new().fg(t.fg).bg(t.bg_dark));
    surface.border(area, Style::new().fg(t.blue).bg(t.bg_dark), Some(title));
    let keys = key_column_width(rows) as u16;
    for (i, (key, label)) in rows.iter().enumerate() {
        let y = area.y + 1 + i as u16;
        if y + 1 >= area.bottom() {
            break;
        }
        let bg = if current == Some(i) {
            t.bg_highlight
        } else {
            t.bg_dark
        };
        let row = Rect::new(area.x + 1, y, area.w.saturating_sub(2), 1);
        surface.fill(row, Style::new().bg(bg));
        let key_style = Style::new().fg(t.orange).bg(bg).bold();
        surface.put_str(row.x + 1, y, key, key_style, row.right());
        let label_x = row.x + 1 + keys + 2;
        surface.put_str(label_x, y, label, Style::new().fg(t.fg).bg(bg), row.right());
    }
}

/// The style of a checkbox of a form: the find bar, the tree filters.
pub fn checkbox_style(t: &theme::Theme, ticked: bool, focused: bool) -> Style {
    match (ticked, focused) {
        (true, true) => Style::new().fg(t.cyan).bg(t.bg_highlight).bold(),
        (true, false) => Style::new().fg(t.cyan).bg(t.bg_dark).bold(),
        (false, true) => Style::new().fg(t.fg).bg(t.bg_highlight),
        (false, false) => Style::new().fg(t.dark5).bg(t.bg_dark),
    }
}

const FIND_LABEL: &str = " FIND ";
const FIND_COUNT_WIDTH: u16 = 12;
const FIND_TEXT_WIDTH: u16 = 10;

fn find_options_width(find: &FindState, gap: u16) -> u16 {
    let rows = find.rows();
    let options = rows[0].iter().filter(|field| !field.is_text());
    let widths = options.map(|field| find_control_width(*field) + gap);
    widths.sum::<u16>() + FIND_COUNT_WIDTH
}

/// The space between the controls of the find bar: less on a narrow screen.
fn find_gap(find: &FindState, area: Rect) -> u16 {
    let label = text::str_width(FIND_LABEL) as u16 + 1;
    let roomy = label + FIND_TEXT_WIDTH + find_options_width(find, 2);
    if area.w >= roomy { 2 } else { 1 }
}

fn find_control_width(field: FindField) -> u16 {
    let label = text::str_width(field.label()) as u16;
    match field {
        FindField::Query | FindField::Replacement => 0,
        FindField::Case | FindField::Regex | FindField::Replace => label + 4,
        FindField::ReplaceOne | FindField::ReplaceAll => label + 2,
    }
}

/// Where each control of the find bar is drawn, for drawing and for mouse
/// clicks: a text field, then the checkboxes or buttons of its row.
pub fn find_controls(find: &FindState, area: Rect) -> Vec<(FindField, Rect)> {
    let rows = find.rows();
    let gap = find_gap(find, area);
    let label = text::str_width(FIND_LABEL) as u16 + 1;
    let options = find_options_width(find, gap);
    let widest = (area.w / 3).max(12);
    let text_width = area.w.saturating_sub(label + options);
    let text_width = text_width.clamp(FIND_TEXT_WIDTH, widest);
    let mut controls = Vec::new();
    for (i, row) in rows.iter().enumerate() {
        let mut x = area.x + label;
        for field in row {
            let width = match field.is_text() {
                true => text_width,
                false => find_control_width(*field),
            };
            let shown = width.min(area.right().saturating_sub(x));
            controls.push((*field, Rect::new(x, area.y + i as u16, shown, 1)));
            x += width + gap;
        }
    }
    controls
}

fn render_find(app: &App, surface: &mut Surface, area: Rect) -> Option<(u16, u16)> {
    let t = &app.theme;
    let find = &app.find;
    let accent = focus_color(t, Focus::Find);
    surface.fill(area, Style::new().fg(t.fg).bg(t.bg_dark));
    let label_style = Style::new().fg(t.bg_dark).bg(accent).bold();
    surface.put_str(area.x, area.y, FIND_LABEL, label_style, area.right());
    let focused = (app.focus == Focus::Find).then_some(find.field);
    let field_style = Style::new().fg(t.fg).bg(t.bg_highlight);
    let mut cursor = None;
    let mut count_x = area.x;
    let gap = find_gap(find, area);
    for (field, rect) in find_controls(find, area) {
        let has_focus = focused == Some(field);
        let (x, y) = (rect.x, rect.y);
        match field {
            FindField::Query | FindField::Replacement => {
                let input = match field {
                    FindField::Query => &find.input,
                    _ => &find.replace,
                };
                surface.fill(rect, field_style);
                let width = rect.w.saturating_sub(2) as usize;
                let (shown, cursor_col) = input.visible(width);
                let is_query = field == FindField::Query;
                let style = match (is_query && find.error, input.selected) {
                    (true, _) => field_style.fg(t.red),
                    (false, true) => field_style.bg(t.bg_visual),
                    (false, false) => field_style,
                };
                let right = rect.right().saturating_sub(1);
                if input.text.is_empty() && !is_query {
                    surface.put_str(x + 1, y, "replace with", field_style.fg(t.dark3), right);
                }
                surface.put_str(x + 1, y, &shown, style, right);
                if has_focus {
                    cursor = Some((x + 1 + cursor_col as u16, y));
                }
            }
            FindField::Case | FindField::Regex | FindField::Replace => {
                let ticked = match field {
                    FindField::Case => find.case_sensitive,
                    FindField::Regex => find.regex,
                    _ => find.replace_open,
                };
                let mark = if ticked { "x" } else { " " };
                let style = checkbox_style(t, ticked, has_focus);
                let checkbox = format!("[{mark}] {}", field.label());
                count_x = surface.put_str(x, y, &checkbox, style, area.right());
                if has_focus {
                    cursor = Some((x + 1, y));
                }
            }
            FindField::ReplaceOne | FindField::ReplaceAll => {
                let style = if has_focus {
                    Style::new().fg(t.bg_dark).bg(accent).bold()
                } else {
                    field_style
                };
                let button = format!(" {} ", field.label());
                surface.put_str(x, y, &button, style, area.right());
                if has_focus {
                    cursor = Some((x + 1, y));
                }
            }
        }
    }
    let count = match (find.current, find.matches.len()) {
        (_, 0) if !find.input.text.is_empty() => "no matches".to_string(),
        (Some(current), total) => format!("{} of {total}", current + 1),
        (None, total) if total > 0 => format!("{total}"),
        _ => String::new(),
    };
    let count_style = Style::new().fg(t.fg_dark).bg(t.bg_dark);
    let x = surface.put_str(count_x + gap, area.y, &count, count_style, area.right());
    let hint = "↑↓ matches · tab fields";
    let hint_x = area
        .right()
        .saturating_sub(text::str_width(hint) as u16 + 1);
    if hint_x >= x + gap {
        let hint_style = Style::new().fg(t.dark3).bg(t.bg_dark);
        surface.put_str(hint_x, area.y, hint, hint_style, area.right());
    }
    cursor
}

const DIALOG_BUTTON: &str = "Close";
const DIALOG_WIDEST: u16 = 100;
const DIALOG_NARROWEST: usize = 30;

/// A message dialog on screen, for drawing, scrolling and mouse clicks.
pub struct DialogView {
    pub area: Rect,
    /// The text, wrapped to the width of the dialog.
    pub lines: Vec<String>,
    /// How many lines of the text are on screen.
    pub rows: usize,
    /// The Close button; empty when the screen has no row for it.
    pub button: Rect,
}

pub fn dialog_view(dialog: &Dialog, layout: &Layout) -> DialogView {
    let screen = Rect::new(0, 0, layout.status.w, layout.status.y);
    let widest = screen
        .w
        .saturating_sub(4)
        .clamp(screen.w.min(12), DIALOG_WIDEST);
    let lines = text::wrap(&dialog.text, widest.saturating_sub(4) as usize);
    let longest = lines.iter().map(|line| text::str_width(line)).max();
    let title = text::str_width(&dialog.title) + 6;
    let width = (longest.unwrap_or(0) + 4).max(title).max(DIALOG_NARROWEST);
    let width = (width as u16).min(widest);
    let chrome = match screen.h {
        0..=3 => 2,
        4..=6 => 3,
        _ => 4,
    };
    let rows = lines.len().min(screen.h.saturating_sub(chrome) as usize);
    let rows = rows.max(1);
    let height = (rows as u16 + chrome).min(screen.h);
    let area = Rect::new(
        screen.w.saturating_sub(width) / 2,
        screen.h.saturating_sub(height) / 3,
        width,
        height,
    );
    let button_width = match chrome {
        2 => 0,
        _ => text::str_width(DIALOG_BUTTON) as u16 + 2,
    };
    let button = Rect::new(area.x + 2, area.bottom().saturating_sub(2), button_width, 1);
    DialogView {
        area,
        lines,
        rows,
        button,
    }
}

fn render_dialog(
    dialog: &Dialog,
    surface: &mut Surface,
    t: &theme::Theme,
    layout: &Layout,
) -> Option<(u16, u16)> {
    let view = dialog_view(dialog, layout);
    let area = view.area;
    let accent = if dialog.error { t.red } else { t.blue };
    surface.fill(area, Style::new().fg(t.fg).bg(t.bg_dark));
    let title = (!dialog.title.is_empty()).then_some(dialog.title.as_str());
    surface.border(area, Style::new().fg(accent).bg(t.bg_dark), title);
    let right = area.right().saturating_sub(2);
    let last = view.lines.len().saturating_sub(view.rows);
    let scroll = dialog.scroll.min(last);
    let shown = view.lines.iter().skip(scroll).take(view.rows);
    for (i, line) in shown.enumerate() {
        let style = Style::new().fg(t.fg).bg(t.bg_dark);
        surface.put_str(area.x + 2, area.y + 1 + i as u16, line, style, right);
    }
    let button = view.button;
    if button.w == 0 {
        return None;
    }
    let button_style = Style::new().fg(t.bg_dark).bg(accent).bold();
    let label = format!(" {DIALOG_BUTTON} ");
    surface.put_str(button.x, button.y, &label, button_style, right);
    if last > 0 {
        let end = scroll + view.rows;
        let range = format!("↑↓ {}-{end} of {}", scroll + 1, view.lines.len());
        let range_style = Style::new().fg(t.dark5).bg(t.bg_dark);
        if button.right() + 2 + text::str_width(&range) as u16 <= right {
            surface.put_str_right(right, button.y, &range, range_style);
        }
    }
    Some((button.x + 1, button.y))
}

fn prompt_area(prompt: &Prompt, layout: &Layout) -> Rect {
    let screen = Rect::new(0, 0, layout.status.w, layout.status.y + 1);
    let width = (text::str_width(&prompt.title) as u16 + 6)
        .clamp(50.min(screen.w), screen.w.saturating_sub(4).max(10));
    Rect::new(
        screen.x + (screen.w.saturating_sub(width)) / 2,
        screen.h / 4,
        width,
        3,
    )
}

/// Where the buttons of a confirmation are drawn, for drawing and for
/// mouse clicks.
pub fn prompt_buttons(prompt: &Prompt, layout: &Layout) -> Vec<Rect> {
    let area = prompt_area(prompt, layout);
    let mut x = area.x + 2;
    let mut buttons = Vec::new();
    for choice in prompt.choices() {
        let width = text::str_width(choice.label) as u16 + 2;
        buttons.push(Rect::new(x, area.y + 1, width, 1));
        x += width + 1;
    }
    buttons
}

fn render_prompt(
    prompt: &Prompt,
    surface: &mut Surface,
    t: &theme::Theme,
    layout: &Layout,
) -> Option<(u16, u16)> {
    let area = prompt_area(prompt, layout);
    surface.fill(area, Style::new().fg(t.fg).bg(t.bg_dark));
    surface.border(
        area,
        Style::new().fg(t.blue).bg(t.bg_dark),
        Some(&prompt.title),
    );
    if prompt.confirm {
        let buttons = prompt_buttons(prompt, layout);
        let right = area.right() - 1;
        for (i, (choice, button)) in prompt.choices().iter().zip(&buttons).enumerate() {
            let style = if i == prompt.choice {
                Style::new().fg(t.bg_dark).bg(t.blue).bold()
            } else {
                Style::new().fg(t.fg).bg(t.bg_highlight)
            };
            let label = format!(" {} ", choice.label);
            surface.put_str(button.x, button.y, &label, style, right);
            let letter: String = choice.label.chars().take(1).collect();
            surface.put_str(button.x + 1, button.y, &letter, style.underline(), right);
        }
        let focused = buttons.get(prompt.choice)?;
        return Some((focused.x + 1, focused.y));
    }
    let inner_width = area.w.saturating_sub(4) as usize;
    let (shown, cursor) = prompt.input.visible(inner_width);
    surface.put_str(
        area.x + 2,
        area.y + 1,
        &shown,
        Style::new().fg(t.fg).bg(t.bg_dark),
        area.right() - 2,
    );
    Some((area.x + 2 + cursor as u16, area.y + 1))
}
