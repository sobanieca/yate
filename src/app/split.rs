//! Split view and the Alt+W (Alt+Tab) switcher.
//!
//! In a split, the focused pane edits through the active document as usual;
//! the other pane keeps its own document, cursor and scroll in `Pane`, and
//! focusing it swaps the two. With scroll sync on, every scroll of the focused
//! pane moves the other pane by the same number of lines and columns.
//!
//! Alt+W arms the switcher: a digit then focuses the file tree (1) or an
//! open file (2-9), and every further Alt+W moves to the next open file.
//! The numbered targets pop up after `keyHintDelay` milliseconds.

use super::{App, Focus};
use crate::editor::document::ViewState;
use crate::editor::selection::Selection;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use std::time::{Duration, Instant};

pub struct Pane {
    pub doc_id: usize,
    pub selection: Selection,
    pub view: ViewState,
}

pub struct Split {
    pub other: Pane,
    pub focus_right: bool,
    pub sync_scroll: bool,
    /// Document id and scroll of the focused pane at the previous frame.
    last_scroll: Option<(usize, usize, usize)>,
}

pub struct Switcher {
    pub started: Instant,
}

pub const MAX_SWITCH_TARGETS: usize = 9;

impl App {
    pub fn toggle_split(&mut self) {
        if self.split.take().is_some() {
            return;
        }
        if self.review.is_some() {
            self.review = None;
            self.focus = Focus::Editor;
        }
        let doc = self.doc();
        self.split = Some(Split {
            other: Pane {
                doc_id: doc.id,
                selection: doc.selection,
                view: doc.view.clone(),
            },
            focus_right: true,
            sync_scroll: false,
            last_scroll: None,
        });
        self.focus = Focus::Editor;
    }

    pub fn focus_other_pane(&mut self) {
        let doc = self.doc();
        let current = Pane {
            doc_id: doc.id,
            selection: doc.selection,
            view: doc.view.clone(),
        };
        let Some(split) = &mut self.split else {
            self.notify("No split view: ctrl+\\ splits the editor");
            return;
        };
        let target = std::mem::replace(&mut split.other, current);
        split.focus_right = !split.focus_right;
        split.last_scroll = None;
        if let Some(index) = self.docs.iter().position(|d| d.id == target.doc_id) {
            self.active = index;
            let doc = self.doc_mut();
            doc.selection = target.selection.clamp(doc.rope.len_chars());
            doc.view = target.view;
        }
        self.review = None;
        self.focus = Focus::Editor;
        self.find.key = None;
        self.relayout();
        self.follow_in_tree();
    }

    pub fn toggle_scroll_sync(&mut self) {
        let Some(split) = &mut self.split else {
            self.notify("Scroll sync needs a split view: ctrl+\\ splits the editor");
            return;
        };
        split.sync_scroll = !split.sync_scroll;
        split.last_scroll = None;
        let state = if split.sync_scroll { "on" } else { "off" };
        self.notify(format!("Scroll sync {state}"));
    }

    /// The document shown in the other pane, when the editor is split.
    pub fn other_pane_doc(&self) -> Option<usize> {
        let split = self.split.as_ref()?;
        self.docs.iter().position(|d| d.id == split.other.doc_id)
    }

    /// Closes the pane of a document that is being closed. When it is the
    /// focused pane, the other pane stays and its document id is returned,
    /// with its cursor and scroll.
    pub fn close_pane_of(&mut self, doc_id: usize) -> Option<usize> {
        let split = self.split.as_ref()?;
        let active_id = self.doc().id;
        if split.other.doc_id != doc_id && active_id != doc_id {
            return None;
        }
        let split = self.split.take()?;
        if active_id != doc_id || split.other.doc_id == doc_id {
            return None;
        }
        let doc = self.docs.iter_mut().find(|d| d.id == split.other.doc_id)?;
        doc.selection = split.other.selection.clamp(doc.rope.len_chars());
        doc.view = split.other.view;
        Some(doc.id)
    }

    /// Scrolls the other pane like the focused pane scrolled since the
    /// previous frame.
    pub(super) fn sync_split_scroll(&mut self) {
        let doc = &self.docs[self.active];
        let now = (doc.id, doc.view.scroll_line, doc.view.scroll_col);
        let Some(split) = &mut self.split else {
            return;
        };
        let other_lines = self
            .docs
            .iter()
            .find(|d| d.id == split.other.doc_id)
            .map(|d| d.line_count())
            .unwrap_or(1);
        if split.sync_scroll
            && let Some((id, line, col)) = split.last_scroll
            && id == now.0
        {
            let view = &mut split.other.view;
            view.scroll_line =
                shift(view.scroll_line, line, now.1).min(other_lines.saturating_sub(1));
            view.scroll_col = shift(view.scroll_col, col, now.2);
        }
        split.last_scroll = Some(now);
    }

