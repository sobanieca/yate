//! Live grep: searches the project, or one folder of it, in background
//! threads. A newer query bumps the generation, and searches of older
//! generations stop.

use super::glob::GlobFilter;
use ignore::WalkState;
use regex::{Regex, RegexBuilder};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};

pub const MAX_RESULTS: usize = 5_000;
const MAX_FILE_BYTES: u64 = 4 * 1024 * 1024;
const MAX_LINE_CHARS: usize = 300;

#[derive(Clone, Debug)]
pub struct GrepMatch {
    pub path: PathBuf,
    pub relative: String,
    pub line: usize,
    pub col: usize,
    pub text: String,
    /// Byte range of the match in `text`.
    pub range: (usize, usize),
}

pub enum GrepEvent {
    Matches(u64, Vec<GrepMatch>),
    Done(u64),
}

/// Smart case like `ignorecase` + `smartcase`; an invalid regex is searched
/// literally.
pub fn build_regex(query: &str) -> Option<Regex> {
    if query.is_empty() {
        return None;
    }
    let case_insensitive = !query.chars().any(char::is_uppercase);
    RegexBuilder::new(query)
        .case_insensitive(case_insensitive)
        .build()
        .or_else(|_| {
            RegexBuilder::new(&regex::escape(query))
                .case_insensitive(case_insensitive)
                .build()
        })
        .ok()
}

pub struct GrepRequest {
    pub root: PathBuf,
    /// The searched folder: the root or a folder in it. Globs are relative
    /// to it.
    pub folder: PathBuf,
    pub query: String,
    pub globs: String,
}

pub fn spawn<F>(request: GrepRequest, generation: Arc<AtomicU64>, send: F)
where
    F: Fn(GrepEvent) + Send + Sync + Clone + 'static,
{
    let current = generation.load(Ordering::SeqCst);
    std::thread::spawn(move || {
        if let Some(regex) = build_regex(&request.query) {
            let filter = GlobFilter::parse(&request.globs);
            let search_in = Search {
                root: &request.root,
                folder: &request.folder,
                regex: &regex,
                filter: filter.as_ref(),
            };
            search_in.run(&generation, current, &send);
        }
        send(GrepEvent::Done(current));
    });
}

struct Search<'a> {
    root: &'a Path,
    folder: &'a Path,
    regex: &'a Regex,
    filter: Option<&'a GlobFilter>,
}

impl Search<'_> {
    fn run<F>(&self, generation: &AtomicU64, current: u64, send: &F)
    where
        F: Fn(GrepEvent) + Send + Sync + Clone,
    {
        let found = AtomicUsize::new(0);
        super::files::walker(self.folder).build_parallel().run(|| {
            let send = send.clone();
            let found = &found;
            Box::new(move |entry| {
                if generation.load(Ordering::Relaxed) != current
                    || found.load(Ordering::Relaxed) >= MAX_RESULTS
                {
                    return WalkState::Quit;
                }
                let Ok(entry) = entry else {
                    return WalkState::Continue;
                };
                let relative = entry
                    .path()
                    .strip_prefix(self.folder)
                    .unwrap_or(entry.path());
                let Some(file_type) = entry.file_type() else {
                    return WalkState::Continue;
                };
                if file_type.is_dir() {
                    let excluded = self.filter.is_some_and(|f| f.excludes_dir(relative));
                    return if excluded && entry.depth() > 0 {
                        WalkState::Skip
                    } else {
                        WalkState::Continue
                    };
                }
                if !file_type.is_file()
                    || self.filter.is_some_and(|f| !f.allows(relative))
                    || entry.metadata().is_ok_and(|m| m.len() > MAX_FILE_BYTES)
                {
                    return WalkState::Continue;
                }
                let matches = search_file(self.root, entry.path(), self.regex);
                if !matches.is_empty() {
                    found.fetch_add(matches.len(), Ordering::Relaxed);
                    send(GrepEvent::Matches(current, matches));
                }
                WalkState::Continue
            })
        });
    }
}

pub fn search_file(root: &Path, path: &Path, regex: &Regex) -> Vec<GrepMatch> {
    let Ok(bytes) = std::fs::read(path) else {
        return Vec::new();
    };
    if bytes[..bytes.len().min(8192)].contains(&0) {
        return Vec::new();
    }
    let Ok(text) = String::from_utf8(bytes) else {
        return Vec::new();
    };
    let relative = path
        .strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .into_owned();
    let mut matches = Vec::new();
    for (line_index, line) in text.lines().enumerate() {
        let Some(found) = regex.find(line) else {
            continue;
        };
        let line_text: String = line.chars().take(MAX_LINE_CHARS).collect();
        let end = found.end().min(line_text.len());
        let start = found.start().min(end);
        matches.push(GrepMatch {
            path: path.to_path_buf(),
            relative: relative.clone(),
            line: line_index,
            col: line[..found.start()].chars().count(),
            text: line_text,
            range: (start, end),
        });
        if matches.len() >= 200 {
            break;
        }
    }
    matches
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn smart_case_and_literal_fallback() {
        assert!(build_regex("foo").unwrap().is_match("FOO"));
        assert!(!build_regex("Foo").unwrap().is_match("foo"));
        assert!(build_regex("a(b").unwrap().is_match("xa(b"));
    }

    #[test]
    fn files_are_searched_line_by_line() {
        let dir = std::env::temp_dir().join(format!("yate-grep-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("a.txt");
        std::fs::write(&file, "one\ntwo needle\nthree\n").unwrap();
        let matches = search_file(&dir, &file, &build_regex("needle").unwrap());
        std::fs::remove_dir_all(&dir).unwrap();
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].line, 1);
        assert_eq!(matches[0].col, 4);
        assert_eq!(matches[0].relative, "a.txt");
    }
}
