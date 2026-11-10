//! Built-in commands. Every key binding and the command palette run
//! commands by id; plugin commands share the same namespace.

use super::{App, Focus, Overlay, Preview, Prompt, PromptKind, Review};
use crate::clipboard::CopyOutcome;
use crate::editor::history::EditKind;
use crate::editor::ops;
use crate::editor::selection::Selection;
use crate::picker::{Picker, PickerItem, PickerKind};
use crate::syntax::{self, languages};
use crate::ui::input::LineInput;
use crate::workspace::grep::{self, GrepEvent};
use crate::workspace::resolve;
use std::sync::atomic::Ordering;

pub struct Command {
    pub id: &'static str,
    pub title: &'static str,
    run: fn(&mut App),
}

macro_rules! commands {
    ($($id:literal, $title:literal => $run:expr;)*) => {
        pub const COMMANDS: &[Command] = &[$(Command { id: $id, title: $title, run: $run }),*];
    };
}

commands! {
    "app.quit", "Quit" => quit;
    "help.keys", "Help: key bindings" => open_keys_picker;
    "file.save", "Save file" => save;
    "file.new", "New file…" => |app| new_file_prompt(app, None);
    "file.reload", "Reload file from disk" => reload;
    "file.rename", "Rename file or folder…" => rename_prompt;
    "file.delete", "Delete file or folder…" => delete_prompt;
    "file.copyPath", "Copy relative path" => copy_path;
    "file.compare", "Compare: mark for compare, or compare with the marked file or folder" => compare_marked;
    "file.compareWith", "Compare current file with…" => open_compare_picker;
    "picker.files", "Find file" => |app| open_files_picker(app, None);
    "picker.grep", "Live grep" => |app| open_grep_picker(app, false, None);
    "picker.grepGlob", "Live grep in files matching a glob" => |app| open_grep_picker(app, true, None);
    "picker.filesInFolder", "Find file in the folder selected in the tree" => |app| {
        let folder = scope_folder(app);
        open_files_picker(app, folder);
    };
    "picker.grepInFolder", "Live grep in the folder selected in the tree" => |app| {
        let folder = scope_folder(app);
        open_grep_picker(app, false, folder);
    };
    "picker.buffers", "Open buffers" => open_buffers_picker;
    "picker.commands", "Command palette" => open_commands_picker;
    "picker.bookmarks", "Bookmarks" => open_bookmarks_picker;
    "picker.resume", "Resume last picker" => resume_picker;
    "tree.toggle", "File tree: toggle" => toggle_tree;
    "tree.reveal", "File tree: reveal current file" => reveal_in_tree;
    "tree.lock", "File tree: lock / unlock following the current file" => toggle_tree_lock;
    "tree.toggleIgnored", "File tree: show / hide git-ignored files" => |app| {
        app.tree.show_ignored = !app.tree.show_ignored;
        app.tree.rebuild(&app.git);
    };
    "tree.toggleChanged", "File tree: show only changed files" => |app| {
        app.tree.changed_only = !app.tree.changed_only;
        app.tree.rebuild(&app.git);
    };
    "tree.refresh", "File tree: refresh" => |app| {
        app.tree.rebuild(&app.git);
        app.refresh_git();
        app.refresh_files();
    };
    "git.review", "Git: review changes" => open_review;
    "review.nextChange", "Review: next change" => |app| review_step(app, true);
    "review.previousChange", "Review: previous change" => |app| review_step(app, false);
    "review.refresh", "Review: refresh" => refresh_review;
    "switch.next", "Switch: tree (1) or open file (2-9); again: next file" => |app| {
        if app.switcher.is_some() {
            app.cycle_buffer(1);
        } else {
            app.start_switcher();
        }
    };
    "switch.previous", "Switch: tree or open file; again: previous file" => |app| {
        if app.switcher.is_some() {
            app.cycle_buffer(-1);
        } else {
            app.start_switcher();
        }
    };
    "view.split", "Split editor / close split" => App::toggle_split;
    "view.focusOtherPane", "Focus the other pane of the split" => App::focus_other_pane;
    "view.toggleScrollSync", "Scroll both panes together (sync on / off)" => App::toggle_scroll_sync;
    "buffer.close", "Close buffer" => close_buffer;
    "buffer.next", "Next buffer" => |app| app.cycle_buffer(1);
    "buffer.previous", "Previous buffer" => |app| app.cycle_buffer(-1);
    "jump.back", "Jump back" => |app| app.jump(true);
    "jump.forward", "Jump forward" => |app| app.jump(false);
    "goto.line", "Go to line…" => goto_line_prompt;
    "goto.definition", "Go to definition or file under the cursor" => super::goto::goto_definition;
    "find.open", "Find in file" => |app| open_find(app, false);
    "find.replace", "Replace in file" => |app| open_find(app, true);
    "find.next", "Find next" => |app| find_step(app, true);
    "find.previous", "Find previous" => |app| find_step(app, false);
    "bookmark.toggle", "Bookmark: toggle line" => toggle_bookmark;
    "bookmark.next", "Bookmark: next" => |app| bookmark_step(app, true);
    "bookmark.previous", "Bookmark: previous" => |app| bookmark_step(app, false);
    "bookmark.clear", "Bookmark: clear in file" => clear_bookmarks;
    "edit.undo", "Undo" => |app| edit(app, |doc, _| { doc.undo(); });
    "edit.redo", "Redo" => |app| edit(app, |doc, _| { doc.redo(); });
    "edit.copy", "Copy" => |app| copy(app, false);
    "edit.cut", "Cut" => |app| copy(app, true);
    "edit.paste", "Paste" => paste;
    "edit.selectAll", "Select all" => |app| {
        let doc = app.doc_mut();
        doc.selection = Selection::new(0, doc.rope.len_chars());
    };
    "edit.selectNext", "Select word / next occurrence" => |app| edit(app, |doc, _| ops::select_next_occurrence(doc));
    "edit.toggleComment", "Toggle line comment" => |app| edit(app, |doc, _| ops::toggle_comment(doc));
    "edit.moveLineUp", "Move line up" => |app| edit(app, |doc, _| ops::move_lines(doc, false));
    "edit.moveLineDown", "Move line down" => |app| edit(app, |doc, _| ops::move_lines(doc, true));
    "edit.duplicateLine", "Duplicate line" => |app| edit(app, |doc, _| ops::duplicate_lines(doc));
    "edit.deleteLine", "Delete line" => |app| edit(app, |doc, _| ops::delete_lines(doc));
    "edit.indent", "Indent lines" => |app| edit(app, |doc, options| {
        let (first, last) = ops::selected_lines(doc);
        ops::indent_lines(doc, first, last, options);
    });
    "edit.outdent", "Outdent lines" => |app| edit(app, ops::outdent);
    "view.toggleIndentGuides", "Toggle indent guides" => |app| app.options.indent_guides = !app.options.indent_guides;
    "view.toggleLineNumbers", "Toggle line numbers" => |app| app.options.line_numbers = !app.options.line_numbers;
    "language.choose", "Set language of buffer…" => open_language_picker;
    "config.reload", "Reload yate.json and plugins" => |app| {
        if app.load_config() {
            app.notify("Reloaded settings and plugins");
        }
    };
    "settings.open", "Open yate.json (the closest one, or a new ~/yate.jsonc)" => open_settings;
    "app.log", "Open the yate log" => |app| {
        let path = app.paths.state_dir.join("yate.log");
        app.open(&path, None);
    };
}

