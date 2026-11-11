//! `yate.json` settings: theme, options, key bindings and plugins.
//!
//! yate reads `~/yate.json`, then every `yate.json` in the folders from the
//! top down to the working directory (and the project root, when it is below
//! the working directory). `yate.jsonc` is read too, after `yate.json` in the
//! same folder. Later files override earlier ones key by key, so the file
//! closest to where yate runs wins. The files are data only: they never run
//! code, so a project can ship one safely. The `plugins` section only picks
//! plugins by name; their code comes from the config folder.

use crate::config::{OptionValue, Paths};
use crate::json::{self, Json};
use crate::keymap::Context;
use std::path::{Path, PathBuf};

pub const FILE_NAMES: [&str; 2] = ["yate.json", "yate.jsonc"];

/// The file that "Open yate.json" creates when there is none.
pub const NEW_FILE_NAME: &str = "yate.jsonc";

const TEMPLATE_START: &str = r##"{
  // yate settings. A yate.json or yate.jsonc closer to the working directory
  // overrides this file key by key. Comments and trailing commas are allowed.
  "theme": {
    // tokyonight-storm, tokyonight-night, tokyonight-moon, tokyonight-day,
    // gruvbox-dark, gruvbox-light, nord or solarized-light
    "preset": "tokyonight-storm",
    // Palette colors: "bg": "#1a1b26", "blue": "#7aa2f7"...
    "colors": {},
    // Syntax scopes: "keyword": "magenta", "comment": { "fg": "#565f89", "italic": true }
    "syntax": {}
  },
  // Same names as yate.setOption: "tabSize": 4, "nerdIcons": true, "keyHintDelay": 500...
  "options": {},
  // Every key binding, by context: global (everywhere), editor and tree.
  // A context binding wins over a global one. Change a key or a command id here;
  // "" removes a key. Some keys open a menu of commands (ctrl+g, ctrl+w...);
  // binding one of those commands to a key of its own skips the menu.
  "keys": {
"##;

const TEMPLATE_END: &str = r##"  },
  // Plugins by name: true loads ~/.config/yate/plugins/<name>.js; false turns
  // one off, also the bundled git and format.
  "plugins": {}
}
"##;

/// The commented `yate.json` that "Open yate.json" creates, with every
/// default key binding listed so it can be changed in place.
pub fn template() -> String {
    let mut text = TEMPLATE_START.to_string();
    let commands = crate::app::builtin_commands();
    let title = |id: &str| {
        commands
            .iter()
            .find(|(cid, _)| *cid == id)
            .map(|(_, title)| title.trim_end_matches('…'))
            .unwrap_or("")
    };
    let keymap = crate::keymap::Keymap::defaults();
    for (i, context) in Context::ALL.iter().enumerate() {
        text.push_str(&format!("    \"{}\": {{\n", context.name()));
        let bindings: Vec<_> = crate::keymap::DEFAULTS
            .iter()
            .filter(|(c, _, _)| c == context)
            .collect();
        for (j, (_, key, command)) in bindings.iter().enumerate() {
            let comma = if j + 1 < bindings.len() { "," } else { "" };
            let entry = format!("\"{}\": \"{command}\"{comma}", json_escape(key));
            text.push_str(&format!("      {entry:<44} // {}\n", title(command)));
        }
        let comma = if i + 1 < Context::ALL.len() { "," } else { "" };
        text.push_str(&format!("    }}{comma}\n"));
    }
    let unbound: Vec<&str> = commands
        .iter()
        .map(|(id, _)| *id)
        .filter(|id| keymap.keys_for(id).is_empty())
        .collect();
    text.push_str("    // Commands without a key:\n");
    let mut line = String::from("    //");
    for id in unbound {
        if line.len() + id.len() + 1 > 88 {
            text.push_str(&line);
            text.push('\n');
            line = String::from("    //");
        }
        line.push(' ');
        line.push_str(id);
    }
    text.push_str(&line);
    text.push('\n');
    text.push_str(TEMPLATE_END);
    text
}

fn json_escape(text: &str) -> String {
    text.replace('\\', "\\\\").replace('"', "\\\"")
}

pub fn is_settings_file(path: &Path) -> bool {
    path.file_name()
        .is_some_and(|name| FILE_NAMES.iter().any(|file_name| name == *file_name))
}

pub struct Settings {
    /// The files read, lowest priority first.
    pub files: Vec<PathBuf>,
    /// All files merged.
    pub merged: Json,
    pub errors: Vec<String>,
}

