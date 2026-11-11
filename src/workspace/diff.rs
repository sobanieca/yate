//! Line diffs against the file at HEAD: the signs in the editor gutter.

use crate::editor::document::Sign;
use similar::{Algorithm, DiffTag, TextDiff};
use std::time::Duration;

const TIMEOUT: Duration = Duration::from_millis(150);

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
}
