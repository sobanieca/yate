//! Finds the file a path or a module specifier points to: plain paths (relative to the
//! current file or the project, absolute, `~/`, with an optional `:line:col`), and
//! JavaScript/TypeScript imports the way Node and TypeScript resolve them: extensions,
//! `index` files, `tsconfig.json`/`jsconfig.json` paths and baseUrl, `package.json`
//! `imports` and `exports`, and `node_modules`.

use crate::app::normalize;
use crate::json::{self, Json};
use std::path::{Path, PathBuf};

const EXTENSIONS: &[&str] = &[
    "ts", "tsx", "d.ts", "js", "jsx", "mjs", "cjs", "mts", "cts", "json", "vue", "svelte",
];
const JS_TO_TS: &[(&str, &[&str])] = &[
    ("js", &["ts", "tsx"]),
    ("jsx", &["tsx"]),
    ("mjs", &["mts"]),
    ("cjs", &["cts"]),
];
const CONDITIONS: &[&str] = &[
    "types", "import", "module", "browser", "node", "require", "default",
];
const MAX_EXTENDS: usize = 8;
const MAX_PATH_CHARS: usize = 1024;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Location {
    pub path: PathBuf,
    /// 0-based.
    pub line: Option<usize>,
    /// 0-based, in chars.
    pub col: Option<usize>,
}

/// Splits `path:line` and `path:line:col` (1-based) into the path and a
/// 0-based line and column.
pub fn split_location(text: &str) -> (&str, Option<usize>, Option<usize>) {
    let mut rest = text;
    let mut numbers = Vec::new();
    while numbers.len() < 2 {
        let Some((head, tail)) = rest.rsplit_once(':') else {
            break;
        };
        let Ok(number) = tail.parse::<usize>() else {
            break;
        };
        if head.is_empty() {
            break;
        }
        numbers.push(number);
        rest = head;
    }
    numbers.reverse();
    let line = numbers.first().map(|n| n.saturating_sub(1));
    let col = numbers.get(1).map(|n| n.saturating_sub(1));
    (rest, line, col)
}

/// Resolves a path someone wrote: `file:line:col`, relative to the folder of
/// `from`, to `root`, absolute or in `home`. With `modules`, a JavaScript or
/// TypeScript module specifier is resolved as an import from `from`.
pub fn resolve(
    text: &str,
    from: Option<&Path>,
    root: &Path,
    home: Option<&Path>,
    modules: bool,
) -> Option<Location> {
    let text = text.trim();
    let text = text.strip_prefix("file://").unwrap_or(text);
    if text.is_empty() || text.len() > MAX_PATH_CHARS || text.contains('\n') {
        return None;
    }
    let (path, line, col) = split_location(text);
    let at = |path: PathBuf| Location { path, line, col };
    let written = Path::new(path);
    let mut candidates = Vec::new();
    if let (Some(rest), Some(home)) = (path.strip_prefix("~/"), home) {
        candidates.push(home.join(rest));
    } else if written.is_absolute() {
        candidates.push(written.to_path_buf());
    } else {
        if let Some(dir) = from.and_then(Path::parent) {
            candidates.push(dir.join(written));
        }
        candidates.push(root.join(written));
    }
    if let Some(found) = candidates
        .iter()
        .map(|candidate| normalize(candidate))
        .find(|candidate| candidate.is_file())
    {
        return Some(at(found));
    }
    let from = from.filter(|_| modules)?;
    resolve_module(path, from).map(at)
}

/// Resolves an import specifier (`./view`, `@/lib/date`, `#config`, `react`)
/// from the file `importer`.
pub fn resolve_module(specifier: &str, importer: &Path) -> Option<PathBuf> {
    let dir = importer.parent()?;
    let specifier = specifier
        .split_once('?')
        .map_or(specifier, |(path, _)| path);
    if specifier.is_empty() {
        return None;
    }
    let relative = specifier == "."
        || specifier == ".."
        || specifier.starts_with("./")
        || specifier.starts_with("../");
    if relative {
        return file_or_directory(&normalize(&dir.join(specifier)));
    }
    if Path::new(specifier).is_absolute() {
        return file_or_directory(&normalize(Path::new(specifier)));
    }
    if specifier.starts_with('#')
        && let Some(found) = package_imports(specifier, dir)
    {
        return Some(found);
    }
    if let Some(found) = tsconfig_paths(specifier, dir) {
        return Some(found);
    }
    node_modules(specifier, dir)
}

fn file_or_directory(path: &Path) -> Option<PathBuf> {
    file(path).or_else(|| directory(path))
}

