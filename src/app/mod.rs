//! The application: the files on screen, panels, overlays and the event loop.
//! Background work (file index, git status, live grep, update check) reports back through
//! one channel, so all state is changed on the main thread.

mod commands;
mod goto;
mod input;
mod panes;
mod split;
mod tabs;

pub use commands::menu_key_label;
pub use panes::{Direction, Pane, PaneArea, Panes, Side};
pub use split::{MAX_SWITCH_TARGETS, Switcher};

use crate::bookmarks::{self, Bookmarks};
use crate::clipboard::Clipboard;
use crate::config::{self, Options};
use crate::editor::document::{Document, Sign, ViewState};
use crate::editor::selection::Selection;
use crate::keymap::{Context, Keymap};
use crate::picker::Picker;
use crate::plugins::{Action, PluginHost, Snapshot};
use crate::terminal::Terminal;
use crate::ui::input::LineInput;
use crate::ui::surface::{Rect, Style, Surface};
use crate::workspace::git::{self, GitStatus};
use crate::workspace::grep::GrepEvent;
use crate::workspace::{diff, files, tree::FileTree};
use crossterm::event::Event;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::AtomicU64;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::time::{Duration, Instant};

const TICK: Duration = Duration::from_millis(500);
const DISK_CHECK: Duration = Duration::from_secs(2);
const GIT_REFRESH: Duration = Duration::from_secs(10);
const MESSAGE_TIME: Duration = Duration::from_secs(5);
const MAX_SIGN_BYTES: usize = 4 * 1024 * 1024;
const MAX_RECENT: usize = 50;
/// Git signs of files larger than this are recomputed at most every
/// `SIGN_THROTTLE` while typing.
const SIGN_THROTTLE_BYTES: usize = 256 * 1024;
const SIGN_THROTTLE: Duration = Duration::from_millis(400);

pub enum AppEvent {
    Terminal(Event),
    FileIndex(Vec<String>),
    Git(GitStatus, Option<String>),
    Grep(GrepEvent),
    /// A release newer than this build, when there is one.
    UpdateCheck(Option<String>),
    Definitions(goto::DefinitionSearch),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Focus {
    Editor,
    Tree,
    Find,
}

pub enum PromptKind {
    GoToLine,
    NewFile,
    SaveAs(usize),
    Rename(PathBuf),
    Delete(PathBuf),
    Plugin(u32),
    ResizePane,
    QuitUnsaved,
    /// Asked before a file with unsaved changes leaves the screen; `then`
    /// repeats what was asked for once they are saved or discarded.
    LeaveUnsaved {
        doc_id: usize,
        then: Box<dyn FnOnce(&mut App)>,
    },
}

pub struct Prompt {
    pub title: String,
    pub input: LineInput,
    pub kind: PromptKind,
    /// Answered with a button instead of text.
    pub confirm: bool,
    /// The focused button of a confirmation.
    pub choice: usize,
}

/// A button of a confirmation and the answer it gives.
pub struct Choice {
    pub label: &'static str,
    pub answer: &'static str,
}

const SAVE_CHOICES: &[Choice] = &[
    Choice {
        label: "Save",
        answer: "s",
    },
    Choice {
        label: "Discard",
        answer: "d",
    },
    Choice {
        label: "Cancel",
        answer: "n",
    },
];

const YES_NO_CHOICES: &[Choice] = &[
    Choice {
        label: "Yes",
        answer: "y",
    },
    Choice {
        label: "No",
        answer: "n",
    },
];

impl Prompt {
    /// A prompt answered by typing text.
    pub fn text(title: impl Into<String>, input: LineInput, kind: PromptKind) -> Prompt {
        Prompt {
            title: title.into(),
            input,
            kind,
            confirm: false,
            choice: 0,
        }
    }

    /// A prompt answered with a button: Save focused first, or No.
    pub fn confirm(title: impl Into<String>, kind: PromptKind) -> Prompt {
        let choice = match kind {
            PromptKind::LeaveUnsaved { .. } => 0,
            _ => 1,
        };
        Prompt {
            title: title.into(),
            input: LineInput::default(),
            kind,
            confirm: true,
            choice,
        }
    }

    pub fn choices(&self) -> &'static [Choice] {
        match self.kind {
            PromptKind::LeaveUnsaved { .. } => SAVE_CHOICES,
            _ => YES_NO_CHOICES,
        }
    }

    /// The answer of the button whose label starts with `letter`.
    pub fn answer_for(&self, letter: char) -> Option<&'static str> {
        let letter = letter.to_ascii_lowercase();
        let starts = |choice: &&Choice| choice.label.to_ascii_lowercase().starts_with(letter);
        self.choices()
            .iter()
            .find(starts)
            .map(|choice| choice.answer)
    }

    pub fn focus_choice(&mut self, forward: bool) {
        let count = self.choices().len();
        let step = if forward { 1 } else { count - 1 };
        self.choice = (self.choice + step) % count;
    }
}

/// A command of a menu and the keys that choose it while the menu is open
/// (`l`, `up`...), with or without Ctrl.
pub struct MenuItem {
    pub keys: &'static [&'static str],
    pub label: &'static str,
    pub command: &'static str,
    /// Runs again on Ctrl and its key right after the menu closed.
    pub repeats: bool,
}

/// A group of commands behind one key: a popup of keys and commands.
pub struct MenuSpec {
    pub title: &'static str,
    /// The command that opens the menu.
    pub command: &'static str,
    /// The command of the menu that its own key runs: Ctrl+T Ctrl+T.
    pub again: Option<&'static str>,
    pub items: &'static [MenuItem],
}

impl MenuSpec {
    /// The item `key` chooses: a key name, Ctrl ignored.
    pub fn item_for(&self, key: &str) -> Option<&'static MenuItem> {
        let key = key.strip_prefix("ctrl+").unwrap_or(key);
        self.items.iter().find(|item| item.keys.contains(&key))
    }
}

