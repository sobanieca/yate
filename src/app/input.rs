//! Keyboard, mouse and paste handling. Overlays get keys first, then key
//! bindings, then the focused panel.

use super::commands::{self, edit};
use super::{App, FindField, Focus, Menu, MenuItem, MenuRepeat, MenuSpec, Overlay, Prompt};
use crate::editor::history::EditKind;
use crate::editor::ops;
use crate::editor::selection::Selection;
use crate::keymap;
use crate::picker::{Picker, PickerKind};
use crate::text;
use crate::ui::input::LineInput;
use crate::ui::surface::Rect;
use crate::workspace::tree::TreeFilter;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use std::time::{Duration, Instant};

const DOUBLE_CLICK: Duration = Duration::from_millis(400);
const MENU_REPEAT_WINDOW: Duration = Duration::from_millis(1500);
const DOUBLE_ESC: Duration = Duration::from_millis(600);

impl App {
    pub fn handle_key(&mut self, key: KeyEvent) {
        let repeat = self.menu_repeat.take();
        if self.is_double_esc(key) && self.dialog.is_none() && self.overlay.is_none() {
            self.hint_quit_key();
        }
        if self.dialog.is_some() {
            self.dialog_key(key);
            return;
        }
        if self.overlay.is_some() {
            self.switcher = None;
            self.overlay_key(key);
            return;
        }
        let name = keymap::key_name(&key);
        if let Some(repeat) = repeat
            && repeat.at.elapsed() < MENU_REPEAT_WINDOW
            && name.starts_with("ctrl+")
            && let Some(item) = repeat.spec.item_for(&name).filter(|item| item.repeats)
        {
            self.run_menu_item(repeat.spec, item);
            return;
        }
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

    fn is_double_esc(&mut self, key: KeyEvent) -> bool {
        if key.code != KeyCode::Esc || !key.modifiers.is_empty() {
            self.last_esc = None;
            return false;
        }
        let double = self.last_esc.is_some_and(|at| at.elapsed() < DOUBLE_ESC);
        self.last_esc = (!double).then(Instant::now);
        double
    }

    fn hint_quit_key(&mut self) {
        if let Some(key) = self.keymap.keys_for("app.quit").into_iter().next() {
            self.notify(format!("To quit yate, press {key}"));
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
        let shift = key.modifiers.contains(KeyModifiers::SHIFT);
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        let field = self.find.field;
        let at_edge = |input: &LineInput, right: bool| {
            let edge = if right { input.text.chars().count() } else { 0 };
            !input.selected && input.cursor == edge
        };
        let leaves_field = match (key.code, self.find.focused_input()) {
            (KeyCode::Left, Some(input)) => at_edge(input, false),
            (KeyCode::Right, Some(input)) => at_edge(input, true),
            (KeyCode::Left | KeyCode::Right, None) => true,
            _ => false,
        };
        match key.code {
            KeyCode::Esc => {
                self.find_open = false;
                self.focus = Focus::Editor;
                return;
            }
            KeyCode::Tab => {
                self.find.focus_next(true);
                return;
            }
            KeyCode::BackTab => {
                self.find.focus_next(false);
                return;
            }
            KeyCode::Left | KeyCode::Right if leaves_field => {
                self.find.focus_beside(key.code == KeyCode::Right);
                return;
            }
            KeyCode::Char('v') if ctrl => {
                let (text, _) = self.clipboard.paste();
                let text = text.lines().next().unwrap_or("").to_string();
                self.handle_paste(&text);
                return;
            }
            KeyCode::Enter if field == FindField::Query => {
                commands::find_step(self, !shift);
                return;
            }
            KeyCode::Enter => {
                self.find_activate(field);
                return;
            }
            KeyCode::Char(' ') if !field.is_text() => {
                self.find_activate(field);
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
        if !field.is_text() && matches!(key.code, KeyCode::Char(_)) && !ctrl {
            self.find.field = FindField::Query;
        }
        let field = self.find.field;
        let Some(input) = self.find.focused_input() else {
            return;
        };
        if input.handle(key) == Some(true) && field == FindField::Query {
            self.find_query_changed();
        }
    }

    fn find_query_changed(&mut self) {
        self.find.key = None;
        commands::update_find_matches(self);
        if let Some(index) = self.find.current {
            let (start, end) = self.find.matches[index];
            self.doc_mut().selection = Selection::new(start, end);
            self.scroll_to_cursor();
        }
    }

    /// Enter, Space or a click on a control of the find bar: ticks a
    /// checkbox, presses a button.
    fn find_activate(&mut self, field: FindField) {
        match field {
            FindField::Query => {}
            FindField::Case => self.find.case_sensitive = !self.find.case_sensitive,
            FindField::Regex => self.find.regex = !self.find.regex,
            FindField::Replace => {
                let open = !self.find.replace_open;
                self.find.set_replace_open(open);
            }
            FindField::Replacement | FindField::ReplaceOne => commands::replace_current(self),
            FindField::ReplaceAll => commands::replace_all(self),
        }
    }

    fn find_click(&mut self, x: u16, y: u16) {
        self.focus = Focus::Find;
        let Some(area) = self.layout.find else {
            return;
        };
        let controls = crate::ui::find_controls(&self.find, area);
        let clicked = controls.iter().find(|(_, rect)| rect.contains(x, y));
        let Some((field, rect)) = clicked.copied() else {
            return;
        };
        self.find.field = field;
        match self.find.focused_input() {
            Some(input) => {
                let col = x.saturating_sub(rect.x + 1) as usize;
                input.click(col, rect.w.saturating_sub(2) as usize);
            }
            None => self.find_activate(field),
        }
    }

    /// The filter checkboxes of the tree, row by row as they are drawn.
    fn tree_filter_rows(&self) -> Vec<Vec<TreeFilter>> {
        let Some(sidebar) = self.layout.sidebar else {
            return vec![TreeFilter::ALL.to_vec()];
        };
        let mut rows: Vec<(u16, Vec<TreeFilter>)> = Vec::new();
        for (filter, rect) in crate::ui::sidebar::filter_controls(sidebar) {
            match rows.last_mut() {
                Some((y, row)) if *y == rect.y => row.push(filter),
                _ => rows.push((rect.y, vec![filter])),
            }
        }
        rows.into_iter().map(|(_, row)| row).collect()
    }

    /// A key while a filter checkbox of the tree has the focus: the arrows
    /// move between the checkboxes, Down from the last row goes to the files.
    fn tree_filter_key(&mut self, filter: TreeFilter, key: KeyEvent) {
        let rows = self.tree_filter_rows();
        let row = rows.iter().position(|row| row.contains(&filter));
        let row = row.unwrap_or(0);
        let all = TreeFilter::ALL;
        let at = all.iter().position(|f| *f == filter).unwrap_or(0);
        let first_of = |row: Option<&Vec<TreeFilter>>| row.and_then(|row| row.first().copied());
        match key.code {
            KeyCode::Up => {
                let above = row.checked_sub(1).and_then(|row| rows.get(row));
                self.tree.filter_focus = first_of(above).or(Some(filter));
            }
            KeyCode::Down => self.tree.filter_focus = first_of(rows.get(row + 1)),
            KeyCode::Left => self.tree.filter_focus = Some(all[at.saturating_sub(1)]),
            KeyCode::Right => self.tree.filter_focus = Some(all[(at + 1).min(all.len() - 1)]),
            KeyCode::Enter | KeyCode::Char(' ') => self.run_command(filter.command()),
            KeyCode::Esc | KeyCode::Tab => self.focus = Focus::Editor,
            _ => {}
        }
    }

    fn tree_key(&mut self, key: KeyEvent) {
        if let Some(filter) = self.tree.filter_focus {
            self.tree_filter_key(filter, key);
            return;
        }
        let height = self.tree_list_height();
        let page = height.max(1) as isize;
        match key.code {
            KeyCode::Up if self.tree.selected == 0 => {
                let rows = self.tree_filter_rows();
                self.tree.filter_focus = rows.last().and_then(|row| row.first().copied());
            }
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

    /// How far the text of the open dialog scrolls, and a page of it.
    fn dialog_scroll_range(&self) -> (usize, usize) {
        let Some(dialog) = &self.dialog else {
            return (0, 1);
        };
        let view = crate::ui::dialog_view(dialog, &self.layout);
        (view.lines.len().saturating_sub(view.rows), view.rows.max(1))
    }

    fn scroll_dialog(&mut self, lines: isize) {
        let (last, _) = self.dialog_scroll_range();
        if let Some(dialog) = &mut self.dialog {
            dialog.scroll = dialog
                .scroll
                .min(last)
                .saturating_add_signed(lines)
                .min(last);
        }
    }

    /// A key while a message dialog is open: Esc, Enter or Space close it,
    /// the arrows scroll a long text.
    fn dialog_key(&mut self, key: KeyEvent) {
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        let (last, page) = self.dialog_scroll_range();
        let (last, page) = (last as isize, page as isize);
        match key.code {
            KeyCode::Esc | KeyCode::Enter | KeyCode::Char(' ') => self.dialog = None,
            KeyCode::Char('c') if ctrl => self.dialog = None,
            KeyCode::Up => self.scroll_dialog(-1),
            KeyCode::Down => self.scroll_dialog(1),
            KeyCode::PageUp => self.scroll_dialog(-page),
            KeyCode::PageDown => self.scroll_dialog(page),
            KeyCode::Home => self.scroll_dialog(-last),
            KeyCode::End => self.scroll_dialog(last),
            _ => {}
        }
    }

    /// The wheel scrolls a message dialog; a click on its button or
    /// outside of it closes it.
    fn dialog_mouse(&mut self, mouse: MouseEvent) {
        let Some(dialog) = &self.dialog else {
            return;
        };
        let view = crate::ui::dialog_view(dialog, &self.layout);
        let lines = self.options.mouse_scroll_lines as isize;
        match mouse.kind {
            MouseEventKind::ScrollUp => self.scroll_dialog(-lines),
            MouseEventKind::ScrollDown => self.scroll_dialog(lines),
            MouseEventKind::Down(MouseButton::Left) => {
                let on_button = view.button.contains(mouse.column, mouse.row);
                if on_button || !view.area.contains(mouse.column, mouse.row) {
                    self.dialog = None;
                }
            }
            _ => {}
        }
    }

    fn overlay_key(&mut self, key: KeyEvent) {
        match self.overlay.take() {
            Some(Overlay::Picker(picker)) => self.picker_key(picker, key),
            Some(Overlay::Prompt(prompt)) => self.prompt_key(prompt, key),
            Some(Overlay::Menu(menu)) => self.menu_key(menu, key),
            None => {}
        }
    }

    /// A key while a menu is open: the key of an item, with or without
    /// Ctrl, or the key of the menu again for the item it names.
    fn menu_key(&mut self, menu: Menu, key: KeyEvent) {
        let name = keymap::key_name(&key);
        if matches!(name.as_str(), "esc" | "ctrl+c") {
            return;
        }
        let spec = menu.spec;
        let pressed_again = self.keymap.lookup(self.context(), &name) == Some(spec.command);
        let named = |command| spec.items.iter().find(|item| item.command == command);
        let again = spec.again.filter(|_| pressed_again).and_then(named);
        match spec.item_for(&name).or(again) {
            Some(item) => self.run_menu_item(spec, item),
            None => self.overlay = Some(Overlay::Menu(menu)),
        }
    }

    fn run_menu_item(&mut self, spec: &'static MenuSpec, item: &'static MenuItem) {
        self.run_command(item.command);
        if item.repeats && self.overlay.is_none() {
            self.menu_repeat = Some(MenuRepeat {
                spec,
                at: Instant::now(),
            });
        }
    }

    /// The menu whose arrow commands Ctrl and an arrow run again right now.
    pub fn repeating_menu(&self) -> Option<&'static MenuSpec> {
        let repeat = self.menu_repeat.as_ref()?;
        (repeat.at.elapsed() < MENU_REPEAT_WINDOW).then_some(repeat.spec)
    }

    fn picker_key(&mut self, mut picker: Box<Picker>, key: KeyEvent) {
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        let list_height = self.picker_list_height();
        let moves_up =
            key.code == KeyCode::Up || (ctrl && matches!(key.code, KeyCode::Char('p' | 'k')));
        if moves_up && self.is_last_search_again(&picker) {
            self.overlay = self.last_picker.take().map(Overlay::Picker);
            return;
        }
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

    /// Whether `picker` is the file finder or live grep closed last, opened
    /// again with nothing typed or chosen yet: Up brings the last one back.
    fn is_last_search_again(&self, picker: &Picker) -> bool {
        let untouched = picker.input.text.is_empty() && picker.selected == 0;
        let searches = matches!(picker.kind, PickerKind::Files | PickerKind::Grep);
        self.last_picker.as_deref().is_some_and(|last| {
            let same = last.kind == picker.kind && last.title == picker.title;
            untouched && searches && same && !picker.glob_focused && !last.input.text.is_empty()
        })
    }

    fn close_picker(&mut self, picker: Box<Picker>) {
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
            let answer = match key.code {
                KeyCode::Left | KeyCode::BackTab => {
                    prompt.focus_choice(false);
                    None
                }
                KeyCode::Right | KeyCode::Tab => {
                    prompt.focus_choice(true);
                    None
                }
                KeyCode::Enter | KeyCode::Char(' ') => {
                    prompt.choices().get(prompt.choice).map(|c| c.answer)
                }
                KeyCode::Char('c') if ctrl => Some("n"),
                KeyCode::Esc => Some("n"),
                KeyCode::Char(c) => prompt.answer_for(c),
                _ => None,
            };
            match answer {
                Some(answer) => commands::answer_prompt(self, prompt, Some(answer.to_string())),
                None => self.overlay = Some(Overlay::Prompt(prompt)),
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
        self.menu_repeat = None;
        if self.dialog.is_some() {
            return;
        }
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
                if let Some(input) = self.find.focused_input() {
                    input.insert(&text);
                }
                if self.find.field == FindField::Query {
                    self.find_query_changed();
                }
            }
            Focus::Editor => edit(self, |doc, _| ops::insert_text(doc, &text, EditKind::Other)),
            _ => {}
        }
    }

    pub fn handle_mouse(&mut self, mouse: MouseEvent) {
        let (x, y) = (mouse.column, mouse.row);
        let lines = self.options.mouse_scroll_lines;
        self.menu_repeat = None;
        if self.dialog.is_some() {
            self.dialog_mouse(mouse);
            return;
        }
        if self.menu_visible() {
            self.menu_mouse(mouse);
            return;
        }
        if matches!(self.overlay, Some(Overlay::Menu(_))) {
            self.overlay = None;
        }
        if matches!(self.overlay, Some(Overlay::Prompt(_))) {
            self.prompt_mouse(mouse);
            return;
        }
        if self.overlay.is_some() {
            self.picker_mouse(mouse);
            return;
        }
        if self.tab_mouse(mouse) || self.tree_resize_mouse(mouse) {
            return;
        }
        let pressed = mouse.kind == MouseEventKind::Down(MouseButton::Left);
        if pressed && self.layout.find.is_some_and(|area| area.contains(x, y)) {
            self.find_click(x, y);
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
                    let height = self.tree_list_height();
                    self.tree.scroll = if down {
                        (self.tree.scroll + lines).min(rows.saturating_sub(height))
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
                    self.sidebar_click(x, y);
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
        let in_tabs = self.layout.tabs.is_some_and(|tabs| tabs.contains(x, y));
        let beside = x == sidebar.right() && !in_title && !in_tabs;
        in_rows && (x + 1 == sidebar.right() || beside)
    }

    /// A click on a tab shows its file; a click on its `×`, or a middle
    /// click, closes it. Returns true when the event was on the tab bar.
    fn tab_mouse(&mut self, mouse: MouseEvent) -> bool {
        let (x, y) = (mouse.column, mouse.row);
        let Some(area) = self.layout.tabs.filter(|area| area.contains(x, y)) else {
            return false;
        };
        let closes = match mouse.kind {
            MouseEventKind::Down(MouseButton::Left) => false,
            MouseEventKind::Down(MouseButton::Middle) => true,
            _ => return true,
        };
        let cells = crate::ui::tabs::cells(self, area);
        let Some(cell) = cells.iter().find(|cell| cell.area.contains(x, y)) else {
            return true;
        };
        let path = self.tabs[cell.index].clone();
        if closes || cell.close.contains(x, y) {
            self.close_tab(&path);
        } else {
            self.show_file(&path);
            self.focus = Focus::Editor;
        }
        true
    }

    fn menu_mouse(&mut self, mouse: MouseEvent) {
        let Some(Overlay::Menu(menu)) = &self.overlay else {
            return;
        };
        if mouse.kind != MouseEventKind::Down(MouseButton::Left) {
            return;
        }
        let spec = menu.spec;
        let area = crate::ui::menu_area(spec, &self.layout);
        let row = mouse.row.wrapping_sub(area.y + 1) as usize;
        let inside = area.contains(mouse.column, mouse.row);
        self.overlay = None;
        if let Some(item) = spec.items.get(row).filter(|_| inside) {
            self.run_command(item.command);
        }
    }

    /// A click on a button of a confirmation answers it.
    fn prompt_mouse(&mut self, mouse: MouseEvent) {
        if mouse.kind != MouseEventKind::Down(MouseButton::Left) {
            return;
        }
        let Some(Overlay::Prompt(prompt)) = self.overlay.take() else {
            return;
        };
        let buttons = crate::ui::prompt_buttons(&prompt, &self.layout);
        let hit = |button: &Rect| button.contains(mouse.column, mouse.row);
        let clicked = buttons.iter().position(hit).filter(|_| prompt.confirm);
        match clicked.and_then(|i| prompt.choices().get(i)) {
            Some(choice) => commands::answer_prompt(self, prompt, Some(choice.answer.to_string())),
            None => self.overlay = Some(Overlay::Prompt(prompt)),
        }
    }

    fn sidebar_click(&mut self, x: u16, y: u16) {
        let Some(sidebar) = self.layout.sidebar else {
            return;
        };
        let filters = crate::ui::sidebar::filter_controls(sidebar);
        if let Some((filter, _)) = filters.iter().find(|(_, rect)| rect.contains(x, y)) {
            self.focus = Focus::Tree;
            self.tree.filter_focus = Some(*filter);
            self.run_command(filter.command());
            return;
        }
        let list = crate::ui::sidebar::list_area(sidebar);
        if y < list.y {
            return;
        }
        let index = self.tree.scroll + (y - list.y) as usize;
        if index < self.tree.rows.len() {
            self.tree.selected = index;
            self.tree.filter_focus = None;
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
