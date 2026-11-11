//! Keyboard, mouse and paste handling. Overlays get keys first, then key
//! bindings, then the focused panel.

use super::commands::{self, edit};
use super::{App, Focus, Menu, Overlay, Prompt};
use crate::editor::history::EditKind;
use crate::editor::ops;
use crate::editor::selection::Selection;
use crate::keymap;
use crate::text;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use std::time::{Duration, Instant};

const DOUBLE_CLICK: Duration = Duration::from_millis(400);

impl App {
    pub fn handle_key(&mut self, key: KeyEvent) {
        if self.overlay.is_some() {
            self.switcher = None;
            self.overlay_key(key);
            return;
        }
        let name = keymap::key_name(&key);
        let context = self.context();
        if self.switcher.is_some() {
            let command = self.keymap.lookup(context, &name).map(str::to_string);
            if self.switcher_key(key, command.as_deref()) {
                return;
            }
        }
        let text_input_focused = self.focus == Focus::Find;
        let plain_char = matches!(key.code, KeyCode::Char(_))
            && !key
                .modifiers
                .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT);
        let editing_key_in_find = text_input_focused
            && (plain_char
                || matches!(
                    name.as_str(),
                    "ctrl+h" | "ctrl+backspace" | "alt+backspace" | "ctrl+a"
                ));
        if !editing_key_in_find && let Some(command) = self.keymap.lookup(context, &name) {
            let command = command.to_string();
            let editor_only = command.starts_with("edit.");
            if !editor_only || self.focus == Focus::Editor {
                self.run_command(&command);
                return;
            }
        }
        match self.focus {
            Focus::Editor => self.editor_key(key),
            Focus::Find => self.find_key(key),
            Focus::Tree => self.tree_key(key),
        }
    }

    fn editor_key(&mut self, key: KeyEvent) {
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        let alt = key.modifiers.contains(KeyModifiers::ALT);
        let shift = key.modifiers.contains(KeyModifiers::SHIFT);
        let options = self.options.clone();
        let page = self.editor_height().saturating_sub(2).max(1) as isize;
        let tab_size = options.tab_size;
        let jump_lines = options.jump_lines as isize;
        match key.code {
            KeyCode::Char(c) if !ctrl && !alt => {
                edit(self, |doc, options| ops::type_char(doc, c, options));
                return;
            }
            KeyCode::Enter if !ctrl && !alt => {
                edit(self, ops::newline);
                return;
            }
            KeyCode::Tab => {
                edit(self, ops::tab);
                return;
            }
            KeyCode::BackTab => {
                edit(self, ops::outdent);
                return;
            }
            KeyCode::Backspace => {
                edit(self, ops::backspace);
                return;
            }
            KeyCode::Delete => {
                edit(self, |doc, _| ops::delete_forward(doc));
                return;
            }
            KeyCode::Esc => {
                if self.find_open {
                    self.find_open = false;
                }
                let doc = self.doc_mut();
                doc.selection = Selection::point(doc.selection.head);
                return;
            }
            _ => {}
        }
        let doc = self.doc_mut();
        let sel = doc.selection;
        let head = sel.head;
        let collapse = !shift && !sel.is_empty();
        let mut keep_preferred = false;
        let new_head = match key.code {
            KeyCode::Left if ctrl => Some(ops::jump_left(doc, head, options.jump_chars)),
            KeyCode::Right if ctrl => Some(ops::jump_right(doc, head, options.jump_chars)),
            KeyCode::Left if collapse => Some(sel.start()),
            KeyCode::Right if collapse => Some(sel.end()),
            KeyCode::Left => Some(ops::left(doc, head)),
            KeyCode::Right => Some(ops::right(doc, head)),
            KeyCode::Up | KeyCode::Down | KeyCode::PageUp | KeyCode::PageDown => {
                let lines = match key.code {
                    KeyCode::Up if ctrl => -jump_lines,
                    KeyCode::Down if ctrl => jump_lines,
                    KeyCode::Up => -1,
                    KeyCode::Down => 1,
                    KeyCode::PageUp => -page,
                    _ => page,
                };
                ops::vertical(doc, lines, shift, tab_size);
                keep_preferred = true;
                None
            }
            KeyCode::Home if ctrl => Some(0),
            KeyCode::End if ctrl => Some(doc.rope.len_chars()),
            KeyCode::Home => Some(ops::smart_home(doc, head)),
            KeyCode::End => Some(doc.line_end(doc.line_of(head))),
            _ => return,
        };
        if let Some(new_head) = new_head {
            doc.selection = sel.move_to(new_head, shift);
        }
        if !keep_preferred {
            doc.view.preferred_col = None;
        }
        doc.history.seal();
        self.scroll_to_cursor();
    }

    fn find_key(&mut self, key: KeyEvent) {
        let alt = key.modifiers.contains(KeyModifiers::ALT);
        let shift = key.modifiers.contains(KeyModifiers::SHIFT);
        match key.code {
            KeyCode::Esc => {
                self.find_open = false;
                self.focus = Focus::Editor;
                return;
            }
            KeyCode::Char('c') if alt => {
                self.find.case_sensitive = !self.find.case_sensitive;
                return;
            }
            KeyCode::Char('r') if alt => {
                self.find.regex = !self.find.regex;
                return;
            }
            KeyCode::Char('a') if alt && self.find.replace_open => {
                commands::replace_all(self);
                return;
            }
            KeyCode::Tab | KeyCode::BackTab if self.find.replace_open => {
                self.find.replace_focused = !self.find.replace_focused;
                return;
            }
            KeyCode::Char('v') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                let (text, _) = self.clipboard.paste();
                let text = text.lines().next().unwrap_or("").to_string();
                self.handle_paste(&text);
                return;
            }
            KeyCode::Enter if self.find.replace_focused => {
                commands::replace_current(self);
                return;
            }
            KeyCode::Enter => {
                commands::find_step(self, !shift);
                return;
            }
            KeyCode::Down => {
                commands::find_step(self, true);
                return;
            }
            KeyCode::Up => {
                commands::find_step(self, false);
                return;
            }
            _ => {}
        }
        let input = if self.find.replace_focused {
            &mut self.find.replace
        } else {
            &mut self.find.input
        };
        if input.handle(key) == Some(true) && !self.find.replace_focused {
            self.find.key = None;
            commands::update_find_matches(self);
            if let Some(index) = self.find.current {
                let (start, end) = self.find.matches[index];
                self.doc_mut().selection = Selection::new(start, end);
                self.scroll_to_cursor();
            }
        }
    }

    fn tree_key(&mut self, key: KeyEvent) {
        let height = self
            .layout
            .sidebar
            .map(|r| r.h as usize)
            .unwrap_or(20)
            .saturating_sub(1);
        let page = height.max(1) as isize;
        match key.code {
            KeyCode::Up => self.tree.move_selection(-1),
            KeyCode::Down => self.tree.move_selection(1),
            KeyCode::PageUp => self.tree.move_selection(-page),
            KeyCode::PageDown => self.tree.move_selection(page),
            KeyCode::Home => self.tree.selected = 0,
            KeyCode::End => self.tree.selected = self.tree.rows.len().saturating_sub(1),
            KeyCode::Left => self.tree.collapse(&self.git),
            KeyCode::Right => self.tree.expand(&self.git),
            KeyCode::Enter | KeyCode::Char(' ') => self.tree_activate(),
            KeyCode::Esc | KeyCode::Tab => self.focus = Focus::Editor,
            _ => {}
        }
        self.tree.ensure_visible(height);
    }

    fn tree_activate(&mut self) {
        let Some(row) = self.tree.selected_row().cloned() else {
            return;
        };
        if row.is_dir {
            self.tree.toggle(&self.git);
            return;
        }
        self.open(&row.path, None);
    }

    fn overlay_key(&mut self, key: KeyEvent) {
        match self.overlay.take() {
            Some(Overlay::Picker(picker)) => self.picker_key(picker, key),
            Some(Overlay::Prompt(prompt)) => self.prompt_key(prompt, key),
            Some(Overlay::Menu(menu)) => self.menu_key(menu, key),
            None => {}
        }
    }

    fn menu_key(&mut self, mut menu: Menu, key: KeyEvent) {
        let modified = key
            .modifiers
            .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT);
        let count = menu.spec.items.len();
        match key.code {
            KeyCode::Esc => return,
            KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => return,
            KeyCode::Char(digit @ '1'..='9') if !modified => {
                if let Some((_, command)) = menu.spec.items.get(digit as usize - '1' as usize) {
                    self.run_command(command);
                    return;
                }
            }
            KeyCode::Enter => {
                self.run_command(menu.spec.items[menu.selected].1);
                return;
            }
            KeyCode::Up => menu.selected = (menu.selected + count - 1) % count,
            KeyCode::Down => menu.selected = (menu.selected + 1) % count,
            _ => {}
        }
        self.overlay = Some(Overlay::Menu(menu));
    }

    fn picker_key(&mut self, mut picker: Box<crate::picker::Picker>, key: KeyEvent) {
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        let list_height = self.picker_list_height();
        match key.code {
            KeyCode::Esc => {
                self.close_picker(picker);
                return;
            }
            KeyCode::Char('c') if ctrl => {
                self.close_picker(picker);
                return;
            }
            KeyCode::Enter => {
                self.preview = None;
                commands::accept_picker(self, picker);
                return;
            }
            KeyCode::Up => picker.move_selection(-1),
            KeyCode::Down => picker.move_selection(1),
            KeyCode::Char('p') if ctrl => picker.move_selection(-1),
            KeyCode::Char('n') if ctrl => picker.move_selection(1),
            KeyCode::Char('k') if ctrl => picker.move_selection(-1),
            KeyCode::Char('j') if ctrl => picker.move_selection(1),
            KeyCode::PageUp => picker.move_selection(-(list_height.max(1) as isize)),
            KeyCode::PageDown => picker.move_selection(list_height.max(1) as isize),
            KeyCode::Tab | KeyCode::BackTab if picker.has_glob() => {
                picker.glob_focused = !picker.glob_focused;
            }
            KeyCode::Char('f') if ctrl && picker.has_glob() => {
                picker.glob_focused = !picker.glob_focused;
            }
            _ => {
                let input = if picker.glob_focused {
                    &mut picker.glob
                } else {
                    &mut picker.input
                };
                if input.handle(key) == Some(true) {
                    if picker.glob_focused {
                        self.picker_glob = picker.glob.text.clone();
                    }
                    if picker.is_live() {
                        self.overlay = Some(Overlay::Picker(picker));
                        commands::start_grep(self);
                        return;
                    }
                    picker.refilter();
                }
            }
        }
        picker.ensure_visible(list_height);
        self.overlay = Some(Overlay::Picker(picker));
    }

    fn close_picker(&mut self, picker: Box<crate::picker::Picker>) {
        if picker.searching {
            self.grep_generation
                .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        }
        let mut picker = picker;
        picker.searching = false;
        self.last_picker = Some(picker);
        self.preview = None;
    }

    pub fn picker_list_height(&self) -> usize {
        crate::ui::picker_view::list_height(self.layout.status.y + 1)
    }

    fn prompt_key(&mut self, mut prompt: Prompt, key: KeyEvent) {
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        if prompt.confirm {
            let choices = if prompt.saves_or_discards() {
                "sd"
            } else {
                "y"
            };
            match key.code {
                KeyCode::Char(c) if choices.contains(c.to_ascii_lowercase()) => {
                    let answer = c.to_ascii_lowercase().to_string();
                    commands::answer_prompt(self, prompt, Some(answer))
                }
                KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc | KeyCode::Enter => {
                    commands::answer_prompt(self, prompt, Some("n".to_string()))
                }
                _ => self.overlay = Some(Overlay::Prompt(prompt)),
            }
            return;
        }
        match key.code {
            KeyCode::Esc => commands::answer_prompt(self, prompt, None),
            KeyCode::Char('c') if ctrl => commands::answer_prompt(self, prompt, None),
            KeyCode::Enter => {
                let answer = prompt.input.text.clone();
                commands::answer_prompt(self, prompt, Some(answer));
            }
            _ => {
                prompt.input.handle(key);
                self.overlay = Some(Overlay::Prompt(prompt));
            }
        }
    }

    pub fn handle_paste(&mut self, text: &str) {
        let text = text.replace("\r\n", "\n").replace('\r', "\n");
        match &mut self.overlay {
            Some(Overlay::Picker(picker)) => {
                let live = picker.is_live();
                if picker.glob_focused {
                    picker.glob.insert(&text);
                    self.picker_glob = picker.glob.text.clone();
                } else {
                    picker.input.insert(&text);
                }
                if live {
                    commands::start_grep(self);
                } else {
                    picker.refilter();
                }
                return;
            }
            Some(Overlay::Prompt(prompt)) => {
                prompt.input.insert(&text);
                return;
            }
            Some(Overlay::Menu(_)) => return,
            None => {}
        }
        match self.focus {
            Focus::Find => {
                if self.find.replace_focused {
                    self.find.replace.insert(&text);
                } else {
                    self.find.input.insert(&text);
                    self.find.key = None;
                }
            }
            Focus::Editor => edit(self, |doc, _| ops::insert_text(doc, &text, EditKind::Other)),
            _ => {}
        }
    }

    pub fn handle_mouse(&mut self, mouse: MouseEvent) {
        let (x, y) = (mouse.column, mouse.row);
        let lines = self.options.mouse_scroll_lines;
        if matches!(self.overlay, Some(Overlay::Menu(_))) {
            self.menu_mouse(mouse);
            return;
        }
        if self.overlay.is_some() {
            self.picker_mouse(mouse);
            return;
        }
        if self.tree_resize_mouse(mouse) {
            return;
        }
        let focused_pane = self.panes.focused;
        let pane_at = |x: u16, y: u16| {
            let panes = self.layout.panes.iter();
            let mut others = panes.filter(|pane| pane.id != focused_pane);
            others.find(|pane| pane.title.contains(x, y) || pane.body.contains(x, y))
        };
        let other_pane = pane_at(x, y).copied();
        let in_other_pane = other_pane.is_some_and(|pane| pane.body.contains(x, y));
        let in_title = self.layout.panes.iter().any(|p| p.title.contains(x, y));
        let in_editor = self.layout.editor.contains(x, y);
        let in_sidebar = self.layout.sidebar.is_some_and(|r| r.contains(x, y));
        match mouse.kind {
            MouseEventKind::ScrollUp | MouseEventKind::ScrollDown => {
                let down = mouse.kind == MouseEventKind::ScrollDown;
                if in_sidebar {
                    let rows = self.tree.rows.len();
                    let height = self.layout.sidebar.map(|r| r.h as usize).unwrap_or(1);
                    self.tree.scroll = if down {
                        (self.tree.scroll + lines)
                            .min(rows.saturating_sub(height.saturating_sub(1)))
                    } else {
                        self.tree.scroll.saturating_sub(lines)
                    };
                } else if let Some(pane) = other_pane {
                    let delta = if down {
                        lines as isize
                    } else {
                        -(lines as isize)
                    };
                    self.scroll_pane(pane.id, delta);
                } else if in_editor {
                    let height = self.editor_height();
                    let doc = self.doc_mut();
                    let max = doc.line_count().saturating_sub(height / 2);
                    doc.view.scroll_line = if down {
                        (doc.view.scroll_line + lines).min(max)
                    } else {
                        doc.view.scroll_line.saturating_sub(lines)
                    };
                }
            }
            MouseEventKind::Down(MouseButton::Left) => {
                let double = self
                    .last_click
                    .is_some_and(|(at, cx, cy)| at.elapsed() < DOUBLE_CLICK && cx == x && cy == y);
                self.last_click = Some((Instant::now(), x, y));
                if in_title {
                    if let Some(pane) = other_pane {
                        self.focus_pane(pane.id);
                    }
                } else if in_sidebar {
                    self.sidebar_click(y);
                } else if in_other_pane || in_editor {
                    if let Some(pane) = other_pane {
                        self.focus_pane(pane.id);
                    }
                    self.focus = Focus::Editor;
                    let extend = mouse.modifiers.contains(KeyModifiers::SHIFT);
                    let pos = self.position_at(x, y);
                    let doc = self.doc_mut();
                    if double {
                        let (line, col) = doc.line_col(pos);
                        if let Some((start, end)) = text::word_at(&doc.line_chars(line), col) {
                            let base = doc.line_start(line);
                            doc.selection = Selection::new(base + start, base + end);
                        }
                    } else {
                        doc.selection = doc.selection.move_to(pos, extend);
                    }
                    doc.view.preferred_col = None;
                    doc.history.seal();
                    self.dragging = !double;
                    if mouse.modifiers.contains(KeyModifiers::CONTROL) && !double && !extend {
                        self.dragging = false;
                        self.run_command("goto.definition");
                    }
                }
            }
            MouseEventKind::Drag(MouseButton::Left) if self.dragging => {
                let pos = self.position_at(x, y);
                let editor = self.layout.editor;
                let doc = self.doc_mut();
                doc.selection = doc.selection.move_to(pos, true);
                if y <= editor.y {
                    doc.view.scroll_line = doc.view.scroll_line.saturating_sub(1);
                } else if y + 1 >= editor.bottom() {
                    doc.view.scroll_line += 1;
                }
            }
            MouseEventKind::Up(MouseButton::Left) => self.dragging = false,
            _ => {}
        }
    }

    /// Dragging the right edge of the file tree resizes it. Returns true
    /// when the event belongs to the resize.
    fn tree_resize_mouse(&mut self, mouse: MouseEvent) -> bool {
        match mouse.kind {
            MouseEventKind::Down(MouseButton::Left) => {
                self.resizing_tree = self.on_tree_edge(mouse.column, mouse.row);
                self.resizing_tree
            }
            MouseEventKind::Drag(MouseButton::Left) if self.resizing_tree => {
                self.set_tree_width(mouse.column + 1);
                true
            }
            MouseEventKind::Up(MouseButton::Left) if self.resizing_tree => {
                self.resizing_tree = false;
                true
            }
            _ => false,
        }
    }

    /// The last column of the file tree, or the column right of it below
    /// the header.
    fn on_tree_edge(&self, x: u16, y: u16) -> bool {
        let Some(sidebar) = self.layout.sidebar else {
            return false;
        };
        let in_rows = y >= sidebar.y && y < sidebar.bottom();
        let in_title = self.layout.panes.iter().any(|p| p.title.contains(x, y));
        let beside = x == sidebar.right() && !in_title;
        in_rows && (x + 1 == sidebar.right() || beside)
    }

    fn menu_mouse(&mut self, mouse: MouseEvent) {
        let Some(Overlay::Menu(menu)) = &mut self.overlay else {
            return;
        };
        let count = menu.spec.items.len();
        match mouse.kind {
            MouseEventKind::ScrollUp => menu.selected = (menu.selected + count - 1) % count,
            MouseEventKind::ScrollDown => menu.selected = (menu.selected + 1) % count,
            MouseEventKind::Down(MouseButton::Left) => {
                let area = crate::ui::menu_area(menu.spec, &self.layout);
                let row = mouse.row.wrapping_sub(area.y + 1) as usize;
                let command = (area.contains(mouse.column, mouse.row) && row < count)
                    .then(|| menu.spec.items[row].1);
                self.overlay = None;
                if let Some(command) = command {
                    self.run_command(command);
                }
            }
            _ => {}
        }
    }

    fn sidebar_click(&mut self, y: u16) {
        let Some(sidebar) = self.layout.sidebar else {
            return;
        };
        if y <= sidebar.y {
            return;
        }
        let row = (y - sidebar.y - 1) as usize;
        let index = self.tree.scroll + row;
        if index < self.tree.rows.len() {
            self.tree.selected = index;
            self.focus = Focus::Tree;
            self.tree_activate();
        }
    }

    fn picker_mouse(&mut self, mouse: MouseEvent) {
        let Some(Overlay::Picker(picker)) = &mut self.overlay else {
            return;
        };
        let height = crate::ui::picker_view::list_height(self.layout.status.y + 1);
        match mouse.kind {
            MouseEventKind::ScrollUp => picker.move_selection(-1),
            MouseEventKind::ScrollDown => picker.move_selection(1),
            MouseEventKind::Down(MouseButton::Left) => {
                let area = crate::ui::picker_view::list_area(
                    self.layout.status.w,
                    self.layout.status.y + 1,
                    picker.kind,
                );
                if area.contains(mouse.column, mouse.row) {
                    let index = picker.scroll + (mouse.row - area.y) as usize;
                    if index < picker.shown.len() {
                        picker.selected = index;
                        if let Some(Overlay::Picker(picker)) = self.overlay.take() {
                            self.preview = None;
                            commands::accept_picker(self, picker);
                        }
                        return;
                    }
                }
            }
            _ => {}
        }
        if let Some(Overlay::Picker(picker)) = &mut self.overlay {
            picker.ensure_visible(height);
        }
    }

    /// The document position under a screen cell of the editor.
    fn position_at(&self, x: u16, y: u16) -> usize {
        let doc = self.doc();
        let area = self.layout.editor;
        let gutter = super::gutter_width(doc, &self.options);
        let row = y.saturating_sub(area.y) as usize;
        let line = (doc.view.scroll_line + row).min(doc.line_count().saturating_sub(1));
        let col = (x.saturating_sub(area.x + gutter)) as usize + doc.view.scroll_col;
        let char_col = text::display_col_to_char(&doc.line(line), col, self.options.tab_size);
        doc.line_start(line) + char_col
    }
}