fn file(path: &Path) -> Option<PathBuf> {
    if path.is_file() {
        return Some(path.to_path_buf());
    }
    let name = path.file_name()?.to_str()?;
    if let Some((stem, extension)) = name.rsplit_once('.')
        && let Some((_, replacements)) = JS_TO_TS.iter().find(|(js, _)| *js == extension)
        && let Some(found) = replacements
            .iter()
            .map(|ts| path.with_file_name(format!("{stem}.{ts}")))
            .find(|candidate| candidate.is_file())
    {
        return Some(found);
    }
    EXTENSIONS
        .iter()
        .map(|extension| path.with_file_name(format!("{name}.{extension}")))
        .find(|candidate| candidate.is_file())
}

fn directory(path: &Path) -> Option<PathBuf> {
    if !path.is_dir() {
        return None;
    }
    if let Some(package) = read_json(&path.join("package.json")) {
        for field in ["types", "typings", "module", "main"] {
            if let Some(entry) = package.get(field).and_then(Json::as_str) {
                let entry = normalize(&path.join(entry));
                if let Some(found) = file(&entry).or_else(|| file(&entry.join("index"))) {
                    return Some(found);
                }
            }
        }
    }
    file(&path.join("index"))
}

fn read_json(path: &Path) -> Option<Json> {
    let text = std::fs::read_to_string(path).ok()?;
    json::parse(&text).ok()
}

/// Targets of an `exports`/`imports` value, most preferred first.
fn condition_targets(value: &Json, targets: &mut Vec<String>) {
    match value {
        Json::String(target) => {
            if !targets.contains(target) {
                targets.push(target.clone());
            }
        }
        Json::Array(values) => {
            for value in values {
                condition_targets(value, targets);
            }
        }
        Json::Object(_) => {
            for condition in CONDITIONS {
                if let Some(value) = value.get(condition) {
                    condition_targets(value, targets);
                }
            }
        }
        _ => {}
    }
}

/// The targets for `key` in a subpath map (`exports`, `imports` or
/// tsconfig `paths`): an exact key, or the `*` pattern with the longest
/// prefix, with `*` replaced.
fn subpath_targets<'a>(
    entries: impl Iterator<Item = (&'a str, &'a Json)>,
    key: &str,
) -> Vec<String> {
    let mut best: Option<(usize, &str, &Json)> = None;
    for (pattern, value) in entries {
        if pattern == key {
            best = Some((usize::MAX, "", value));
            break;
        }
        if let Some((prefix, suffix)) = pattern.split_once('*')
            && key.len() >= prefix.len() + suffix.len()
            && key.starts_with(prefix)
            && key.ends_with(suffix)
            && best.is_none_or(|(length, _, _)| prefix.len() > length)
        {
            best = Some((
                prefix.len(),
                &key[prefix.len()..key.len() - suffix.len()],
                value,
            ));
        }
    }
    let Some((_, star, value)) = best else {
        return Vec::new();
    };
    let mut targets = Vec::new();
    condition_targets(value, &mut targets);
    targets
        .into_iter()
        .map(|target| target.replace('*', star))
        .collect()
}

fn first_existing(base: &Path, targets: Vec<String>) -> Option<PathBuf> {
    targets
        .iter()
        .find_map(|target| file_or_directory(&normalize(&base.join(target))))
}

fn package_imports(specifier: &str, dir: &Path) -> Option<PathBuf> {
    let package_dir = dir
        .ancestors()
        .find(|ancestor| ancestor.join("package.json").is_file())?;
    let package = read_json(&package_dir.join("package.json"))?;
    let imports = package.get("imports")?;
    let entries = imports.entries().iter().map(|(k, v)| (k.as_str(), v));
    first_existing(package_dir, subpath_targets(entries, specifier))
}

struct TsConfig {
    base_url: Option<PathBuf>,
    paths: Option<(PathBuf, Json)>,
}

fn find_tsconfig(dir: &Path) -> Option<PathBuf> {
    dir.ancestors().find_map(|ancestor| {
        ["tsconfig.json", "jsconfig.json"]
            .iter()
            .map(|name| ancestor.join(name))
            .find(|candidate| candidate.is_file())
    })
}

fn load_tsconfig(path: &Path, depth: usize) -> Option<TsConfig> {
    let config = read_json(path)?;
    let dir = path.parent()?;
    let mut result = TsConfig {
        base_url: None,
        paths: None,
    };
    if depth < MAX_EXTENDS {
        let extends: Vec<&str> = match config.get("extends") {
            Some(Json::String(parent)) => vec![parent.as_str()],
            Some(Json::Array(parents)) => parents.iter().filter_map(Json::as_str).collect(),
            _ => Vec::new(),
        };
        for parent in extends {
            if let Some(parent) = extended_config(parent, dir)
                .and_then(|parent_path| load_tsconfig(&parent_path, depth + 1))
            {
                result.base_url = parent.base_url.or(result.base_url);
                result.paths = parent.paths.or(result.paths);
            }
        }
    }
    let options = config.get("compilerOptions");
    if let Some(base_url) = options
        .and_then(|o| o.get("baseUrl"))
        .and_then(Json::as_str)
    {
        result.base_url = Some(normalize(&dir.join(base_url)));
    }
    if let Some(paths) = options.and_then(|o| o.get("paths")) {
        result.paths = Some((dir.to_path_buf(), paths.clone()));
    }
    Some(result)
}

