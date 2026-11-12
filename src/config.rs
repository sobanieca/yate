//! Editor options and the directories yate reads and writes. Options are set
//! in `yate.json` or from plugins (`yate.setOption`).

use std::path::PathBuf;

#[derive(Clone, Debug)]
pub struct Options {
    pub tab_size: usize,
    pub insert_spaces: bool,
    /// Lines kept above and below the cursor; a large value keeps the cursor
    /// line centered (like `scrolloff=999`).
    pub scroll_off: usize,
    pub indent_guides: bool,
    pub auto_pairs: bool,
    pub line_numbers: bool,
    pub cursor_line: bool,
    /// Shows `#rrggbb` colors in their color.
    pub colorize: bool,
    /// Uses Nerd Font icons in the file tree.
    pub nerd_icons: bool,
    /// Shows git-ignored files in the file tree.
    pub show_ignored: bool,
    /// The file tree follows the current file.
    pub tree_follow: bool,
    /// Closes the tree after opening a file when the terminal is narrow.
    pub tree_auto_close_width: u16,
    /// Columns of the file tree; 0 fits the names.
    pub tree_width: u16,
    pub mouse_scroll_lines: usize,
    /// Characters moved by Ctrl+Left and Ctrl+Right.
    pub jump_chars: usize,
    /// Lines moved by Ctrl+Up and Ctrl+Down.
    pub jump_lines: usize,
    /// Checks once a day for a new release and tells when there is one.
    pub update_check: bool,
    /// Milliseconds before the popup of a menu or of the switcher shows.
    pub key_hint_delay: u64,
}

impl Default for Options {
    fn default() -> Self {
        Options {
            tab_size: 2,
            insert_spaces: true,
            scroll_off: 999,
            indent_guides: true,
            auto_pairs: true,
            line_numbers: true,
            cursor_line: true,
            colorize: true,
            nerd_icons: false,
            show_ignored: false,
            tree_follow: true,
            tree_auto_close_width: 200,
            tree_width: 0,
            mouse_scroll_lines: 3,
            jump_chars: 10,
            jump_lines: 5,
            update_check: true,
            key_hint_delay: 500,
        }
    }
}

#[derive(Clone, Debug)]
pub enum OptionValue {
    Bool(bool),
    Number(f64),
    Text(String),
}

impl OptionValue {
    fn as_bool(&self, name: &str) -> Result<bool, String> {
        match self {
            OptionValue::Bool(value) => Ok(*value),
            OptionValue::Text(text) => Err(format!(
                "option {name} expects true or false, not \"{text}\""
            )),
            _ => Err(format!("option {name} expects true or false")),
        }
    }

    fn as_usize(&self, name: &str) -> Result<usize, String> {
        match self {
            OptionValue::Number(value) if *value >= 0.0 => Ok(*value as usize),
            OptionValue::Text(text) => {
                Err(format!("option {name} expects a number, not \"{text}\""))
            }
            _ => Err(format!("option {name} expects a positive number")),
        }
    }
}

/// The options by name, as in `yate.json` and `yate.setOption`, with what
/// they change.
pub const OPTIONS: &[(&str, &str)] = &[
    ("tabSize", "columns of an indent level and of a tab"),
    ("insertSpaces", "tab inserts spaces"),
    (
        "scrollOff",
        "lines kept around the cursor; 999 keeps it centered",
    ),
    ("indentGuides", "show indent guides"),
    ("autoPairs", "close brackets and quotes as you type"),
    ("lineNumbers", "show line numbers"),
    ("cursorLine", "highlight the cursor line"),
    ("colorize", "show #rrggbb colors in their color"),
    ("nerdIcons", "Nerd Font icons in the file tree"),
    ("showIgnored", "show git-ignored files in the file tree"),
    ("treeFollow", "the file tree follows the current file"),
    (
        "treeAutoCloseWidth",
        "close the tree after opening a file below this width",
    ),
    ("treeWidth", "columns of the file tree; 0 fits the names"),
    ("mouseScrollLines", "lines a mouse wheel step scrolls"),
    ("jumpChars", "characters ctrl+left and ctrl+right move"),
    ("jumpLines", "lines ctrl+up and ctrl+down move"),
    ("updateCheck", "check once a day for a new release"),
    (
        "keyHintDelay",
        "milliseconds before the popup of a menu shows",
    ),
];

