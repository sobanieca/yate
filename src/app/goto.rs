//! Go to definition (F12, Alt+D, Ctrl+click). What is under the cursor decides:
//! a string or a `path:line:col` opens that file (ES imports are resolved like Node
//! and TypeScript do); a name jumps to its definition in this file, in the module it
//! is imported from, or anywhere in the project (a picker when there are several).

use super::{App, AppEvent, Overlay, commands};
use crate::editor::selection::Selection;
use crate::picker::{Picker, PickerItem, PickerKind};
use crate::syntax::definitions::{self, Export, Import, Imported};
use crate::syntax::languages::{self, LanguageConfig};
use crate::text;
use crate::workspace::definitions::{Found, search};
use crate::workspace::resolve;
use std::path::{Path, PathBuf};
use tree_sitter::Tree;

const MAX_REEXPORTS: usize = 6;

pub struct DefinitionSearch {
    pub generation: u64,
    pub name: String,
    pub at_definition: bool,
    pub found: Vec<Found>,
}

struct Context {
    text: String,
    tree: Option<Tree>,
    language: Option<&'static LanguageConfig>,
    path: Option<PathBuf>,
}

impl Context {
    fn es_modules(&self) -> bool {
        self.language
            .is_some_and(|language| definitions::uses_es_modules(language.name))
    }
}

fn is_path_char(c: char) -> bool {
    c.is_alphanumeric() || "_-./~@+#%:".contains(c)
}

/// The run of path characters around `col`, without trailing punctuation.
fn path_at(line: &[char], col: usize) -> Option<String> {
    let mut start = col.min(line.len());
    while start > 0 && is_path_char(line[start - 1]) {
        start -= 1;
    }
    let mut end = col.min(line.len());
    while end < line.len() && is_path_char(line[end]) {
        end += 1;
    }
    let run: String = line[start..end].iter().collect();
    let run = run.trim_end_matches(['.', ',', ':', ';']);
    (!run.is_empty()).then(|| run.to_string())
}

pub fn goto_definition(app: &mut App) {
    app.doc_mut().refresh_syntax();
    let doc = app.doc();
    let context = Context {
        text: doc.text(),
        tree: doc.syntax.as_ref().and_then(|s| s.tree()).cloned(),
        language: doc.language,
        path: doc.path.clone(),
    };
    let selected = doc.selected_text();
    let selected = selected.trim();
    if !selected.is_empty() && !selected.contains('\n') {
        let selected = selected.to_string();
        if open_path(app, &context, &selected, true) {
            return;
        }
        if selected.chars().all(text::is_word_char) {
            let at = app.doc().rope.char_to_byte(app.doc().selection.start());
            goto_name(app, &context, &selected, at);
        } else {
            missing_file(app, &selected);
        }
        return;
    }
    let cursor = doc.rope.char_to_byte(doc.selection.head);
    if let Some(tree) = &context.tree
        && let Some(string) = definitions::string_at(tree, &context.text, cursor)
    {
        if !open_path(app, &context, &string, true) {
            missing_file(app, &string);
        }
        return;
    }
    let (line, col) = doc.line_col(doc.selection.head);
    let chars = doc.line_chars(line);
    let line_start = doc.line_start(line);
    if let Some(path) = path_at(&chars, col).filter(|p| p.contains(['/', '.']))
        && open_path(app, &context, &path, false)
    {
        return;
    }
    let Some((start, end)) = text::word_at(&chars, col) else {
        app.notify("No name or path under the cursor");
        return;
    };
    let name: String = chars[start..end].iter().collect();
    let at = app.doc().rope.char_to_byte(line_start + start);
    goto_name(app, &context, &name, at);
}

fn open_path(app: &mut App, context: &Context, written: &str, string: bool) -> bool {
    let modules = string && (context.es_modules() || written.starts_with('.'));
    let home = app.paths.home.clone();
    let Some(location) = resolve::resolve(
        written,
        context.path.as_deref(),
        &app.root,
        home.as_deref(),
        modules,
    ) else {
        return false;
    };
    open_at(
        app,
        &location.path,
        location.line,
        location.col.unwrap_or(0),
    );
    true
}

fn missing_file(app: &mut App, written: &str) {
    if written.contains(char::is_whitespace) {
        app.notify(format!("No file {written}"));
        return;
    }
    let query = written
        .trim_start_matches("./")
        .trim_start_matches("../")
        .trim_start_matches("~/")
        .trim_start_matches("@/");
    commands::open_files_picker_with(app, query);
}

fn open_at(app: &mut App, path: &Path, line: Option<usize>, col: usize) {
    if !app.open(path, line) {
        return;
    }
    if let Some(line) = line {
        let doc = app.doc_mut();
        let pos = doc.pos(line, col);
        doc.selection = Selection::point(pos);
    }
    app.scroll_to_cursor();
}

