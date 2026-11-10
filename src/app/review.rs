//! The review view: changed files on the left, the diff of the selected
//! file on the right, syntax highlighted. It shows the changes against git
//! HEAD, or the differences between two files or two folders.

use crate::syntax::{self, languages};
use crate::ui::surface::Style;
use crate::workspace::diff::{self, DiffLine, LineKind};
use crate::workspace::git::{self, FileStatus, GitStatus};
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ReviewSource {
    Git,
    /// `left` is the old side, `right` the new one: two files or two folders.
    Compare {
        left: PathBuf,
        right: PathBuf,
    },
}

pub struct Review {
    pub source: ReviewSource,
    /// Changed files: the path on disk (the right side, or the left side
    /// for a file only there) and its status.
    pub files: Vec<(PathBuf, FileStatus)>,
    pub selected: usize,
    pub list_scroll: usize,
    pub lines: Vec<DiffLine>,
    pub styles: Vec<Vec<Option<Style>>>,
    /// The highlighted diff line.
    pub cursor: usize,
    pub scroll: usize,
    pub diff_focused: bool,
    loaded: Option<PathBuf>,
}

impl Review {
    pub fn git(git: &GitStatus) -> Review {
        let mut review = Review::empty(ReviewSource::Git);
        review.update_files(git);
        review
    }

    pub fn compare(left: PathBuf, right: PathBuf) -> Review {
        let mut review = Review::empty(ReviewSource::Compare { left, right });
        review.refresh_compare();
        review
    }

    /// Lists the files that differ between the compared files or folders.
    pub fn refresh_compare(&mut self) {
        let ReviewSource::Compare { left, right } = &self.source else {
            return;
        };
        let selected = self.selected_path().map(Path::to_path_buf);
        self.files = if right.is_dir() {
            crate::workspace::compare::folders(left, right)
        } else {
            vec![(right.clone(), FileStatus::Modified)]
        };
        self.selected = selected
            .and_then(|path| self.files.iter().position(|(p, _)| *p == path))
            .unwrap_or(0);
        self.loaded = None;
    }

    fn empty(source: ReviewSource) -> Review {
        Review {
            source,
            files: Vec::new(),
            selected: 0,
            list_scroll: 0,
            lines: Vec::new(),
            styles: Vec::new(),
            cursor: 0,
            scroll: 0,
            diff_focused: false,
            loaded: None,
        }
    }

    /// The old side of `path`: the left file of a comparison.
    pub fn old_path(&self, path: &Path) -> Option<PathBuf> {
        let ReviewSource::Compare { left, right } = &self.source else {
            return None;
        };
        if !right.is_dir() {
            return Some(left.clone());
        }
        match path.strip_prefix(right) {
            Ok(relative) => Some(left.join(relative)),
            Err(_) => Some(path.to_path_buf()),
        }
    }

    /// `path` as shown in the list: relative to the compared folders, or
    /// `None` to show it relative to the project.
    pub fn compared_name(&self, path: &Path) -> Option<String> {
        let ReviewSource::Compare { left, right } = &self.source else {
            return None;
        };
        if !right.is_dir() {
            return None;
        }
        path.strip_prefix(right)
            .or_else(|_| path.strip_prefix(left))
            .ok()
            .map(|relative| relative.to_string_lossy().into_owned())
    }

    pub fn update_files(&mut self, git: &GitStatus) {
        let selected = self.selected_path().map(Path::to_path_buf);
        self.files = git.files.clone();
        self.files.sort_by(|a, b| a.0.cmp(&b.0));
        if let Some(selected) = selected {
            self.selected = self
                .files
                .iter()
                .position(|(p, _)| *p == selected)
                .unwrap_or(0);
        }
        self.selected = self.selected.min(self.files.len().saturating_sub(1));
        self.loaded = None;
    }

    pub fn selected_path(&self) -> Option<&Path> {
        self.files
            .get(self.selected)
            .map(|(path, _)| path.as_path())
    }