impl Options {
    pub fn set(&mut self, name: &str, value: &OptionValue) -> Result<(), String> {
        match name {
            "tabSize" => self.tab_size = value.as_usize(name)?.clamp(1, 16),
            "insertSpaces" => self.insert_spaces = value.as_bool(name)?,
            "scrollOff" => self.scroll_off = value.as_usize(name)?,
            "indentGuides" => self.indent_guides = value.as_bool(name)?,
            "autoPairs" => self.auto_pairs = value.as_bool(name)?,
            "lineNumbers" => self.line_numbers = value.as_bool(name)?,
            "cursorLine" => self.cursor_line = value.as_bool(name)?,
            "colorize" => self.colorize = value.as_bool(name)?,
            "nerdIcons" => self.nerd_icons = value.as_bool(name)?,
            "showIgnored" => self.show_ignored = value.as_bool(name)?,
            "treeFollow" => self.tree_follow = value.as_bool(name)?,
            "treeAutoCloseWidth" => self.tree_auto_close_width = value.as_usize(name)? as u16,
            "treeWidth" => self.tree_width = value.as_usize(name)?.min(u16::MAX as usize) as u16,
            "mouseScrollLines" => self.mouse_scroll_lines = value.as_usize(name)?.max(1),
            "jumpChars" => self.jump_chars = value.as_usize(name)?.max(1),
            "jumpLines" => self.jump_lines = value.as_usize(name)?.max(1),
            "updateCheck" => self.update_check = value.as_bool(name)?,
            "keyHintDelay" => self.key_hint_delay = value.as_usize(name)? as u64,
            _ => {
                return Err(format!(
                    "unknown option {name} (known: {})",
                    OPTIONS
                        .iter()
                        .map(|(name, _)| *name)
                        .collect::<Vec<_>>()
                        .join(", ")
                ));
            }
        }
        Ok(())
    }

    /// The value of an option as written in `yate.json`: `2`, `true`.
    pub fn json_value(&self, name: &str) -> Option<String> {
        let value = match name {
            "tabSize" => self.tab_size.to_string(),
            "insertSpaces" => self.insert_spaces.to_string(),
            "scrollOff" => self.scroll_off.to_string(),
            "indentGuides" => self.indent_guides.to_string(),
            "autoPairs" => self.auto_pairs.to_string(),
            "lineNumbers" => self.line_numbers.to_string(),
            "cursorLine" => self.cursor_line.to_string(),
            "colorize" => self.colorize.to_string(),
            "nerdIcons" => self.nerd_icons.to_string(),
            "showIgnored" => self.show_ignored.to_string(),
            "treeFollow" => self.tree_follow.to_string(),
            "treeAutoCloseWidth" => self.tree_auto_close_width.to_string(),
            "treeWidth" => self.tree_width.to_string(),
            "mouseScrollLines" => self.mouse_scroll_lines.to_string(),
            "jumpChars" => self.jump_chars.to_string(),
            "jumpLines" => self.jump_lines.to_string(),
            "updateCheck" => self.update_check.to_string(),
            "keyHintDelay" => self.key_hint_delay.to_string(),
            _ => return None,
        };
        Some(value)
    }
}

fn home() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
}

/// Where yate reads its configuration and keeps its state. Tests use
/// their own folders.
#[derive(Clone, Debug)]
pub struct Paths {
    /// The yate folder (`dir()`): the global `yate.json`, plugins, log,
    /// bookmarks.
    pub dir: PathBuf,
    /// For `~` in paths and messages; `yate.json` files above it are ignored.
    pub home: Option<PathBuf>,
    /// The working directory: `yate.json` files on the way to it apply.
    pub cwd: PathBuf,
    /// `yate.json` files above this folder are ignored (tests).
    pub ceiling: Option<PathBuf>,
}

impl Paths {
    pub fn from_env() -> Paths {
        Paths {
            dir: dir(),
            home: std::env::var_os("HOME").map(PathBuf::from),
            cwd: std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
            ceiling: None,
        }
    }
}

/// `~/.yate`, or `$YATE_HOME` when it is an absolute path: the global
/// `yate.json`, `plugins/<name>.js`, `yate.log`, `bookmarks/` and the
/// update check's `latest-version`.
pub fn dir() -> PathBuf {
    std::env::var_os("YATE_HOME")
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .unwrap_or_else(|| home().join(".yate"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn options_are_validated() {
        let mut options = Options::default();
        options.set("tabSize", &OptionValue::Number(4.0)).unwrap();
        assert_eq!(options.tab_size, 4);
        assert!(options.set("tabSize", &OptionValue::Bool(true)).is_err());
        assert!(options.set("nope", &OptionValue::Bool(true)).is_err());
    }

    #[test]
    fn every_option_has_a_value() {
        let options = Options::default();
        for (name, _) in OPTIONS {
            assert!(options.json_value(name).is_some(), "{name}");
        }
    }
}
