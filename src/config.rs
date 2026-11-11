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
    /// Milliseconds after Alt+W before the numbered targets pop up.
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

pub const OPTION_NAMES: &[&str] = &[
    "tabSize",
    "insertSpaces",
    "scrollOff",
    "indentGuides",
    "autoPairs",
    "lineNumbers",
    "cursorLine",
    "colorize",
    "nerdIcons",
    "showIgnored",
    "treeFollow",
    "treeAutoCloseWidth",
    "treeWidth",
    "mouseScrollLines",
    "jumpChars",
    "jumpLines",
    "updateCheck",
    "keyHintDelay",
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
                    OPTION_NAMES.join(", ")
                ));
            }
        }
        Ok(())
    }
}

fn home() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
}

fn xdg(variable: &str, fallback: &str) -> PathBuf {
    std::env::var_os(variable)
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .unwrap_or_else(|| home().join(fallback))
        .join("yate")
}

/// Where yate reads its configuration and keeps its state. Tests use
/// their own folders.
#[derive(Clone, Debug)]
pub struct Paths {
    pub config_dir: PathBuf,
    pub state_dir: PathBuf,
    /// Holds the global `yate.json`.
    pub home: Option<PathBuf>,
    /// The working directory: `yate.json` files on the way to it apply.
    pub cwd: PathBuf,
    /// `yate.json` files above this folder are ignored (tests).
    pub ceiling: Option<PathBuf>,
}

impl Paths {
    pub fn from_env() -> Paths {
        Paths {
            config_dir: config_dir(),
            state_dir: state_dir(),
            home: std::env::var_os("HOME").map(PathBuf::from),
            cwd: std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
            ceiling: None,
        }
    }
}

/// `~/.config/yate`: the plugins, `plugins/<name>.js`.
pub fn config_dir() -> PathBuf {
    xdg("XDG_CONFIG_HOME", ".config")
}

/// `~/.local/state/yate`: the log and the bookmarks.
pub fn state_dir() -> PathBuf {
    xdg("XDG_STATE_HOME", ".local/state")
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
}