impl App {
    pub fn run_command(&mut self, id: &str) {
        if self.command_depth > 8 {
            self.error(format!("{id}: commands nested too deeply"));
            return;
        }
        self.command_depth += 1;
        if let Some(command) = COMMANDS.iter().find(|c| c.id == id) {
            (command.run)(self);
        } else if self.plugin_commands.iter().any(|(cid, _)| cid == id) {
            let id_owned = id.to_string();
            if let Err(err) = self.with_plugins(|host| host.run_command(&id_owned)) {
                self.error(format!("{id}: {err}"));
            }
        } else {
            self.error(format!("Unknown command {id}"));
        }
        self.command_depth -= 1;
    }
}

/// Runs an editing operation on the active document and keeps the view,
/// the bookmarks and the find matches in sync.
pub fn edit(
    app: &mut App,
    op: impl FnOnce(&mut crate::editor::document::Document, &crate::config::Options),
) {
    let options = app.options.clone();
    let before_lines = app.doc().line_count();
    let before_line = app.doc().line_of(app.doc().selection.start());
    let before_version = app.doc().version;
    op(app.doc_mut(), &options);
    let doc = app.doc();
    if doc.version != before_version {
        let delta = doc.line_count() as isize - before_lines as isize;
        if delta != 0
            && let Some(path) = doc.path.clone()
        {
            app.bookmarks.shift(&path, before_line, delta);
        }
    }
    app.doc_mut().view.preferred_col = None;
    app.scroll_to_cursor();
}

fn open_settings(app: &mut App) {
    let path = app.settings_files.last().cloned().unwrap_or_else(|| {
        app.paths
            .home
            .clone()
            .unwrap_or_else(|| app.paths.cwd.clone())
            .join(crate::settings::NEW_FILE_NAME)
    });
    let exists = path.exists();
    if app.open(&path, None) && !exists && app.doc().rope.len_chars() == 0 {
        app.doc_mut().set_text(&crate::settings::template());
        let shown = crate::settings::display_path(&app.paths, &app.root, &path);
        app.notify(format!("New {shown}: save it to apply"));
    }
}

fn quit(app: &mut App) {
    let unsaved: Vec<String> = app
        .docs
        .iter()
        .filter(|d| d.is_modified() && d.path.is_some())
        .map(|d| d.name())
        .collect();
    if unsaved.is_empty() {
        app.quit = true;
        return;
    }
    app.overlay = Some(Overlay::Prompt(Prompt {
        title: format!("Unsaved: {}. Quit anyway? (y/n)", unsaved.join(", ")),
        input: LineInput::default(),
        kind: PromptKind::QuitUnsaved,
        confirm: true,
    }));
}

pub fn save(app: &mut App) {
    if app.doc().path.is_none() {
        let id = app.doc().id;
        app.overlay = Some(Overlay::Prompt(Prompt {
            title: "Save as".to_string(),
            input: LineInput::default(),
            kind: PromptKind::SaveAs(id),
            confirm: false,
        }));
        return;
    }
    let path = app
        .doc()
        .path
        .clone()
        .map(|p| p.to_string_lossy().into_owned());
    app.emit_event("beforeSave", path.clone());
    let name = app.relative(app.doc().path.as_ref().expect("checked above"));
    match app.doc_mut().save() {
        Ok(()) => {
            app.notify(format!("Saved {name}"));
            let saved = app.doc().path.clone().unwrap_or_default();
            let is_config = crate::settings::is_settings_file(&saved)
                || saved.starts_with(&app.paths.config_dir);
            if is_config && app.load_config() {
                app.notify(format!("Saved {name}, settings reloaded"));
            }
            app.emit_event("save", path);
            app.refresh_git();
            if app.tree_visible {
                app.tree.rebuild(&app.git);
            }
        }
        Err(err) => app.error(format!("Could not save {name}: {err}")),
    }
}

