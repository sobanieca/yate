//! The file tree: expanded folders, the visible rows and the filters for
//! git-ignored and git-changed files.

use super::git::GitStatus;
use std::collections::HashSet;
use std::path::{Path, PathBuf};

/// Columns of indentation per folder level.
pub const INDENT: usize = 2;

#[derive(Clone, Debug)]
pub struct Row {
    pub path: PathBuf,
    pub name: String,
    pub depth: usize,
    pub is_dir: bool,
    pub ignored: bool,
}

/// A checkbox above the tree that chooses the files it lists.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TreeFilter {
    Changed,
    Ignored,
}

impl TreeFilter {
    pub const ALL: [TreeFilter; 2] = [TreeFilter::Changed, TreeFilter::Ignored];

    pub fn label(self) -> &'static str {
        match self {
            TreeFilter::Changed => "changed",
            TreeFilter::Ignored => "ignored",
        }
    }

    /// The command that ticks or clears the checkbox.
    pub fn command(self) -> &'static str {
        match self {
            TreeFilter::Changed => "tree.toggleChanged",
            TreeFilter::Ignored => "tree.toggleIgnored",
        }
    }
}

pub struct FileTree {
    pub root: PathBuf,
    pub rows: Vec<Row>,
    pub expanded: HashSet<PathBuf>,
    pub selected: usize,
    pub scroll: usize,
    /// Columns of indentation scrolled off to the left, so deep names fit.
    pub h_scroll: usize,
    pub show_ignored: bool,
    /// Shows only files git reports as changed.
    pub changed_only: bool,
    /// Stops following the current file (`TreeLock` in the nvim setup).
    pub locked: bool,
    /// The filter checkbox that has the focus instead of a row.
    pub filter_focus: Option<TreeFilter>,
}

struct Entry {
    path: PathBuf,
    name: String,
    is_dir: bool,
    ignored: bool,
}

impl FileTree {
    pub fn new(root: &Path) -> FileTree {
        FileTree {
            root: root.to_path_buf(),
            rows: Vec::new(),
            expanded: HashSet::new(),
            selected: 0,
            scroll: 0,
            h_scroll: 0,
            show_ignored: false,
            changed_only: false,
            locked: false,
            filter_focus: None,
        }
    }

    pub fn filter_on(&self, filter: TreeFilter) -> bool {
        match filter {
            TreeFilter::Changed => self.changed_only,
            TreeFilter::Ignored => self.show_ignored,
        }
    }

    pub fn rebuild(&mut self, git: &GitStatus) {
        let selected = self.selected_path().map(Path::to_path_buf);
        self.rows.clear();
        let root = self.root.clone();
        self.add_children(&root, 0, git);
        if let Some(path) = selected {
            self.select_path(&path);
        }
        self.selected = self.selected.min(self.rows.len().saturating_sub(1));
    }

    fn add_children(&mut self, dir: &Path, depth: usize, git: &GitStatus) {
        for entry in list_dir(dir) {
            if entry.ignored && !self.show_ignored {
                continue;
            }
            if self.changed_only && !git.is_changed(&entry.path) {
                continue;
            }
            let expand = entry.is_dir && self.expanded.contains(&entry.path);
            let path = entry.path.clone();
            self.rows.push(Row {
                path: entry.path,
                name: entry.name,
                depth,
                is_dir: entry.is_dir,
                ignored: entry.ignored,
            });
            if expand {
                self.add_children(&path, depth + 1, git);
            }
        }
    }

    pub fn selected_row(&self) -> Option<&Row> {
        self.rows.get(self.selected)
    }

    pub fn selected_path(&self) -> Option<&Path> {
        self.selected_row().map(|row| row.path.as_path())
    }

    pub fn select_path(&mut self, path: &Path) -> bool {
        match self.rows.iter().position(|row| row.path == path) {
            Some(index) => {
                self.selected = index;
                true
            }
            None => false,
        }
    }

    /// Expands the folders above `path` and selects it.
    pub fn reveal(&mut self, path: &Path, git: &GitStatus) {
        let mut current = path.parent();
        while let Some(dir) = current {
            if !dir.starts_with(&self.root) || dir == self.root {
                break;
            }
            self.expanded.insert(dir.to_path_buf());
            current = dir.parent();
        }
        self.rebuild(git);
        self.select_path(path);
    }

    pub fn toggle(&mut self, git: &GitStatus) {
        let Some(row) = self.selected_row().cloned() else {
            return;
        };
        if !row.is_dir {
            return;
        }
        if !self.expanded.remove(&row.path) {
            self.expanded.insert(row.path);
        }
        self.rebuild(git);
    }

    pub fn expand(&mut self, git: &GitStatus) {
        if let Some(row) = self.selected_row().cloned()
            && row.is_dir
            && self.expanded.insert(row.path)
        {
            self.rebuild(git);
        }
    }