pub struct Menu {
    pub spec: &'static MenuSpec,
    pub opened: Instant,
}

/// The menu whose repeating commands run again on Ctrl and their key, for
/// a moment after it closed.
pub struct MenuRepeat {
    pub spec: &'static MenuSpec,
    pub at: Instant,
}

pub enum Overlay {
    Picker(Box<Picker>),
    Prompt(Prompt),
    Menu(Menu),
}

/// A control of the find bar: a text field, a checkbox or a button.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum FindField {
    #[default]
    Query,
    Replacement,
    Case,
    Regex,
    Replace,
    ReplaceOne,
    ReplaceAll,
}

impl FindField {
    pub fn is_text(self) -> bool {
        matches!(self, FindField::Query | FindField::Replacement)
    }

    pub fn label(self) -> &'static str {
        match self {
            FindField::Query | FindField::Replacement => "",
            FindField::Case => "Aa",
            FindField::Regex => ".*",
            FindField::Replace => "replace",
            FindField::ReplaceOne => "Replace",
            FindField::ReplaceAll => "Replace all",
        }
    }
}

#[derive(Default)]
pub struct FindState {
    pub input: LineInput,
    pub replace: LineInput,
    pub replace_open: bool,
    /// The control of the bar that has the focus.
    pub field: FindField,
    pub regex: bool,
    pub case_sensitive: bool,
    /// Char ranges of matches in the active document.
    pub matches: Vec<(usize, usize)>,
    pub current: Option<usize>,
    pub key: Option<(usize, u64, String, bool, bool)>,
    pub error: bool,
}

impl FindState {
    /// The controls on each row of the bar, left to right.
    pub fn rows(&self) -> Vec<Vec<FindField>> {
        use FindField::*;
        let mut rows = vec![vec![Query, Case, Regex, Replace]];
        if self.replace_open {
            rows.push(vec![Replacement, ReplaceOne, ReplaceAll]);
        }
        rows
    }

    /// The order Tab follows: the text fields, then the rest.
    fn tab_order(&self) -> Vec<FindField> {
        let fields: Vec<FindField> = self.rows().concat();
        let (text, rest): (Vec<_>, Vec<_>) = fields.into_iter().partition(|field| field.is_text());
        [text, rest].concat()
    }

    pub fn focus_next(&mut self, forward: bool) {
        let order = self.tab_order();
        let at = order.iter().position(|field| *field == self.field);
        let step = if forward { 1 } else { order.len() - 1 };
        self.field = order[(at.unwrap_or(0) + step) % order.len()];
    }

    /// Moves the focus to the control left or right of the focused one.
    /// Returns false at the end of the row.
    pub fn focus_beside(&mut self, right: bool) -> bool {
        let rows = self.rows();
        let Some(row) = rows.iter().find(|row| row.contains(&self.field)) else {
            return false;
        };
        let at = row.iter().position(|field| *field == self.field);
        let beside = match (at, right) {
            (Some(at), true) => row.get(at + 1),
            (Some(at), false) => at.checked_sub(1).and_then(|at| row.get(at)),
            (None, _) => None,
        };
        if let Some(field) = beside {
            self.field = *field;
        }
        beside.is_some()
    }

    /// Shows or hides the replace row; showing it focuses its text field.
    pub fn set_replace_open(&mut self, open: bool) {
        self.replace_open = open;
        let on_replace_row = !self.rows().concat().contains(&self.field);
        if open {
            self.field = FindField::Replacement;
        } else if on_replace_row {
            self.field = FindField::Replace;
        }
    }

    pub fn focused_input(&mut self) -> Option<&mut LineInput> {
        match self.field {
            FindField::Query => Some(&mut self.input),
            FindField::Replacement => Some(&mut self.replace),
            _ => None,
        }
    }
}

pub struct Message {
    pub text: String,
    pub error: bool,
    pub at: Instant,
    /// Stays in the status line, cut short, when it does not fit there.
    quiet: bool,
    /// Whether it was decided where it shows: status line or dialog.
    placed: bool,
}

/// A message in a box over the screen until it is closed: one that does
/// not fit the status line, or one a plugin shows.
pub struct Dialog {
    pub title: String,
    pub text: String,
    pub error: bool,
    /// The first line of the text on screen.
    pub scroll: usize,
}

pub struct Preview {
    pub path: PathBuf,
    pub lines: Vec<String>,
    pub styles: Vec<Vec<Option<Style>>>,
}

#[derive(Default, Clone)]
pub struct Layout {
    pub sidebar: Option<Rect>,
    /// The text of the focused editor pane.
    pub editor: Rect,
    /// Everything the editor panes share.
    pub editor_area: Rect,
    /// The row of tabs above the panes.
    pub tabs: Option<Rect>,
    /// The panes on screen; in a split each has a title row with its file.
    pub panes: Vec<PaneArea>,
    /// The columns between side by side panes.
    pub separators: Vec<Rect>,
    pub find: Option<Rect>,
    pub status: Rect,
}

#[derive(Clone)]
pub struct Location {
    pub path: PathBuf,
    pub pos: usize,
}

/// A file that was on screen, with the cursor and scroll it had.
pub struct Recent {
    pub path: PathBuf,
    cursor: usize,
    view: ViewState,
}