fn reload(app: &mut App) {
    let name = app.doc().name();
    match app.doc_mut().reload() {
        Ok(()) => app.notify(format!("Reloaded {name}")),
        Err(err) => app.error(format!("{name}: {err}")),
    }
    app.scroll_to_cursor();
}

pub fn new_file_prompt(app: &mut App, dir: Option<std::path::PathBuf>) {
    let dir = dir.unwrap_or_else(|| {
        if app.focus == Focus::Tree {
            return app.tree.target_dir();
        }
        app.doc()
            .path
            .as_ref()
            .and_then(|p| p.parent().map(|p| p.to_path_buf()))
            .unwrap_or_else(|| app.root.clone())
    });
    let mut initial = app.relative(&dir);
    if !initial.is_empty() && dir != app.root {
        initial.push('/');
    } else {
        initial.clear();
    }
    app.overlay = Some(Overlay::Prompt(Prompt {
        title: "New file (end with / for a folder)".to_string(),
        input: LineInput::with_text(&initial),
        kind: PromptKind::NewFile,
        confirm: false,
    }));
}

fn goto_line_prompt(app: &mut App) {
    app.overlay = Some(Overlay::Prompt(Prompt {
        title: format!("Go to line (1-{})", app.doc().line_count()),
        input: LineInput::default(),
        kind: PromptKind::GoToLine,
        confirm: false,
    }));
}

pub fn file_items(files: &[String]) -> Vec<PickerItem> {
    files
        .iter()
        .map(|file| PickerItem {
            label: file.clone(),
            ..PickerItem::default()
        })
        .collect()
}

fn set_picker(app: &mut App, picker: Picker) {
    app.overlay = Some(Overlay::Picker(Box::new(picker)));
}

/// The folder of the tree selection (a file's own folder), relative to the
/// root, or the folder of the current file when the tree is hidden. None is
/// the root.
fn scope_folder(app: &App) -> Option<String> {
    let folder = if app.tree_visible || app.focus == Focus::Tree {
        app.tree.selected_row().map(|row| {
            if row.is_dir {
                row.path.clone()
            } else {
                row.path.parent().unwrap_or(&app.root).to_path_buf()
            }
        })
    } else {
        app.doc()
            .path
            .as_ref()
            .and_then(|p| p.parent())
            .map(|p| p.to_path_buf())
    }?;
    let relative = folder
        .strip_prefix(&app.root)
        .ok()?
        .to_string_lossy()
        .into_owned();
    (!relative.is_empty()).then_some(relative)
}

/// A file finder or live grep picker, scoped to `folder` when given, with
/// the glob filter used last.
fn search_picker(app: &App, kind: PickerKind, title: &str, folder: Option<String>) -> Picker {
    let items = if kind == PickerKind::Files {
        file_items(&app.files)
    } else {
        Vec::new()
    };
    let title = match &folder {
        Some(folder) => format!("{title} in {folder}/"),
        None => title.to_string(),
    };
    let mut picker = Picker::new(kind, &title, items);
    picker.folder = folder;
    picker.glob.set(&app.picker_glob);
    picker.refilter();
    picker
}

fn open_files_picker(app: &mut App, folder: Option<String>) {
    let mut picker = search_picker(app, PickerKind::Files, "Find file", folder);
    picker.searching = app.files.is_empty();
    set_picker(app, picker);
    if app.files.is_empty() {
        app.refresh_files();
    }
}

pub fn open_files_picker_with(app: &mut App, query: &str) {
    open_files_picker(app, None);
    if let Some(Overlay::Picker(picker)) = &mut app.overlay {
        picker.input.set(query);
        picker.refilter();
    }
}

pub fn open_grep_picker_with(app: &mut App, query: &str) {
    let mut picker = search_picker(app, PickerKind::Grep, "Live grep", None);
    picker.input.set(query);
    set_picker(app, picker);
    start_grep(app);
}

fn open_grep_picker(app: &mut App, glob_first: bool, folder: Option<String>) {
    let mut picker = search_picker(app, PickerKind::Grep, "Live grep", folder);
    let selected = app.doc().selected_text();
    if !selected.is_empty() && !selected.contains('\n') {
        picker.input.prefill(&selected);
    }
    picker.glob_focused = glob_first;
    set_picker(app, picker);
    if !glob_first {
        start_grep(app);
    }
}

pub fn start_grep(app: &mut App) {
    let Some(Overlay::Picker(picker)) = &mut app.overlay else {
        return;
    };
    app.grep_generation.fetch_add(1, Ordering::SeqCst);
    picker.set_items(Vec::new());
    if picker.input.text.is_empty() {
        picker.searching = false;
        return;
    }
    picker.searching = true;
    app.pending_jobs += 1;
    let tx = app.tx.clone();
    let request = grep::GrepRequest {
        root: app.root.clone(),
        folder: match &picker.folder {
            Some(folder) => app.root.join(folder),
            None => app.root.clone(),
        },
        query: picker.input.text.clone(),
        globs: picker.glob.text.clone(),
    };
    grep::spawn(request, app.grep_generation.clone(), move |event| {
        let _ = tx.send(super::AppEvent::Grep(event));
    });
}

pub fn handle_grep(app: &mut App, event: GrepEvent) {
    if matches!(event, GrepEvent::Done(_)) {
        app.pending_jobs = app.pending_jobs.saturating_sub(1);
    }
    let current = app.grep_generation.load(Ordering::SeqCst);
    let Some(Overlay::Picker(picker)) = &mut app.overlay else {
        return;
    };
    if picker.kind != PickerKind::Grep {
        return;
    }
    match event {
        GrepEvent::Matches(generation, matches) if generation == current => {
            let items = matches
                .into_iter()
                .map(|m| PickerItem {
                    label: format!("{}:{}", m.relative, m.line + 1),
                    detail: m.text,
                    detail_match: Some(m.range),
                    path: Some(m.path),
                    line: Some(m.line),
                    col: m.col,
                    data: String::new(),
                })
                .collect();
            picker.append_items(items);
        }
        GrepEvent::Done(generation) if generation == current => picker.searching = false,
        _ => {}
    }
}

