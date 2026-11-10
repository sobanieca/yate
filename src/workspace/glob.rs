//! Glob filters of the file finder and live grep: comma-separated gitignore
//! globs like `*.rs, src/**, !*.test.ts`. A plain glob keeps only matching
//! files (or files inside a matching folder); a `!glob` drops matching files
//! and folders. Paths are relative to the searched folder.

use ignore::overrides::{Override, OverrideBuilder};
use std::path::Path;

#[derive(Clone)]
pub struct GlobFilter {
    includes: Option<Override>,
    excludes: Option<Override>,
}

impl GlobFilter {
    /// The filter for `globs`, or None when there is nothing to filter.
    pub fn parse(globs: &str) -> Option<GlobFilter> {
        let globs: Vec<&str> = globs
            .split(',')
            .map(str::trim)
            .filter(|glob| !glob.is_empty() && *glob != "!")
            .collect();
        if globs.is_empty() {
            return None;
        }
        let build = |negated: bool| {
            let mut builder = OverrideBuilder::new(".");
            let mut any = false;
            for glob in globs.iter().filter(|g| g.starts_with('!') == negated) {
                any |= builder.add(glob).is_ok();
            }
            any.then(|| builder.build().ok()).flatten()
        };
        let filter = GlobFilter {
            includes: build(false),
            excludes: build(true),
        };
        (filter.includes.is_some() || filter.excludes.is_some()).then_some(filter)
    }

    /// Whether a folder (relative path) is dropped with everything in it.
    pub fn excludes_dir(&self, relative: &Path) -> bool {
        self.excludes
            .as_ref()
            .is_some_and(|excludes| excludes.matched(relative, true).is_ignore())
    }

    /// Whether a file (relative path) passes the filter.
    pub fn allows(&self, relative: &Path) -> bool {
        let folders: Vec<&Path> = relative
            .ancestors()
            .skip(1)
            .filter(|folder| !folder.as_os_str().is_empty())
            .collect();
        if folders.iter().any(|folder| self.excludes_dir(folder)) {
            return false;
        }
        if let Some(excludes) = &self.excludes
            && excludes.matched(relative, false).is_ignore()
        {
            return false;
        }
        let Some(includes) = &self.includes else {
            return true;
        };
        includes.matched(relative, false).is_whitelist()
            || folders
                .iter()
                .any(|folder| includes.matched(folder, true).is_whitelist())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn allows(globs: &str, path: &str) -> bool {
        GlobFilter::parse(globs).unwrap().allows(Path::new(path))
    }

    #[test]
    fn plain_globs_keep_matching_files_and_folders() {
        assert!(allows("*.rs", "src/app/mod.rs"));
        assert!(!allows("*.rs", "docs/index.html"));
        assert!(allows("src/**", "src/app/mod.rs"));
        assert!(allows("src", "src/app/mod.rs"));
        assert!(!allows("src", "docs/src.md/x"));
        assert!(allows("*.md, *.rs", "README.md"));
    }

    #[test]
    fn negated_globs_drop_files_and_folders() {
        assert!(!allows("!*.test.ts", "src/a.test.ts"));
        assert!(allows("!*.test.ts", "src/a.ts"));
        assert!(!allows("!tests", "tests/e2e/main.rs"));
        assert!(!allows("!tests/**", "tests/e2e/main.rs"));
        assert!(allows("!tests", "src/tests.rs"));
        assert!(!allows("*.rs, !src/app/**", "src/app/mod.rs"));
        assert!(allows("*.rs, !src/app/**", "src/main.rs"));
    }

    #[test]
    fn empty_globs_filter_nothing() {
        assert!(GlobFilter::parse(" , ").is_none());
        assert!(GlobFilter::parse("!").is_none());
    }
}