pub struct App {
    pub root: PathBuf,
    pub options: Options,
    /// The documents on screen: one per file the panes show.
    pub docs: Vec<Document>,
    /// The document of the focused pane.
    pub active: usize,
    next_doc_id: usize,
    /// The files shown lately, the latest first.
    pub recent: Vec<Recent>,
    /// The files in the tab bar, in the order they were opened.
    pub tabs: Vec<PathBuf>,
    /// Set while an action runs again after its unsaved changes were given up.
    leave_confirmed: bool,
    pub focus: Focus,
    pub tree: FileTree,
    pub tree_visible: bool,
    pub overlay: Option<Overlay>,
    /// Drawn over everything else; gets the keys until it is closed.
    pub dialog: Option<Dialog>,
    pub last_picker: Option<Box<Picker>>,
    /// The glob filter typed last in the file finder or live grep.
    pub picker_glob: String,
    pub find: FindState,
    pub find_open: bool,
    pub panes: Panes,
    pub switcher: Option<Switcher>,
    pub menu_repeat: Option<MenuRepeat>,
    /// When Esc was last pressed, to spot a double Esc.
    pub last_esc: Option<Instant>,
    pub files: Vec<String>,
    pub git: GitStatus,
    head: Option<String>,
    pub message: Option<Message>,
    pub keymap: Keymap,
    pub plugins: Option<PluginHost>,
    /// Commands registered by plugins: id and title.
    pub plugin_commands: Vec<(String, String)>,
    pub clipboard: Clipboard,
    pub bookmarks: Bookmarks,
    jumps: Vec<Location>,
    jump_index: usize,
    pub preview: Option<Preview>,
    pub terminal_focused: bool,
    pub layout: Layout,
    pub quit: bool,
    pub paths: config::Paths,
    pub theme: crate::ui::theme::Theme,
    /// The `yate.json` files in use, lowest priority first.
    pub settings_files: Vec<PathBuf>,
    /// Background jobs (file index, git status, grep, definition search) not
    /// reported back yet.
    pub pending_jobs: usize,
    /// A newer release found by the update check.
    pub update_available: Option<String>,
    tx: Sender<AppEvent>,
    grep_generation: Arc<AtomicU64>,
    definition_generation: u64,
    pending_terminal_writes: Vec<Vec<u8>>,
    git_pending: bool,
    last_git: Instant,
    last_disk_check: Instant,
    pub(crate) last_click: Option<(Instant, u16, u16)>,
    pub dragging: bool,
    pub(crate) resizing_tree: bool,
    command_depth: usize,
}

impl App {
    fn new(root: PathBuf, tx: Sender<AppEvent>, paths: config::Paths, clipboard: Clipboard) -> App {
        let bookmarks = Bookmarks::load(Some(bookmarks::file_for(&paths.state_dir, &root)));
        App {
            paths,
            theme: crate::ui::theme::Theme::default(),
            settings_files: Vec::new(),
            pending_jobs: 0,
            update_available: None,
            tree: FileTree::new(&root),
            root,
            options: Options::default(),
            docs: Vec::new(),
            active: 0,
            next_doc_id: 1,
            recent: Vec::new(),
            tabs: Vec::new(),
            leave_confirmed: false,
            focus: Focus::Editor,
            tree_visible: false,
            overlay: None,
            dialog: None,
            last_picker: None,
            picker_glob: String::new(),
            find: FindState::default(),
            find_open: false,
            panes: Panes::default(),
            switcher: None,
            menu_repeat: None,
            last_esc: None,
            files: Vec::new(),
            git: GitStatus::default(),
            head: None,
            message: None,
            keymap: Keymap::defaults(),
            plugins: None,
            plugin_commands: Vec::new(),
            clipboard,
            bookmarks,
            jumps: Vec::new(),
            jump_index: 0,
            preview: None,
            terminal_focused: true,
            layout: Layout::default(),
            quit: false,
            tx,
            grep_generation: Arc::new(AtomicU64::new(0)),
            definition_generation: 0,
            pending_terminal_writes: Vec::new(),
            git_pending: false,
            last_git: Instant::now(),
            last_disk_check: Instant::now(),
            last_click: None,
            dragging: false,
            resizing_tree: false,
            command_depth: 0,
        }
    }

    /// The key binding context of the focused panel.
    pub fn context(&self) -> Context {
        match self.focus {
            Focus::Editor | Focus::Find => Context::Editor,
            Focus::Tree => Context::Tree,
        }
    }

    /// Sets the width of the file tree, leaving room for the editor.
    pub fn set_tree_width(&mut self, width: u16) {
        let screen_width = self.layout.status.w;
        self.options.tree_width = crate::ui::sidebar::clamp_width(width, screen_width);
    }

    pub fn doc(&self) -> &Document {
        &self.docs[self.active]
    }

    pub fn doc_mut(&mut self) -> &mut Document {
        &mut self.docs[self.active]
    }

    /// Shows a message in the status line, or in a dialog when it does
    /// not fit there.
    pub fn notify(&mut self, text: impl Into<String>) {
        self.set_message(text.into(), false, false);
    }

    /// A message nobody asked for (background work): it never opens a
    /// dialog, the status line cuts it short.
    pub fn notify_quietly(&mut self, text: impl Into<String>) {
        self.set_message(text.into(), false, true);
    }

    pub fn error(&mut self, text: impl Into<String>) {
        let text = text.into();
        crate::log::write_to(&self.paths.state_dir, &text);
        self.set_message(text, true, false);
    }

    fn set_message(&mut self, text: String, error: bool, quiet: bool) {
        self.message = Some(Message {
            text: text.trim_end().to_string(),
            error,
            at: Instant::now(),
            quiet,
            placed: false,
        });
    }

    pub fn show_dialog(&mut self, title: impl Into<String>, text: impl Into<String>, error: bool) {
        self.dialog = Some(Dialog {
            title: title.into(),
            text: text.into().trim_end().to_string(),
            error,
            scroll: 0,
        });
    }

    /// Moves a new message that the status line cannot show whole (too
    /// long, several lines) into a dialog.
    fn place_message(&mut self) {
        let room = crate::ui::status_message_room(self);
        let Some(message) = &mut self.message else {
            return;
        };
        if std::mem::replace(&mut message.placed, true) || message.quiet {
            return;
        }
        let one_line = !message.text.contains('\n');
        if one_line && crate::text::str_width(&message.text) < room {
            return;
        }
        if let Some(message) = self.message.take() {
            let title = if message.error { "Error" } else { "Message" };
            self.show_dialog(title, message.text, message.error);
        }
    }

