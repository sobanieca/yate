//! Tabs: the files you opened, in the order you opened them, in a bar
//! above the editor.
//!
//! A tab is a path, not a document: only the files on screen and the files
//! with unsaved changes are loaded (see `drop_hidden_docs`), so showing
//! another tab never asks anything, and a tab with unsaved changes shows a
//! `●` until it is saved. Closing such a tab asks to save or discard them.
//! Every file a pane shows has a tab; a new one goes right of the tab of
//! the file shown before. Closing a tab shows the tab next to it in the
//! focused pane; the tab goes away once no pane shows its file.

use super::App;
use crate::editor::document::Document;
use std::path::{Path, PathBuf};

impl App {
    /// Gives the file of the focused pane a tab, right of the tab of
    /// `previous`.
    pub(super) fn add_tab(&mut self, previous: Option<&Path>) {
        let Some(path) = self.doc().path.clone() else {
            return;
        };
        if self.tabs.contains(&path) {
            return;
        }
        let after = previous.and_then(|previous| self.tabs.iter().position(|t| t == previous));
        match after {
            Some(index) => self.tabs.insert(index + 1, path),
            None => self.tabs.push(path),
        }
    }

    /// The tab of the file in the focused pane.
    pub fn current_tab(&self) -> Option<usize> {
        let path = self.doc().path.as_ref()?;
        self.tabs.iter().position(|tab| tab == path)
    }

    /// Shows the tab `delta` places right (or left) of the current one.
    pub fn step_tab(&mut self, delta: isize) {
        if self.tabs.is_empty() {
            self.notify("No tabs");
            return;
        }
        let count = self.tabs.len() as isize;
        let next = match self.current_tab() {
            Some(index) => (index as isize + delta).rem_euclid(count) as usize,
            None => 0,
        };
        let path = self.tabs[next].clone();
        self.show_file(&path);
    }

    pub fn close_current_tab(&mut self) {
        match self.doc().path.clone() {
            Some(path) => self.close_tab(&path),
            None => self.notify("No tab to close"),
        }
    }

    /// Closes the tab of `path`, after its unsaved changes are saved or
    /// discarded. When the focused pane shows it, the tab next to it takes
    /// its place.
    pub fn close_tab(&mut self, path: &Path) {
        let Some(index) = self.tabs.iter().position(|tab| tab == path) else {
            return;
        };
        let target = path.to_path_buf();
        let again = move |app: &mut App| app.close_tab(&target);
        if self.doc().path.as_deref() != Some(path) {
            if self.on_screen(path) {
                self.show_file(path);
                if self.doc().path.as_deref() == Some(path) {
                    self.close_tab(path);
                }
                return;
            }
            let hidden = self
                .docs
                .iter()
                .position(|d| d.path.as_deref() == Some(path));
            if let Some(hidden) = hidden
                && !self.may_drop(hidden, again)
            {
                return;
            }
            self.tabs.remove(index);
            self.drop_hidden_docs();
            return;
        }
        if !self.in_another_pane(self.active) && !self.may_drop(self.active, again) {
            return;
        }
        let neighbor = self
            .tabs
            .get(index + 1)
            .or_else(|| index.checked_sub(1).and_then(|i| self.tabs.get(i)))
            .cloned();
        match neighbor {
            Some(next) => {
                if !self.open(&next, None) {
                    return;
                }
            }
            None => self.show_empty_editor(),
        }
        if !self.on_screen(path) {
            self.tabs.retain(|tab| tab != path);
        }
        self.relayout();
    }

    /// Closes the tabs of the files no pane shows, except the ones with
    /// unsaved changes.
    pub fn close_other_tabs(&mut self) {
        let unsaved = |tab: &PathBuf| !self.on_screen(tab) && self.is_unsaved(tab);
        let kept_unsaved = self.tabs.iter().filter(|tab| unsaved(tab)).count();
        let kept: Vec<PathBuf> = self
            .tabs
            .iter()
            .filter(|tab| self.on_screen(tab) || self.is_unsaved(tab))
            .cloned()
            .collect();
        let closed = self.tabs.len() - kept.len();
        self.tabs = kept;
        self.relayout();
        let plural = if closed == 1 { "" } else { "s" };
        let mut message = format!("Closed {closed} tab{plural}");
        if kept_unsaved > 0 {
            message.push_str(&format!("; kept {kept_unsaved} with unsaved changes"));
        }
        self.notify(message);
    }

    /// Puts an empty, unnamed document in the focused pane.
    fn show_empty_editor(&mut self) {
        let id = self.next_doc_id;
        self.next_doc_id += 1;
        self.docs.push(Document::scratch(id));
        self.active = self.docs.len() - 1;
        self.drop_hidden_docs();
        self.relayout();
        self.follow_in_tree();
    }

    /// Follows a rename of `from` (a file or a folder) to `to`.
    pub(super) fn rename_tabs(&mut self, from: &Path, to: &Path) {
        for tab in &mut self.tabs {
            if let Ok(rest) = tab.strip_prefix(from) {
                *tab = if rest.as_os_str().is_empty() {
                    to.to_path_buf()
                } else {
                    to.join(rest)
                };
            }
        }
    }
}