fn open_buffers_picker(app: &mut App) {
    let items = app
        .docs
        .iter()
        .enumerate()
        .map(|(index, doc)| PickerItem {
            label: doc
                .path
                .as_ref()
                .map(|p| app.relative(p))
                .unwrap_or_else(|| "untitled".to_string()),
            detail: if doc.is_modified() {
                "●".to_string()
            } else {
                String::new()
            },
            path: doc.path.clone(),
            data: index.to_string(),
            ..PickerItem::default()
        })
        .collect();
    set_picker(app, Picker::new(PickerKind::Buffers, "Open buffers", items));
}

fn open_commands_picker(app: &mut App) {
    let mut items: Vec<PickerItem> = COMMANDS
        .iter()
        .map(|c| (c.id.to_string(), c.title.to_string()))
        .chain(app.plugin_commands.iter().cloned())
        .map(|(id, title)| PickerItem {
            detail: app
                .keymap
                .keys_for(&id)
                .into_iter()
                .take(2)
                .collect::<Vec<_>>()
                .join("  "),
            label: title,
            data: id,
            ..PickerItem::default()
        })
        .collect();
    items.sort_by(|a, b| a.label.cmp(&b.label));
    set_picker(app, Picker::new(PickerKind::Commands, "Commands", items));
}

fn open_language_picker(app: &mut App) {
    let items = languages::all()
        .iter()
        .map(|language| PickerItem {
            label: language.name.to_string(),
            data: format!("language:{}", language.name),
            ..PickerItem::default()
        })
        .chain(std::iter::once(PickerItem {
            label: "plain text".to_string(),
            data: "language:".to_string(),
            ..PickerItem::default()
        }))
        .collect();
    set_picker(app, Picker::new(PickerKind::Commands, "Language", items));
}

fn open_bookmarks_picker(app: &mut App) {
    let items = app
        .bookmarks
        .items
        .iter()
        .map(|bookmark| {
            let text = std::fs::read_to_string(&bookmark.path)
                .ok()
                .and_then(|t| t.lines().nth(bookmark.line).map(|l| l.trim().to_string()))
                .unwrap_or_default();
            PickerItem {
                label: format!("{}:{}", app.relative(&bookmark.path), bookmark.line + 1),
                detail: text,
                path: Some(bookmark.path.clone()),
                line: Some(bookmark.line),
                ..PickerItem::default()
            }
        })
        .collect();
    set_picker(app, Picker::new(PickerKind::Bookmarks, "Bookmarks", items));
}

fn resume_picker(app: &mut App) {
    match app.last_picker.take() {
        Some(picker) => app.overlay = Some(Overlay::Picker(picker)),
        None => app.notify("No picker to resume"),
    }
}

/// Enter in a picker.
pub fn accept_picker(app: &mut App, picker: Box<Picker>) {
    if picker.kind == PickerKind::Files && picker.selected_item().is_none() {
        let home = app.paths.home.clone();
        let written = resolve::resolve(&picker.input.text, None, &app.root, home.as_deref(), false);
        app.last_picker = Some(picker);
        match written {
            Some(found) => open_from_picker(app, &found.path, found.line, found.col.unwrap_or(0)),
            None => app.notify("No such file"),
        }
        return;
    }
    let Some(item) = picker.selected_item().cloned() else {
        app.last_picker = Some(picker);
        return;
    };
    let (_, query_line, query_col) = resolve::split_location(picker.input.text.trim());
    let kind = picker.kind;
    if kind != PickerKind::Commands {
        app.last_picker = Some(picker);
    }
    match kind {
        PickerKind::Files => {
            let path = app.root.join(&item.label);
            open_from_picker(app, &path, query_line, query_col.unwrap_or(0));
        }
        PickerKind::CompareWith => {
            if let Some(current) = app.doc().path.clone() {
                start_compare(app, app.root.join(&item.label), current);
            }
        }
        PickerKind::Grep | PickerKind::Bookmarks | PickerKind::Definitions => {
            if let Some(path) = item.path {
                open_from_picker(app, &path, item.line, item.col);
            }
        }
        PickerKind::Buffers => {
            if let Ok(index) = item.data.parse::<usize>()
                && index < app.docs.len()
            {
                app.show_doc(index);
            }
        }
        PickerKind::Commands => {
            if item.data.is_empty() {
                return;
            }
            if let Some(name) = item.data.strip_prefix("language:") {
                let language = languages::by_name(name);
                app.doc_mut().set_language(language);
            } else {
                app.run_command(&item.data);
            }
        }
    }
}

fn open_from_picker(app: &mut App, path: &std::path::Path, line: Option<usize>, col: usize) {
    if !app.open(path, line) {
        return;
    }
    if let Some(line) = line {
        let doc = app.doc_mut();
        let pos = doc.pos(line, col);
        doc.selection = Selection::point(pos);
    }
    app.scroll_to_cursor();
    if app.tree_visible
        && app.layout.status.w <= app.options.tree_auto_close_width
        && app.focus == Focus::Tree
    {
        app.tree_visible = false;
    }
}