    /// Collapses the selected folder, or goes to the parent folder.
    pub fn collapse(&mut self, git: &GitStatus) {
        let Some(row) = self.selected_row().cloned() else {
            return;
        };
        if row.is_dir && self.expanded.remove(&row.path) {
            self.rebuild(git);
            return;
        }
        if let Some(parent) = row.path.parent() {
            self.select_path(parent);
        }
    }

    pub fn move_selection(&mut self, delta: isize) {
        if self.rows.is_empty() {
            return;
        }
        let last = self.rows.len() as isize - 1;
        self.selected = (self.selected as isize + delta).clamp(0, last) as usize;
    }

    /// The folder new files go to: the selected folder, or the folder of
    /// the selected file.
    pub fn target_dir(&self) -> PathBuf {
        match self.selected_row() {
            Some(row) if row.is_dir => row.path.clone(),
            Some(row) => row
                .path
                .parent()
                .map(Path::to_path_buf)
                .unwrap_or_else(|| self.root.clone()),
            None => self.root.clone(),
        }
    }

    pub fn ensure_visible(&mut self, height: usize) {
        if height == 0 {
            return;
        }
        if self.selected < self.scroll {
            self.scroll = self.selected;
        } else if self.selected >= self.scroll + height {
            self.scroll = self.selected + 1 - height;
        }
        let max_scroll = self.rows.len().saturating_sub(height);
        self.scroll = self.scroll.min(max_scroll);
    }

    /// Scrolls sideways as little as needed for the selected row's label
    /// (`label_width` columns after its indentation) to fit in `width`
    /// columns, hiding at most that row's indentation, in whole levels.
    pub fn fit_selected(&mut self, width: usize, label_width: usize) {
        let Some(row) = self.selected_row() else {
            self.h_scroll = 0;
            return;
        };
        let indent = row.depth * INDENT;
        let needed = (indent + label_width).saturating_sub(width);
        let needed = needed.div_ceil(INDENT) * INDENT;
        self.h_scroll = self.h_scroll.max(needed).min(indent);
    }
}

/// Folders first, then files, by name; marks entries that git ignores.
fn list_dir(dir: &Path) -> Vec<Entry> {
    let Ok(read) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let visible: HashSet<PathBuf> = super::files::walker(dir)
        .max_depth(Some(1))
        .build()
        .filter_map(Result::ok)
        .map(|entry| entry.into_path())
        .collect();
    let mut entries: Vec<Entry> = read
        .filter_map(Result::ok)
        .filter(|entry| entry.file_name() != ".git")
        .map(|entry| {
            let path = entry.path();
            let is_dir = path.is_dir();
            Entry {
                ignored: !visible.contains(&path),
                name: entry.file_name().to_string_lossy().into_owned(),
                path,
                is_dir,
            }
        })
        .collect();
    entries.sort_by(|a, b| {
        b.is_dir
            .cmp(&a.is_dir)
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
            .then_with(|| a.name.cmp(&b.name))
    });
    entries
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn folders_come_first_and_reveal_expands_parents() {
        let root = std::env::temp_dir().join(format!("yate-tree-{}", std::process::id()));
        std::fs::create_dir_all(root.join("src/deep")).unwrap();
        std::fs::write(root.join("b.txt"), "").unwrap();
        std::fs::write(root.join("src/deep/a.rs"), "").unwrap();
        let mut tree = FileTree::new(&root);
        let git = GitStatus::default();
        tree.rebuild(&git);
        let names: Vec<&str> = tree.rows.iter().map(|r| r.name.as_str()).collect();
        assert_eq!(names, ["src", "b.txt"]);
        tree.reveal(&root.join("src/deep/a.rs"), &git);
        assert_eq!(tree.selected_row().unwrap().name, "a.rs");
        assert_eq!(tree.selected_row().unwrap().depth, 2);
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn fit_selected_scrolls_off_whole_indent_levels_only() {
        let mut tree = FileTree::new(Path::new("/p"));
        tree.rows = (0..4)
            .map(|depth| Row {
                path: PathBuf::from(format!("/p/{depth}")),
                name: "name".into(),
                depth,
                is_dir: true,
                ignored: false,
            })
            .collect();
        tree.selected = 3;
        tree.fit_selected(10, 7);
        assert_eq!(tree.h_scroll, 4);
        tree.fit_selected(3, 20);
        assert_eq!(tree.h_scroll, 6);
        tree.selected = 2;
        tree.fit_selected(30, 7);
        assert_eq!(tree.h_scroll, 4);
        tree.selected = 0;
        tree.fit_selected(30, 7);
        assert_eq!(tree.h_scroll, 0);
    }
}
