//! Split view and the Ctrl+O switcher.
//!
//! The editor is split into any number of panes, side by side or one above
//! the other (`panes.rs` holds the tree and its sizes). The focused pane
//! edits through the active document as usual; every other pane keeps its
//! own document, cursor and scroll in `Pane`, and focusing it swaps them
//! in. With scroll sync on, every scroll of the focused pane moves the
//! other panes by the same number of lines and columns.
//!
//! Each pane shows one file. A file that leaves the screen is forgotten,
//! except for its path and cursor in the recent files.
//!
//! Ctrl+O arms the switcher: a digit then focuses the file tree (1) or shows
//! a recent file (2-9), and every further Ctrl+O shows the next recent file.
//! The numbered targets pop up after `keyHintDelay` milliseconds.

use super::panes::{self, Direction, Side};
use super::{App, Focus, Overlay, Prompt, PromptKind, gutter_width};
use crate::editor::document::Document;
use crate::text;
use crate::ui::input::LineInput;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

pub struct Switcher {
    pub started: Instant,
    /// The recent files when the switcher was armed, so their numbers stay
    /// while it is used.
    pub files: Vec<PathBuf>,
}

pub const MAX_SWITCH_TARGETS: usize = 9;

impl App {
    /// Writes the document, cursor and scroll of the focused pane to its
    /// `Pane`, which is only current while another pane has the focus.
    fn store_focused_pane(&mut self) {
        let doc = &self.docs[self.active];
        if let Some(pane) = self.panes.pane_mut(self.panes.focused) {
            pane.doc_id = doc.id;
            pane.selection = doc.selection;
            pane.view = doc.view.clone();
        }
    }

    /// Makes the document of the focused pane the active one, with the
    /// cursor and scroll of the pane.
    fn load_focused_pane(&mut self) {
        let Some(pane) = self.panes.pane(self.panes.focused).cloned() else {
            return;
        };
        if let Some(index) = self.docs.iter().position(|d| d.id == pane.doc_id) {
            self.active = index;
            let doc = self.doc_mut();
            doc.selection = pane.selection.clamp(doc.rope.len_chars());
            doc.view = pane.view;
        }
    }

    fn panes_changed(&mut self) {
        self.focus = Focus::Editor;
        self.find.key = None;
        self.panes.last_scroll = None;
        self.touch_recent();
        self.relayout();
        self.follow_in_tree();
    }

    /// After the panes got another size.
    fn panes_resized(&mut self) {
        self.panes_changed();
        self.scroll_to_cursor();
    }

    /// Says how to split when the editor is not split.
    fn needs_split(&mut self) -> bool {
        if self.panes.is_split() {
            return false;
        }
        let key = self.keymap.hint("view.paneMenu");
        let how = key.map(|key| format!(": {key} splits the editor"));
        self.notify(format!("Not split{}", how.unwrap_or_default()));
        true
    }

    /// Splits the focused pane in two that show the same file; the new one
    /// gets the focus.
    pub fn split_pane(&mut self, direction: Direction) {
        self.store_focused_pane();
        let before = self.panes.clone();
        self.panes.split(direction);
        if !self.panes.fits(self.layout.editor_area) {
            self.panes = before;
            self.notify("No room for another pane");
            return;
        }
        self.panes_resized();
    }

    /// Closes the focused pane; the pane next to it gets the focus.
    pub fn close_pane(&mut self) {
        if self.needs_split() || !self.may_leave(App::close_pane) {
            return;
        }
        if self.panes.remove(self.panes.focused).is_none() {
            return;
        }
        self.load_focused_pane();
        self.drop_hidden_docs();
        self.panes_resized();
    }

    /// Closes every pane but the focused one, asking about the unsaved
    /// changes of each file that leaves the screen.
    pub fn close_other_panes(&mut self) {
        if self.needs_split() {
            return;
        }
        let focused_doc = self.doc().id;
        while let Some((id, doc_id)) = self.panes.others().first().map(|p| (p.id, p.doc_id)) {
            let others = self.panes.others();
            let views = others.iter().filter(|p| p.doc_id == doc_id).count();
            let leaves_the_screen = doc_id != focused_doc && views == 1;
            let index = self.docs.iter().position(|d| d.id == doc_id);
            if leaves_the_screen
                && let Some(index) = index
                && !self.may_drop(index, App::close_other_panes)
            {
                break;
            }
            self.leave_confirmed = false;
            self.panes.remove(id);
        }
        self.drop_hidden_docs();
        self.panes_resized();
    }

