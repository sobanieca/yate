//! Built-in commands. Every key binding and the command palette run
//! commands by id; plugin commands share the same namespace.

use super::{
    App, Direction, FindField, Focus, Menu, MenuItem, MenuSpec, Overlay, Preview, Prompt,
    PromptKind, Side,
};
use crate::clipboard::CopyOutcome;
use crate::editor::history::EditKind;
use crate::editor::ops;
use crate::editor::selection::Selection;
use crate::keymap::Context;
use crate::picker::{Picker, PickerItem, PickerKind};
use crate::syntax::{self, languages};
use crate::ui::input::LineInput;
use crate::workspace::grep::{self, GrepEvent};
use crate::workspace::resolve;
use std::collections::HashSet;
use std::sync::atomic::Ordering;
use std::time::Instant;

/// Where a command makes sense: the command palette lists the commands of
/// the focused panel only.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Scope {
    Anywhere,
    Editor,
    Tree,
    /// The current file in the editor, or the selected row of the file tree.
    File,
}

impl Scope {
    pub fn includes(self, context: Context) -> bool {
        match self {
            Scope::Anywhere => true,
            Scope::Editor => context == Context::Editor,
            Scope::Tree => context == Context::Tree,
            Scope::File => matches!(context, Context::Editor | Context::Tree),
        }
    }
}

/// The sections of the command palette.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Group {
    File,
    Edit,
    Search,
    GoTo,
    Bookmarks,
    View,
    Tree,
    Plugins,
    App,
}

impl Group {
    pub fn name(self) -> &'static str {
        match self {
            Group::File => "File",
            Group::Edit => "Edit",
            Group::Search => "Search",
            Group::GoTo => "Go to",
            Group::Bookmarks => "Bookmarks",
            Group::View => "View",
            Group::Tree => "File tree",
            Group::Plugins => "Plugins",
            Group::App => "yate",
        }
    }

    /// The sections of the palette for a panel, the ones about the panel
    /// first.
    fn palette_order(context: Context) -> &'static [Group] {
        match context {
            Context::Tree => &[Group::File, Group::Tree, Group::Search, Group::App],
            _ => &[
                Group::File,
                Group::Edit,
                Group::Search,
                Group::GoTo,
                Group::Bookmarks,
                Group::View,
                Group::Tree,
                Group::Plugins,
                Group::App,
            ],
        }
    }
}

pub struct Command {
    pub id: &'static str,
    pub title: &'static str,
    pub group: Group,
    pub scope: Scope,
    run: fn(&mut App),
}

macro_rules! commands {
    ($($id:literal, $title:literal, $group:ident, $scope:ident => $run:expr;)*) => {
        pub const COMMANDS: &[Command] = &[$(Command {
            id: $id,
            title: $title,
            group: Group::$group,
            scope: Scope::$scope,
            run: $run,
        }),*];
    };
}

const TREE_RESIZE_STEP: i32 = 4;

const fn once(
    keys: &'static [&'static str],
    label: &'static str,
    command: &'static str,
) -> MenuItem {
    MenuItem {
        keys,
        label,
        command,
        repeats: false,
    }
}

const fn repeating(
    keys: &'static [&'static str],
    label: &'static str,
    command: &'static str,
) -> MenuItem {
    MenuItem {
        keys,
        label,
        command,
        repeats: true,
    }
}

const GO_TO_MENU: MenuSpec = MenuSpec {
    title: "Go to",
    command: "goto.menu",
    again: None,
    items: &[
        once(&["l"], "Line…", "goto.line"),
        once(
            &["d"],
            "Definition or file under the cursor",
            "goto.definition",
        ),
        repeating(&["left"], "Back", "jump.back"),
        repeating(&["right"], "Forward", "jump.forward"),
    ],
};