    /// A path for display: relative to the project, `~/` in the home folder.
    pub fn relative(&self, path: &Path) -> String {
        crate::settings::display_path(&self.paths, &self.root, path)
    }

    pub fn resolve(&self, path: &str) -> PathBuf {
        let path = Path::new(path);
        let joined = if path.is_absolute() {
            path.to_path_buf()
        } else {
            self.root.join(path)
        };
        normalize(&joined)
    }

    /// Shows `path` in the focused pane, in place of the file shown there.
    pub fn open(&mut self, path: &Path, line: Option<usize>) -> bool {
        let path = normalize(path);
        if path.is_dir() {
            self.error(format!("{} is a folder", self.relative(&path)));
            return false;
        }
        let shown = self.doc().path.as_deref() == Some(&path);
        if !shown {
            let target = path.clone();
            let again = move |app: &mut App| {
                app.open(&target, line);
            };
            if !self.may_leave(again) {
                return false;
            }
        }
        self.push_jump();
        if !shown && !self.show_in_focused_pane(&path) {
            return false;
        }
        if let Some(line) = line {
            let doc = self.doc_mut();
            let pos = doc.pos(line, 0);
            let indent = crate::text::leading_whitespace(&doc.line(line))
                .chars()
                .count();
            doc.selection = Selection::point(pos + indent);
            doc.view.preferred_col = None;
        }
        self.touch_recent();
        if self.focus == Focus::Tree && self.layout.status.w <= self.options.tree_auto_close_width {
            self.tree_visible = false;
        }
        self.focus = Focus::Editor;
        self.find.key = None;
        self.relayout();
        self.scroll_to_cursor();
        self.follow_in_tree();
        true
    }

    /// Like `open`, with the cursor at `col` of the line.
    pub fn open_at(&mut self, path: &Path, line: Option<usize>, col: usize) -> bool {
        let target = normalize(path);
        if self.doc().path.as_deref() != Some(&target) {
            let again = move |app: &mut App| {
                app.open_at(&target, line, col);
            };
            if !self.may_leave(again) {
                return false;
            }
        }
        if !self.open(path, line) {
            return false;
        }
        if let Some(line) = line {
            let doc = self.doc_mut();
            let pos = doc.pos(line, col);
            doc.selection = Selection::point(pos);
        }
        self.scroll_to_cursor();
        true
    }

    /// Puts the document of `path` in the focused pane: the one another
    /// pane shows, or the file read from disk.
    fn show_in_focused_pane(&mut self, path: &Path) -> bool {
        if let Some(index) = self
            .docs
            .iter()
            .position(|d| d.path.as_deref() == Some(path))
        {
            self.active = index;
            self.drop_hidden_docs();
            return true;
        }
        let id = self.next_doc_id;
        self.next_doc_id += 1;
        let mut doc = match Document::open(id, path) {
            Ok(doc) => doc,
            Err(err) => {
                self.error(format!("{}: {err}", self.relative(path)));
                return false;
            }
        };
        if let Some(root) = &self.git.root {
            doc.git.base = git::head_content(root, path);
        }
        if let Some(recent) = self.recent.iter().find(|recent| recent.path == path) {
            doc.selection = Selection::point(recent.cursor.min(doc.rope.len_chars()));
            doc.view = recent.view.clone();
        }
        let readonly = doc.readonly;
        self.docs.push(doc);
        self.active = self.docs.len() - 1;
        self.drop_hidden_docs();
        if readonly {
            self.notify("Opened read-only (not valid UTF-8 or no write permission)");
        }
        self.emit_event("open", Some(path.to_string_lossy().into_owned()));
        true
    }

    /// Forgets the documents no pane shows; the recent files keep their
    /// cursor and scroll.
    pub(crate) fn drop_hidden_docs(&mut self) {
        let focused = self.doc().id;
        let others: Vec<usize> = self.panes.others().iter().map(|p| p.doc_id).collect();
        let (shown, hidden): (Vec<Document>, Vec<Document>) = std::mem::take(&mut self.docs)
            .into_iter()
            .partition(|doc| doc.id == focused || others.contains(&doc.id));
        self.docs = shown;
        self.active = self
            .docs
            .iter()
            .position(|doc| doc.id == focused)
            .unwrap_or(0);
        for doc in hidden {
            if let Some(recent) = self
                .recent
                .iter_mut()
                .find(|recent| Some(&recent.path) == doc.path.as_ref())
            {
                recent.cursor = doc.selection.head;
                recent.view = doc.view;
            }
        }
    }

    /// Moves the file of the focused pane to the front of the recent files
    /// and gives it a tab.
    pub(crate) fn touch_recent(&mut self) {
        let Some(path) = self.doc().path.clone() else {
            return;
        };
        let previous = self.recent.first().map(|recent| recent.path.clone());
        self.add_tab(previous.as_deref());
        let recent = match self.recent.iter().position(|recent| recent.path == path) {
            Some(index) => self.recent.remove(index),
            None => Recent {
                path,
                cursor: 0,
                view: ViewState::default(),
            },
        };
        self.recent.insert(0, recent);
        self.recent.truncate(MAX_RECENT);
    }

    /// True when the file of the focused pane may leave the screen. With
    /// unsaved changes it asks first, and runs `then` once they are saved or
    /// discarded.
    pub fn may_leave(&mut self, then: impl FnOnce(&mut App) + 'static) -> bool {
        self.in_another_pane(self.active) || self.may_drop(self.active, then)
    }

    pub(crate) fn may_drop(&mut self, index: usize, then: impl FnOnce(&mut App) + 'static) -> bool {
        let doc = &self.docs[index];
        let unsaved = doc.is_modified() && (doc.path.is_some() || doc.rope.len_chars() > 0);
        if self.leave_confirmed || !unsaved {
            return true;
        }
        let doc_id = doc.id;
        let name = match &doc.path {
            Some(path) => self.relative(path),
            None => doc.name(),
        };
        let kind = PromptKind::LeaveUnsaved {
            doc_id,
            then: Box::new(then),
        };
        let title = format!("{name} has unsaved changes");
        self.overlay = Some(Overlay::Prompt(Prompt::confirm(title, kind)));
        false
    }