    pub fn focus_pane(&mut self, id: usize) {
        if id == self.panes.focused || self.panes.pane(id).is_none() {
            return;
        }
        self.store_focused_pane();
        self.panes.focused = id;
        self.load_focused_pane();
        self.panes_changed();
    }

    pub fn focus_next_pane(&mut self) {
        if self.needs_split() {
            return;
        }
        let ids = self.panes.ids();
        let index = ids.iter().position(|id| *id == self.panes.focused);
        let next = index.map_or(0, |index| (index + 1) % ids.len());
        self.focus_pane(ids[next]);
    }

    /// Focuses the pane that touches the focused one on `side`, level with
    /// the cursor.
    pub fn focus_pane_on(&mut self, side: Side) {
        if self.needs_split() {
            return;
        }
        let near = self.cursor_cell();
        match panes::neighbor(&self.layout.panes, self.panes.focused, side, near) {
            Some(id) => self.focus_pane(id),
            None => self.notify("No pane on that side"),
        }
    }

    /// The screen cell of the cursor, or the closest one of its pane when
    /// the cursor is scrolled out of view.
    fn cursor_cell(&self) -> (u16, u16) {
        let area = self.layout.editor;
        let doc = self.doc();
        let (line, col) = doc.line_col(doc.selection.head);
        let display = text::char_to_display_col(&doc.line(line), col, self.options.tab_size);
        let column =
            gutter_width(doc, &self.options) as usize + display.saturating_sub(doc.view.scroll_col);
        let row = line.saturating_sub(doc.view.scroll_line);
        (
            area.x + column.min(area.w.saturating_sub(1) as usize) as u16,
            area.y + row.min(area.h.saturating_sub(1) as usize) as u16,
        )
    }

    pub fn toggle_scroll_sync(&mut self) {
        if self.needs_split() {
            return;
        }
        self.panes.sync_scroll = !self.panes.sync_scroll;
        self.panes.last_scroll = None;
        let state = if self.panes.sync_scroll { "on" } else { "off" };
        self.notify(format!("Scroll sync {state}"));
    }

    /// Asks how many columns and rows to add to the focused pane.
    pub fn resize_pane_prompt(&mut self) {
        if self.needs_split() {
            return;
        }
        let focused = self.panes.focused;
        let area = self.layout.panes.iter().find(|pane| pane.id == focused);
        let (width, height) = area.map(|pane| pane.size()).unwrap_or_default();
        let wide = self.panes.resizable(Direction::SideBySide);
        let tall = self.panes.resizable(Direction::Stacked);
        let title = match (wide, tall) {
            (true, true) => {
                format!("Add columns,rows to this pane ({width},{height} now), e.g. 10,-2")
            }
            (true, false) => format!("Add columns to this pane ({width} now), e.g. 10 or -10"),
            _ => format!("Add rows to this pane ({height} now), e.g. 3 or -3"),
        };
        let prompt = Prompt::text(title, LineInput::default(), PromptKind::ResizePane);
        self.overlay = Some(Overlay::Prompt(prompt));
    }

    /// Resizes the focused pane by the answer to `resize_pane_prompt`.
    pub(super) fn resize_pane(&mut self, answer: &str) {
        if answer.trim().is_empty() {
            return;
        }
        let wide = self.panes.resizable(Direction::SideBySide);
        let Some((columns, rows)) = parse_resize(answer, wide) else {
            self.notify(format!(
                "{answer}: type the columns,rows to add, like 10 or -5,2"
            ));
            return;
        };
        let area = self.layout.editor_area;
        let moved = (
            self.panes.resize(Direction::SideBySide, columns, area),
            self.panes.resize(Direction::Stacked, rows, area),
        );
        if moved != (columns, rows) {
            let (columns, rows) = moved;
            self.notify(format!("No room for more: resized by {columns},{rows}"));
        }
        self.panes_resized();
    }

    pub fn equalize_panes(&mut self) {
        if self.needs_split() {
            return;
        }
        self.panes.reset_sizes();
        self.panes_resized();
    }

    /// Whether a pane without the focus shows document `index`.
    pub fn in_another_pane(&self, index: usize) -> bool {
        let id = self.docs[index].id;
        self.panes.others().iter().any(|pane| pane.doc_id == id)
    }