const BOOKMARK_MENU: MenuSpec = MenuSpec {
    title: "Bookmarks",
    command: "bookmark.menu",
    again: None,
    items: &[
        once(&["t"], "Toggle on this line", "bookmark.toggle"),
        repeating(&["down"], "Next", "bookmark.next"),
        repeating(&["up"], "Previous", "bookmark.previous"),
        once(&["l"], "List", "picker.bookmarks"),
        once(&["c"], "Clear in this file", "bookmark.clear"),
    ],
};

const PANE_MENU: MenuSpec = MenuSpec {
    title: "Panes and tabs",
    command: "view.paneMenu",
    again: Some("view.focusNextPane"),
    items: &[
        once(&["v"], "Split: side by side", "view.splitVertical"),
        once(&["s"], "Split: one above the other", "view.splitHorizontal"),
        once(&["h", "left"], "Focus left", "view.focusPaneLeft"),
        once(&["j", "down"], "Focus below", "view.focusPaneBelow"),
        once(&["k", "up"], "Focus above", "view.focusPaneAbove"),
        once(&["l", "right"], "Focus right", "view.focusPaneRight"),
        once(&["w"], "Focus the next pane", "view.focusNextPane"),
        once(&["r"], "Resize this pane…", "view.resizePane"),
        once(&["="], "Equal sizes", "view.equalPanes"),
        once(&["c"], "Close this pane", "view.closePane"),
        once(&["o"], "Close the other panes", "view.closeOtherPanes"),
        once(&["x"], "Close this tab", "tab.close"),
        once(&["t"], "Close the tabs not on screen", "tab.closeOthers"),
        once(
            &["y"],
            "Scroll together (on / off)",
            "view.toggleScrollSync",
        ),
    ],
};

const TREE_MENU: MenuSpec = MenuSpec {
    title: "File tree",
    command: "tree.menu",
    again: Some("tree.toggle"),
    items: &[
        once(&["t"], "Toggle", "tree.toggle"),
        once(&["r"], "Reveal current file", "tree.reveal"),
        once(&["f"], "Focus", "tree.focus"),
        once(&["l"], "Lock / unlock following the file", "tree.lock"),
    ],
};

const LINE_MENU: MenuSpec = MenuSpec {
    title: "Line",
    command: "edit.lineMenu",
    again: None,
    items: &[
        repeating(&["up"], "Move up", "edit.moveLineUp"),
        repeating(&["down"], "Move down", "edit.moveLineDown"),
        repeating(&["left"], "Outdent", "edit.outdent"),
        repeating(&["right"], "Indent", "edit.indent"),
        once(&["d"], "Duplicate", "edit.duplicateLine"),
        once(&["k"], "Delete", "edit.deleteLine"),
    ],
};

/// Every menu, for the keys the palette shows next to a command.
pub const MENUS: &[&MenuSpec] = &[
    &GO_TO_MENU,
    &BOOKMARK_MENU,
    &PANE_MENU,
    &TREE_MENU,
    &LINE_MENU,
];

/// A key of a menu as shown on screen: the arrow keys as arrows.
pub fn menu_key_label(key: &str) -> &str {
    match key {
        "up" => "↑",
        "down" => "↓",
        "left" => "←",
        "right" => "→",
        other => other,
    }
}

/// The keys that run command `id` while the panel of `context` has the
/// focus: its own keys, then the key of a menu with its key in the menu.
pub fn key_hints(app: &App, context: Context, id: &str) -> Vec<String> {
    let mut keys = app.keymap.keys_in(context, id);
    let listed = |menu: &&&MenuSpec| menu.command != id;
    for menu in MENUS.iter().filter(listed) {
        let item_keys = menu.items.iter().filter(|item| item.command == id);
        for item_key in item_keys.filter_map(|item| item.keys.first()) {
            for menu_key in app.keymap.keys_in(context, menu.command) {
                keys.push(format!("{menu_key} {}", menu_key_label(item_key)));
            }
        }
    }
    keys
}

