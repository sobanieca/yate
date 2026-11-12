//! Line bookmarks (like bookmarks.nvim), kept per project in
//! `~/.yate/bookmarks` as `path<TAB>line` lines.

use std::path::{Path, PathBuf};

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Bookmark {
    pub path: PathBuf,
    pub line: usize,
}

pub struct Bookmarks {
    file: Option<PathBuf>,
    pub items: Vec<Bookmark>,
}

impl Bookmarks {
    pub fn load(file: Option<PathBuf>) -> Bookmarks {
        let items = file
            .as_ref()
            .and_then(|file| std::fs::read_to_string(file).ok())
            .map(|text| parse(&text))
            .unwrap_or_default();
        Bookmarks { file, items }
    }

    pub fn lines_in(&self, path: &Path) -> Vec<usize> {
        self.items
            .iter()
            .filter(|b| b.path == path)
            .map(|b| b.line)
            .collect()
    }

    /// Returns true when the bookmark was added, false when removed.
    pub fn toggle(&mut self, path: &Path, line: usize) -> bool {
        let added = if let Some(index) = self
            .items
            .iter()
            .position(|b| b.path == path && b.line == line)
        {
            self.items.remove(index);
            false
        } else {
            self.items.push(Bookmark {
                path: path.to_path_buf(),
                line,
            });
            self.items.sort();
            true
        };
        self.save();
        added
    }

    /// The next (or previous) bookmark after `path:line`, wrapping around.
    pub fn next(&self, path: &Path, line: usize, forward: bool) -> Option<&Bookmark> {
        let current = Bookmark {
            path: path.to_path_buf(),
            line,
        };
        if forward {
            self.items
                .iter()
                .find(|b| **b > current)
                .or_else(|| self.items.first())
        } else {
            self.items
                .iter()
                .rev()
                .find(|b| **b < current)
                .or_else(|| self.items.last())
        }
    }

    /// Moves bookmarks of `path` when lines were inserted or removed above.
    pub fn shift(&mut self, path: &Path, from_line: usize, delta: isize) {
        let mut changed = false;
        for bookmark in self.items.iter_mut().filter(|b| b.path == path) {
            if bookmark.line > from_line || (delta > 0 && bookmark.line == from_line) {
                bookmark.line = (bookmark.line as isize + delta).max(from_line as isize) as usize;
                changed = true;
            }
        }
        if changed {
            self.items.sort();
            self.items.dedup();
            self.save();
        }
    }

    fn save(&self) {
        let Some(file) = &self.file else {
            return;
        };
        if let Some(dir) = file.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let text: String = self
            .items
            .iter()
            .map(|b| format!("{}\t{}\n", b.path.display(), b.line + 1))
            .collect();
        let _ = std::fs::write(file, text);
    }
}

fn parse(text: &str) -> Vec<Bookmark> {
    let mut items: Vec<Bookmark> = text
        .lines()
        .filter_map(|line| {
            let (path, number) = line.rsplit_once('\t')?;
            let number: usize = number.trim().parse().ok()?;
            Some(Bookmark {
                path: PathBuf::from(path),
                line: number.saturating_sub(1),
            })
        })
        .collect();
    items.sort();
    items.dedup();
    items
}

/// The bookmarks file of a project: one file per project root.
pub fn file_for(yate_dir: &Path, root: &Path) -> PathBuf {
    let name: String = root
        .to_string_lossy()
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '_' })
        .collect();
    yate_dir.join("bookmarks").join(name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn toggling_and_navigation_wrap_around() {
        let mut bookmarks = Bookmarks::load(None);
        let a = Path::new("/p/a.rs");
        assert!(bookmarks.toggle(a, 10));
        assert!(bookmarks.toggle(a, 2));
        assert_eq!(bookmarks.next(a, 2, true).unwrap().line, 10);
        assert_eq!(bookmarks.next(a, 10, true).unwrap().line, 2);
        assert_eq!(bookmarks.next(a, 2, false).unwrap().line, 10);
        assert!(!bookmarks.toggle(a, 2));
        assert_eq!(bookmarks.lines_in(a), [10]);
        bookmarks.shift(a, 5, 3);
        assert_eq!(bookmarks.lines_in(a), [13]);
    }

    #[test]
    fn the_file_format_round_trips() {
        assert_eq!(
            parse("/a\t3\nbroken\n/b\t1\n"),
            vec![
                Bookmark {
                    path: PathBuf::from("/a"),
                    line: 2
                },
                Bookmark {
                    path: PathBuf::from("/b"),
                    line: 0
                },
            ]
        );
    }
}