    /// Takes a document off the screen without asking, e.g. when its file
    /// was deleted: its panes close, or an empty editor is left.
    pub(super) fn forget_doc(&mut self, doc_id: usize) {
        self.store_focused_pane();
        let shown_in: Vec<usize> = self
            .panes
            .all()
            .iter()
            .filter(|pane| pane.doc_id == doc_id)
            .map(|pane| pane.id)
            .collect();
        for id in shown_in {
            self.panes.remove(id);
        }
        self.docs.retain(|doc| doc.id != doc_id);
        if self.docs.is_empty() {
            let scratch = Document::scratch(0);
            if let Some(pane) = self.panes.pane_mut(self.panes.focused) {
                pane.doc_id = scratch.id;
                pane.selection = scratch.selection;
                pane.view = scratch.view.clone();
            }
            self.docs.push(scratch);
        }
        self.active = 0;
        self.load_focused_pane();
        self.panes.last_scroll = None;
        self.relayout();
        self.follow_in_tree();
    }

    /// Scrolls the other panes like the focused pane scrolled since the
    /// previous frame.
    pub(super) fn sync_pane_scroll(&mut self) {
        let doc = &self.docs[self.active];
        let now = (doc.id, doc.view.scroll_line, doc.view.scroll_col);
        let before = self.panes.last_scroll.replace(now);
        let Some((_, line, col)) = before.filter(|before| before.0 == now.0) else {
            return;
        };
        if !self.panes.sync_scroll {
            return;
        }
        for pane in self.panes.others_mut() {
            let lines = self
                .docs
                .iter()
                .find(|d| d.id == pane.doc_id)
                .map_or(1, |d| d.line_count());
            let view = &mut pane.view;
            view.scroll_line = shift(view.scroll_line, line, now.1).min(lines.saturating_sub(1));
            view.scroll_col = shift(view.scroll_col, col, now.2);
        }
    }

    /// Mouse wheel over a pane without the focus.
    pub(super) fn scroll_pane(&mut self, id: usize, delta: isize) {
        if self.panes.sync_scroll {
            let doc = self.doc_mut();
            let max = doc.line_count().saturating_sub(1);
            doc.view.scroll_line = doc.view.scroll_line.saturating_add_signed(delta).min(max);
            return;
        }
        let Some(pane) = self.panes.pane_mut(id) else {
            return;
        };
        let lines = self
            .docs
            .iter()
            .find(|d| d.id == pane.doc_id)
            .map_or(1, |d| d.line_count());
        pane.view.scroll_line = pane
            .view
            .scroll_line
            .saturating_add_signed(delta)
            .min(lines.saturating_sub(1));
    }

    pub fn start_switcher(&mut self) {
        self.switcher = Some(Switcher {
            started: Instant::now(),
            files: self
                .recent
                .iter()
                .take(MAX_SWITCH_TARGETS - 1)
                .map(|recent| recent.path.clone())
                .collect(),
        });
    }

    pub fn switcher_visible(&self) -> bool {
        self.switcher.as_ref().is_some_and(|s| {
            s.started.elapsed() >= Duration::from_millis(self.options.key_hint_delay)
        })
    }

    /// Whether the popup of the open menu is on screen: like the switcher,
    /// it waits `keyHintDelay` so a key pressed right away shows nothing.
    pub fn menu_visible(&self) -> bool {
        let delay = Duration::from_millis(self.options.key_hint_delay);
        matches!(&self.overlay, Some(Overlay::Menu(menu)) if menu.opened.elapsed() >= delay)
    }

    /// How long the event loop may wait before the next frame is due.
    pub fn wake_in(&self, tick: Duration) -> Duration {
        let waiting_since = match (&self.overlay, &self.switcher) {
            (Some(Overlay::Menu(menu)), _) if !self.menu_visible() => Some(menu.opened),
            (None, Some(switcher)) if !self.switcher_visible() => Some(switcher.started),
            _ => None,
        };
        match waiting_since {
            Some(since) => Duration::from_millis(self.options.key_hint_delay)
                .saturating_sub(since.elapsed())
                .max(Duration::from_millis(1))
                .min(tick),
            None => tick,
        }
    }

    /// The numbered targets of the switcher: the file tree, then the recent
    /// files.
    pub fn switch_targets(&self) -> Vec<String> {
        let first = "File tree".to_string();
        let files = self.switcher.iter().flat_map(|s| &s.files).map(|path| {
            let modified = self
                .docs
                .iter()
                .any(|d| d.path.as_ref() == Some(path) && d.is_modified());
            let marker = if modified { " ●" } else { "" };
            format!("{}{marker}", self.relative(path))
        });
        std::iter::once(first).chain(files).collect()
    }

