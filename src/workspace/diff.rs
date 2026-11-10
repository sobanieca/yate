//! Line diffs against the file at HEAD: the signs in the editor gutter and
//! the hunks of the review view, with the changed words emphasized.

use crate::editor::document::Sign;
use similar::{Algorithm, ChangeTag, DiffTag, TextDiff};
use std::time::Duration;

const TIMEOUT: Duration = Duration::from_millis(150);
pub const CONTEXT_LINES: usize = 3;

/// A gutter sign per line of `current`. A deleted block is marked on the
/// line after it.
pub fn line_signs(base: &str, current: &str) -> Vec<Option<Sign>> {
    let diff = TextDiff::configure()
        .algorithm(Algorithm::Myers)
        .timeout(TIMEOUT)
        .diff_lines(base, current);
    let lines = current.lines().count().max(1) + 1;
    let mut signs = vec![None; lines];
    for op in diff.ops() {
        let (tag, old, new) = op.as_tag_tuple();
        match tag {
            DiffTag::Equal => {}
            DiffTag::Insert => {
                for line in new {
                    signs[line.min(lines - 1)] = Some(Sign::Added);
                }
            }
            DiffTag::Delete => {
                let at = new.start.min(lines - 1);
                if signs[at].is_none() {
                    signs[at] = Some(Sign::Deleted);
                }
            }
            DiffTag::Replace => {
                let changed = old.len().min(new.len());
                for (i, line) in new.enumerate() {
                    signs[line.min(lines - 1)] = Some(if i < changed {
                        Sign::Changed
                    } else {
                        Sign::Added
                    });
                }
            }
        }
    }
    signs
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LineKind {
    Hunk,
    Context,
    Added,
    Removed,
}

#[derive(Clone, Debug)]
pub struct DiffLine {
    pub kind: LineKind,
    pub old_line: Option<usize>,
    pub new_line: Option<usize>,
    pub text: String,
    /// Byte ranges of `text` that changed within the line.
    pub emphasis: Vec<(usize, usize)>,
}

pub fn file_diff(old: &str, new: &str) -> Vec<DiffLine> {
    let diff = TextDiff::configure()
        .algorithm(Algorithm::Myers)
        .timeout(Duration::from_secs(1))
        .diff_lines(old, new);
    let mut lines = Vec::new();
    for group in diff.grouped_ops(CONTEXT_LINES) {
        let (Some(first), Some(last)) = (group.first(), group.last()) else {
            continue;
        };
        let old_start = first.old_range().start;
        let new_start = first.new_range().start;
        let old_len = last.old_range().end - old_start;
        let new_len = last.new_range().end - new_start;
        lines.push(DiffLine {
            kind: LineKind::Hunk,
            old_line: Some(old_start),
            new_line: Some(new_start),
            text: format!(
                "@@ -{},{} +{},{} @@",
                old_start + 1,
                old_len,
                new_start + 1,
                new_len
            ),
            emphasis: Vec::new(),
        });
        for op in &group {
            for change in diff.iter_inline_changes(op) {
                let kind = match change.tag() {
                    ChangeTag::Equal => LineKind::Context,
                    ChangeTag::Insert => LineKind::Added,
                    ChangeTag::Delete => LineKind::Removed,
                };
                let mut text = String::new();
                let mut emphasis = Vec::new();
                for (emphasized, value) in change.iter_strings_lossy() {
                    let start = text.len();
                    text.push_str(&value);
                    if emphasized && kind != LineKind::Context {
                        emphasis.push((start, text.len()));
                    }
                }
                let trimmed = text.trim_end_matches(['\n', '\r']).len();
                text.truncate(trimmed);
                for range in &mut emphasis {
                    range.1 = range.1.min(trimmed);
                    range.0 = range.0.min(range.1);
                }
                let whole_line = emphasis.len() == 1 && emphasis[0] == (0, trimmed);
                if whole_line {
                    emphasis.clear();
                }
                lines.push(DiffLine {
                    kind,
                    old_line: change.old_index(),
                    new_line: change.new_index(),
                    text,
                    emphasis,
                });
            }
        }
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn signs_mark_added_changed_and_deleted_lines() {
        let signs = line_signs("a\nb\nc\nd\n", "a\nB\nc\nnew\n");
        assert_eq!(signs[0], None);
        assert_eq!(signs[1], Some(Sign::Changed));
        assert_eq!(signs[3], Some(Sign::Changed));
        let signs = line_signs("a\nb\nc\n", "a\nc\n");
        assert_eq!(signs[1], Some(Sign::Deleted));
        let signs = line_signs("a\n", "a\nb\n");
        assert_eq!(signs[1], Some(Sign::Added));
    }

    #[test]
    fn hunks_have_headers_line_numbers_and_emphasis() {
        let lines = file_diff(
            "one\nlet value = 1;\nthree\n",
            "one\nlet value = 2;\nthree\n",
        );
        assert_eq!(lines[0].kind, LineKind::Hunk);
        assert_eq!(lines[0].text, "@@ -1,3 +1,3 @@");
        let removed = lines.iter().find(|l| l.kind == LineKind::Removed).unwrap();
        let added = lines.iter().find(|l| l.kind == LineKind::Added).unwrap();
        assert_eq!(removed.old_line, Some(1));
        assert_eq!(added.new_line, Some(1));
        assert_eq!(added.text, "let value = 2;");
        assert!(!added.emphasis.is_empty());
    }
}