    pub fn follow_in_tree(&mut self) {
        if self.tree_visible
            && self.options.tree_follow
            && !self.tree.locked
            && let Some(path) = self.doc().path.clone()
            && path.starts_with(&self.root)
        {
            self.tree.reveal(&path, &self.git);
            self.tree.ensure_visible(self.tree_list_height());
        }
    }

    /// The rows of the file tree that fit below its title and filters.
    pub fn tree_list_height(&self) -> usize {
        let list = self.layout.sidebar.map(crate::ui::sidebar::list_area);
        list.map_or(20, |list| list.h as usize)
    }

    /// Recomputes the layout for the current screen size, e.g. after the
    /// focused pane changed.
    pub fn relayout(&mut self) {
        let status = self.layout.status;
        if status.w > 0 {
            self.layout = crate::ui::layout(self, status.w, status.y + 1);
        }
    }

    pub fn editor_height(&self) -> usize {
        (self.layout.editor.h as usize).max(1)
    }

    pub fn text_width(&self) -> usize {
        (self.layout.editor.w as usize)
            .saturating_sub(gutter_width(self.doc(), &self.options) as usize)
            .max(1)
    }

    /// Scrolls so the cursor is visible, keeping `scroll_off` lines around it.
    pub fn scroll_to_cursor(&mut self) {
        let height = self.editor_height();
        let width = self.text_width();
        let scroll_off = self.options.scroll_off.min(height.saturating_sub(1) / 2);
        let tab_size = self.options.tab_size;
        let doc = self.doc_mut();
        let (line, col) = doc.line_col(doc.selection.head);
        let view = &mut doc.view;
        if line < view.scroll_line + scroll_off {
            view.scroll_line = line.saturating_sub(scroll_off);
        } else if line + scroll_off >= view.scroll_line + height {
            view.scroll_line = line + scroll_off + 1 - height;
        }
        let max_scroll = doc.line_count().saturating_sub(height);
        doc.view.scroll_line = doc.view.scroll_line.min(max_scroll);
        let display = crate::text::char_to_display_col(&doc.line(line), col, tab_size);
        let margin = 4.min(width / 4);
        let view = &mut doc.view;
        if display < view.scroll_col + margin {
            view.scroll_col = display.saturating_sub(margin);
        } else if display + margin >= view.scroll_col + width {
            view.scroll_col = display + margin + 1 - width;
        }
    }

    pub fn push_jump(&mut self) {
        if self.docs.is_empty() {
            return;
        }
        let Some(path) = self.doc().path.clone() else {
            return;
        };
        let location = Location {
            path,
            pos: self.doc().selection.head,
        };
        self.jumps.truncate(self.jump_index);
        let duplicate = self
            .jumps
            .last()
            .is_some_and(|last| last.path == location.path && last.pos.abs_diff(location.pos) < 2);
        if !duplicate {
            self.jumps.push(location);
        }
        if self.jumps.len() > 100 {
            self.jumps.remove(0);
        }
        self.jump_index = self.jumps.len();
    }

    pub fn jump(&mut self, back: bool) {
        let before = (self.jumps.clone(), self.jump_index);
        if back {
            if self.jump_index == 0 {
                return;
            }
            if self.jump_index == self.jumps.len() {
                self.push_jump();
                self.jump_index = self.jumps.len() - 1;
            }
            self.jump_index -= 1;
        } else {
            if self.jump_index + 1 >= self.jumps.len() {
                return;
            }
            self.jump_index += 1;
        }
        let location = self.jumps[self.jump_index].clone();
        let leaves = self.doc().path.as_ref() != Some(&location.path);
        if leaves && !self.may_leave(move |app| app.jump(back)) {
            (self.jumps, self.jump_index) = before;
            return;
        }
        let index = self.jump_index;
        let saved = std::mem::take(&mut self.jumps);
        self.open(&location.path, None);
        self.jumps = saved;
        self.jump_index = index;
        let doc = self.doc_mut();
        doc.selection = Selection::point(location.pos.min(doc.rope.len_chars()));
        self.scroll_to_cursor();
    }