commands! {
    "file.save", "Save", File, Editor => save;
    "file.new", "New file…", File, Anywhere => |app| new_file_prompt(app, None);
    "file.rename", "Rename…", File, File => rename_prompt;
    "file.delete", "Delete…", File, File => delete_prompt;
    "file.copyPath", "Copy relative path", File, File => copy_path;
    "file.reload", "Reload from disk", File, Editor => reload;
    "edit.undo", "Undo", Edit, Editor => |app| edit(app, |doc, _| { doc.undo(); });
    "edit.redo", "Redo", Edit, Editor => |app| edit(app, |doc, _| { doc.redo(); });
    "edit.copy", "Copy", Edit, Editor => |app| copy(app, false);
    "edit.cut", "Cut", Edit, Editor => |app| copy(app, true);
    "edit.paste", "Paste", Edit, Editor => paste;
    "edit.selectAll", "Select all", Edit, Editor => |app| {
        let doc = app.doc_mut();
        doc.selection = Selection::new(0, doc.rope.len_chars());
    };
    "edit.selectNext", "Select word / next occurrence", Edit, Editor => |app| edit(app, |doc, _| ops::select_next_occurrence(doc));
    "edit.toggleComment", "Toggle line comment", Edit, Editor => |app| edit(app, |doc, _| ops::toggle_comment(doc));
    "edit.moveLineUp", "Move line up", Edit, Editor => |app| edit(app, |doc, _| ops::move_lines(doc, false));
    "edit.moveLineDown", "Move line down", Edit, Editor => |app| edit(app, |doc, _| ops::move_lines(doc, true));
    "edit.duplicateLine", "Duplicate line", Edit, Editor => |app| edit(app, |doc, _| ops::duplicate_lines(doc));
    "edit.deleteLine", "Delete line", Edit, Editor => |app| edit(app, |doc, _| ops::delete_lines(doc));
    "edit.indent", "Indent lines", Edit, Editor => |app| edit(app, |doc, options| {
        let (first, last) = ops::selected_lines(doc);
        ops::indent_lines(doc, first, last, options);
    });
    "edit.outdent", "Outdent lines", Edit, Editor => |app| edit(app, ops::outdent);
    "edit.lineMenu", "Move, indent, duplicate or delete lines…", Edit, Editor => |app| open_menu(app, &LINE_MENU);
    "edit.insertEmoji", "Insert emoji…", Edit, Editor => open_emoji_picker;
    "find.open", "Find in file", Search, Editor => |app| open_find(app, false);
    "find.replace", "Replace in file", Search, Editor => |app| open_find(app, true);
    "find.next", "Find next", Search, Editor => |app| find_step(app, true);
    "find.previous", "Find previous", Search, Editor => |app| find_step(app, false);
    "picker.files", "Find file", Search, Anywhere => |app| open_files_picker(app, None);
    "picker.filesInFolder", "Find file in the selected folder", Search, Tree => |app| {
        let folder = scope_folder(app);
        open_files_picker(app, folder);
    };
    "picker.grep", "Live grep", Search, Anywhere => |app| open_grep_picker(app, false, None);
    "picker.grepInFolder", "Live grep in the selected folder", Search, Tree => |app| {
        let folder = scope_folder(app);
        open_grep_picker(app, false, folder);
    };
    "picker.grepGlob", "Live grep in files matching a glob", Search, Anywhere => |app| open_grep_picker(app, true, None);
    "picker.resume", "Resume last picker", Search, Anywhere => resume_picker;
    "goto.menu", "Line, definition, back or forward…", GoTo, Editor => |app| open_menu(app, &GO_TO_MENU);
    "goto.line", "Line…", GoTo, Editor => goto_line_prompt;
    "goto.definition", "Definition or file under the cursor", GoTo, Editor => super::goto::goto_definition;
    "jump.back", "Jump back", GoTo, Anywhere => |app| app.jump(true);
    "jump.forward", "Jump forward", GoTo, Anywhere => |app| app.jump(false);
    "bookmark.menu", "Toggle, next, previous, list…", Bookmarks, Editor => |app| open_menu(app, &BOOKMARK_MENU);
    "bookmark.toggle", "Toggle on this line", Bookmarks, Editor => toggle_bookmark;
    "bookmark.next", "Next", Bookmarks, Editor => |app| bookmark_step(app, true);
    "bookmark.previous", "Previous", Bookmarks, Editor => |app| bookmark_step(app, false);
    "picker.bookmarks", "List", Bookmarks, Anywhere => open_bookmarks_picker;
    "bookmark.clear", "Clear in this file", Bookmarks, Editor => clear_bookmarks;
    "view.paneMenu", "Split, focus, resize or close panes, close tabs…", View, Anywhere => |app| open_menu(app, &PANE_MENU);
    "view.splitVertical", "Split vertically (side by side)", View, Editor => |app| app.split_pane(Direction::SideBySide);
    "view.splitHorizontal", "Split horizontally (one above the other)", View, Editor => |app| app.split_pane(Direction::Stacked);
    "view.resizePane", "Resize this pane…", View, Editor => App::resize_pane_prompt;
    "view.equalPanes", "Make the panes equal in size", View, Editor => App::equalize_panes;
    "view.closePane", "Close this pane", View, Editor => App::close_pane;
    "view.closeOtherPanes", "Close the other panes", View, Editor => App::close_other_panes;
    "view.focusNextPane", "Focus the next pane", View, Editor => App::focus_next_pane;
    "view.focusPaneLeft", "Focus the pane on the left", View, Editor => |app| app.focus_pane_on(Side::Left);
    "view.focusPaneRight", "Focus the pane on the right", View, Editor => |app| app.focus_pane_on(Side::Right);
    "view.focusPaneAbove", "Focus the pane above", View, Editor => |app| app.focus_pane_on(Side::Above);
    "view.focusPaneBelow", "Focus the pane below", View, Editor => |app| app.focus_pane_on(Side::Below);
    "view.toggleScrollSync", "Scroll all panes together (sync on / off)", View, Editor => App::toggle_scroll_sync;
    "switch.next", "Switch to the tree (1) or a tab (2-9); again: next tab", View, Anywhere => |app| {
        if app.switcher.is_some() {
            app.step_tab(1);
        } else {
            app.start_switcher();
        }
    };
    "switch.previous", "Switch to the tree or a tab; again: previous tab", View, Anywhere => |app| {
        if app.switcher.is_some() {
            app.step_tab(-1);
        } else {
            app.start_switcher();
        }
    };
    "tab.next", "Next tab", View, Anywhere => |app| app.step_tab(1);
    "tab.previous", "Previous tab", View, Anywhere => |app| app.step_tab(-1);
    "tab.close", "Close this tab", View, Editor => App::close_current_tab;
    "tab.closeOthers", "Close the tabs not on screen", View, Anywhere => App::close_other_tabs;
    "view.toggleLineNumbers", "Toggle line numbers", View, Editor => |app| app.options.line_numbers = !app.options.line_numbers;
    "view.toggleIndentGuides", "Toggle indent guides", View, Editor => |app| app.options.indent_guides = !app.options.indent_guides;
    "language.choose", "Set language of the file…", View, Editor => open_language_picker;
    "tree.menu", "Toggle, reveal, focus or lock…", Tree, Anywhere => |app| open_menu(app, &TREE_MENU);
    "tree.toggle", "Toggle", Tree, Anywhere => toggle_tree;
    "tree.focus", "Focus", Tree, Anywhere => App::focus_tree;
    "tree.reveal", "Reveal current file", Tree, Anywhere => reveal_in_tree;
    "tree.lock", "Lock / unlock following the current file", Tree, Anywhere => toggle_tree_lock;
    "tree.toggleIgnored", "Show / hide git-ignored files", Tree, Tree => |app| {
        app.tree.show_ignored = !app.tree.show_ignored;
        app.tree.rebuild(&app.git);
    };
    "tree.toggleChanged", "Show only changed files", Tree, Tree => |app| {
        app.tree.changed_only = !app.tree.changed_only;
        app.tree.rebuild(&app.git);
    };
    "tree.refresh", "Refresh", Tree, Tree => |app| {
        app.tree.rebuild(&app.git);
        app.refresh_git();
        app.refresh_files();
    };
    "tree.narrow", "Narrower", Tree, Tree => |app| resize_tree(app, -TREE_RESIZE_STEP);
    "tree.widen", "Wider", Tree, Tree => |app| resize_tree(app, TREE_RESIZE_STEP);
    "tree.autoWidth", "Automatic width", Tree, Tree => |app| {
        app.options.tree_width = 0;
        app.notify("File tree: automatic width");
    };
    "picker.commands", "Command palette and keys", App, Anywhere => open_commands_picker;
    "settings.open", "Open yate.json (the closest one, or a new global one)", App, Anywhere => open_settings;
    "config.reload", "Reload yate.json and plugins", App, Anywhere => |app| {
        if app.load_config() {
            app.notify("Reloaded settings and plugins");
        }
    };
    "app.log", "Open the log", App, Anywhere => |app| {
        let path = app.paths.dir.join("yate.log");
        app.open(&path, None);
    };
    "app.quit", "Quit", App, Anywhere => quit;
}