    /// The switcher target that has the focus.
    pub fn switch_target_focused(&self) -> Option<usize> {
        if self.focus == Focus::Tree {
            return Some(0);
        }
        let path = self.doc().path.as_ref()?;
        let files = &self.switcher.as_ref()?.files;
        files.iter().position(|file| file == path).map(|i| i + 1)
    }

    /// Keys while the switcher is armed. Returns false when the key leaves
    /// the switcher and should be handled as usual.
    pub(super) fn switcher_key(&mut self, key: KeyEvent, command: Option<&str>) -> bool {
        match command {
            Some("switch.next") => {
                self.cycle_recent(1);
                return true;
            }
            Some("switch.previous") => {
                self.cycle_recent(-1);
                return true;
            }
            _ => {}
        }
        let files = self.switcher.take().map(|s| s.files).unwrap_or_default();
        let modified = key
            .modifiers
            .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT);
        match key.code {
            KeyCode::Char(digit @ '1'..='9') if !modified => {
                self.switch_to(digit as usize - '0' as usize, &files);
                true
            }
            KeyCode::Esc | KeyCode::Enter => true,
            _ => false,
        }
    }

    /// Focuses switcher target `number`: 1 is the file tree, the others
    /// are `files`.
    fn switch_to(&mut self, number: usize, files: &[PathBuf]) {
        if number == 1 {
            self.focus_tree();
            return;
        }
        match files.get(number - 2) {
            Some(path) => self.show_file(path),
            None => self.notify(format!("No recent file {number}")),
        }
    }

    /// Focuses the file tree, showing it when it is hidden.
    pub fn focus_tree(&mut self) {
        if !self.tree_visible {
            self.tree_visible = true;
            self.tree.rebuild(&self.git);
        }
        self.follow_in_tree();
        self.focus = Focus::Tree;
    }

    /// Focuses the pane that shows `path`, or shows it in the focused pane.
    pub fn show_file(&mut self, path: &Path) {
        let shown = |doc_id: usize| {
            self.docs
                .iter()
                .any(|d| d.id == doc_id && d.path.as_deref() == Some(path))
        };
        let others = self.panes.others();
        let pane = others.iter().find(|pane| shown(pane.doc_id)).map(|p| p.id);
        match pane {
            Some(id) if self.doc().path.as_deref() != Some(path) => self.focus_pane(id),
            _ => {
                self.open(path, None);
            }
        }
    }

    /// Shows the next (or previous) recent file of the armed switcher.
    pub fn cycle_recent(&mut self, delta: isize) {
        let Some(switcher) = &self.switcher else {
            return;
        };
        let files = &switcher.files;
        if files.is_empty() {
            self.notify("No recent files");
            return;
        }
        let current = self
            .doc()
            .path
            .as_ref()
            .and_then(|path| files.iter().position(|file| file == path));
        let next = match current {
            Some(index) => (index as isize + delta).rem_euclid(files.len() as isize) as usize,
            None => 0,
        };
        let path = files[next].clone();
        self.show_file(&path);
    }
}

fn shift(value: usize, before: usize, after: usize) -> usize {
    value.saturating_add_signed(after as isize - before as isize)
}

/// The columns and rows of a resize: `10`, `-5,2` or `,3`. A single number
/// is the columns of a pane with a pane beside it (`wide`), else the rows.
fn parse_resize(answer: &str, wide: bool) -> Option<(i32, i32)> {
    let number = |part: &str| match part.trim() {
        "" => Some(0),
        digits => digits.parse::<i32>().ok(),
    };
    match answer.split_once(',') {
        Some((columns, rows)) => Some((number(columns)?, number(rows)?)),
        None if wide => Some((number(answer)?, 0)),
        None => Some((0, number(answer)?)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resize_answers() {
        assert_eq!(parse_resize("10", true), Some((10, 0)));
        assert_eq!(parse_resize(" -4 ", false), Some((0, -4)));
        assert_eq!(parse_resize("+5, -2", true), Some((5, -2)));
        assert_eq!(parse_resize(",3", true), Some((0, 3)));
        assert_eq!(parse_resize("wide", true), None);
        assert_eq!(parse_resize("1,2,3", true), None);
    }
}