fn goto_name(app: &mut App, context: &Context, name: &str, at: usize) {
    let (Some(language), Some(tree)) = (context.language, &context.tree) else {
        commands::open_grep_picker_with(app, name);
        return;
    };
    if context.es_modules()
        && let Some(importer) = &context.path
        && let Some(import) = definitions::import_of(tree, &context.text, name)
    {
        follow_import(app, &import, importer);
        return;
    }
    if language.definitions().is_none() {
        commands::open_grep_picker_with(app, name);
        return;
    }
    let found = definitions::collect(language, tree, &context.text, name);
    let mut at_definition = false;
    if let Some(definition) = definitions::locate(&found, at) {
        if definition.name_range.start != at {
            app.push_jump();
            let doc = app.doc_mut();
            let pos = doc.rope.byte_to_char(definition.name_range.start);
            doc.selection = Selection::point(pos);
            doc.view.preferred_col = None;
            app.scroll_to_cursor();
            return;
        }
        if definition.local {
            app.notify(format!("This is the definition of {name}"));
            return;
        }
        at_definition = true;
    }
    let skip = context
        .path
        .clone()
        .filter(|_| at_definition)
        .map(|path| (path, app.doc().line_of(app.doc().rope.byte_to_char(at))));
    search_project(
        app,
        name,
        definitions::family(language.name),
        skip,
        at_definition,
    );
}

fn follow_import(app: &mut App, import: &Import, importer: &Path) {
    let Some(module) = resolve::resolve_module(&import.source, importer) else {
        app.error(format!("Cannot find module {}", import.source));
        return;
    };
    match find_in_module(&module, &import.imported, 0) {
        Some((path, line, col)) => open_at(app, &path, Some(line), col),
        None => open_at(app, &module, None, 0),
    }
}

/// Where `imported` is defined, starting at the module `file` and following
/// re-exports: path, line and column.
fn find_in_module(
    file: &Path,
    imported: &Imported,
    depth: usize,
) -> Option<(PathBuf, usize, usize)> {
    if depth > MAX_REEXPORTS {
        return None;
    }
    let text = std::fs::read_to_string(file).ok()?;
    let language = languages::detect(file, text.lines().next().unwrap_or(""))
        .filter(|language| definitions::uses_es_modules(language.name))?;
    let tree = crate::syntax::parse_text(language, &text)?;
    match definitions::find_export(language, &tree, &text, imported)? {
        Export::Here(byte) => {
            let line_start = text[..byte].rfind('\n').map_or(0, |i| i + 1);
            let line = text[..byte].matches('\n').count();
            Some((
                file.to_path_buf(),
                line,
                text[line_start..byte].chars().count(),
            ))
        }
        Export::From(import) => {
            let next = resolve::resolve_module(&import.source, file)?;
            find_in_module(&next, &import.imported, depth + 1).or(Some((next, 0, 0)))
        }
        Export::Star(sources) => sources.iter().find_map(|source| {
            let next = resolve::resolve_module(source, file)?;
            find_in_module(&next, imported, depth + 1)
        }),
    }
}

fn search_project(
    app: &mut App,
    name: &str,
    family: &str,
    skip: Option<(PathBuf, usize)>,
    at_definition: bool,
) {
    app.definition_generation += 1;
    app.pending_jobs += 1;
    let generation = app.definition_generation;
    let root = app.root.clone();
    let tx = app.tx.clone();
    let name = name.to_string();
    let family = family.to_string();
    std::thread::spawn(move || {
        let found = search(&root, &name, &family, skip.as_ref());
        let _ = tx.send(AppEvent::Definitions(DefinitionSearch {
            generation,
            name,
            at_definition,
            found,
        }));
    });
}

pub fn handle_definitions(app: &mut App, result: DefinitionSearch) {
    app.pending_jobs = app.pending_jobs.saturating_sub(1);
    if result.generation != app.definition_generation {
        return;
    }
    let name = result.name;
    match result.found.as_slice() {
        [] if result.at_definition => app.notify(format!("No other definition of {name}")),
        [] => app.error(format!("No definition of {name} found")),
        [only] => {
            let (path, line, col) = (only.path.clone(), only.line, only.col);
            open_at(app, &path, Some(line), col);
        }
        found => {
            let items = found
                .iter()
                .map(|f| PickerItem {
                    label: format!("{}:{}", f.relative, f.line + 1),
                    detail: f.text.clone(),
                    detail_match: Some(f.range),
                    path: Some(f.path.clone()),
                    line: Some(f.line),
                    col: f.col,
                    data: String::new(),
                })
                .collect();
            let title = format!("Definitions of {name}");
            let picker = Picker::new(PickerKind::Definitions, &title, items);
            app.overlay = Some(Overlay::Picker(Box::new(picker)));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paths_stop_at_quotes_brackets_and_trailing_punctuation() {
        let chars = |s: &str| s.chars().collect::<Vec<char>>();
        assert_eq!(
            path_at(&chars("see [docs](docs/guide.md)."), 14).as_deref(),
            Some("docs/guide.md")
        );
        assert_eq!(
            path_at(&chars("--> src/main.rs:12:5:"), 6).as_deref(),
            Some("src/main.rs:12:5")
        );
        assert_eq!(path_at(&chars("a  b"), 2), None);
    }
}