/// Commands the palette leaves out: the ones that only make sense as a
/// key (the palette itself, the switcher, the menus whose entries it lists
/// one by one).
const NOT_IN_PALETTE: &[&str] = &[
    "picker.commands",
    "goto.menu",
    "bookmark.menu",
    "view.paneMenu",
    "tree.menu",
    "edit.lineMenu",
    "switch.next",
    "switch.previous",
];

type Condition = fn(&App) -> bool;

/// Commands the palette lists only while they have something to work on.
const LISTED_WHEN: &[(&[&str], Condition)] = &[
    (&["file.reload", "tree.reveal", "bookmark.toggle"], |app| {
        app.doc().path.is_some()
    }),
    (&["file.rename", "file.delete", "file.copyPath"], |app| {
        target_path(app).is_some()
    }),
    (&["picker.filesInFolder", "picker.grepInFolder"], |app| {
        app.tree.selected_row().is_some()
    }),
    (&["find.next", "find.previous"], |app| {
        !app.find.input.text.is_empty()
    }),
    (&["picker.resume"], |app| app.last_picker.is_some()),
    (&["tab.next", "tab.previous"], |app| app.tabs.len() > 1),
    (&["tab.close"], |app| app.current_tab().is_some()),
    (&["tab.closeOthers"], |app| {
        let shown =
            |tab: &std::path::PathBuf| app.docs.iter().any(|d| d.path.as_ref() == Some(tab));
        app.tabs.iter().any(|tab| !shown(tab))
    }),
    (&["jump.back"], |app| app.jump_index > 0),
    (&["jump.forward"], |app| {
        app.jump_index + 1 < app.jumps.len()
    }),
    (
        &["bookmark.next", "bookmark.previous", "picker.bookmarks"],
        |app| !app.bookmarks.items.is_empty(),
    ),
    (&["bookmark.clear"], |app| {
        app.doc()
            .path
            .as_ref()
            .is_some_and(|path| !app.bookmarks.lines_in(path).is_empty())
    }),
    (
        &[
            "view.resizePane",
            "view.equalPanes",
            "view.closePane",
            "view.closeOtherPanes",
            "view.focusNextPane",
            "view.focusPaneLeft",
            "view.focusPaneRight",
            "view.focusPaneAbove",
            "view.focusPaneBelow",
            "view.toggleScrollSync",
        ],
        |app| app.panes.is_split(),
    ),
    (&["tree.lock"], |app| app.tree_visible),
    (&["tree.focus"], |app| app.focus != Focus::Tree),
];