/// Keeps the preview of the selected picker item up to date.
pub fn update_preview(app: &mut App) {
    let Some(Overlay::Picker(picker)) = &app.overlay else {
        return;
    };
    let path = match picker.selected_item() {
        Some(item) if matches!(picker.kind, PickerKind::Files | PickerKind::CompareWith) => {
            Some(app.root.join(&item.label))
        }
        Some(item) => item.path.clone(),
        None => None,
    };
    let Some(path) = path else {
        app.preview = None;
        return;
    };
    if app.preview.as_ref().is_some_and(|p| p.path == path) {
        return;
    }
    let text = match std::fs::metadata(&path) {
        Ok(meta) if meta.len() <= 2 * 1024 * 1024 => std::fs::read(&path)
            .ok()
            .and_then(|bytes| String::from_utf8(bytes).ok())
            .unwrap_or_else(|| "(binary file)".to_string()),
        Ok(_) => "(file too large to preview)".to_string(),
        Err(_) => String::new(),
    };
    let text = text.replace("\r\n", "\n");
    let first_line = text.lines().next().unwrap_or("");
    let styles = languages::detect(&path, first_line)
        .filter(|_| text.len() < 512 * 1024)
        .map(|language| syntax::highlight_text(language, &text, &app.theme))
        .unwrap_or_default();
    app.preview = Some(Preview {
        path,
        lines: text.split('\n').map(str::to_string).collect(),
        styles,
    });
}

fn toggle_tree(app: &mut App) {
    if app.review.is_some() {
        app.review = None;
    }
    app.tree_visible = !app.tree_visible;
    if app.tree_visible {
        app.tree.rebuild(&app.git);
        app.follow_in_tree();
        app.focus = Focus::Tree;
    } else if app.focus == Focus::Tree {
        app.focus = Focus::Editor;
    }
}

fn reveal_in_tree(app: &mut App) {
    app.review = None;
    app.tree_visible = true;
    match app.doc().path.clone() {
        Some(path) => {
            app.tree.reveal(&path, &app.git);
            let height = app.layout.sidebar.map(|r| r.h as usize).unwrap_or(20);
            app.tree.ensure_visible(height.saturating_sub(1));
        }
        None => app.tree.rebuild(&app.git),
    }
    app.focus = Focus::Tree;
}

fn toggle_tree_lock(app: &mut App) {
    app.tree.locked = !app.tree.locked;
    if app.tree.locked {
        app.notify("File tree: following the current file is off (locked)");
    } else {
        app.notify("File tree: following the current file is on");
        app.follow_in_tree();
    }
}

fn open_review(app: &mut App) {
    if app.git.root.is_none() {
        app.error("Not a git repository");
        return;
    }
    let reviewing_git = app
        .review
        .as_ref()
        .is_some_and(|r| r.source == super::ReviewSource::Git);
    if reviewing_git && app.focus == Focus::Review {
        app.review = None;
        app.focus = Focus::Editor;
        return;
    }
    let mut review = Review::git(&app.git);
    if let Some(path) = app.doc().path.clone()
        && let Some(index) = review.files.iter().position(|(p, _)| *p == path)
    {
        review.selected = index;
    }
    app.review = Some(review);
    load_review_diff(app);
    app.focus = Focus::Review;
    app.refresh_git();
    if app.git.files.is_empty() {
        app.notify("No changes against HEAD");
    }
}

fn review_step(app: &mut App, forward: bool) {
    let height = app.layout.editor.h as usize;
    if let Some(review) = &mut app.review {
        review.next_change(forward);
        review.diff_focused = true;
        review.ensure_visible(height, 0);
        app.focus = Focus::Review;
    }
}

fn refresh_review(app: &mut App) {
    match app.review.as_ref().map(|r| r.source.clone()) {
        Some(super::ReviewSource::Git) => app.refresh_git(),
        Some(super::ReviewSource::Compare { .. }) => {
            if let Some(review) = &mut app.review {
                review.refresh_compare();
            }
            load_review_diff(app);
        }
        None => {}
    }
}

/// The file or folder a file command works on: the selected tree row while
/// the tree has focus, otherwise the current file.
fn target_path(app: &App) -> Option<std::path::PathBuf> {
    if app.focus == Focus::Tree {
        return app.tree.selected_path().map(|p| p.to_path_buf());
    }
    app.doc().path.clone()
}

fn rename_prompt(app: &mut App) {
    let Some(path) = target_path(app) else {
        app.notify("Nothing to rename: the buffer has no file");
        return;
    };
    let relative = app.relative(&path);
    app.overlay = Some(Overlay::Prompt(Prompt {
        title: "Rename to".to_string(),
        input: LineInput::with_text(&relative),
        kind: PromptKind::Rename(path),
        confirm: false,
    }));
}

fn delete_prompt(app: &mut App) {
    let Some(path) = target_path(app) else {
        app.notify("Nothing to delete: the buffer has no file");
        return;
    };
    app.overlay = Some(Overlay::Prompt(Prompt {
        title: format!("Delete {}? (y/n)", app.relative(&path)),
        input: LineInput::default(),
        kind: PromptKind::Delete(path),
        confirm: true,
    }));
}

fn copy_path(app: &mut App) {
    let Some(path) = target_path(app) else {
        return;
    };
    let relative = app.relative(&path);
    if let CopyOutcome::Osc52(bytes) = app.clipboard.copy(&relative, false) {
        app.write_to_terminal(bytes);
    }
    app.notify(format!("Copied {relative}"));
}

fn compare_marked(app: &mut App) {
    let Some(path) = target_path(app) else {
        app.notify("Nothing to compare: the buffer has no file");
        return;
    };
    let compare_key = app.keymap.hint("file.compare").unwrap_or_default();
    match app.compare_mark.take() {
        Some(marked) if marked == path => app.notify("Compare mark removed"),
        Some(marked) => start_compare(app, marked, path),
        None => {
            let shown = app.relative(&path);
            app.notify(format!(
                "Marked {shown}; {compare_key} on another file or folder compares them"
            ));
            app.compare_mark = Some(path);
        }
    }
}