    pub fn refresh_git(&mut self) {
        if self.git_pending {
            return;
        }
        self.git_pending = true;
        self.pending_jobs += 1;
        self.last_git = Instant::now();
        let root = self.root.clone();
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            let status = git::status(&root);
            let head = status.root.as_ref().and_then(|r| {
                std::process::Command::new("git")
                    .args(["rev-parse", "HEAD"])
                    .current_dir(r)
                    .stderr(std::process::Stdio::null())
                    .output()
                    .ok()
                    .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
            });
            let _ = tx.send(AppEvent::Git(status, head));
        });
    }

    pub fn refresh_files(&mut self) {
        self.pending_jobs += 1;
        let root = self.root.clone();
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            let _ = tx.send(AppEvent::FileIndex(files::index(&root)));
        });
    }

    pub fn check_for_update(&mut self) {
        self.pending_jobs += 1;
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            let _ = tx.send(AppEvent::UpdateCheck(crate::update::newer_version()));
        });
    }

    pub(crate) fn handle_event(&mut self, event: AppEvent) {
        match event {
            AppEvent::Terminal(event) => self.handle_terminal(event),
            AppEvent::FileIndex(files) => {
                self.pending_jobs = self.pending_jobs.saturating_sub(1);
                self.files = files;
                let items = commands::file_items(self);
                if let Some(Overlay::Picker(picker)) = &mut self.overlay
                    && picker.kind == crate::picker::PickerKind::Files
                {
                    picker.set_items(items);
                }
            }
            AppEvent::Git(status, head) => {
                self.pending_jobs = self.pending_jobs.saturating_sub(1);
                self.git_pending = false;
                let head_changed = head != self.head;
                self.head = head;
                self.git = status;
                if head_changed && let Some(root) = self.git.root.clone() {
                    for doc in &mut self.docs {
                        if let Some(path) = &doc.path {
                            doc.git.base = git::head_content(&root, path);
                            doc.git.signs_version = None;
                        }
                    }
                }
                if self.tree_visible {
                    self.tree.rebuild(&self.git);
                }
            }
            AppEvent::Grep(event) => commands::handle_grep(self, event),
            AppEvent::Definitions(result) => goto::handle_definitions(self, result),
            AppEvent::UpdateCheck(latest) => {
                self.pending_jobs = self.pending_jobs.saturating_sub(1);
                if let Some(latest) = latest {
                    self.notify_quietly(crate::update::available_message(&latest));
                    self.update_available = Some(latest);
                }
            }
        }
    }

    fn handle_terminal(&mut self, event: Event) {
        match event {
            Event::Key(key) if key.kind != crossterm::event::KeyEventKind::Release => {
                self.handle_key(key)
            }
            Event::Mouse(mouse) => self.handle_mouse(mouse),
            Event::Paste(text) => self.handle_paste(&text),
            Event::FocusGained => {
                self.terminal_focused = true;
                self.check_disk();
                self.refresh_git();
                self.refresh_files();
            }
            Event::FocusLost => self.terminal_focused = false,
            _ => {}
        }
    }

    fn tick(&mut self) {
        if self
            .message
            .as_ref()
            .is_some_and(|m| m.at.elapsed() > MESSAGE_TIME)
        {
            self.message = None;
        }
        if self.last_disk_check.elapsed() > DISK_CHECK {
            self.check_disk();
        }
        if self.last_git.elapsed() > GIT_REFRESH {
            self.refresh_git();
        }
    }

    /// Reloads files changed on disk (`autoread`), or warns when they have
    /// unsaved changes.
    pub fn check_disk(&mut self) {
        self.last_disk_check = Instant::now();
        let mut messages = Vec::new();
        for doc in &mut self.docs {
            if !doc.changed_on_disk() {
                continue;
            }
            let name = doc.name();
            if doc.is_modified() {
                doc.disk_mtime = doc
                    .path
                    .as_ref()
                    .and_then(|p| std::fs::metadata(p).ok())
                    .and_then(|m| m.modified().ok());
                messages.push((
                    format!("{name} changed on disk; you have unsaved changes"),
                    true,
                ));
            } else if doc.path.as_ref().is_some_and(|p| p.exists()) {
                match doc.reload() {
                    Ok(()) => messages.push((format!("Reloaded {name}"), false)),
                    Err(err) => messages.push((format!("{name}: {err}"), true)),
                }
            }
        }
        for (text, error) in messages {
            if error {
                self.error(text);
            } else {
                self.notify_quietly(text);
            }
        }
    }

    /// Work done before each frame: syntax trees, git signs, find matches.
    fn prepare_frame(&mut self) {
        if self.focus != Focus::Tree {
            self.tree.filter_focus = None;
        }
        if self.layout.tabs.is_some() == self.tabs.is_empty() {
            self.relayout();
        }
        for index in 0..self.docs.len() {
            self.refresh_doc(index);
        }
        if self.find_open || !self.find.input.text.is_empty() {
            commands::update_find_matches(self);
        }
        self.sync_pane_scroll();
        commands::update_preview(self);
    }

    fn refresh_doc(&mut self, index: usize) {
        let doc = &mut self.docs[index];
        doc.refresh_syntax();
        let throttled = doc.rope.len_bytes() > SIGN_THROTTLE_BYTES
            && doc
                .git
                .signs_at
                .is_some_and(|at| at.elapsed() < SIGN_THROTTLE);
        if doc.git.signs_version != Some(doc.version) && !throttled {
            doc.git.signs_version = Some(doc.version);
            doc.git.signs_at = Some(Instant::now());
            doc.git.signs = match &doc.git.base {
                Some(base) if doc.rope.len_bytes() < MAX_SIGN_BYTES => {
                    diff::line_signs(base, &doc.rope.to_string())
                }
                _ if doc
                    .path
                    .as_ref()
                    .is_some_and(|p| self.git.get(p) == Some(git::FileStatus::Untracked)) =>
                {
                    vec![Some(Sign::Added); doc.line_count() + 1]
                }
                _ => Vec::new(),
            };
        }
    }

    pub(crate) fn take_terminal_writes(&mut self) -> Vec<Vec<u8>> {
        std::mem::take(&mut self.pending_terminal_writes)
    }

    pub fn write_to_terminal(&mut self, bytes: Vec<u8>) {
        self.pending_terminal_writes.push(bytes);
    }

    pub fn snapshot(&self) -> Snapshot {
        let doc = self.doc();
        Snapshot {
            root: self.root.to_string_lossy().into_owned(),
            path: doc.path.as_ref().map(|p| p.to_string_lossy().into_owned()),
            language: doc.language.map(|l| l.name.to_string()),
            rope: doc.rope.clone(),
            anchor: doc.selection.anchor,
            head: doc.selection.head,
        }
    }

    /// Runs `call` against the plugin host with a fresh snapshot, then
    /// applies the actions it queued.
    pub fn with_plugins(
        &mut self,
        call: impl FnOnce(&mut PluginHost) -> Result<(), String>,
    ) -> Result<(), String> {
        let snapshot = self.snapshot();
        let Some(host) = &mut self.plugins else {
            return Err("plugins are not available".to_string());
        };
        host.bridge.borrow_mut().snapshot = snapshot;
        let result = call(host);
        let actions = host.take_actions();
        self.apply_plugin_actions(actions);
        let errors = self.sync_plugin_registrations();
        match (result, errors.is_empty()) {
            (result, true) => result,
            (Ok(()), false) => Err(errors.join("; ")),
            (Err(err), false) => Err(format!("{err}; {}", errors.join("; "))),
        }
    }

    pub fn emit_event(&mut self, event: &str, arg: Option<String>) {
        if self.plugins.is_none() {
            return;
        }
        let name = event.to_string();
        if let Err(err) = self.with_plugins(|host| host.emit(&name, arg)) {
            self.error(format!("plugin {event} listener: {err}"));
        }
    }

    fn apply_plugin_actions(&mut self, actions: Vec<Action>) {
        for action in actions {
            match action {
                Action::Notify(text, error) => {
                    if error {
                        self.error(text);
                    } else {
                        self.notify(text);
                    }
                }
                Action::Show(title, text) => self.show_dialog(title, text, false),
                Action::Insert(text) => {
                    let head = self.doc().selection.head;
                    self.doc_mut().selection = Selection::point(head);
                    crate::editor::ops::insert_text(
                        self.doc_mut(),
                        &text,
                        crate::editor::history::EditKind::Other,
                    );
                }
                Action::ReplaceSelection(text) => crate::editor::ops::insert_text(
                    self.doc_mut(),
                    &text,
                    crate::editor::history::EditKind::Other,
                ),
                Action::SetText(text) => self.doc_mut().set_text(&text),
                Action::SetCursor(line, col) => {
                    let doc = self.doc_mut();
                    let pos = doc.pos(line, col);
                    doc.selection = Selection::point(pos);
                }
                Action::Select(start_line, start_col, end_line, end_col) => {
                    let doc = self.doc_mut();
                    let anchor = doc.pos(start_line, start_col);
                    let head = doc.pos(end_line, end_col);
                    doc.selection = Selection::new(anchor, head);
                }
                Action::OpenFile(path, line) => {
                    let path = self.resolve(&path);
                    self.open(&path, line);
                }
                Action::RunCommand(id) => self.run_command(&id),
                Action::Prompt { label, initial, id } => {
                    let input = LineInput::with_text(&initial);
                    let prompt = Prompt::text(label, input, PromptKind::Plugin(id));
                    self.overlay = Some(Overlay::Prompt(prompt));
                }
            }
        }
        if !self.docs.is_empty() {
            self.scroll_to_cursor();
        }
    }

    /// Applies commands, key bindings and options plugins registered;
    /// returns the bindings and options that were refused.
    fn sync_plugin_registrations(&mut self) -> Vec<String> {
        let Some(host) = &self.plugins else {
            return Vec::new();
        };
        let (commands, bindings, options) = {
            let mut bridge = host.bridge.borrow_mut();
            (
                std::mem::take(&mut bridge.commands),
                std::mem::take(&mut bridge.bindings),
                std::mem::take(&mut bridge.options),
            )
        };
        for (id, title) in commands {
            self.plugin_commands.retain(|(existing, _)| *existing != id);
            self.plugin_commands.push((id, title));
        }
        let mut errors = Vec::new();
        for (key, command, context) in bindings {
            let result = match crate::keymap::Context::parse(&context) {
                Some(context) => self.keymap.bind(context, &key, &command),
                None => Err(format!("unknown key context {context}")),
            };
            if let Err(err) = result {
                errors.push(err);
            }
        }
        for (name, value) in options {
            if let Err(err) = self.options.set(&name, &value) {
                errors.push(err);
            }
        }
        errors
    }

    /// Loads the configuration: `yate.json` files (theme, options, keys,
    /// plugins), then the JavaScript plugins they switch on, which can
    /// override them.
    /// Returns false when something was wrong (reported in the status
    /// line or a dialog, and the log).
    pub fn load_config(&mut self) -> bool {
        self.plugin_commands.clear();
        self.keymap = Keymap::defaults();
        self.options = Options::default();
        let mut problems = Vec::new();
        let mut settings_errors = Vec::new();
        let plugin_switches = self.apply_settings(&mut settings_errors);
        problems.append(&mut settings_errors);
        match PluginHost::new() {
            Ok(host) => {
                self.plugins = Some(host);
                let config_dir = self.paths.config_dir.clone();
                let mut errors = Vec::new();
                let result = self.with_plugins(|host| {
                    errors = host.load_all(&config_dir, &plugin_switches);
                    Ok(())
                });
                if let Err(err) = result {
                    errors.push(err);
                }
                problems.extend(errors.iter().map(|error| format!("plugin {error}")));
            }
            Err(err) => problems.push(format!("plugins disabled: {err}")),
        }
        if problems.is_empty() {
            return true;
        }
        self.error(problems.join("\n"));
        false
    }
}

