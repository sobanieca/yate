//! Pickers (Telescope style): a query, a filtered list and a preview. Files,
//! commands and bookmarks are filtered with fuzzy matching;
//! live grep results come from background searches.

use crate::ui::input::LineInput;
use crate::workspace::glob::GlobFilter;
use nucleo_matcher::pattern::{CaseMatching, Normalization, Pattern};
use nucleo_matcher::{Config, Matcher, Utf32Str};
use std::borrow::Cow;
use std::path::PathBuf;

const MAX_SHOWN: usize = 2_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PickerKind {
    Files,
    Grep,
    Commands,
    Bookmarks,
    Definitions,
}

#[derive(Clone, Debug, Default)]
pub struct PickerItem {
    pub label: String,
    /// Shown dimmed after the label (key bindings, line text...).
    pub detail: String,
    pub path: Option<PathBuf>,
    pub line: Option<usize>,
    pub col: usize,
    /// Command id for the command palette.
    pub data: String,
    /// The section of a command: shown in a column before the label and
    /// matched by the query like the label.
    pub group: String,
    /// Byte range in `detail` to emphasize (the grep match).
    pub detail_match: Option<(usize, usize)>,
}

impl PickerItem {
    /// What the query is matched against: the label, after the section
    /// when the item has one.
    fn searched_text(&self) -> Cow<'_, str> {
        if self.group.is_empty() {
            Cow::Borrowed(&self.label)
        } else {
            Cow::Owned(format!("{} {}", self.group, self.label))
        }
    }

    /// The char index in the matched text where the label starts.
    pub fn label_offset(&self) -> usize {
        if self.group.is_empty() {
            0
        } else {
            self.group.chars().count() + 1
        }
    }
}

#[derive(Clone)]
pub struct Picker {
    pub kind: PickerKind,
    pub title: String,
    pub input: LineInput,
    /// Glob filter of the file finder and live grep (`*.ts, !*.test.ts`),
    /// relative to `folder`.
    pub glob: LineInput,
    pub glob_focused: bool,
    /// The folder searched, relative to the project root; None searches
    /// the whole project.
    pub folder: Option<String>,
    filter: Option<GlobFilter>,
    filter_globs: String,
    pub items: Vec<PickerItem>,
    /// Indices of shown items and the char positions of matched characters
    /// (in the section and the label, see `PickerItem::label_offset`).
    pub shown: Vec<(usize, Vec<u32>)>,
    pub selected: usize,
    pub scroll: usize,
    pub searching: bool,
    matcher: Matcher,
}

impl Picker {
    pub fn new(kind: PickerKind, title: &str, items: Vec<PickerItem>) -> Picker {
        let config = if kind == PickerKind::Files {
            Config::DEFAULT.match_paths()
        } else {
            Config::DEFAULT
        };
        let mut picker = Picker {
            kind,
            title: title.to_string(),
            input: LineInput::default(),
            glob: LineInput::default(),
            glob_focused: false,
            folder: None,
            filter: None,
            filter_globs: String::new(),
            items,
            shown: Vec::new(),
            selected: 0,
            scroll: 0,
            searching: false,
            matcher: Matcher::new(config),
        };
        picker.refilter();
        picker
    }

    pub fn is_live(&self) -> bool {
        self.kind == PickerKind::Grep
    }

    /// Pickers with a glob field.
    pub fn has_glob(&self) -> bool {
        matches!(self.kind, PickerKind::Files | PickerKind::Grep)
    }

    /// Whether a file item passes the folder scope and the glob filter.
    fn in_scope(&self, label: &str) -> bool {
        let relative = match &self.folder {
            Some(folder) => match label
                .strip_prefix(folder.as_str())
                .and_then(|rest| rest.strip_prefix('/'))
            {
                Some(rest) => rest,
                None => return false,
            },
            None => label,
        };
        self.filter
            .as_ref()
            .is_none_or(|filter| filter.allows(std::path::Path::new(relative)))
    }

    pub fn set_items(&mut self, items: Vec<PickerItem>) {
        self.items = items;
        self.refilter();
    }

    /// Adds results; live results are kept ordered by file and line, so the
    /// list does not depend on which search thread finished first.
    pub fn append_items(&mut self, items: Vec<PickerItem>) {
        if self.is_live() {
            self.items.extend(items);
            self.items
                .sort_by(|a, b| (&a.path, a.line, a.col).cmp(&(&b.path, b.line, b.col)));
            self.shown = (0..self.items.len().min(MAX_SHOWN))
                .map(|i| (i, Vec::new()))
                .collect();
            self.selected = self.selected.min(self.shown.len().saturating_sub(1));
        } else {
            self.items.extend(items);
            self.refilter();
        }
    }