fn open_compare_picker(app: &mut App) {
    if app.doc().path.is_none() {
        app.notify("Save the buffer before comparing it");
        return;
    }
    let items = file_items(&app.files);
    let picker = Picker::new(PickerKind::CompareWith, "Compare with", items);
    set_picker(app, picker);
    if app.files.is_empty() {
        app.refresh_files();
    }
}

/// Opens the review view on the differences between two files or two
/// folders; `left` is the old side.
pub fn start_compare(app: &mut App, left: std::path::PathBuf, right: std::path::PathBuf) {
    if left.is_dir() != right.is_dir() {
        app.error("Compare two files or two folders, not a file with a folder");
        return;
    }
    let folders = right.is_dir();
    let review = Review::compare(left.clone(), right.clone());
    app.message = None;
    if folders && review.files.is_empty() {
        let (left, right) = (app.relative(&left), app.relative(&right));
        app.notify(format!("{left} and {right} have the same files"));
    }
    app.split = None;
    app.review = Some(review);
    load_review_diff(app);
    if let Some(review) = &mut app.review {
        review.diff_focused = !folders;
    }
    app.focus = Focus::Review;
}

/// Help: every key binding, the bindings of the focused panel first, then
/// the keys that are not commands. Enter runs the selected command.
fn open_keys_picker(app: &mut App) {
    let context = match app.focus {
        Focus::Editor | Focus::Find => crate::keymap::Context::Editor,
        Focus::Tree => crate::keymap::Context::Tree,
        Focus::Review => crate::keymap::Context::Review,
    };
    let title_of = |id: &str| {
        COMMANDS
            .iter()
            .find(|c| c.id == id)
            .map(|c| c.title.to_string())
            .or_else(|| {
                app.plugin_commands
                    .iter()
                    .find(|(cid, _)| cid == id)
                    .map(|(_, title)| title.clone())
            })
            .unwrap_or_else(|| id.to_string())
    };
    let mut grouped: Vec<(crate::keymap::Context, String, Vec<String>)> = Vec::new();
    for (bound_context, key, command) in app.keymap.all() {
        match grouped
            .iter_mut()
            .find(|(c, id, _)| *c == bound_context && *id == command)
        {
            Some((_, _, keys)) => keys.push(key),
            None => grouped.push((bound_context, command, vec![key])),
        }
    }
    let rank = |c: crate::keymap::Context| {
        if c == context || c == crate::keymap::Context::Global {
            0
        } else {
            1
        }
    };
    let order = |id: &str| {
        COMMANDS
            .iter()
            .position(|c| c.id == id)
            .unwrap_or(COMMANDS.len())
    };
    grouped.sort_by_key(|(c, id, _)| (rank(*c), order(id)));
    let mut items: Vec<PickerItem> = grouped
        .into_iter()
        .map(|(bound_context, command, keys)| PickerItem {
            label: format!(
                "{:<20} {}",
                keys.into_iter().take(2).collect::<Vec<_>>().join(" "),
                title_of(&command)
            ),
            detail: bound_context.name().to_string(),
            data: command,
            ..PickerItem::default()
        })
        .collect();
    items.extend(BUILT_IN_KEYS.iter().map(|(keys, what, where_)| PickerItem {
        label: format!("{keys:<20} {what}"),
        detail: where_.to_string(),
        ..PickerItem::default()
    }));
    set_picker(
        app,
        Picker::new(PickerKind::Commands, "Key bindings (enter runs)", items),
    );
}

/// Keys handled by the panels themselves, not bound to commands.
const BUILT_IN_KEYS: &[(&str, &str, &str)] = &[
    ("alt+w 1", "Focus the file tree", "switcher"),
    ("alt+w 2-9", "Focus open file 1-8", "switcher"),
    ("alt+w alt+w", "Next open file", "switcher"),
    ("shift+arrows", "Select", "editor"),
    (
        "ctrl+left ctrl+right",
        "Move jumpChars characters (10)",
        "editor",
    ),
    ("ctrl+up ctrl+down", "Move jumpLines lines (5)", "editor"),
    ("ctrl+home ctrl+end", "Start / end of the file", "editor"),
    ("tab shift+tab", "Indent / outdent", "editor"),
    ("ctrl+click", "Go to definition", "editor"),
    (
        "ctrl+f tab",
        "Glob filter (*.rs, src/**, !*.test.ts)",
        "picker",
    ),
    ("enter down up", "Next / previous match", "find"),
    ("alt+c alt+r", "Case sensitive / regex", "find"),
    ("alt+a", "Replace all", "find"),
    ("enter space", "Open file / fold folder", "tree"),
    ("left right", "Fold / unfold", "tree"),
    ("esc tab", "Back to the editor", "tree"),
    ("up down", "Changed files / diff lines", "review"),
    ("tab", "Switch list / diff", "review"),
    ("enter", "Open the file at the line", "review"),
    ("esc", "Close", "review"),
];

pub fn load_review_diff(app: &mut App) {
    let git_root = app.git.root.clone();
    let Some(review) = &app.review else {
        return;
    };
    let current = review.selected_path().and_then(|path| {
        app.docs
            .iter()
            .find(|d| d.path.as_deref() == Some(path))
            .map(|d| d.text())
    });
    if let Some(review) = &mut app.review {
        review.load(git_root.as_deref(), current, &app.theme);
    }
}

pub fn close_buffer(app: &mut App) {
    let doc = app.doc();
    if doc.is_modified() && doc.path.is_some() {
        let id = doc.id;
        app.overlay = Some(Overlay::Prompt(Prompt {
            title: format!("{} has unsaved changes. Close anyway? (y/n)", doc.name()),
            input: LineInput::default(),
            kind: PromptKind::CloseUnsaved(id),
            confirm: true,
        }));
        return;
    }
    let id = doc.id;
    close_buffer_now(app, id);
}