/// The `yate.json` files that apply, lowest priority first.
pub fn files(paths: &Paths, root: &Path) -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = Vec::new();
    for start in [paths.cwd.as_path(), root] {
        for dir in start.ancestors() {
            let below_ceiling = paths.ceiling.as_ref().is_none_or(|c| dir.starts_with(c));
            let above_home = paths
                .home
                .as_ref()
                .is_some_and(|home| home.starts_with(dir));
            if below_ceiling && !above_home && !dirs.iter().any(|d| d == dir) {
                dirs.push(dir.to_path_buf());
            }
        }
    }
    dirs.sort_by_key(|dir| dir.components().count());
    let mut files: Vec<PathBuf> = Vec::new();
    for dir in paths.home.iter().chain(&dirs) {
        for name in FILE_NAMES {
            let file = dir.join(name);
            if !files.contains(&file) {
                files.push(file);
            }
        }
    }
    files.retain(|file| file.is_file());
    files
}

/// A path for messages: relative to the project root, `~/` for the home
/// folder, absolute otherwise.
pub fn display_path(paths: &Paths, root: &Path, path: &Path) -> String {
    if let Ok(relative) = path.strip_prefix(root) {
        return relative.to_string_lossy().into_owned();
    }
    if let Some(home) = &paths.home
        && let Ok(relative) = path.strip_prefix(home)
    {
        return format!("~/{}", relative.to_string_lossy());
    }
    path.to_string_lossy().into_owned()
}

pub fn load(paths: &Paths, root: &Path) -> Settings {
    let mut merged = Json::Object(Vec::new());
    let mut errors = Vec::new();
    let mut read = Vec::new();
    for file in files(paths, root) {
        let parsed = std::fs::read_to_string(&file)
            .map_err(|err| err.to_string())
            .and_then(|text| json::parse(&text));
        match parsed {
            Ok(value @ Json::Object(_)) => {
                merged.merge(value);
                read.push(file);
            }
            Ok(other) => errors.push(format!(
                "{}: expected an object, not {}",
                display_path(paths, root, &file),
                other.type_name()
            )),
            Err(err) => errors.push(format!("{}: {err}", display_path(paths, root, &file))),
        }
    }
    Settings {
        files: read,
        merged,
        errors,
    }
}

impl Settings {
    /// Options to set, with problems of the `options` section.
    pub fn options(&self, errors: &mut Vec<String>) -> Vec<(String, OptionValue)> {
        let mut options = Vec::new();
        for (name, value) in self
            .merged
            .get("options")
            .map(Json::entries)
            .unwrap_or_default()
        {
            let value = match value {
                Json::Bool(flag) => OptionValue::Bool(*flag),
                Json::Number(number) => OptionValue::Number(*number),
                Json::String(text) => OptionValue::Text(text.clone()),
                other => {
                    errors.push(format!("options.{name}: unexpected {}", other.type_name()));
                    continue;
                }
            };
            options.push((name.clone(), value));
        }
        options
    }

    /// Key bindings: context, key and command (empty to remove the key).
    pub fn keys(&self, errors: &mut Vec<String>) -> Vec<(Context, String, String)> {
        let mut keys = Vec::new();
        for (context_name, bindings) in self
            .merged
            .get("keys")
            .map(Json::entries)
            .unwrap_or_default()
        {
            let Some(context) = Context::parse(context_name) else {
                errors.push(format!(
                    "keys.{context_name}: unknown context (known: global, editor, tree)"
                ));
                continue;
            };
            for (key, command) in bindings.entries() {
                match command {
                    Json::String(command) => keys.push((context, key.clone(), command.clone())),
                    Json::Null => keys.push((context, key.clone(), String::new())),
                    other => errors.push(format!(
                        "keys.{context_name}.{key}: expected a command id, not {}",
                        other.type_name()
                    )),
                }
            }
        }
        keys
    }

    /// Plugins switched on (`true`) or off (`false`) by name.
    pub fn plugins(&self, errors: &mut Vec<String>) -> Vec<(String, bool)> {
        let mut plugins = Vec::new();
        for (name, value) in self
            .merged
            .get("plugins")
            .map(Json::entries)
            .unwrap_or_default()
        {
            let plain_name = !name.is_empty()
                && name
                    .chars()
                    .all(|c| c.is_alphanumeric() || matches!(c, '-' | '_'));
            match value {
                _ if !plain_name => errors.push(format!(
                    "plugins.{name}: a plugin name is a file name in ~/.config/yate/plugins without .js"
                )),
                Json::Bool(enabled) => plugins.push((name.clone(), *enabled)),
                other => errors.push(format!(
                    "plugins.{name}: expected true or false, not {}",
                    other.type_name()
                )),
            }
        }
        plugins
    }

