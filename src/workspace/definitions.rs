//! Project-wide go to definition: walks the project like live grep, parses only the
//! files that mention the name, and keeps the definitions the tags queries find.

use crate::syntax::{self, definitions, languages};
use ignore::WalkState;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

const MAX_FILE_BYTES: u64 = 2 * 1024 * 1024;
const MAX_RESULTS: usize = 500;
const MAX_LINE_CHARS: usize = 300;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Found {
    pub path: PathBuf,
    pub relative: String,
    pub line: usize,
    pub col: usize,
    pub text: String,
    /// Byte range of the name in `text`.
    pub range: (usize, usize),
}

/// Definitions of `name` in files of the language `family`, ordered by
/// path and line, without the one at `skip` (path and line).
pub fn search(
    root: &Path,
    name: &str,
    family: &str,
    skip: Option<&(PathBuf, usize)>,
) -> Vec<Found> {
    let found = Mutex::new(Vec::new());
    super::files::walker(root).build_parallel().run(|| {
        let found = &found;
        Box::new(move |entry| {
            let Ok(entry) = entry else {
                return WalkState::Continue;
            };
            if !entry.file_type().is_some_and(|t| t.is_file())
                || entry.metadata().is_ok_and(|m| m.len() > MAX_FILE_BYTES)
            {
                return WalkState::Continue;
            }
            let results = search_file(root, entry.path(), name, family);
            let mut found = found.lock().expect("definition search lock");
            found.extend(results);
            if found.len() >= MAX_RESULTS {
                return WalkState::Quit;
            }
            WalkState::Continue
        })
    });
    let mut found = found.into_inner().expect("definition search lock");
    found.retain(|f| skip.is_none_or(|(path, line)| f.path != *path || f.line != *line));
    found.sort_by(|a, b| (&a.relative, a.line, a.col).cmp(&(&b.relative, b.line, b.col)));
    found.truncate(MAX_RESULTS);
    found
}

fn search_file(root: &Path, path: &Path, name: &str, family: &str) -> Vec<Found> {
    let Some(language) = languages::detect(path, "")
        .filter(|language| definitions::family(language.name) == family)
        .filter(|language| language.definitions().is_some())
    else {
        return Vec::new();
    };
    let Ok(text) = std::fs::read_to_string(path) else {
        return Vec::new();
    };
    if !mentions(&text, name) {
        return Vec::new();
    }
    let Some(tree) = syntax::parse_text(language, &text) else {
        return Vec::new();
    };
    let relative = path
        .strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .into_owned();
    definitions::collect(language, &tree, &text, name)
        .into_iter()
        .filter(|definition| !definition.local)
        .map(|definition| {
            let start = definition.name_range.start;
            let line_start = text[..start].rfind('\n').map_or(0, |i| i + 1);
            let line_end = text[start..].find('\n').map_or(text.len(), |i| start + i);
            let name_start = start - line_start;
            Found {
                range: (name_start, name_start + name.len()),
                path: path.to_path_buf(),
                relative: relative.clone(),
                line: text[..start].matches('\n').count(),
                col: text[line_start..start].chars().count(),
                text: text[line_start..line_end]
                    .trim_end()
                    .chars()
                    .take(MAX_LINE_CHARS)
                    .collect(),
            }
        })
        .collect()
}

/// Whether `name` appears in `text` as a whole word.
fn mentions(text: &str, name: &str) -> bool {
    let is_word = |c: Option<char>| c.is_some_and(|c| c.is_alphanumeric() || c == '_' || c == '$');
    text.match_indices(name).any(|(at, _)| {
        !is_word(text[..at].chars().next_back()) && !is_word(text[at + name.len()..].chars().next())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn definitions_are_found_across_files_of_the_same_family() {
        let root = std::env::temp_dir().join(format!("yate-definitions-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("src")).unwrap();
        std::fs::write(root.join("src/a.ts"), "export function render() {}\n").unwrap();
        std::fs::write(root.join("src/b.js"), "render();\nclass render {}\n").unwrap();
        std::fs::write(root.join("src/c.py"), "def render():\n    pass\n").unwrap();
        std::fs::write(root.join("src/d.ts"), "const rendered = 1;\n").unwrap();
        let found = search(&root, "render", "javascript", None);
        let skip = (root.join("src/a.ts"), 0);
        let skipped = search(&root, "render", "javascript", Some(&skip));
        std::fs::remove_dir_all(&root).unwrap();
        let places: Vec<(String, usize, usize)> = found
            .iter()
            .map(|f| (f.relative.clone(), f.line, f.col))
            .collect();
        assert_eq!(
            places,
            [
                ("src/a.ts".to_string(), 0, 16),
                ("src/b.js".to_string(), 1, 6)
            ]
        );
        assert_eq!(skipped.len(), 1);
        assert_eq!(found[1].text, "class render {}");
    }
}