    /// Mouse wheel over the other pane.
    pub(super) fn scroll_other_pane(&mut self, delta: isize) {
        let synced = self.split.as_ref().is_some_and(|s| s.sync_scroll);
        if synced {
            let doc = self.doc_mut();
            let max = doc.line_count().saturating_sub(1);
            doc.view.scroll_line = doc.view.scroll_line.saturating_add_signed(delta).min(max);
            return;
        }
        let Some(lines) = self.other_pane_doc().map(|i| self.docs[i].line_count()) else {
            return;
        };
        if let Some(split) = &mut self.split {
            let view = &mut split.other.view;
            view.scroll_line = view
                .scroll_line
                .saturating_add_signed(delta)
                .min(lines.saturating_sub(1));
        }
    }

    pub fn start_switcher(&mut self) {
        self.switcher = Some(Switcher {
            started: Instant::now(),
        });
    }

    pub fn switcher_visible(&self) -> bool {
        self.switcher.as_ref().is_some_and(|s| {
            s.started.elapsed() >= Duration::from_millis(self.options.key_hint_delay)
        })
    }

    /// How long the event loop may wait before the next frame is due.
    pub fn wake_in(&self, tick: Duration) -> Duration {
        match &self.switcher {
            Some(switcher) if !self.switcher_visible() => {
                let delay = Duration::from_millis(self.options.key_hint_delay);
                delay
                    .saturating_sub(switcher.started.elapsed())
                    .max(Duration::from_millis(1))
                    .min(tick)
            }
            _ => tick,
        }
    }

    /// The numbered targets of the switcher: the file tree (or the changed
    /// files while reviewing), then the open files.
    pub fn switch_targets(&self) -> Vec<String> {
        let first = if self.review.is_some() {
            "Changed files".to_string()
        } else {
            "File tree".to_string()
        };
        std::iter::once(first)
            .chain(self.docs.iter().map(|doc| {
                let marker = if doc.is_modified() { " ●" } else { "" };
                format!("{}{marker}", doc.name())
            }))
            .take(MAX_SWITCH_TARGETS)
            .collect()
    }

    /// Keys while the switcher is armed. Returns false when the key leaves
    /// the switcher and should be handled as usual.
    pub(super) fn switcher_key(&mut self, key: KeyEvent, command: Option<&str>) -> bool {
        match command {
            Some("switch.next") => {
                self.cycle_buffer(1);
                return true;
            }
            Some("switch.previous") => {
                self.cycle_buffer(-1);
                return true;
            }
            _ => {}
        }
        self.switcher = None;
        let modified = key
            .modifiers
            .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT);
        match key.code {
            KeyCode::Char(digit @ '1'..='9') if !modified => {
                self.switch_to(digit as usize - '0' as usize);
                true
            }
            KeyCode::Esc | KeyCode::Enter => true,
            _ => false,
        }
    }

    /// Focuses switcher target `number` (1 is the file tree).
    pub fn switch_to(&mut self, number: usize) {
        if number == 1 {
            if let Some(review) = &mut self.review {
                review.diff_focused = false;
                self.focus = Focus::Review;
                return;
            }
            if !self.tree_visible {
                self.tree_visible = true;
                self.tree.rebuild(&self.git);
            }
            self.follow_in_tree();
            self.focus = Focus::Tree;
            return;
        }
        let index = number - 2;
        if index >= self.docs.len() {
            self.notify(format!("No file {number}: {} open", self.docs.len()));
            return;
        }
        self.show_doc(index);
    }

    /// Shows open document `index`: focuses the pane that shows it, or
    /// shows it in the focused pane.
    pub fn show_doc(&mut self, index: usize) {
        self.review = None;
        let id = self.docs[index].id;
        if self.other_pane_doc() == Some(index) && self.doc().id != id {
            self.focus_other_pane();
            return;
        }
        if index != self.active {
            self.push_jump();
            self.active = index;
            self.find.key = None;
        }
        self.focus = Focus::Editor;
        self.follow_in_tree();
    }

    pub fn cycle_buffer(&mut self, delta: isize) {
        let count = self.docs.len() as isize;
        self.review = None;
        self.active = ((self.active as isize + delta).rem_euclid(count)) as usize;
        self.focus = Focus::Editor;
        self.find.key = None;
        self.follow_in_tree();
    }
}

fn shift(value: usize, before: usize, after: usize) -> usize {
    value.saturating_add_signed(after as isize - before as isize)
}