/// Whether the palette of the focused panel lists a command right now.
fn in_palette(app: &App, command: &Command) -> bool {
    command.scope.includes(app.context())
        && !NOT_IN_PALETTE.contains(&command.id)
        && LISTED_WHEN
            .iter()
            .filter(|(ids, _)| ids.contains(&command.id))
            .all(|(_, listed)| listed(app))
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
    let path = app
        .settings_files
        .last()
        .cloned()
        .unwrap_or_else(|| app.paths.dir.join(crate::settings::NEW_FILE_NAME));
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
    let title = format!("Unsaved: {}. Quit anyway?", unsaved.join(", "));
    let prompt = Prompt::confirm(title, PromptKind::QuitUnsaved);
    app.overlay = Some(Overlay::Prompt(prompt));
}

pub fn save(app: &mut App) {
    if app.doc().path.is_none() {
        let id = app.doc().id;
        let prompt = Prompt::text("Save as", LineInput::default(), PromptKind::SaveAs(id));
        app.overlay = Some(Overlay::Prompt(prompt));
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
                || saved.starts_with(app.paths.dir.join("plugins"));
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
    let input = LineInput::with_text(&initial);
    let title = "New file (end with / for a folder)";
    let prompt = Prompt::text(title, input, PromptKind::NewFile);
    app.overlay = Some(Overlay::Prompt(prompt));
}

fn goto_line_prompt(app: &mut App) {
    let title = format!("Go to line (1-{})", app.doc().line_count());
    let prompt = Prompt::text(title, LineInput::default(), PromptKind::GoToLine);
    app.overlay = Some(Overlay::Prompt(prompt));
}

/// The files of the project for a picker: the recent ones first, the
/// latest on top, without the file being edited.
pub fn file_items(app: &App) -> Vec<PickerItem> {
    let current = app.doc().path.as_ref();
    let recent: Vec<String> = app
        .recent
        .iter()
        .filter(|recent| Some(&recent.path) != current)
        .filter_map(|recent| recent.path.strip_prefix(&app.root).ok())
        .map(|path| path.to_string_lossy().into_owned())
        .collect();
    let wanted: HashSet<&str> = recent.iter().map(String::as_str).collect();
    let indexed: HashSet<&str> = app
        .files
        .iter()
        .map(String::as_str)
        .filter(|file| wanted.contains(file))
        .collect();
    let first = recent.iter().filter(|file| indexed.contains(file.as_str()));
    let rest = app
        .files
        .iter()
        .filter(|file| !indexed.contains(file.as_str()));
    first
        .chain(rest)
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
        file_items(app)
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
                    ..PickerItem::default()
                })
                .collect();
            picker.append_items(items);
        }
        GrepEvent::Done(generation) if generation == current => picker.searching = false,
        _ => {}
    }
}

