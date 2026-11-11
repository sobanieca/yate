//! Key bindings: key names like `alt+shift+down`, bound to command ids per
//! context. Plugins rebind keys with `yate.bindKey`.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use std::collections::HashMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Context {
    /// Everywhere except inside pickers and prompts.
    Global,
    Editor,
    Tree,
}

impl Context {
    pub const ALL: [Context; 3] = [Context::Global, Context::Editor, Context::Tree];

    pub fn name(self) -> &'static str {
        match self {
            Context::Global => "global",
            Context::Editor => "editor",
            Context::Tree => "tree",
        }
    }

    pub fn parse(name: &str) -> Option<Context> {
        match name {
            "global" => Some(Context::Global),
            "editor" => Some(Context::Editor),
            "tree" => Some(Context::Tree),
            _ => None,
        }
    }
}

/// The name of a key press, e.g. `ctrl+shift+p`, `alt+up`, `ctrl+tab`.
pub fn key_name(key: &KeyEvent) -> String {
    let mut shift = key.modifiers.contains(KeyModifiers::SHIFT);
    let base = match key.code {
        KeyCode::Char(' ') => "space".to_string(),
        KeyCode::Char(c) if c.is_uppercase() => {
            shift = true;
            c.to_lowercase().to_string()
        }
        KeyCode::Char(c) => {
            if !c.is_alphabetic() {
                shift = false;
            }
            c.to_string()
        }
        KeyCode::BackTab => {
            shift = true;
            "tab".to_string()
        }
        KeyCode::Enter => "enter".to_string(),
        KeyCode::Tab => "tab".to_string(),
        KeyCode::Backspace => "backspace".to_string(),
        KeyCode::Delete => "delete".to_string(),
        KeyCode::Insert => "insert".to_string(),
        KeyCode::Esc => "esc".to_string(),
        KeyCode::Up => "up".to_string(),
        KeyCode::Down => "down".to_string(),
        KeyCode::Left => "left".to_string(),
        KeyCode::Right => "right".to_string(),
        KeyCode::Home => "home".to_string(),
        KeyCode::End => "end".to_string(),
        KeyCode::PageUp => "pageup".to_string(),
        KeyCode::PageDown => "pagedown".to_string(),
        KeyCode::F(n) => format!("f{n}"),
        _ => "unknown".to_string(),
    };
    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
    let base = if ctrl {
        legacy_control_key(&base).unwrap_or(&base).to_string()
    } else {
        base
    };
    let mut name = String::new();
    if ctrl {
        name.push_str("ctrl+");
    }
    if key.modifiers.contains(KeyModifiers::ALT) {
        name.push_str("alt+");
    }
    if shift {
        name.push_str("shift+");
    }
    name.push_str(&base);
    name
}

/// Without the kitty keyboard protocol, terminals send `Ctrl+\`, Ctrl+] and
/// Ctrl+/ as the control characters that crossterm reports as Ctrl+4, Ctrl+5
/// and Ctrl+7.
fn legacy_control_key(base: &str) -> Option<&'static str> {
    match base {
        "4" => Some("\\"),
        "5" => Some("]"),
        "7" => Some("/"),
        _ => None,
    }
}

/// Normalizes a key written by a person (`Ctrl+Shift+P`) to a key name.
pub fn normalize(key: &str) -> Result<String, String> {
    let lower = key.trim().to_lowercase();
    let parts: Vec<&str> = lower.split('+').collect();
    let (base, modifiers) = match parts.split_last() {
        Some((base, modifiers)) if !base.is_empty() => (*base, modifiers),
        _ if lower.ends_with('+') => ("+", &parts[..parts.len().saturating_sub(2)]),
        _ => return Err(format!("invalid key {key}")),
    };
    let (mut ctrl, mut alt, mut shift) = (false, false, false);
    for modifier in modifiers {
        match *modifier {
            "ctrl" | "control" | "c" => ctrl = true,
            "alt" | "meta" | "option" | "m" => alt = true,
            "shift" | "s" => shift = true,
            other => return Err(format!("unknown modifier {other} in {key}")),
        }
    }
    let base = match base {
        "return" => "enter",
        "escape" => "esc",
        "del" => "delete",
        "pgup" => "pageup",
        "pgdown" | "pgdn" => "pagedown",
        other => other,
    };
    let mut name = String::new();
    if ctrl {
        name.push_str("ctrl+");
    }
    if alt {
        name.push_str("alt+");
    }
    if shift {
        name.push_str("shift+");
    }
    name.push_str(base);
    Ok(name)
}

