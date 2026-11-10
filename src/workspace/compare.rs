//! Folder comparison: the files that differ between two folders, with
//! git-ignored files and `.git` left out like in the file index.

use super::files;
use super::git::FileStatus;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// Files that differ, sorted by their path inside the folders: the right
/// path for files in `right` (added or modified), the left path for files
/// only in `left` (deleted).
pub fn folders(left: &Path, right: &Path) -> Vec<(PathBuf, FileStatus)> {
    let mut sides: BTreeMap<PathBuf, (bool, bool)> = BTreeMap::new();
    for relative in relative_files(left) {
        sides.entry(relative).or_default().0 = true;
    }
    for relative in relative_files(right) {
        sides.entry(relative).or_default().1 = true;
    }
    sides
        .into_iter()
        .filter_map(|(relative, sides)| match sides {
            (true, true) if same_content(&left.join(&relative), &right.join(&relative)) => None,
            (true, true) => Some((right.join(relative), FileStatus::Modified)),
            (false, true) => Some((right.join(relative), FileStatus::Added)),
            (true, false) => Some((left.join(relative), FileStatus::Deleted)),
            (false, false) => None,
        })
        .collect()
}

fn relative_files(dir: &Path) -> Vec<PathBuf> {
    files::walker(dir)
        .parents(false)
        .build()
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_some_and(|t| t.is_file()))
        .take(files::MAX_FILES)
        .filter_map(|entry| entry.path().strip_prefix(dir).ok().map(Path::to_path_buf))
        .collect()
}

fn same_content(a: &Path, b: &Path) -> bool {
    let size = |path: &Path| std::fs::metadata(path).map(|m| m.len()).ok();
    size(a) == size(b) && std::fs::read(a).ok() == std::fs::read(b).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn folders_list_added_deleted_and_modified_files() {
        let dir = std::env::temp_dir().join(format!("yate-compare-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let (left, right) = (dir.join("a"), dir.join("b"));
        for (path, text) in [
            (left.join("same.txt"), "x"),
            (right.join("same.txt"), "x"),
            (left.join("sub/changed.txt"), "1"),
            (right.join("sub/changed.txt"), "2"),
            (left.join("gone.txt"), "old"),
            (right.join("new.txt"), "new"),
        ] {
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, text).unwrap();
        }
        let found = folders(&left, &right);
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(
            found,
            vec![
                (left.join("gone.txt"), FileStatus::Deleted),
                (right.join("new.txt"), FileStatus::Added),
                (right.join("sub/changed.txt"), FileStatus::Modified),
            ]
        );
    }
}