/// The command palette: what the focused panel can do right now, section
/// by section, the sections about the panel first. Plugin commands belong
/// to the editor.
fn open_commands_picker(app: &mut App) {
    let context = app.context();
    let order = Group::palette_order(context);
    let rank = |group: Group| order.iter().position(|g| *g == group);
    let built_in = COMMANDS
        .iter()
        .filter(|command| in_palette(app, command))
        .map(|command| {
            (
                command.group,
                command.id.to_string(),
                command.title.to_string(),
            )
        });
    let from_plugins = app
        .plugin_commands
        .iter()
        .map(|(id, title)| (Group::Plugins, id.clone(), title.clone()));
    let mut listed: Vec<(usize, Group, String, String)> = built_in
        .chain(from_plugins)
        .filter_map(|(group, id, title)| Some((rank(group)?, group, id, title)))
        .collect();
    listed.sort_by_key(|(rank, ..)| *rank);
    let commands = listed.into_iter().map(|(_, group, id, title)| PickerItem {
        detail: key_hints(app, context, &id).join("  "),
        label: title,
        group: group.name().to_string(),
        data: id,
        ..PickerItem::default()
    });
    let built_in = BUILT_IN_KEYS.iter().map(|(keys, what)| PickerItem {
        detail: keys.to_string(),
        label: what.to_string(),
        group: "Keys".to_string(),
        ..PickerItem::default()
    });
    let items = commands.chain(built_in).collect();
    let title = format!("Commands ({})", context.name());
    set_picker(app, Picker::new(PickerKind::Commands, &title, items));
}