    /// Recomputes the shown items for the current query.
    pub fn refilter(&mut self) {
        self.selected = 0;
        self.scroll = 0;
        let query = self.input.text.trim();
        let query = if self.kind == PickerKind::Files {
            crate::workspace::resolve::split_location(query).0
        } else {
            query
        };
        if self.filter_globs != self.glob.text {
            self.filter_globs = self.glob.text.clone();
            self.filter = GlobFilter::parse(&self.filter_globs);
        }
        let filtered = self.kind == PickerKind::Files;
        let candidates: Vec<usize> = (0..self.items.len())
            .filter(|&i| !filtered || self.in_scope(&self.items[i].label))
            .collect();
        if self.is_live() || query.is_empty() {
            self.shown = candidates
                .into_iter()
                .take(MAX_SHOWN)
                .map(|i| (i, Vec::new()))
                .collect();
            return;
        }
        let pattern = Pattern::parse(query, CaseMatching::Smart, Normalization::Smart);
        let mut buffer = Vec::new();
        let mut scored: Vec<(u32, usize)> = candidates
            .into_iter()
            .filter_map(|index| {
                let text = self.items[index].searched_text();
                let haystack = Utf32Str::new(&text, &mut buffer);
                pattern
                    .score(haystack, &mut self.matcher)
                    .map(|score| (score, index))
            })
            .collect();
        scored.sort_by(|a, b| {
            b.0.cmp(&a.0)
                .then_with(|| {
                    self.items[a.1]
                        .label
                        .len()
                        .cmp(&self.items[b.1].label.len())
                })
                .then(a.1.cmp(&b.1))
        });
        scored.truncate(MAX_SHOWN);
        self.shown = scored
            .into_iter()
            .map(|(_, index)| {
                let mut indices = Vec::new();
                let text = self.items[index].searched_text();
                let haystack = Utf32Str::new(&text, &mut buffer);
                pattern.indices(haystack, &mut self.matcher, &mut indices);
                indices.sort_unstable();
                indices.dedup();
                (index, indices)
            })
            .collect();
    }

    pub fn selected_item(&self) -> Option<&PickerItem> {
        self.shown
            .get(self.selected)
            .and_then(|(index, _)| self.items.get(*index))
    }

    pub fn move_selection(&mut self, delta: isize) {
        if self.shown.is_empty() {
            return;
        }
        let last = self.shown.len() as isize - 1;
        let next = self.selected as isize + delta;
        self.selected = if next < 0 {
            if delta == -1 { last as usize } else { 0 }
        } else if next > last {
            if delta == 1 { 0 } else { last as usize }
        } else {
            next as usize
        };
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
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(label: &str) -> PickerItem {
        PickerItem {
            label: label.to_string(),
            ..PickerItem::default()
        }
    }

    #[test]
    fn fuzzy_matching_ranks_and_marks_characters() {
        let mut picker = Picker::new(
            PickerKind::Files,
            "Files",
            vec![
                item("src/editor/view.rs"),
                item("src/app.rs"),
                item("docs/apple.md"),
            ],
        );
        assert_eq!(picker.shown.len(), 3);
        picker.input.set("app");
        picker.refilter();
        assert_eq!(picker.selected_item().unwrap().label, "src/app.rs");
        assert_eq!(picker.shown[0].1, vec![4, 5, 6]);
        picker.input.set("zzz");
        picker.refilter();
        assert!(picker.shown.is_empty());
    }

    #[test]
    fn files_are_scoped_to_a_folder_and_filtered_by_globs() {
        let files = ["src/a.rs", "src/a.test.rs", "src/ui/b.rs", "docs/c.md"];
        let mut picker = Picker::new(
            PickerKind::Files,
            "Files",
            files.iter().map(|f| item(f)).collect(),
        );
        let shown = |picker: &Picker| -> Vec<String> {
            picker
                .shown
                .iter()
                .map(|(i, _)| picker.items[*i].label.clone())
                .collect()
        };
        picker.glob.set("!*.test.rs, !ui");
        picker.refilter();
        assert_eq!(shown(&picker), ["src/a.rs", "docs/c.md"]);
        picker.folder = Some("src".to_string());
        picker.glob.set("*.rs, !a.*");
        picker.refilter();
        assert_eq!(shown(&picker), ["src/ui/b.rs"]);
        picker.input.set("b");
        picker.refilter();
        assert_eq!(shown(&picker), ["src/ui/b.rs"]);
    }

    #[test]
    fn selection_wraps_one_step_at_a_time() {
        let mut picker = Picker::new(PickerKind::Commands, "C", vec![item("a"), item("b")]);
        picker.move_selection(-1);
        assert_eq!(picker.selected, 1);
        picker.move_selection(1);
        assert_eq!(picker.selected, 0);
        picker.move_selection(10);
        assert_eq!(picker.selected, 1);
    }
}
