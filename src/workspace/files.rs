//! The list of project files for Ctrl+P: every file that git does not
//! ignore, hidden files included, `.git` excluded.

use ignore::{WalkBuilder, WalkState};
use std::path::Path;
use std::sync::Mutex;

pub const MAX_FILES: usize = 200_000;

pub fn walker(root: &Path) -> WalkBuilder {
    let mut builder = WalkBuilder::new(root);
    builder
        .hidden(false)
        .git_ignore(true)
        .git_global(true)
        .git_exclude(true)
        .parents(true)
        .filter_entry(|entry| entry.file_name() != ".git");
    builder
}

/// Paths relative to `root`, sorted.
pub fn index(root: &Path) -> Vec<String> {
    let files = Mutex::new(Vec::new());
    walker(root).build_parallel().run(|| {
        Box::new(|entry| {
            let Ok(entry) = entry else {
                return WalkState::Continue;
            };
            if entry.file_type().is_some_and(|t| t.is_file()) {
                let relative = entry
                    .path()
                    .strip_prefix(root)
                    .unwrap_or(entry.path())
                    .to_string_lossy()
                    .into_owned();
                let mut files = files.lock().expect("file index lock");
                if files.len() >= MAX_FILES {
                    return WalkState::Quit;
                }
                files.push(relative);
            }
            WalkState::Continue
        })
    });
    let mut files = files.into_inner().expect("file index lock");
    files.sort();
    files
}