    pub fn unknown_sections(&self) -> Vec<String> {
        self.merged
            .entries()
            .iter()
            .filter(|(key, _)| !matches!(key.as_str(), "theme" | "options" | "keys" | "plugins"))
            .map(|(key, _)| {
                format!("{key}: unknown setting (known: theme, options, keys, plugins)")
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Dirs(PathBuf);

    impl Drop for Dirs {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn setup(name: &str) -> (Dirs, Paths) {
        let base =
            std::env::temp_dir().join(format!("yate-settings-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(base.join("home/code/project/app")).unwrap();
        let paths = Paths {
            config_dir: base.join("config"),
            state_dir: base.join("state"),
            home: Some(base.join("home")),
            cwd: base.join("home/code/project/app"),
            ceiling: Some(base.clone()),
        };
        (Dirs(base), paths)
    }

    #[test]
    fn closer_files_override_farther_ones() {
        let (dirs, paths) = setup("cascade");
        let home = dirs.0.join("home");
        std::fs::write(
            home.join("yate.json"),
            r#"{"options": {"tabSize": 2, "colorize": false}}"#,
        )
        .unwrap();
        std::fs::write(
            home.join("code/project/yate.json"),
            r#"{"options": {"tabSize": 4}}"#,
        )
        .unwrap();
        std::fs::write(
            home.join("code/project/app/yate.json"),
            r#"{"options": {"tabSize": 8}, "x": 1}"#,
        )
        .unwrap();
        let settings = load(&paths, &paths.cwd);
        assert_eq!(settings.files.len(), 3);
        assert!(settings.files[0].ends_with("home/yate.json"));
        let mut errors = Vec::new();
        let options = settings.options(&mut errors);
        let option = |name: &str| {
            options
                .iter()
                .rev()
                .find(|(n, _)| n == name)
                .map(|(_, v)| format!("{v:?}"))
        };
        assert_eq!(option("tabSize").as_deref(), Some("Number(8.0)"));
        assert_eq!(option("colorize").as_deref(), Some("Bool(false)"));
        assert_eq!(
            settings.unknown_sections(),
            ["x: unknown setting (known: theme, options, keys, plugins)"]
        );
    }

    #[test]
    fn a_project_root_below_the_working_directory_counts_too() {
        let (dirs, mut paths) = setup("root");
        let home = dirs.0.join("home");
        paths.cwd = home.join("code");
        std::fs::write(home.join("code/project/yate.json"), "{}").unwrap();
        std::fs::write(home.join("code/project/app/yate.json"), "{}").unwrap();
        let files = files(&paths, &home.join("code/project"));
        assert_eq!(files, vec![home.join("code/project/yate.json")]);
    }

    #[test]
    fn yate_jsonc_is_read_after_yate_json_in_the_same_folder() {
        let (dirs, paths) = setup("jsonc");
        let home = dirs.0.join("home");
        std::fs::write(home.join("yate.jsonc"), r#"{"options": {"tabSize": 3}}"#).unwrap();
        std::fs::write(
            home.join("code/project/yate.json"),
            r#"{"options": {"tabSize": 4}}"#,
        )
        .unwrap();
        std::fs::write(
            home.join("code/project/yate.jsonc"),
            "{\n  // closer\n  \"options\": {\"tabSize\": 8,},\n}",
        )
        .unwrap();
        let settings = load(&paths, &paths.cwd);
        assert_eq!(
            settings.files,
            [
                home.join("yate.jsonc"),
                home.join("code/project/yate.json"),
                home.join("code/project/yate.jsonc"),
            ]
        );
        let options = settings.options(&mut Vec::new());
        assert_eq!(
            options.last().map(|(_, v)| format!("{v:?}")).as_deref(),
            Some("Number(8.0)")
        );
    }

    #[test]
    fn plugins_are_switched_by_plain_names() {
        let settings = Settings {
            files: Vec::new(),
            merged: json::parse(
                r#"{"plugins": {"sort-lines": true, "git": false, "../evil": true, "x": 1}}"#,
            )
            .unwrap(),
            errors: Vec::new(),
        };
        let mut errors = Vec::new();
        assert_eq!(
            settings.plugins(&mut errors),
            [("sort-lines".to_string(), true), ("git".to_string(), false)]
        );
        assert_eq!(
            errors,
            [
                "plugins.../evil: a plugin name is a file name in ~/.config/yate/plugins without .js",
                "plugins.x: expected true or false, not a number",
            ]
        );
    }

    #[test]
    fn broken_files_are_reported_with_their_path() {
        let (dirs, paths) = setup("broken");
        std::fs::write(dirs.0.join("home/yate.json"), "{\n  \"theme\": \n}").unwrap();
        let settings = load(&paths, &paths.cwd);
        assert!(settings.files.is_empty());
        assert_eq!(
            settings.errors,
            [
                "~/yate.json: line 3, column 1: expected a value (string, number, true, false, null, object or array)"
            ]
        );
    }

    #[test]
    fn the_template_is_valid() {
        let template = json::parse(&template()).unwrap();
        let settings = Settings {
            files: Vec::new(),
            merged: template,
            errors: Vec::new(),
        };
        let mut errors = Vec::new();
        settings.keys(&mut errors);
        settings.options(&mut errors);
        settings.plugins(&mut errors);
        assert!(errors.is_empty() && settings.unknown_sections().is_empty());
        let (_, theme_errors) = crate::ui::theme::Theme::from_json(settings.merged.get("theme"));
        assert!(theme_errors.is_empty(), "{theme_errors:?}");
    }
}
