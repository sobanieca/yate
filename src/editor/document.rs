//! An open file: its text, syntax tree, undo history, cursor and scroll.
//! All text changes go through `Document::apply`, which keeps the syntax
//! tree and the history in sync.

use super::history::{AppliedChange, EditKind, History};
use super::selection::Selection;
use crate::syntax::languages::{self, LanguageConfig};
use crate::syntax::{self, Syntax};
use ropey::Rope;
use std::borrow::Cow;
use std::io;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

/// Files larger than this are refused, a terminal editor is the wrong tool.
const MAX_FILE_BYTES: u64 = 64 * 1024 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Sign {
    Added,
    Changed,
    Deleted,
}

#[derive(Clone, Default)]
pub struct ViewState {
    pub scroll_line: usize,
    pub scroll_col: usize,
    /// Display column kept while moving up and down through shorter lines.
    pub preferred_col: Option<usize>,
}

#[derive(Default)]
pub struct GitState {
    /// Content of the file at HEAD, `None` when not tracked.
    pub base: Option<String>,
    pub signs: Vec<Option<Sign>>,
    pub signs_version: Option<u64>,
    pub signs_at: Option<std::time::Instant>,
}

pub struct Change {
    pub start: usize,
    pub end: usize,
    pub text: String,
}

impl Change {
    pub fn new(start: usize, end: usize, text: impl Into<String>) -> Self {
        Change {
            start,
            end,
            text: text.into(),
        }
    }

    pub fn insert(at: usize, text: impl Into<String>) -> Self {
        Change::new(at, at, text)
    }
}

pub struct Document {
    pub id: usize,
    pub path: Option<PathBuf>,
    pub rope: Rope,
    pub language: Option<&'static LanguageConfig>,
    pub syntax: Option<Syntax>,
    pub history: History,
    /// Increases on every change; caches compare against it.
    pub version: u64,
    pub crlf: bool,
    pub readonly: bool,
    pub disk_mtime: Option<SystemTime>,
    pub selection: Selection,
    pub view: ViewState,
    pub git: GitState,
}

impl Document {
    pub fn scratch(id: usize) -> Document {
        let mut history = History::default();
        history.clear();
        Document {
            id,
            path: None,
            rope: Rope::new(),
            language: None,
            syntax: None,
            history,
            version: 0,
            crlf: false,
            readonly: false,
            disk_mtime: None,
            selection: Selection::default(),
            view: ViewState::default(),
            git: GitState::default(),
        }
    }

    pub fn open(id: usize, path: &Path) -> io::Result<Document> {
        let mut document = Document::scratch(id);
        document.path = Some(path.to_path_buf());
        if path.exists() {
            document.load()?;
        } else {
            document.detect_language();
        }
        Ok(document)
    }

    fn load(&mut self) -> io::Result<()> {
        let path = self.path.clone().expect("only files are loaded");
        let metadata = std::fs::metadata(&path)?;
        if metadata.len() > MAX_FILE_BYTES {
            return Err(io::Error::other("file is too large"));
        }
        let bytes = std::fs::read(&path)?;
        let (text, valid) = match String::from_utf8(bytes) {
            Ok(text) => (text, true),
            Err(err) => (String::from_utf8_lossy(err.as_bytes()).into_owned(), false),
        };
        self.crlf = text.contains("\r\n");
        let text = if self.crlf {
            text.replace("\r\n", "\n")
        } else {
            text
        };
        self.rope = Rope::from_str(&text);
        self.readonly = !valid || metadata.permissions().readonly();
        self.disk_mtime = metadata.modified().ok();
        self.history.clear();
        self.version += 1;
        self.selection = self.selection.clamp(self.rope.len_chars());
        self.detect_language();
        Ok(())
    }

    /// Reloads the file from disk, keeping the cursor line.
    pub fn reload(&mut self) -> io::Result<()> {
        let line = self.cursor_line();
        self.load()?;
        let line = line.min(self.rope.len_lines().saturating_sub(1));
        self.selection = Selection::point(self.rope.line_to_char(line));
        Ok(())
    }

    pub fn detect_language(&mut self) {
        let first_line = self.line(0);
        self.language = self
            .path
            .as_ref()
            .and_then(|path| languages::detect(path, &first_line));
        self.syntax = self
            .language
            .and_then(|language| Syntax::new(language, &self.rope));
    }