fn open_menu(app: &mut App, spec: &'static MenuSpec) {
    app.overlay = Some(Overlay::Menu(Menu {
        spec,
        opened: Instant::now(),
    }));
}

fn resize_tree(app: &mut App, delta: i32) {
    if !app.tree_visible {
        app.notify("The file tree is hidden");
        return;
    }
    let screen_width = app.layout.status.w;
    let current = crate::ui::sidebar::width(app, screen_width) as i32;
    app.set_tree_width((current + delta).max(0) as u16);
    let width = app.options.tree_width;
    app.notify(format!("File tree: {width} columns"));
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

fn open_emoji_picker(app: &mut App) {
    let items = crate::emoji::all()
        .into_iter()
        .map(|emoji| PickerItem {
            label: format!("{} {}", emoji.symbol, emoji.name),
            group: emoji.group.to_string(),
            data: format!("emoji:{}", emoji.symbol),
            ..PickerItem::default()
        })
        .collect();
    set_picker(app, Picker::new(PickerKind::Commands, "Emoji", items));
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
            Some(found) => {
                app.open_at(&found.path, found.line, found.col.unwrap_or(0));
            }
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
            app.open_at(&path, query_line, query_col.unwrap_or(0));
        }
        PickerKind::Grep | PickerKind::Bookmarks | PickerKind::Definitions => {
            if let Some(path) = item.path {
                app.open_at(&path, item.line, item.col);
            }
        }
        PickerKind::Commands => {
            if item.data.is_empty() {
                return;
            }
            if let Some(name) = item.data.strip_prefix("language:") {
                let language = languages::by_name(name);
                app.doc_mut().set_language(language);
            } else if let Some(emoji) = item.data.strip_prefix("emoji:") {
                edit(app, |doc, _| ops::insert_text(doc, emoji, EditKind::Other));
            } else {
                app.run_command(&item.data);
            }
        }
    }
}