    /// Computes the diff of the selected file. `current` is the text of an
    /// open buffer, so unsaved edits are reviewed too.
    pub fn load(
        &mut self,
        git_root: Option<&Path>,
        current: Option<String>,
        theme: &crate::ui::theme::Theme,
    ) {
        let Some((path, status)) = self.files.get(self.selected).cloned() else {
            self.lines.clear();
            self.styles.clear();
            self.loaded = None;
            return;
        };
        if self.loaded.as_ref() == Some(&path) {
            return;
        }
        let read = |path: &Path| std::fs::read_to_string(path).unwrap_or_default();
        let old = match self.old_path(&path) {
            Some(_) if status == FileStatus::Added => String::new(),
            Some(old_path) => read(&old_path),
            None => git_root
                .and_then(|root| git::head_content(root, &path))
                .unwrap_or_default(),
        };
        let new = if status == FileStatus::Deleted {
            String::new()
        } else {
            current.unwrap_or_else(|| read(&path))
        };
        self.lines = diff::file_diff(&old, &new);
        let first_line = new.lines().next().unwrap_or("");
        let language = languages::detect(&path, first_line);
        let (old_styles, new_styles) = match language {
            Some(language) => (
                syntax::highlight_text(language, &old, theme),
                syntax::highlight_text(language, &new, theme),
            ),
            None => (Vec::new(), Vec::new()),
        };
        self.styles = self
            .lines
            .iter()
            .map(|line| {
                let source = match line.kind {
                    LineKind::Removed => line.old_line.and_then(|l| old_styles.get(l)),
                    LineKind::Added | LineKind::Context => {
                        line.new_line.and_then(|l| new_styles.get(l))
                    }
                    LineKind::Hunk => None,
                };
                source
                    .filter(|styles| styles.len() >= line.text.len())
                    .map(|styles| styles[..line.text.len()].to_vec())
                    .unwrap_or_default()
            })
            .collect();
        self.cursor = self
            .lines
            .iter()
            .position(|l| matches!(l.kind, LineKind::Added | LineKind::Removed))
            .unwrap_or(0);
        self.scroll = 0;
        self.loaded = Some(path);
    }

    pub fn select_file(&mut self, delta: isize) {
        if self.files.is_empty() {
            return;
        }
        let last = self.files.len() as isize - 1;
        self.selected = (self.selected as isize + delta).clamp(0, last) as usize;
    }

    pub fn move_cursor(&mut self, delta: isize) {
        if self.lines.is_empty() {
            return;
        }
        let last = self.lines.len() as isize - 1;
        self.cursor = (self.cursor as isize + delta).clamp(0, last) as usize;
    }

    /// Moves to the next (or previous) block of changed lines.
    pub fn next_change(&mut self, forward: bool) {
        let is_change = |line: &DiffLine| matches!(line.kind, LineKind::Added | LineKind::Removed);
        let mut i = self.cursor;
        if forward {
            while i < self.lines.len() && is_change(&self.lines[i]) {
                i += 1;
            }
            while i < self.lines.len() && !is_change(&self.lines[i]) {
                i += 1;
            }
            if i < self.lines.len() {
                self.cursor = i;
            }
        } else {
            while i > 0 && is_change(&self.lines[i]) {
                i -= 1;
            }
            while i > 0 && !is_change(&self.lines[i]) {
                i -= 1;
            }
            while i > 0 && is_change(&self.lines[i - 1]) {
                i -= 1;
            }
            if is_change(&self.lines[i]) {
                self.cursor = i;
            }
        }
    }

    /// The line of the working file under the cursor.
    /// A removed line maps to the line that now stands in its place.
    pub fn target_line(&self) -> usize {
        let cursor = self.cursor.min(self.lines.len().saturating_sub(1));
        let following = self.lines[cursor..]
            .iter()
            .take_while(|line| line.kind != LineKind::Hunk || line.new_line.is_none())
            .find_map(|line| line.new_line);
        following
            .or_else(|| {
                self.lines[..cursor]
                    .iter()
                    .rev()
                    .find_map(|line| line.new_line)
            })
            .unwrap_or(0)
    }

    pub fn ensure_visible(&mut self, height: usize, list_height: usize) {
        if height > 0 {
            let margin = 3.min(height / 3);
            if self.cursor < self.scroll + margin {
                self.scroll = self.cursor.saturating_sub(margin);
            } else if self.cursor + margin >= self.scroll + height {
                self.scroll = self.cursor + margin + 1 - height;
            }
            self.scroll = self.scroll.min(self.lines.len().saturating_sub(height / 2));
        }
        if list_height > 0 {
            if self.selected < self.list_scroll {
                self.list_scroll = self.selected;
            } else if self.selected >= self.list_scroll + list_height {
                self.list_scroll = self.selected + 1 - list_height;
            }
        }
    }
}
