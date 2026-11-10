//! Definitions in a syntax tree, found with the grammar's tags query (`@name` and
//! `@definition.*` captures), extra queries in `queries/definitions/`, and for
//! JavaScript and TypeScript the locals query (`@local.scope`, `@local.definition`).
//! Also reads ES `import` and `export` statements, so go to definition can follow an
//! imported name to the file that defines it.

use super::languages::LanguageConfig;
use std::ops::Range;
use tree_sitter::{Language, Node, Query, QueryCursor, StreamingIterator, Tree};

const ES_EXTRA: &str = include_str!("../../queries/definitions/ecmascript.scm");
const JS_EXTRA: &str = include_str!("../../queries/definitions/javascript.scm");
const TS_EXTRA: &str = include_str!("../../queries/definitions/typescript.scm");
const RUST_EXTRA: &str = include_str!("../../queries/definitions/rust.scm");

/// Nodes that wrap a definition without being a scope of their own.
const WRAPPERS: &[&str] = &[
    "export_statement",
    "decorated_definition",
    "lexical_declaration",
    "variable_declaration",
    "expression_statement",
    "template_declaration",
];

fn sources(language: &str) -> &'static [&'static str] {
    match language {
        "javascript" => &[
            tree_sitter_javascript::TAGS_QUERY,
            ES_EXTRA,
            JS_EXTRA,
            tree_sitter_javascript::LOCALS_QUERY,
        ],
        "typescript" | "tsx" => &[
            tree_sitter_typescript::TAGS_QUERY,
            tree_sitter_javascript::TAGS_QUERY,
            ES_EXTRA,
            TS_EXTRA,
            tree_sitter_javascript::LOCALS_QUERY,
            tree_sitter_typescript::LOCALS_QUERY,
        ],
        "rust" => &[tree_sitter_rust::TAGS_QUERY, RUST_EXTRA],
        "go" => &[tree_sitter_go::TAGS_QUERY],
        "python" => &[tree_sitter_python::TAGS_QUERY],
        "c" => &[tree_sitter_c::TAGS_QUERY],
        "cpp" => &[tree_sitter_cpp::TAGS_QUERY],
        "java" => &[tree_sitter_java::TAGS_QUERY],
        "csharp" => &[tree_sitter_c_sharp::TAGS_QUERY],
        "lua" => &[tree_sitter_lua::TAGS_QUERY],
        "ruby" => &[tree_sitter_ruby::TAGS_QUERY],
        "php" => &[tree_sitter_php::TAGS_QUERY],
        "elixir" => &[tree_sitter_elixir::TAGS_QUERY],
        "dart" => &[tree_sitter_dart::TAGS_QUERY],
        "gleam" => &[tree_sitter_gleam::TAGS_QUERY],
        "swift" => &[tree_sitter_swift::TAGS_QUERY],
        _ => &[],
    }
}

/// Languages whose definitions can stand for each other: a TypeScript
/// file can call a function defined in JavaScript.
pub fn family(language: &str) -> &str {
    match language {
        "javascript" | "typescript" | "tsx" | "svelte" | "html" => "javascript",
        "c" | "cpp" => "c",
        other => other,
    }
}