pub fn close_buffer_now(app: &mut App, id: usize) {
    let Some(index) = app.docs.iter().position(|d| d.id == id) else {
        return;
    };
    let remaining_pane = app.close_pane_of(id);
    app.docs.remove(index);
    if app.docs.is_empty() {
        app.docs.push(crate::editor::document::Document::scratch(0));
    }
    if app.active >= index && app.active > 0 {
        app.active -= 1;
    }
    app.active = app.active.min(app.docs.len() - 1);
    if let Some(remaining) = remaining_pane.and_then(|id| app.docs.iter().position(|d| d.id == id))
    {
        app.active = remaining;
    }
    app.follow_in_tree();
}

fn open_find(app: &mut App, replace: bool) {
    if app.focus == Focus::Find && !replace && !app.find.input.text.is_empty() {
        find_step(app, true);
        return;
    }
    app.find_open = true;
    app.find.replace_open = replace || (app.find.replace_open && app.focus == Focus::Find);
    app.find.replace_focused = false;
    let selected = app.doc().selected_text();
    if !selected.is_empty() && !selected.contains('\n') {
        app.find.input.prefill(&selected);
    } else {
        app.find.input.selected = !app.find.input.text.is_empty();
    }
    app.focus = Focus::Find;
    app.find.key = None;
}

pub fn update_find_matches(app: &mut App) {
    let doc = &app.docs[app.active];
    let find = &mut app.find;
    let key = (
        doc.id,
        doc.version,
        find.input.text.clone(),
        find.regex,
        find.case_sensitive,
    );
    if find.key.as_ref() == Some(&key) {
        return;
    }
    find.key = Some(key);
    find.matches.clear();
    find.current = None;
    find.error = false;
    if find.input.text.is_empty() {
        return;
    }
    let pattern = if find.regex {
        find.input.text.clone()
    } else {
        regex::escape(&find.input.text)
    };
    let case_insensitive = !find.case_sensitive && !find.input.text.chars().any(char::is_uppercase);
    let Ok(regex) = regex::RegexBuilder::new(&pattern)
        .case_insensitive(case_insensitive)
        .multi_line(true)
        .build()
    else {
        find.error = true;
        return;
    };
    let text = doc.text();
    for found in regex.find_iter(&text).take(50_000) {
        if found.start() == found.end() {
            continue;
        }
        let start = doc.rope.byte_to_char(found.start());
        let end = doc.rope.byte_to_char(found.end());
        find.matches.push((start, end));
    }
    let head = doc.selection.start();
    find.current = find
        .matches
        .iter()
        .position(|(start, _)| *start >= head)
        .or(if find.matches.is_empty() {
            None
        } else {
            Some(0)
        });
}

pub fn find_step(app: &mut App, forward: bool) {
    update_find_matches(app);
    if app.find.matches.is_empty() {
        if !app.find.input.text.is_empty() {
            app.notify("No matches");
        }
        return;
    }
    let sel = app.doc().selection;
    let matches = &app.find.matches;
    let index = if forward {
        matches
            .iter()
            .position(|(start, _)| {
                *start > sel.start() || (*start == sel.start() && sel.is_empty())
            })
            .unwrap_or(0)
    } else {
        matches
            .iter()
            .rposition(|(start, _)| *start < sel.start())
            .unwrap_or(matches.len() - 1)
    };
    let (start, end) = matches[index];
    app.find.current = Some(index);
    app.doc_mut().selection = Selection::new(start, end);
    app.doc_mut().history.seal();
    app.scroll_to_cursor();
}

pub fn replace_current(app: &mut App) {
    update_find_matches(app);
    let sel = app.doc().selection;
    let replacement = app.find.replace.text.clone();
    if app
        .find
        .matches
        .iter()
        .any(|(start, end)| *start == sel.start() && *end == sel.end())
    {
        edit(app, |doc, _| {
            ops::insert_text(doc, &replacement, EditKind::Other)
        });
    }
    find_step(app, true);
}

pub fn replace_all(app: &mut App) {
    update_find_matches(app);
    let matches = app.find.matches.clone();
    if matches.is_empty() {
        app.notify("No matches");
        return;
    }
    let replacement = app.find.replace.text.clone();
    let count = matches.len();
    edit(app, |doc, _| {
        let changes = matches
            .iter()
            .rev()
            .map(|(start, end)| {
                crate::editor::document::Change::new(*start, *end, replacement.clone())
            })
            .collect();
        let head = doc.selection.head;
        doc.apply(changes, Selection::point(head), EditKind::Other);
    });
    app.notify(format!("Replaced {count} matches"));
}

fn toggle_bookmark(app: &mut App) {
    let Some(path) = app.doc().path.clone() else {
        app.error("Save the buffer before bookmarking it");
        return;
    };
    let line = app.doc().cursor_line();
    if app.bookmarks.toggle(&path, line) {
        app.notify(format!("Bookmarked line {}", line + 1));
    } else {
        app.notify(format!("Removed bookmark at line {}", line + 1));
    }
}

fn bookmark_step(app: &mut App, forward: bool) {
    let path = app.doc().path.clone().unwrap_or_default();
    let line = app.doc().cursor_line();
    match app.bookmarks.next(&path, line, forward).cloned() {
        Some(bookmark) => {
            app.open(&bookmark.path, Some(bookmark.line));
        }
        None => app.notify("No bookmarks"),
    }
}

fn clear_bookmarks(app: &mut App) {
    if let Some(path) = app.doc().path.clone() {
        for line in app.bookmarks.lines_in(&path) {
            app.bookmarks.toggle(&path, line);
        }
        app.notify("Cleared bookmarks in this file");
    }
}