impl App {
    /// Applies the merged `yate.json` files and returns the plugin switches.
    fn apply_settings(&mut self, problems: &mut Vec<String>) -> Vec<(String, bool)> {
        let settings = crate::settings::load(&self.paths, &self.root);
        let mut errors = Vec::new();
        let (theme, theme_errors) =
            crate::ui::theme::Theme::from_json(settings.merged.get("theme"));
        self.theme = theme;
        errors.extend(theme_errors);
        for (name, value) in settings.options(&mut errors) {
            if let Err(err) = self.options.set(&name, &value) {
                errors.push(format!("options.{name}: {err}"));
            }
        }
        for (context, key, command) in settings.keys(&mut errors) {
            if let Err(err) = self.keymap.bind(context, &key, &command) {
                errors.push(format!("keys: {err}"));
            }
        }
        let plugins = settings.plugins(&mut errors);
        errors.extend(settings.unknown_sections());
        self.settings_files = settings.files;
        problems.extend(settings.errors);
        problems.extend(
            errors
                .into_iter()
                .map(|error| format!("yate.json: {error}")),
        );
        plugins
    }
}

/// Built-in command ids and titles.
pub fn builtin_commands() -> Vec<(&'static str, &'static str)> {
    commands::COMMANDS.iter().map(|c| (c.id, c.title)).collect()
}