pub fn uses_es_modules(language: &str) -> bool {
    family(language) == "javascript"
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Role {
    Name,
    Definition,
    LocalDefinition,
    LocalScope,
    Other,
}

pub struct DefinitionQuery {
    query: Query,
    roles: Vec<Role>,
    has_scopes: bool,
}

/// `None` when the language has no definition queries; an error when they
/// do not compile.
pub fn compile(
    language_name: &str,
    language: &Language,
) -> Option<Result<DefinitionQuery, String>> {
    let sources = sources(language_name);
    if sources.is_empty() {
        return None;
    }
    let query = match Query::new(language, &sources.join("\n")) {
        Ok(query) => query,
        Err(err) => return Some(Err(format!("{err:?}"))),
    };
    let roles: Vec<Role> = query
        .capture_names()
        .iter()
        .map(|name| match *name {
            "name" => Role::Name,
            "local.definition" => Role::LocalDefinition,
            "local.scope" => Role::LocalScope,
            name if name.starts_with("definition.") => Role::Definition,
            _ => Role::Other,
        })
        .collect();
    let has_scopes = roles.contains(&Role::LocalScope);
    Some(Ok(DefinitionQuery {
        query,
        roles,
        has_scopes,
    }))
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Definition {
    pub name: String,
    /// A local variable or parameter, not worth looking for in other files.
    pub local: bool,
    /// Byte range of the name.
    pub name_range: Range<usize>,
    /// Byte range where the name is visible.
    pub container: Range<usize>,
}

/// The definitions of `name` in a parsed text, in text order.
pub fn collect(language: &LanguageConfig, tree: &Tree, text: &str, name: &str) -> Vec<Definition> {
    let Some(definitions) = language.definitions() else {
        return Vec::new();
    };
    let root = tree.root_node();
    let mut scopes = Vec::new();
    let mut found: Vec<(Node, Node, bool)> = Vec::new();
    let mut cursor = QueryCursor::new();
    let mut matches = cursor.matches(&definitions.query, root, text.as_bytes());
    while let Some(found_match) = matches.next() {
        let mut name_node = None;
        let mut definition_node = None;
        for capture in found_match.captures {
            match definitions.roles[capture.index as usize] {
                Role::Name => name_node = Some(capture.node),
                Role::Definition => definition_node = Some(capture.node),
                Role::LocalScope => scopes.push(capture.node.byte_range()),
                Role::LocalDefinition if node_text(capture.node, text) == name => {
                    found.push((capture.node, capture.node, true));
                }
                _ => {}
            }
        }
        if let (Some(name_node), Some(definition_node)) = (name_node, definition_node)
            && node_text(name_node, text) == name
        {
            found.push((name_node, definition_node, false));
        }
    }
    found.sort_by_key(|(name_node, _, local)| (name_node.start_byte(), *local));
    found.dedup_by_key(|(name_node, _, _)| name_node.byte_range());
    found
        .into_iter()
        .map(|(name_node, definition_node, local)| Definition {
            name: name.to_string(),
            local,
            name_range: name_node.byte_range(),
            container: if definitions.has_scopes {
                scope_of(definition_node, &scopes, root.byte_range())
            } else {
                container_of(definition_node, root.byte_range())
            },
        })
        .collect()
}

fn node_text<'a>(node: Node, text: &'a str) -> &'a str {
    text.get(node.byte_range()).unwrap_or("")
}

fn scope_of(definition: Node, scopes: &[Range<usize>], root: Range<usize>) -> Range<usize> {
    let range = definition.byte_range();
    scopes
        .iter()
        .filter(|scope| scope.start <= range.start && range.end <= scope.end && **scope != range)
        .min_by_key(|scope| scope.len())
        .cloned()
        .unwrap_or(root)
}

fn container_of(definition: Node, root: Range<usize>) -> Range<usize> {
    let mut node = definition;
    while let Some(parent) = node.parent() {
        if !WRAPPERS.contains(&parent.kind()) {
            return parent.byte_range();
        }
        node = parent;
    }
    root
}