/// The key press for a key name: the inverse of `key_name`, used by the
/// end-to-end tests (`ctrl+p`, `alt+shift+down`, `enter`, `ctrl+tab`, `a`).
pub fn parse_key(name: &str) -> Result<KeyEvent, String> {
    let name = normalize(name)?;
    let mut modifiers = KeyModifiers::NONE;
    let mut base = name.as_str();
    loop {
        if let Some(rest) = base.strip_prefix("ctrl+") {
            modifiers |= KeyModifiers::CONTROL;
            base = rest;
        } else if let Some(rest) = base.strip_prefix("alt+") {
            modifiers |= KeyModifiers::ALT;
            base = rest;
        } else if let Some(rest) = base.strip_prefix("shift+") {
            modifiers |= KeyModifiers::SHIFT;
            base = rest;
        } else {
            break;
        }
    }
    let code = match base {
        "enter" => KeyCode::Enter,
        "tab" if modifiers.contains(KeyModifiers::SHIFT) => KeyCode::BackTab,
        "tab" => KeyCode::Tab,
        "backspace" => KeyCode::Backspace,
        "delete" => KeyCode::Delete,
        "insert" => KeyCode::Insert,
        "esc" => KeyCode::Esc,
        "up" => KeyCode::Up,
        "down" => KeyCode::Down,
        "left" => KeyCode::Left,
        "right" => KeyCode::Right,
        "home" => KeyCode::Home,
        "end" => KeyCode::End,
        "pageup" => KeyCode::PageUp,
        "pagedown" => KeyCode::PageDown,
        "space" => KeyCode::Char(' '),
        f if f.len() > 1 && f.starts_with('f') && f[1..].parse::<u8>().is_ok() => {
            KeyCode::F(f[1..].parse().unwrap_or(1))
        }
        other => {
            let mut chars = other.chars();
            match (chars.next(), chars.next()) {
                (Some(c), None) if modifiers.contains(KeyModifiers::SHIFT) && c.is_alphabetic() => {
                    KeyCode::Char(c.to_ascii_uppercase())
                }
                (Some(c), None) => KeyCode::Char(c),
                _ => return Err(format!("unknown key {name}")),
            }
        }
    };
    Ok(KeyEvent::new(code, modifiers))
}

pub struct Keymap {
    bindings: HashMap<(Context, String), String>,
}