pub fn gutter_width(doc: &Document, options: &Options) -> u16 {
    if !options.line_numbers {
        return 2;
    }
    let digits = doc.line_count().max(99).to_string().len() as u16;
    digits + 3
}

/// Removes `.` and `..` without touching the file system.
pub fn normalize(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => {
                out.pop();
            }
            other => out.push(other),
        }
    }
    out
}

/// Parses `path`, `path:line` and `+line path` style arguments.
pub fn parse_target(arg: &str) -> (String, Option<usize>) {
    if let Some((path, line)) = arg.rsplit_once(':')
        && let Ok(line) = line.parse::<usize>()
        && !path.is_empty()
    {
        return (path.to_string(), Some(line.saturating_sub(1)));
    }
    (arg.to_string(), None)
}

/// Everything `App::start` needs; the terminal and the tests fill it.
pub struct Start {
    pub root: PathBuf,
    pub file: Option<PathBuf>,
    pub line: Option<usize>,
    pub paths: config::Paths,
    pub clipboard: Clipboard,
    pub size: (u16, u16),
}

impl App {
    /// Creates the app for a project, loads plugins and opens the file (or
    /// the file tree when there is no file).
    pub fn start(start: Start, tx: Sender<AppEvent>) -> App {
        let mut app = App::new(start.root.clone(), tx, start.paths, start.clipboard);
        app.layout = crate::ui::layout(&app, start.size.0, start.size.1);
        app.git = git::status(&start.root);
        app.docs.push(Document::scratch(0));
        app.load_config();
        if let Some(file) = &start.file {
            app.open(file, start.line);
            app.jumps.clear();
            app.jump_index = 0;
        } else {
            app.tree_visible = true;
            app.focus = Focus::Tree;
            app.tree.rebuild(&app.git);
        }
        app.refresh_files();
        app.refresh_git();
        app
    }

    /// Lays out and draws a frame: the cells and the terminal cursor.
    pub fn draw(&mut self, width: u16, height: u16) -> (Surface, Option<(u16, u16)>) {
        let previous_editor = self.layout.editor;
        self.layout = crate::ui::layout(self, width, height);
        if self.layout.editor != previous_editor {
            self.scroll_to_cursor();
        }
        self.place_message();
        self.prepare_frame();
        let mut surface = Surface::new(width, height);
        let cursor = crate::ui::render(self, &mut surface);
        (surface, cursor)
    }
}

/// Runs yate on a file or folder; without a path, on the current folder.
pub fn run(path: Option<String>) -> Result<(), String> {
    let cwd = std::env::current_dir().map_err(|err| err.to_string())?;
    let (root, file, line) = open_target(&cwd, path);

    let (tx, rx) = mpsc::channel();
    let size = crossterm::terminal::size().unwrap_or((80, 24));
    let mut app = App::start(
        Start {
            root: root.clone(),
            file,
            line,
            paths: config::Paths::from_env(),
            clipboard: Clipboard::default(),
            size,
        },
        tx.clone(),
    );

    if app.options.update_check && !crate::update::check_disabled_by_env() {
        app.check_for_update();
    }

    let mut terminal = Terminal::enter().map_err(|err| err.to_string())?;
    terminal.set_title(&format!(
        "yate: {}",
        root.file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default()
    ));
    spawn_input(tx);
    let result = event_loop(&mut app, &mut terminal, &rx);
    drop(terminal);
    if let Some(latest) = &app.update_available {
        eprintln!("{}", crate::update::available_message(latest));
    }
    result
}

fn open_target(cwd: &Path, path: Option<String>) -> (PathBuf, Option<PathBuf>, Option<usize>) {
    let (target, line) = path
        .map(|p| parse_target(&p))
        .unwrap_or((".".to_string(), None));
    let target = normalize(&cwd.join(&target));
    if target.is_dir() {
        return (target, None, line);
    }
    let dir = target
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or(cwd.to_path_buf());
    let root = git::repository_root(&dir).unwrap_or(dir);
    (root, Some(target), line)
}

fn spawn_input(tx: Sender<AppEvent>) {
    std::thread::spawn(move || {
        while let Ok(event) = crossterm::event::read() {
            if tx.send(AppEvent::Terminal(event)).is_err() {
                break;
            }
        }
    });
}

fn event_loop(
    app: &mut App,
    terminal: &mut Terminal,
    rx: &Receiver<AppEvent>,
) -> Result<(), String> {
    loop {
        let (width, height) = terminal.size();
        let (surface, cursor) = app.draw(width, height);
        terminal
            .flush(&surface, cursor)
            .map_err(|err| err.to_string())?;
        for bytes in app.take_terminal_writes() {
            terminal.write_raw(&bytes);
        }
        match rx.recv_timeout(app.wake_in(TICK)) {
            Ok(event) => {
                if matches!(event, AppEvent::Terminal(Event::Resize(..))) {
                    terminal.invalidate();
                }
                app.handle_event(event);
            }
            Err(RecvTimeoutError::Timeout) => app.tick(),
            Err(RecvTimeoutError::Disconnected) => return Ok(()),
        }
        let started = Instant::now();
        while let Ok(event) = rx.try_recv() {
            if matches!(event, AppEvent::Terminal(Event::Resize(..))) {
                terminal.invalidate();
            }
            app.handle_event(event);
            if started.elapsed() > Duration::from_millis(30) {
                break;
            }
        }
        if app.quit {
            return Ok(());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn targets_with_line_numbers() {
        assert_eq!(
            parse_target("src/a.rs:12"),
            ("src/a.rs".to_string(), Some(11))
        );
        assert_eq!(parse_target("src/a.rs"), ("src/a.rs".to_string(), None));
        assert_eq!(normalize(Path::new("/a/./b/../c")), PathBuf::from("/a/c"));
    }
}