fn copy(app: &mut App, cut: bool) {
    let doc = app.doc();
    let (text, linewise) = if doc.selection.is_empty() {
        let line = doc.cursor_line();
        (format!("{}\n", doc.line(line)), true)
    } else {
        (doc.selected_text(), false)
    };
    if let CopyOutcome::Osc52(bytes) = app.clipboard.copy(&text, linewise) {
        app.write_to_terminal(bytes);
    }
    if cut {
        if linewise {
            edit(app, |doc, _| ops::delete_lines(doc));
        } else {
            edit(app, |doc, _| ops::delete_selection(doc));
        }
    }
}

fn paste(app: &mut App) {
    let (text, linewise) = app.clipboard.paste();
    if text.is_empty() {
        return;
    }
    let text = text.replace("\r\n", "\n");
    edit(app, |doc, _| {
        if linewise && doc.selection.is_empty() {
            let line = doc.cursor_line();
            let at = doc.line_start(line);
            let len = text.chars().count();
            let head = doc.selection.head + len;
            doc.apply(
                vec![crate::editor::document::Change::insert(at, text.clone())],
                Selection::point(head),
                EditKind::Other,
            );
        } else {
            ops::insert_text(doc, &text, EditKind::Other);
        }
    });
}

/// Answers a prompt (Enter, or y/n for confirmations).
pub fn answer_prompt(app: &mut App, prompt: Prompt, answer: Option<String>) {
    match prompt.kind {
        PromptKind::Plugin(id) => {
            if let Err(err) = app.with_plugins(|host| host.answer_prompt(id, answer)) {
                app.error(err);
            }
        }
        PromptKind::GoToLine => {
            if let Some(line) = answer.and_then(|a| a.trim().parse::<usize>().ok()) {
                app.push_jump();
                let doc = app.doc_mut();
                let line = line
                    .saturating_sub(1)
                    .min(doc.line_count().saturating_sub(1));
                let indent = crate::text::leading_whitespace(&doc.line(line))
                    .chars()
                    .count();
                doc.selection = Selection::point(doc.line_start(line) + indent);
                app.scroll_to_cursor();
            }
        }
        PromptKind::NewFile => {
            let Some(answer) = answer.filter(|a| !a.trim().is_empty()) else {
                return;
            };
            let path = app.resolve(answer.trim());
            if answer.ends_with('/') {
                match std::fs::create_dir_all(&path) {
                    Ok(()) => app.notify(format!("Created {}", app.relative(&path))),
                    Err(err) => app.error(err.to_string()),
                }
            } else if path.exists() {
                app.open(&path, None);
            } else {
                let created = path
                    .parent()
                    .map(std::fs::create_dir_all)
                    .unwrap_or(Ok(()))
                    .and_then(|_| std::fs::write(&path, ""));
                match created {
                    Ok(()) => {
                        app.open(&path, None);
                        app.refresh_files();
                    }
                    Err(err) => app.error(err.to_string()),
                }
            }
            app.tree.rebuild(&app.git);
            app.follow_in_tree();
        }
        PromptKind::SaveAs(id) => {
            let Some(answer) = answer.filter(|a| !a.trim().is_empty()) else {
                return;
            };
            let path = app.resolve(answer.trim());
            if path.exists() {
                app.error(format!("{} already exists", app.relative(&path)));
                return;
            }
            let Some(index) = app.docs.iter().position(|d| d.id == id) else {
                return;
            };
            app.active = index;
            app.doc_mut().path = Some(path);
            app.doc_mut().detect_language();
            save(app);
            app.refresh_files();
        }
        PromptKind::Rename(from) => {
            let Some(answer) = answer.filter(|a| !a.trim().is_empty()) else {
                return;
            };
            let to = app.resolve(answer.trim());
            let result = to
                .parent()
                .map(std::fs::create_dir_all)
                .unwrap_or(Ok(()))
                .and_then(|_| std::fs::rename(&from, &to));
            match result {
                Ok(()) => {
                    for doc in &mut app.docs {
                        if let Some(path) = &doc.path
                            && let Ok(rest) = path.strip_prefix(&from)
                        {
                            doc.path = Some(if rest.as_os_str().is_empty() {
                                to.clone()
                            } else {
                                to.join(rest)
                            });
                        }
                    }
                    app.notify(format!("Renamed to {}", app.relative(&to)));
                    app.tree.rebuild(&app.git);
                    app.tree.select_path(&to);
                    app.refresh_files();
                    app.refresh_git();
                }
                Err(err) => app.error(err.to_string()),
            }
        }
        PromptKind::Delete(path) => {
            if answer.as_deref() != Some("y") {
                return;
            }
            let result = if path.is_dir() {
                std::fs::remove_dir_all(&path)
            } else {
                std::fs::remove_file(&path)
            };
            match result {
                Ok(()) => {
                    let ids: Vec<usize> = app
                        .docs
                        .iter()
                        .filter(|d| d.path.as_ref().is_some_and(|p| p.starts_with(&path)))
                        .map(|d| d.id)
                        .collect();
                    for id in ids {
                        close_buffer_now(app, id);
                    }
                    app.notify(format!("Deleted {}", app.relative(&path)));
                    app.tree.rebuild(&app.git);
                    app.refresh_files();
                    app.refresh_git();
                }
                Err(err) => app.error(err.to_string()),
            }
        }
        PromptKind::QuitUnsaved => {
            if answer.as_deref() == Some("y") {
                app.quit = true;
            }
        }
        PromptKind::CloseUnsaved(id) => {
            if answer.as_deref() == Some("y") {
                close_buffer_now(app, id);
            }
        }
    }
}