/// The default bindings: one key for each command, and no Ctrl+Shift keys,
/// which terminals send only with the kitty keyboard protocol and never
/// through tmux.
pub const DEFAULTS: &[(Context, &str, &str)] = &[
    (Context::Global, "ctrl+q", "app.quit"),
    (Context::Global, "ctrl+h", "help.keys"),
    (Context::Global, "ctrl+s", "file.save"),
    (Context::Global, "ctrl+n", "file.new"),
    (Context::Global, "ctrl+p", "picker.files"),
    (Context::Global, "alt+f", "picker.grep"),
    (Context::Global, "alt+p", "picker.commands"),
    (Context::Global, "alt+r", "picker.resume"),
    (Context::Global, "alt+w", "switch.next"),
    (Context::Global, "alt+shift+w", "switch.previous"),
    (Context::Global, "alt+e", "tree.toggle"),
    (Context::Global, "ctrl+t", "tree.reveal"),
    (Context::Global, "alt+s", "view.splitMenu"),
    (Context::Global, "ctrl+o", "view.focusNextPane"),
    (Context::Global, "alt+h", "view.focusPaneLeft"),
    (Context::Global, "alt+j", "view.focusPaneBelow"),
    (Context::Global, "alt+k", "view.focusPaneAbove"),
    (Context::Global, "alt+l", "view.focusPaneRight"),
    (Context::Global, "ctrl+j", "view.toggleScrollSync"),
    (Context::Global, "alt+left", "jump.back"),
    (Context::Global, "alt+right", "jump.forward"),
    (Context::Global, "ctrl+g", "goto.menu"),
    (Context::Global, "ctrl+f", "find.open"),
    (Context::Global, "ctrl+r", "find.replace"),
    (Context::Global, "alt+m", "bookmark.menu"),
    (Context::Editor, "ctrl+z", "edit.undo"),
    (Context::Editor, "ctrl+y", "edit.redo"),
    (Context::Editor, "ctrl+c", "edit.copy"),
    (Context::Editor, "ctrl+x", "edit.cut"),
    (Context::Editor, "ctrl+v", "edit.paste"),
    (Context::Editor, "ctrl+a", "edit.selectAll"),
    (Context::Editor, "ctrl+d", "edit.selectNext"),
    (Context::Editor, "ctrl+/", "edit.toggleComment"),
    (Context::Editor, "alt+up", "edit.moveLineUp"),
    (Context::Editor, "alt+down", "edit.moveLineDown"),
    (Context::Editor, "alt+shift+down", "edit.duplicateLine"),
    (Context::Editor, "ctrl+k", "edit.deleteLine"),
    (Context::Editor, "alt+i", "edit.indent"),
    (Context::Editor, "alt+o", "edit.outdent"),
    (Context::Tree, "r", "file.rename"),
    (Context::Tree, "d", "file.delete"),
    (Context::Tree, "y", "file.copyPath"),
    (Context::Tree, "i", "tree.toggleIgnored"),
    (Context::Tree, "m", "tree.toggleChanged"),
    (Context::Tree, "shift+r", "tree.refresh"),
    (Context::Tree, "f", "picker.filesInFolder"),
    (Context::Tree, "g", "picker.grepInFolder"),
    (Context::Tree, "[", "tree.narrow"),
    (Context::Tree, "]", "tree.widen"),
];

impl Keymap {
    pub fn defaults() -> Keymap {
        let bindings = DEFAULTS
            .iter()
            .map(|(context, key, command)| ((*context, key.to_string()), command.to_string()))
            .collect();
        Keymap { bindings }
    }

    pub fn bind(&mut self, context: Context, key: &str, command: &str) -> Result<(), String> {
        let key = normalize(key)?;
        if command.is_empty() {
            self.bindings.remove(&(context, key));
        } else {
            self.bindings.insert((context, key), command.to_string());
        }
        Ok(())
    }

    /// The command for `key` in `context`, falling back to global bindings.
    pub fn lookup(&self, context: Context, key: &str) -> Option<&str> {
        self.bindings
            .get(&(context, key.to_string()))
            .or_else(|| self.bindings.get(&(Context::Global, key.to_string())))
            .map(String::as_str)
    }

    /// Keys bound to `command`, for the command palette: one by default,
    /// more when `yate.json` adds some. Plain Ctrl keys come first, then
    /// Alt keys, which work everywhere, then the rest.
    pub fn keys_for(&self, command: &str) -> Vec<String> {
        self.keys_where(command, |_| true)
    }

    /// Keys that run `command` while the panel of `context` has the focus.
    pub fn keys_in(&self, context: Context, command: &str) -> Vec<String> {
        self.keys_where(command, |bound| {
            bound == context || bound == Context::Global
        })
    }

    fn keys_where(&self, command: &str, in_context: impl Fn(Context) -> bool) -> Vec<String> {
        let mut keys: Vec<String> = self
            .bindings
            .iter()
            .filter(|((context, _), bound)| bound.as_str() == command && in_context(*context))
            .map(|((_, key), _)| key.clone())
            .collect();
        keys.sort_by_key(|key| (key_rank(key), key.len(), key.clone()));
        keys.dedup();
        keys
    }

    /// The key shown in hints for `command`, when it has one.
    pub fn hint(&self, command: &str) -> Option<String> {
        self.keys_for(command).into_iter().next()
    }

    /// Every binding: context, key and command, sorted by context and key.
    pub fn all(&self) -> Vec<(Context, String, String)> {
        let mut all: Vec<(Context, String, String)> = self
            .bindings
            .iter()
            .map(|((context, key), command)| (*context, key.clone(), command.clone()))
            .collect();
        all.sort_by_key(|(context, key, _)| {
            (
                Context::ALL.iter().position(|c| c == context),
                key_rank(key),
                key.clone(),
            )
        });
        all
    }
}