fn extended_config(parent: &str, dir: &Path) -> Option<PathBuf> {
    let candidates = |base: PathBuf| {
        let mut with_json = base.clone().into_os_string();
        with_json.push(".json");
        [
            base.clone(),
            PathBuf::from(with_json),
            base.join("tsconfig.json"),
        ]
    };
    if parent.starts_with('.') || Path::new(parent).is_absolute() {
        return candidates(normalize(&dir.join(parent)))
            .into_iter()
            .find(|candidate| candidate.is_file());
    }
    dir.ancestors().find_map(|ancestor| {
        candidates(ancestor.join("node_modules").join(parent))
            .into_iter()
            .find(|candidate| candidate.is_file())
    })
}

fn tsconfig_paths(specifier: &str, dir: &Path) -> Option<PathBuf> {
    let config = load_tsconfig(&find_tsconfig(dir)?, 0)?;
    if let Some((paths_dir, paths)) = &config.paths {
        let base = config.base_url.as_deref().unwrap_or(paths_dir);
        let entries = paths.entries().iter().map(|(k, v)| (k.as_str(), v));
        if let Some(found) = first_existing(base, subpath_targets(entries, specifier)) {
            return Some(found);
        }
    }
    file_or_directory(&normalize(&config.base_url?.join(specifier)))
}

/// `@scope/name/sub/path` → (`@scope/name`, `sub/path`).
fn split_package(specifier: &str) -> Option<(&str, &str)> {
    let split_at = if specifier.starts_with('@') {
        let scope_end = specifier.find('/')?;
        specifier[scope_end + 1..]
            .find('/')
            .map(|i| scope_end + 1 + i)
    } else {
        specifier.find('/')
    };
    let (name, subpath) = match split_at {
        Some(i) => (&specifier[..i], &specifier[i + 1..]),
        None => (specifier, ""),
    };
    (!name.is_empty() && !name.ends_with('/')).then_some((name, subpath))
}

fn node_modules(specifier: &str, dir: &Path) -> Option<PathBuf> {
    let (name, subpath) = split_package(specifier)?;
    let types_name = format!(
        "@types/{}",
        name.trim_start_matches('@').replacen('/', "__", 1)
    );
    for package in [name, types_name.as_str()] {
        for ancestor in dir.ancestors() {
            let package_dir = ancestor.join("node_modules").join(package);
            if package_dir.is_dir()
                && let Some(found) = package_entry(&package_dir, subpath)
            {
                return Some(found);
            }
        }
    }
    None
}

