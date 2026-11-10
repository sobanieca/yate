//! git as a child process: the repository root, the branch, the status of
//! files and file contents at HEAD.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FileStatus {
    Modified,
    Added,
    Deleted,
    Renamed,
    Untracked,
    Conflict,
}

impl FileStatus {
    pub fn letter(self) -> &'static str {
        match self {
            FileStatus::Modified => "M",
            FileStatus::Added => "A",
            FileStatus::Deleted => "D",
            FileStatus::Renamed => "R",
            FileStatus::Untracked => "U",
            FileStatus::Conflict => "!",
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct GitStatus {
    pub root: Option<PathBuf>,
    pub branch: Option<String>,
    /// Changed files by absolute path, in git's order.
    pub files: Vec<(PathBuf, FileStatus)>,
    by_path: HashMap<PathBuf, FileStatus>,
    /// Every directory that contains a changed file.
    dirs: HashSet<PathBuf>,
}

impl GitStatus {
    pub fn get(&self, path: &Path) -> Option<FileStatus> {
        self.by_path.get(path).copied()
    }

    pub fn dir_has_changes(&self, dir: &Path) -> bool {
        self.dirs.contains(dir)
    }

    pub fn is_changed(&self, path: &Path) -> bool {
        self.by_path.contains_key(path) || self.dirs.contains(path)
    }

    fn index(&mut self) {
        self.by_path = self.files.iter().cloned().collect();
        self.dirs.clear();
        for (path, _) in &self.files {
            let mut current = path.parent();
            while let Some(dir) = current {
                if !self.dirs.insert(dir.to_path_buf()) {
                    break;
                }
                current = dir.parent();
            }
        }
    }
}

fn git(dir: &Path, args: &[&str]) -> Option<Vec<u8>> {
    let output = Command::new("git")
        .args(args)
        .current_dir(dir)
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    output.status.success().then_some(output.stdout)
}

pub fn repository_root(dir: &Path) -> Option<PathBuf> {
    let output = git(dir, &["rev-parse", "--show-toplevel"])?;
    let root = String::from_utf8(output).ok()?;
    Some(PathBuf::from(root.trim_end()))
}

pub fn status(dir: &Path) -> GitStatus {
    let Some(root) = repository_root(dir) else {
        return GitStatus::default();
    };
    let mut status = GitStatus {
        root: Some(root.clone()),
        ..GitStatus::default()
    };
    if let Some(output) = git(
        &root,
        &[
            "status",
            "--porcelain=v1",
            "-z",
            "-b",
            "--untracked-files=all",
        ],
    ) {
        parse_status(&root, &output, &mut status);
    }
    status.index();
    status
}

fn parse_status(root: &Path, output: &[u8], status: &mut GitStatus) {
    let mut entries = output.split(|b| *b == 0).filter(|e| !e.is_empty());
    while let Some(entry) = entries.next() {
        let entry = String::from_utf8_lossy(entry);
        if let Some(branch) = entry.strip_prefix("## ") {
            let name = branch.split("...").next().unwrap_or(branch);
            let name = name.strip_prefix("No commits yet on ").unwrap_or(name);
            status.branch = Some(name.to_string());
            continue;
        }
        if entry.len() < 4 {
            continue;
        }
        let (x, y) = (entry.as_bytes()[0], entry.as_bytes()[1]);
        let path = &entry[3..];
        let file_status = match (x, y) {
            (b'?', b'?') => FileStatus::Untracked,
            (b'U', _) | (_, b'U') | (b'A', b'A') | (b'D', b'D') => FileStatus::Conflict,
            (b'R', _) | (_, b'R') => {
                entries.next();
                FileStatus::Renamed
            }
            (b'D', _) | (_, b'D') => FileStatus::Deleted,
            (b'A', _) => FileStatus::Added,
            _ => FileStatus::Modified,
        };
        status.files.push((root.join(path), file_status));
    }
}

/// The file at HEAD, `None` for files git does not track.
pub fn head_content(root: &Path, path: &Path) -> Option<String> {
    let relative = path.strip_prefix(root).ok()?;
    let spec = format!("HEAD:{}", relative.to_string_lossy());
    let bytes = git(root, &["show", &spec])?;
    let text = String::from_utf8(bytes).ok()?;
    Some(text.replace("\r\n", "\n"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn porcelain_output_is_parsed() {
        let output = b"## main...origin/main [ahead 1]\0 M src/a.rs\0?? new.txt\0R  b.rs\0old.rs\0D  gone.rs\0";
        let mut status = GitStatus::default();
        parse_status(Path::new("/repo"), output, &mut status);
        status.index();
        assert_eq!(status.branch.as_deref(), Some("main"));
        assert_eq!(
            status.get(Path::new("/repo/src/a.rs")),
            Some(FileStatus::Modified)
        );
        assert_eq!(
            status.get(Path::new("/repo/new.txt")),
            Some(FileStatus::Untracked)
        );
        assert_eq!(
            status.get(Path::new("/repo/b.rs")),
            Some(FileStatus::Renamed)
        );
        assert_eq!(
            status.get(Path::new("/repo/gone.rs")),
            Some(FileStatus::Deleted)
        );
        assert!(status.dir_has_changes(Path::new("/repo/src")));
        assert_eq!(status.files.len(), 4);
    }
}