    pub fn set_language(&mut self, language: Option<&'static LanguageConfig>) {
        self.language = language;
        self.syntax = language.and_then(|language| Syntax::new(language, &self.rope));
    }

    pub fn save(&mut self) -> io::Result<()> {
        let path = self
            .path
            .clone()
            .ok_or_else(|| io::Error::other("no file name"))?;
        if self.readonly {
            return Err(io::Error::other("file is read-only (or not valid UTF-8)"));
        }
        let mut text = self.rope.to_string();
        if self.crlf {
            text = text.replace('\n', "\r\n");
        }
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&path, text)?;
        self.disk_mtime = std::fs::metadata(&path).and_then(|m| m.modified()).ok();
        self.history.mark_saved();
        Ok(())
    }

    pub fn is_modified(&self) -> bool {
        !self.history.is_saved()
    }

    /// True when the file changed on disk since it was loaded or saved.
    pub fn changed_on_disk(&self) -> bool {
        let Some(path) = &self.path else {
            return false;
        };
        let current = std::fs::metadata(path).and_then(|m| m.modified()).ok();
        current.is_some() && current != self.disk_mtime
    }

    pub fn name(&self) -> String {
        self.path
            .as_ref()
            .and_then(|path| path.file_name())
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| "untitled".to_string())
    }

    /// Lines as VS Code shows them: a text ending with a line break has an
    /// empty last line.
    pub fn line_count(&self) -> usize {
        self.rope.len_lines()
    }

    /// Text of `line` without the line break.
    pub fn line(&self, line: usize) -> Cow<'_, str> {
        if line >= self.rope.len_lines() {
            return Cow::Borrowed("");
        }
        let slice = self.rope.line(line);
        let text: Cow<str> = slice.into();
        match text {
            Cow::Borrowed(text) => Cow::Borrowed(text.strip_suffix('\n').unwrap_or(text)),
            Cow::Owned(mut text) => {
                if text.ends_with('\n') {
                    text.pop();
                }
                Cow::Owned(text)
            }
        }
    }

    pub fn line_chars(&self, line: usize) -> Vec<char> {
        self.line(line).chars().collect()
    }

    pub fn line_len(&self, line: usize) -> usize {
        if line >= self.rope.len_lines() {
            return 0;
        }
        let slice = self.rope.line(line);
        let len = slice.len_chars();
        if len > 0 && slice.char(len - 1) == '\n' {
            len - 1
        } else {
            len
        }
    }

    pub fn line_start(&self, line: usize) -> usize {
        self.rope
            .line_to_char(line.min(self.rope.len_lines().saturating_sub(1)))
    }

    pub fn line_end(&self, line: usize) -> usize {
        self.line_start(line) + self.line_len(line)
    }

    pub fn line_of(&self, pos: usize) -> usize {
        self.rope.char_to_line(pos.min(self.rope.len_chars()))
    }

    pub fn line_col(&self, pos: usize) -> (usize, usize) {
        let line = self.line_of(pos);
        (line, pos - self.line_start(line))
    }

    pub fn pos(&self, line: usize, col: usize) -> usize {
        let line = line.min(self.rope.len_lines().saturating_sub(1));
        self.line_start(line) + col.min(self.line_len(line))
    }

    pub fn cursor_line(&self) -> usize {
        self.line_of(self.selection.head)
    }

    pub fn selected_text(&self) -> String {
        self.rope.slice(self.selection.range()).to_string()
    }

    pub fn text(&self) -> String {
        self.rope.to_string()
    }

    /// Applies `changes` in order (each one sees the text after the previous
    /// ones), records them as one undo step and sets the selection.
    pub fn apply(&mut self, changes: Vec<Change>, after: Selection, kind: EditKind) {
        if self.readonly {
            return;
        }
        let before = self.selection;
        let mut applied = Vec::with_capacity(changes.len());
        for change in changes {
            if change.start == change.end && change.text.is_empty() {
                continue;
            }
            applied.push(self.replace_raw(change.start, change.end, &change.text));
        }
        self.selection = after.clamp(self.rope.len_chars());
        if !applied.is_empty() {
            self.history.record(applied, before, self.selection, kind);
        }
    }

    fn replace_raw(&mut self, start: usize, end: usize, text: &str) -> AppliedChange {
        let len = self.rope.len_chars();
        let start = start.min(len);
        let end = end.clamp(start, len);
        let mut edit = syntax::begin_edit(&self.rope, start, end);
        let removed = self.rope.slice(start..end).to_string();
        self.rope.remove(start..end);
        self.rope.insert(start, text);
        syntax::finish_edit(&self.rope, &mut edit, text.len());
        if let Some(syntax) = &mut self.syntax {
            syntax.edit(&edit);
        }
        self.version += 1;
        AppliedChange {
            start,
            removed,
            inserted: text.to_string(),
        }
    }

    pub fn undo(&mut self) -> bool {
        let Some(transaction) = self.history.undo().cloned() else {
            return false;
        };
        for change in transaction.changes.iter().rev() {
            let end = change.start + change.inserted.chars().count();
            self.replace_raw(change.start, end, &change.removed);
        }
        self.selection = transaction.before.clamp(self.rope.len_chars());
        true
    }

    pub fn redo(&mut self) -> bool {
        let Some(transaction) = self.history.redo().cloned() else {
            return false;
        };
        for change in &transaction.changes {
            let end = change.start + change.removed.chars().count();
            self.replace_raw(change.start, end, &change.inserted);
        }
        self.selection = transaction.after.clamp(self.rope.len_chars());
        true
    }

    /// Replaces the whole text as one undo step (formatters, plugins), keeping
    /// the cursor on the same line.
    pub fn set_text(&mut self, text: &str) {
        if self.rope == text {
            return;
        }
        let (line, col) = self.line_col(self.selection.head);
        let len = self.rope.len_chars();
        let before = self.selection;
        let applied = vec![self.replace_raw(0, len, text)];
        let pos = self.pos(line, col);
        self.selection = Selection::point(pos);
        self.history
            .record(applied, before, self.selection, EditKind::Other);
    }

    pub fn refresh_syntax(&mut self) {
        if let Some(syntax) = &mut self.syntax {
            syntax.update(&self.rope);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn document(text: &str) -> Document {
        let mut document = Document::scratch(1);
        document.rope = Rope::from_str(text);
        document
    }

    #[test]
    fn undo_and_redo_restore_text_and_cursor() {
        let mut doc = document("hello\n");
        doc.apply(
            vec![Change::insert(5, " world")],
            Selection::point(11),
            EditKind::Other,
        );
        doc.apply(
            vec![Change::new(0, 1, "H")],
            Selection::point(1),
            EditKind::Other,
        );
        assert_eq!(doc.text(), "Hello world\n");
        assert!(doc.undo());
        assert_eq!(doc.text(), "hello world\n");
        assert_eq!(doc.selection, Selection::point(11));
        assert!(doc.undo());
        assert_eq!(doc.text(), "hello\n");
        assert!(doc.redo());
        assert_eq!(doc.text(), "hello world\n");
    }

    #[test]
    fn typing_merges_into_one_undo_step() {
        let mut doc = document("");
        for (i, c) in "abc".chars().enumerate() {
            doc.apply(
                vec![Change::insert(i, c.to_string())],
                Selection::point(i + 1),
                EditKind::Insert,
            );
        }
        assert!(doc.undo());
        assert_eq!(doc.text(), "");
        assert!(!doc.undo());
    }

    #[test]
    fn saving_tracks_modification() {
        let mut doc = document("a");
        assert!(!doc.is_modified());
        doc.apply(
            vec![Change::insert(1, "b")],
            Selection::point(2),
            EditKind::Insert,
        );
        assert!(doc.is_modified());
        doc.history.mark_saved();
        assert!(!doc.is_modified());
        doc.apply(
            vec![Change::insert(2, "c")],
            Selection::point(3),
            EditKind::Insert,
        );
        assert!(doc.is_modified());
        doc.undo();
        assert!(!doc.is_modified());
    }

    #[test]
    fn line_helpers_ignore_the_line_break() {
        let doc = document("ab\ncd\n");
        assert_eq!(doc.line(0), "ab");
        assert_eq!(doc.line_len(1), 2);
        assert_eq!(doc.line_count(), 3);
        assert_eq!(doc.pos(1, 10), 5);
        assert_eq!(doc.line_col(4), (1, 1));
    }
}