/// Keeps the preview of the selected picker item up to date.
pub fn update_preview(app: &mut App) {
    let Some(Overlay::Picker(picker)) = &app.overlay else {
        return;
    };
    let path = match picker.selected_item() {
        Some(item) if picker.kind == PickerKind::Files => Some(app.root.join(&item.label)),
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
    app.tree_visible = true;
    match app.doc().path.clone() {
        Some(path) => {
            app.tree.reveal(&path, &app.git);
            app.tree.ensure_visible(app.tree_list_height());
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
        app.notify("Nothing to rename: save the file first");
        return;
    };
    let relative = app.relative(&path);
    let input = LineInput::with_text(&relative);
    let prompt = Prompt::text("Rename to", input, PromptKind::Rename(path));
    app.overlay = Some(Overlay::Prompt(prompt));
}

fn delete_prompt(app: &mut App) {
    let Some(path) = target_path(app) else {
        app.notify("Nothing to delete: save the file first");
        return;
    };
    let title = format!("Delete {}?", app.relative(&path));
    let prompt = Prompt::confirm(title, PromptKind::Delete(path));
    app.overlay = Some(Overlay::Prompt(prompt));
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

/// Keys handled by the panels themselves, not bound to commands: the last
/// section of the palette.
const BUILT_IN_KEYS: &[(&str, &str)] = &[
    ("ctrl+o 1", "Switch: focus the file tree"),
    ("ctrl+o 2-9", "Switch: show tab 1-8"),
    ("ctrl+o ctrl+o", "Switch: next tab"),
    ("ctrl+key", "Menu: the same as the key"),
    ("ctrl+arrow", "Menu: run its arrow command again"),
    ("shift+arrows", "Editor: select"),
    (
        "ctrl+left ctrl+right",
        "Editor: move jumpChars characters (10)",
    ),
    ("ctrl+up ctrl+down", "Editor: move jumpLines lines (5)"),
    ("ctrl+home ctrl+end", "Editor: start / end of the file"),
    ("tab shift+tab", "Editor: indent / outdent"),
    ("ctrl+click", "Editor: go to definition"),
    ("tab", "Picker: glob filter (*.rs, src/**, !*.test.ts)"),
    ("up", "Picker: the last search, before typing"),
    ("enter down up", "Find: next / previous match"),
    ("tab left right", "Find: next field or option"),
    ("space enter", "Find: toggle an option, run a button"),
    ("left right enter", "Dialog: pick a button"),
    ("enter space", "Tree: open file / fold folder"),
    ("left right", "Tree: fold / unfold"),
    ("up", "Tree: from the first file to the filters"),
    ("space enter", "Tree: tick a filter (changed, ignored)"),
    ("esc tab", "Tree: back to the editor"),
    ("drag the right edge", "Tree: resize the file tree"),
];

/// Saves document `id`, also when a pane without the focus shows it.
fn save_doc(app: &mut App, id: usize) {
    let focused = app.doc().id;
    let Some(index) = app.docs.iter().position(|d| d.id == id) else {
        return;
    };
    app.active = index;
    save(app);
    if let Some(index) = app.docs.iter().position(|d| d.id == focused) {
        app.active = index;
    }
}

fn open_find(app: &mut App, replace: bool) {
    if app.focus == Focus::Find && replace {
        let open = !app.find.replace_open;
        app.find.set_replace_open(open);
        return;
    }
    if app.focus == Focus::Find && !app.find.input.text.is_empty() {
        find_step(app, true);
        return;
    }
    app.find_open = true;
    app.find.replace_open = replace || (app.find.replace_open && app.focus == Focus::Find);
    app.find.field = FindField::Query;
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
        app.error("Save the file before bookmarking it");
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

/// Answers a prompt: the typed text, or the answer of a button.
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
            let Some(doc) = app.docs.iter_mut().find(|d| d.id == id) else {
                return;
            };
            doc.path = Some(path);
            doc.detect_language();
            save_doc(app, id);
            app.touch_recent();
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
                    app.rename_tabs(&from, &to);
                    for recent in &mut app.recent {
                        if let Ok(rest) = recent.path.strip_prefix(&from) {
                            recent.path = if rest.as_os_str().is_empty() {
                                to.clone()
                            } else {
                                to.join(rest)
                            };
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
                        app.forget_doc(id);
                    }
                    app.recent.retain(|recent| !recent.path.starts_with(&path));
                    app.tabs.retain(|tab| !tab.starts_with(&path));
                    app.notify(format!("Deleted {}", app.relative(&path)));
                    app.tree.rebuild(&app.git);
                    app.refresh_files();
                    app.refresh_git();
                }
                Err(err) => app.error(err.to_string()),
            }
        }
        PromptKind::ResizePane => {
            if let Some(answer) = answer {
                app.resize_pane(&answer);
            }
        }
        PromptKind::QuitUnsaved => {
            if answer.as_deref() == Some("y") {
                app.quit = true;
            }
        }
        PromptKind::LeaveUnsaved { doc_id, then } => match answer.as_deref() {
            Some("s") => {
                save_doc(app, doc_id);
                let saved = app
                    .docs
                    .iter()
                    .find(|d| d.id == doc_id)
                    .is_none_or(|d| !d.is_modified());
                if saved {
                    then(app);
                }
            }
            Some("d") => {
                app.leave_confirmed = true;
                then(app);
                app.leave_confirmed = false;
            }
            _ => {}
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn palette_rules_name_real_commands() {
        let ruled = LISTED_WHEN
            .iter()
            .flat_map(|(ids, _)| ids.iter())
            .chain(NOT_IN_PALETTE);
        for id in ruled {
            assert!(COMMANDS.iter().any(|c| c.id == *id), "{id} is no command");
        }
    }

    #[test]
    fn every_section_shows_in_the_editor_palette() {
        let order = Group::palette_order(Context::Editor);
        for command in COMMANDS {
            assert!(
                order.contains(&command.group),
                "{} is in a section the editor palette lacks",
                command.id
            );
        }
        for group in Group::palette_order(Context::Tree) {
            assert!(order.contains(group));
        }
    }
}