/// The definition a reference at byte `at` means: the one in the innermost
/// scope around it, the closest before it in that scope, otherwise the first.
pub fn locate(definitions: &[Definition], at: usize) -> Option<&Definition> {
    let visible = definitions
        .iter()
        .filter(|d| d.container.start <= at && at <= d.container.end);
    visible
        .min_by_key(|d| {
            let start = d.name_range.start;
            let distance = if start <= at {
                (0, at - start)
            } else {
                (1, start)
            };
            (d.container.len(), distance)
        })
        .or_else(|| definitions.first())
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Imported {
    Named(String),
    Default,
    Namespace,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Import {
    pub source: String,
    pub imported: Imported,
}

fn string_value(node: Node, text: &str) -> String {
    node_text(node, text)
        .trim_matches(|c| c == '"' || c == '\'' || c == '`')
        .to_string()
}

fn named_children(node: Node) -> Vec<Node> {
    let mut cursor = node.walk();
    node.named_children(&mut cursor).collect()
}

/// The `import` statement that binds `name` in a JavaScript or TypeScript
/// file.
pub fn import_of(tree: &Tree, text: &str, name: &str) -> Option<Import> {
    for statement in named_children(tree.root_node()) {
        if statement.kind() != "import_statement" {
            continue;
        }
        let Some(source) = statement.child_by_field_name("source") else {
            continue;
        };
        let source = string_value(source, text);
        let Some(clause) = named_children(statement)
            .into_iter()
            .find(|child| child.kind() == "import_clause")
        else {
            continue;
        };
        for part in named_children(clause) {
            let imported = match part.kind() {
                "identifier" if node_text(part, text) == name => Some(Imported::Default),
                "namespace_import" => named_children(part)
                    .into_iter()
                    .any(|child| node_text(child, text) == name)
                    .then_some(Imported::Namespace),
                "named_imports" => named_children(part)
                    .into_iter()
                    .filter(|specifier| specifier.kind() == "import_specifier")
                    .find_map(|specifier| {
                        let imported = specifier.child_by_field_name("name")?;
                        let local = specifier.child_by_field_name("alias").unwrap_or(imported);
                        (node_text(local, text) == name)
                            .then(|| Imported::Named(string_value(imported, text)))
                    }),
                _ => None,
            };
            if let Some(imported) = imported {
                return Some(Import { source, imported });
            }
        }
    }
    None
}

/// Where an exported name of a module is.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Export {
    /// Defined in this file, at this byte.
    Here(usize),
    /// Exported from another module under this name.
    From(Import),
    /// `export * from`: maybe in one of these modules.
    Star(Vec<String>),
}

/// Finds `imported` among the exports of a JavaScript or TypeScript file.
pub fn find_export(
    language: &LanguageConfig,
    tree: &Tree,
    text: &str,
    imported: &Imported,
) -> Option<Export> {
    let root = tree.root_node();
    let statements: Vec<Node> = named_children(root)
        .into_iter()
        .filter(|s| s.kind() == "export_statement")
        .collect();
    let top_level = |name: &str| {
        collect(language, tree, text, name)
            .into_iter()
            .filter(|d| !d.local)
            .min_by_key(|d| (d.container != root.byte_range(), d.name_range.start))
            .map(|d| Export::Here(d.name_range.start))
    };
    let name = match imported {
        Imported::Namespace => return None,
        Imported::Default => {
            let statement = statements.iter().find(|s| {
                let mut cursor = s.walk();
                s.children(&mut cursor).any(|c| c.kind() == "default")
            })?;
            if let Some(value) = statement.child_by_field_name("value")
                && value.kind() == "identifier"
            {
                return top_level(node_text(value, text))
                    .or(Some(Export::Here(statement.start_byte())));
            }
            let at = statement
                .child_by_field_name("declaration")
                .and_then(|d| d.child_by_field_name("name"))
                .map_or(statement.start_byte(), |n| n.start_byte());
            return Some(Export::Here(at));
        }
        Imported::Named(name) => name.as_str(),
    };
    let mut stars = Vec::new();
    for statement in &statements {
        let source = statement
            .child_by_field_name("source")
            .map(|s| string_value(s, text));
        let clause = named_children(*statement)
            .into_iter()
            .find(|c| c.kind() == "export_clause");
        let Some(clause) = clause else {
            if let Some(source) = source {
                stars.push(source);
            }
            continue;
        };
        for specifier in named_children(clause) {
            let Some(local) = specifier.child_by_field_name("name") else {
                continue;
            };
            let exported = specifier.child_by_field_name("alias").unwrap_or(local);
            if string_value(exported, text) != name {
                continue;
            }
            let local_name = string_value(local, text);
            return match &source {
                Some(source) => Some(Export::From(Import {
                    source: source.clone(),
                    imported: if local_name == "default" {
                        Imported::Default
                    } else {
                        Imported::Named(local_name)
                    },
                })),
                None => top_level(&local_name).or(Some(Export::Here(specifier.start_byte()))),
            };
        }
    }
    top_level(name).or_else(|| (!stars.is_empty()).then_some(Export::Star(stars)))
}

/// The string literal around byte `at`, without its quotes.
pub fn string_at(tree: &Tree, text: &str, at: usize) -> Option<String> {
    let mut node = tree.root_node().descendant_for_byte_range(at, at)?;
    if !node.kind().contains("string") {
        let quote = matches!(node.kind(), "\"" | "'" | "`");
        node = node
            .parent()
            .filter(|p| quote && p.kind().contains("string"))?;
    }
    while let Some(parent) = node.parent().filter(|p| p.kind().contains("string")) {
        node = parent;
    }
    let literal = node_text(node, text);
    let start = literal.find(['"', '\'', '`'])? + 1;
    let end = literal.rfind(['"', '\'', '`'])?;
    let value = literal.get(start..end)?;
    (!value.is_empty() && !value.contains('\n')).then(|| value.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::syntax::languages::by_name;

    fn parse(language: &str, text: &str) -> (&'static LanguageConfig, Tree) {
        let language = by_name(language).unwrap();
        let tree = crate::syntax::parse_text(language, text).unwrap();
        (language, tree)
    }

    fn lookup(language: &str, text: &str, name: &str, at: &str) -> Option<usize> {
        let (config, tree) = parse(language, text);
        let at = text.find(at).unwrap();
        let definitions = collect(config, &tree, text, name);
        locate(&definitions, at).map(|d| d.name_range.start)
    }

    #[test]
    fn every_definition_query_compiles() {
        let failures: Vec<String> = crate::syntax::languages::all()
            .iter()
            .filter_map(
                |language| match compile(language.name, &language.language()) {
                    Some(Err(err)) => Some(format!("{}: {err}", language.name)),
                    _ => None,
                },
            )
            .collect();
        assert!(failures.is_empty(), "{failures:#?}");
    }

    #[test]
    fn the_innermost_definition_wins() {
        let text = "const x = 1;\nfunction f(x) {\n  return x;\n}\nconsole.log(x);\n";
        assert_eq!(
            lookup("javascript", text, "x", "x;\n}"),
            Some(text.find("(x)").unwrap() + 1)
        );
        assert_eq!(lookup("javascript", text, "x", "x);\n"), Some(6));
        assert_eq!(
            lookup("typescript", "type A = {};\nlet a: A;\n", "A", "A;"),
            Some(5)
        );
    }

    #[test]
    fn rust_items_and_methods_are_found() {
        let text = "struct S;\nimpl S {\n    fn go(&self) {}\n}\nconst MAX: u8 = 1;\nfn main() { S.go(); MAX; }\n";
        assert_eq!(
            lookup("rust", text, "go", "go();"),
            Some(text.find("go(&").unwrap())
        );
        assert_eq!(
            lookup("rust", text, "MAX", "MAX; }"),
            Some(text.find("MAX:").unwrap())
        );
    }

    #[test]
    fn imports_are_read_with_their_original_names() {
        let text = "import React, { useState as useS } from 'react';\nimport * as path from \"node:path\";\n";
        let (_, tree) = parse("typescript", text);
        assert_eq!(
            import_of(&tree, text, "useS"),
            Some(Import {
                source: "react".into(),
                imported: Imported::Named("useState".into())
            })
        );
        assert_eq!(
            import_of(&tree, text, "React").unwrap().imported,
            Imported::Default
        );
        assert_eq!(
            import_of(&tree, text, "path").unwrap().imported,
            Imported::Namespace
        );
        assert_eq!(import_of(&tree, text, "other"), None);
    }

    #[test]
    fn exports_are_found_directly_and_through_reexports() {
        let text = "export { a as b } from './a';\nexport * from './c';\nexport default function main() {}\nexport const value = 1;\n";
        let (config, tree) = parse("typescript", text);
        let named = |name: &str| find_export(config, &tree, text, &Imported::Named(name.into()));
        assert_eq!(
            named("b"),
            Some(Export::From(Import {
                source: "./a".into(),
                imported: Imported::Named("a".into())
            }))
        );
        assert_eq!(
            named("value"),
            Some(Export::Here(text.find("value").unwrap()))
        );
        assert_eq!(named("other"), Some(Export::Star(vec!["./c".into()])));
        assert_eq!(
            find_export(config, &tree, text, &Imported::Default),
            Some(Export::Here(text.find("main").unwrap()))
        );
    }

    #[test]
    fn strings_are_found_around_the_cursor() {
        let text = "import x from './lib/x';\nconst y = `${z}`;\n";
        let (_, tree) = parse("javascript", text);
        assert_eq!(
            string_at(&tree, text, text.find("lib").unwrap()),
            Some("./lib/x".into())
        );
        assert_eq!(
            string_at(&tree, text, text.find("'./").unwrap()),
            Some("./lib/x".into())
        );
        assert_eq!(string_at(&tree, text, text.find("z}").unwrap()), None);
    }
}