fn key_rank(key: &str) -> u8 {
    let shift = key.contains("shift+");
    match (key.starts_with("ctrl+"), key.starts_with("alt+"), shift) {
        (true, _, false) => 0,
        (_, true, _) => 1,
        (true, _, true) => 2,
        _ if key.chars().count() == 1 && key.chars().all(char::is_alphabetic) => 0,
        _ if key.chars().count() == 1 => 1,
        _ => 3,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_names_normalize_shift_and_case() {
        let key = |code, modifiers| key_name(&KeyEvent::new(code, modifiers));
        assert_eq!(
            key(KeyCode::Char('P'), KeyModifiers::CONTROL),
            "ctrl+shift+p"
        );
        assert_eq!(
            key(
                KeyCode::Char('p'),
                KeyModifiers::CONTROL | KeyModifiers::SHIFT
            ),
            "ctrl+shift+p"
        );
        assert_eq!(key(KeyCode::Char('?'), KeyModifiers::SHIFT), "?");
        assert_eq!(key(KeyCode::BackTab, KeyModifiers::SHIFT), "shift+tab");
        assert_eq!(key(KeyCode::Up, KeyModifiers::ALT), "alt+up");
        assert_eq!(key(KeyCode::Char('4'), KeyModifiers::CONTROL), "ctrl+\\");
        assert_eq!(key(KeyCode::Char('7'), KeyModifiers::CONTROL), "ctrl+/");
        assert_eq!(key(KeyCode::Tab, KeyModifiers::CONTROL), "ctrl+tab");
        assert_eq!(
            key(
                KeyCode::BackTab,
                KeyModifiers::CONTROL | KeyModifiers::SHIFT
            ),
            "ctrl+shift+tab"
        );
    }

    #[test]
    fn hints_name_the_key_of_a_command() {
        let keymap = Keymap::defaults();
        assert_eq!(keymap.hint("switch.next").as_deref(), Some("alt+w"));
        assert_eq!(keymap.hint("tree.toggle").as_deref(), Some("alt+e"));
        assert_eq!(keymap.hint("picker.commands").as_deref(), Some("alt+p"));
        assert_eq!(keymap.hint("file.rename").as_deref(), Some("r"));
        assert_eq!(keymap.keys_in(Context::Tree, "file.rename"), ["r"]);
        assert!(keymap.keys_in(Context::Editor, "file.rename").is_empty());
        assert_eq!(keymap.keys_in(Context::Tree, "picker.files"), ["ctrl+p"]);
    }

    #[test]
    fn every_command_has_one_default_key_and_none_needs_ctrl_shift() {
        let mut bound = std::collections::HashSet::new();
        for (_, key, command) in DEFAULTS {
            assert!(!key.contains("ctrl+shift+"), "{key} needs Ctrl+Shift");
            assert!(bound.insert(*command), "{command} has more than one key");
        }
    }

    #[test]
    fn written_keys_are_normalized() {
        assert_eq!(normalize("Ctrl+Shift+P").unwrap(), "ctrl+shift+p");
        assert_eq!(normalize("shift+alt+Escape").unwrap(), "alt+shift+esc");
        assert_eq!(normalize("ctrl++").unwrap(), "ctrl++");
        assert!(normalize("hyper+x").is_err());
    }

    #[test]
    fn parsed_keys_round_trip() {
        for name in [
            "ctrl+p",
            "alt+shift+down",
            "shift+tab",
            "f12",
            "ctrl+tab",
            "alt+tab",
            "ctrl+shift+tab",
            "ctrl+\\",
            "ctrl+/",
            "space",
            "enter",
            "x",
        ] {
            assert_eq!(key_name(&parse_key(name).unwrap()), name);
        }
        assert!(parse_key("ctrl+nope").is_err());
    }

    #[test]
    fn context_bindings_win_over_global_ones() {
        let mut keymap = Keymap::defaults();
        assert_eq!(
            keymap.lookup(Context::Editor, "ctrl+p"),
            Some("picker.files")
        );
        keymap.bind(Context::Editor, "ctrl+p", "custom").unwrap();
        assert_eq!(keymap.lookup(Context::Editor, "ctrl+p"), Some("custom"));
        assert_eq!(keymap.lookup(Context::Tree, "ctrl+p"), Some("picker.files"));
    }
}