fn package_entry(package_dir: &Path, subpath: &str) -> Option<PathBuf> {
    let package = read_json(&package_dir.join("package.json"));
    if let Some(exports) = package.as_ref().and_then(|p| p.get("exports")) {
        let key = if subpath.is_empty() {
            ".".to_string()
        } else {
            format!("./{subpath}")
        };
        let subpath_map = exports
            .entries()
            .first()
            .is_some_and(|(k, _)| k.starts_with('.'));
        let targets = if subpath_map {
            let entries = exports.entries().iter().map(|(k, v)| (k.as_str(), v));
            subpath_targets(entries, &key)
        } else if key == "." {
            let mut targets = Vec::new();
            condition_targets(exports, &mut targets);
            targets
        } else {
            Vec::new()
        };
        if let Some(found) = first_existing(package_dir, targets) {
            return Some(found);
        }
    }
    if subpath.is_empty() {
        directory(package_dir)
    } else {
        file_or_directory(&normalize(&package_dir.join(subpath)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Project(PathBuf);

    impl Project {
        fn new(name: &str, files: &[(&str, &str)]) -> Project {
            let root =
                std::env::temp_dir().join(format!("yate-resolve-{name}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&root);
            for (path, content) in files {
                let path = root.join(path);
                std::fs::create_dir_all(path.parent().unwrap()).unwrap();
                std::fs::write(path, content).unwrap();
            }
            Project(root)
        }

        fn resolve(&self, specifier: &str, importer: &str) -> Option<String> {
            resolve_module(specifier, &self.0.join(importer)).map(|found| {
                found
                    .strip_prefix(&self.0)
                    .unwrap()
                    .to_string_lossy()
                    .into_owned()
            })
        }
    }

    impl Drop for Project {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn locations_split_off_line_and_column() {
        assert_eq!(
            split_location("src/a.rs:12:5"),
            ("src/a.rs", Some(11), Some(4))
        );
        assert_eq!(split_location("src/a.rs:3"), ("src/a.rs", Some(2), None));
        assert_eq!(split_location("src/a.rs"), ("src/a.rs", None, None));
        assert_eq!(split_location("C:1"), ("C", Some(0), None));
    }

    #[test]
    fn relative_imports_try_extensions_and_index_files() {
        let project = Project::new(
            "relative",
            &[
                ("src/app.ts", ""),
                ("src/view.tsx", ""),
                ("src/lib/index.ts", ""),
                ("src/util.ts", ""),
                ("src/data.json", "{}"),
            ],
        );
        assert_eq!(
            project.resolve("./view", "src/app.ts").unwrap(),
            "src/view.tsx"
        );
        assert_eq!(
            project.resolve("./lib", "src/app.ts").unwrap(),
            "src/lib/index.ts"
        );
        assert_eq!(
            project.resolve("./util.js", "src/app.ts").unwrap(),
            "src/util.ts"
        );
        assert_eq!(
            project.resolve("../src/data.json", "src/app.ts").unwrap(),
            "src/data.json"
        );
        assert_eq!(project.resolve("./missing", "src/app.ts"), None);
    }

    #[test]
    fn tsconfig_paths_and_base_url_follow_extends() {
        let project = Project::new(
            "tsconfig",
            &[
                (
                    "tsconfig.base.json",
                    r#"{
                    // comments are allowed
                    "compilerOptions": { "baseUrl": "src", "paths": { "@/*": ["./*"], "config": ["./settings/index.ts"] }, },
                }"#,
                ),
                ("tsconfig.json", r#"{ "extends": "./tsconfig.base.json" }"#),
                ("src/components/button.tsx", ""),
                ("src/settings/index.ts", ""),
                ("src/services/api.ts", ""),
                ("src/app.ts", ""),
            ],
        );
        assert_eq!(
            project
                .resolve("@/components/button", "src/app.ts")
                .unwrap(),
            "src/components/button.tsx"
        );
        assert_eq!(
            project.resolve("config", "src/app.ts").unwrap(),
            "src/settings/index.ts"
        );
        assert_eq!(
            project.resolve("services/api", "src/app.ts").unwrap(),
            "src/services/api.ts"
        );
    }

    #[test]
    fn packages_resolve_through_exports_types_and_main() {
        let project = Project::new(
            "packages",
            &[
                (
                    "package.json",
                    r##"{ "imports": { "#db/*": "./src/db/*.ts" } }"##,
                ),
                ("src/db/users.ts", ""),
                ("src/app.ts", ""),
                (
                    "node_modules/@acme/ui/package.json",
                    r#"{ "exports": { ".": { "import": "./dist/index.mjs", "types": "./dist/index.d.ts" }, "./button": "./dist/button.js" } }"#,
                ),
                ("node_modules/@acme/ui/dist/index.d.ts", ""),
                ("node_modules/@acme/ui/dist/index.mjs", ""),
                ("node_modules/@acme/ui/dist/button.js", ""),
                (
                    "node_modules/left-pad/package.json",
                    r#"{ "main": "lib/pad" }"#,
                ),
                ("node_modules/left-pad/lib/pad.js", ""),
                ("node_modules/@types/lodash/index.d.ts", ""),
            ],
        );
        assert_eq!(
            project.resolve("#db/users", "src/app.ts").unwrap(),
            "src/db/users.ts"
        );
        assert_eq!(
            project.resolve("@acme/ui", "src/app.ts").unwrap(),
            "node_modules/@acme/ui/dist/index.d.ts"
        );
        assert_eq!(
            project.resolve("@acme/ui/button", "src/app.ts").unwrap(),
            "node_modules/@acme/ui/dist/button.js"
        );
        assert_eq!(
            project.resolve("left-pad", "src/app.ts").unwrap(),
            "node_modules/left-pad/lib/pad.js"
        );
        assert_eq!(
            project.resolve("lodash", "src/app.ts").unwrap(),
            "node_modules/@types/lodash/index.d.ts"
        );
    }

    #[test]
    fn plain_paths_resolve_from_the_file_then_the_project() {
        let project = Project::new(
            "plain",
            &[
                ("docs/guide.md", ""),
                ("docs/intro.md", ""),
                ("README.md", ""),
            ],
        );
        let from = project.0.join("docs/guide.md");
        let found = resolve("intro.md:4", Some(&from), &project.0, None, false).unwrap();
        assert_eq!(found.path, project.0.join("docs/intro.md"));
        assert_eq!(found.line, Some(3));
        let found = resolve("README.md", Some(&from), &project.0, None, false).unwrap();
        assert_eq!(found.path, project.0.join("README.md"));
        assert_eq!(
            resolve("./intro", Some(&from), &project.0, None, false),
            None
        );
    }
}
