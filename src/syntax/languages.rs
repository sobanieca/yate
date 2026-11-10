//! The languages yate knows: how to detect them, their tree-sitter grammar,
//! highlight query and comment tokens. Grammars are compiled into the binary.

use std::path::Path;
use std::sync::OnceLock;
use tree_sitter::{Language, Query};
use tree_sitter_language::LanguageFn;

pub struct LanguageConfig {
    pub name: &'static str,
    pub extensions: &'static [&'static str],
    pub filenames: &'static [&'static str],
    pub shebangs: &'static [&'static str],
    pub line_comment: Option<&'static str>,
    pub block_comment: Option<(&'static str, &'static str)>,
    grammar: LanguageFn,
    queries: &'static [&'static str],
    compiled: OnceLock<Option<Highlighting>>,
    definitions: OnceLock<Option<super::definitions::DefinitionQuery>>,
}

pub struct Highlighting {
    pub language: Language,
    pub query: Query,
}

impl LanguageConfig {
    pub fn language(&self) -> Language {
        Language::new(self.grammar)
    }

    /// The compiled highlight query. Compiled on first use, `None` when the
    /// query does not compile.
    pub fn highlighting(&self) -> Option<&Highlighting> {
        self.compiled
            .get_or_init(|| match self.compile() {
                Ok(highlighting) => Some(highlighting),
                Err(message) => {
                    crate::log::write(&format!("highlight query of {}: {message}", self.name));
                    None
                }
            })
            .as_ref()
    }

    /// The compiled definition queries, `None` when the language has none.
    pub fn definitions(&self) -> Option<&super::definitions::DefinitionQuery> {
        self.definitions
            .get_or_init(
                || match super::definitions::compile(self.name, &self.language())? {
                    Ok(query) => Some(query),
                    Err(message) => {
                        crate::log::write(&format!("definition query of {}: {message}", self.name));
                        None
                    }
                },
            )
            .as_ref()
    }

    pub fn compile(&self) -> Result<Highlighting, String> {
        let language = self.language();
        let source = self.queries.join("\n");
        let query = Query::new(&language, &source).map_err(|err| format!("{err:?}"))?;
        Ok(Highlighting { language, query })
    }
}

macro_rules! language {
    (
        $name:literal, $grammar:expr, [$($query:expr),*],
        ext: [$($ext:literal),*], files: [$($file:literal),*], shebang: [$($shebang:literal),*],
        line: $line:expr, block: $block:expr
    ) => {
        LanguageConfig {
            name: $name,
            extensions: &[$($ext),*],
            filenames: &[$($file),*],
            shebangs: &[$($shebang),*],
            line_comment: $line,
            block_comment: $block,
            grammar: $grammar,
            queries: &[$($query),*],
            compiled: OnceLock::new(),
            definitions: OnceLock::new(),
        }
    };
}

const C_BLOCK: Option<(&str, &str)> = Some(("/*", "*/"));
const HTML_BLOCK: Option<(&str, &str)> = Some(("<!--", "-->"));

pub fn all() -> &'static [LanguageConfig] {
    static LANGUAGES: OnceLock<Vec<LanguageConfig>> = OnceLock::new();
    LANGUAGES.get_or_init(|| {
        vec![
            language!("javascript", tree_sitter_javascript::LANGUAGE,
                [tree_sitter_javascript::JSX_HIGHLIGHT_QUERY, tree_sitter_javascript::HIGHLIGHT_QUERY],
                ext: ["js", "mjs", "cjs", "jsx"], files: [], shebang: ["node", "deno", "bun"],
                line: Some("//"), block: C_BLOCK),
            language!("typescript", tree_sitter_typescript::LANGUAGE_TYPESCRIPT,
                [tree_sitter_typescript::HIGHLIGHTS_QUERY, tree_sitter_javascript::HIGHLIGHT_QUERY],
                ext: ["ts", "mts", "cts"], files: [], shebang: ["ts-node", "tsx"],
                line: Some("//"), block: C_BLOCK),
            language!("tsx", tree_sitter_typescript::LANGUAGE_TSX,
                [tree_sitter_typescript::HIGHLIGHTS_QUERY, tree_sitter_javascript::JSX_HIGHLIGHT_QUERY,
                 tree_sitter_javascript::HIGHLIGHT_QUERY],
                ext: ["tsx"], files: [], shebang: [],
                line: Some("//"), block: C_BLOCK),
            language!("json", tree_sitter_json::LANGUAGE, [tree_sitter_json::HIGHLIGHTS_QUERY],
                ext: ["json", "jsonc", "json5", "webmanifest"], files: [".prettierrc", ".eslintrc", ".babelrc", "flake.lock"], shebang: [],
                line: Some("//"), block: C_BLOCK),
            language!("css", tree_sitter_css::LANGUAGE, [tree_sitter_css::HIGHLIGHTS_QUERY],
                ext: ["css", "scss", "less", "pcss"], files: [], shebang: [],
                line: None, block: C_BLOCK),
            language!("html", tree_sitter_html::LANGUAGE, [tree_sitter_html::HIGHLIGHTS_QUERY],
                ext: ["html", "htm", "xhtml", "vue"], files: [], shebang: [],
                line: None, block: HTML_BLOCK),
            language!("svelte", tree_sitter_svelte_ng::LANGUAGE, [tree_sitter_svelte_ng::HIGHLIGHTS_QUERY],
                ext: ["svelte"], files: [], shebang: [],
                line: None, block: HTML_BLOCK),
            language!("markdown", tree_sitter_md::LANGUAGE, [tree_sitter_md::HIGHLIGHT_QUERY_BLOCK],
                ext: ["md", "markdown", "mdx"], files: [], shebang: [],
                line: None, block: HTML_BLOCK),
            language!("yaml", tree_sitter_yaml::LANGUAGE, [tree_sitter_yaml::HIGHLIGHTS_QUERY],
                ext: ["yml", "yaml"], files: [".clang-format"], shebang: [],
                line: Some("#"), block: None),
            language!("toml", tree_sitter_toml_ng::LANGUAGE, [tree_sitter_toml_ng::HIGHLIGHTS_QUERY],
                ext: ["toml"], files: ["Cargo.lock", "uv.lock", "poetry.lock"], shebang: [],
                line: Some("#"), block: None),
            language!("rust", tree_sitter_rust::LANGUAGE, [tree_sitter_rust::HIGHLIGHTS_QUERY],
                ext: ["rs"], files: [], shebang: [],
                line: Some("//"), block: C_BLOCK),
            language!("go", tree_sitter_go::LANGUAGE, [tree_sitter_go::HIGHLIGHTS_QUERY],
                ext: ["go"], files: [], shebang: [],
                line: Some("//"), block: C_BLOCK),
            language!("python", tree_sitter_python::LANGUAGE, [tree_sitter_python::HIGHLIGHTS_QUERY],
                ext: ["py", "pyi", "pyw"], files: [], shebang: ["python", "python3"],
                line: Some("#"), block: None),
            language!("bash", tree_sitter_bash::LANGUAGE, [tree_sitter_bash::HIGHLIGHT_QUERY],
                ext: ["sh", "bash", "zsh", "bats"], files: [".bashrc", ".bash_profile", ".profile", ".zshrc", ".envrc", "bashrcx", "PKGBUILD"],
                shebang: ["sh", "bash", "zsh", "dash"],
                line: Some("#"), block: None),
            language!("c", tree_sitter_c::LANGUAGE, [tree_sitter_c::HIGHLIGHT_QUERY],
                ext: ["c", "h"], files: [], shebang: [],
                line: Some("//"), block: C_BLOCK),
            language!("cpp", tree_sitter_cpp::LANGUAGE,
                [tree_sitter_cpp::HIGHLIGHT_QUERY, tree_sitter_c::HIGHLIGHT_QUERY],
                ext: ["cc", "cpp", "cxx", "hpp", "hh", "hxx", "ino"], files: [], shebang: [],
                line: Some("//"), block: C_BLOCK),
            language!("java", tree_sitter_java::LANGUAGE, [tree_sitter_java::HIGHLIGHTS_QUERY],
                ext: ["java"], files: [], shebang: [],
                line: Some("//"), block: C_BLOCK),
            language!("csharp", tree_sitter_c_sharp::LANGUAGE, [tree_sitter_c_sharp::HIGHLIGHTS_QUERY],
                ext: ["cs", "csx"], files: [], shebang: [],
                line: Some("//"), block: C_BLOCK),
            language!("lua", tree_sitter_lua::LANGUAGE, [tree_sitter_lua::HIGHLIGHTS_QUERY],
                ext: ["lua"], files: [], shebang: ["lua"],
                line: Some("--"), block: None),
            language!("ruby", tree_sitter_ruby::LANGUAGE, [tree_sitter_ruby::HIGHLIGHTS_QUERY],
                ext: ["rb", "rake", "gemspec"], files: ["Gemfile", "Rakefile", "Brewfile"], shebang: ["ruby"],
                line: Some("#"), block: None),
            language!("php", tree_sitter_php::LANGUAGE_PHP, [tree_sitter_php::HIGHLIGHTS_QUERY],
                ext: ["php"], files: [], shebang: ["php"],
                line: Some("//"), block: C_BLOCK),
            language!("sql", tree_sitter_sequel::LANGUAGE, [tree_sitter_sequel::HIGHLIGHTS_QUERY],
                ext: ["sql"], files: [], shebang: [],
                line: Some("--"), block: C_BLOCK),
            language!("zig", tree_sitter_zig::LANGUAGE, [tree_sitter_zig::HIGHLIGHTS_QUERY],
                ext: ["zig", "zon"], files: [], shebang: [],
                line: Some("//"), block: None),
            language!("swift", tree_sitter_swift::LANGUAGE, [tree_sitter_swift::HIGHLIGHTS_QUERY],
                ext: ["swift"], files: [], shebang: [],
                line: Some("//"), block: C_BLOCK),
            language!("elixir", tree_sitter_elixir::LANGUAGE, [tree_sitter_elixir::HIGHLIGHTS_QUERY],
                ext: ["ex", "exs", "heex"], files: [], shebang: ["elixir"],
                line: Some("#"), block: None),
            language!("haskell", tree_sitter_haskell::LANGUAGE, [tree_sitter_haskell::HIGHLIGHTS_QUERY],
                ext: ["hs"], files: [], shebang: [],
                line: Some("--"), block: Some(("{-", "-}"))),
            language!("scala", tree_sitter_scala::LANGUAGE, [tree_sitter_scala::HIGHLIGHTS_QUERY],
                ext: ["scala", "sc", "sbt"], files: [], shebang: [],
                line: Some("//"), block: C_BLOCK),
            language!("dart", tree_sitter_dart::LANGUAGE, [tree_sitter_dart::HIGHLIGHTS_QUERY],
                ext: ["dart"], files: [], shebang: [],
                line: Some("//"), block: C_BLOCK),
            language!("gleam", tree_sitter_gleam::LANGUAGE, [tree_sitter_gleam::HIGHLIGHT_QUERY],
                ext: ["gleam"], files: [], shebang: [],
                line: Some("//"), block: None),
            language!("nix", tree_sitter_nix::LANGUAGE, [tree_sitter_nix::HIGHLIGHTS_QUERY],
                ext: ["nix"], files: [], shebang: [],
                line: Some("#"), block: C_BLOCK),
            language!("graphql", tree_sitter_graphql::LANGUAGE, [include_str!("../../queries/graphql.scm")],
                ext: ["graphql", "gql", "graphqls"], files: [], shebang: [],
                line: Some("#"), block: None),
            language!("prisma", tree_sitter_prisma_io::LANGUAGE, [include_str!("../../queries/prisma.scm")],
                ext: ["prisma"], files: [], shebang: [],
                line: Some("//"), block: None),
            language!("hcl", tree_sitter_hcl::LANGUAGE, [include_str!("../../queries/hcl.scm")],
                ext: ["tf", "tfvars", "hcl", "nomad"], files: [".terraformrc", "terraform.rc"], shebang: [],
                line: Some("#"), block: C_BLOCK),
            language!("xml", tree_sitter_xml::LANGUAGE_XML, [tree_sitter_xml::XML_HIGHLIGHT_QUERY],
                ext: ["xml", "svg", "xsd", "xsl", "plist", "csproj", "fsproj", "props"], files: [], shebang: [],
                line: None, block: HTML_BLOCK),
            language!("make", tree_sitter_make::LANGUAGE, [tree_sitter_make::HIGHLIGHTS_QUERY],
                ext: ["mk", "mak"], files: ["Makefile", "makefile", "GNUmakefile"], shebang: ["make"],
                line: Some("#"), block: None),
            language!("diff", tree_sitter_diff::LANGUAGE, [tree_sitter_diff::HIGHLIGHTS_QUERY],
                ext: ["diff", "patch"], files: ["COMMIT_EDITMSG"], shebang: [],
                line: Some("#"), block: None),
        ]
    })
}

pub fn by_name(name: &str) -> Option<&'static LanguageConfig> {
    all().iter().find(|language| language.name == name)
}

/// Detects the language from the file name, then from a `#!` line.
pub fn detect(path: &Path, first_line: &str) -> Option<&'static LanguageConfig> {
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("");
    if let Some(language) = all()
        .iter()
        .find(|language| language.filenames.contains(&file_name))
    {
        return Some(language);
    }
    if file_name.starts_with("Dockerfile") || file_name.starts_with(".env") {
        return by_name("bash");
    }
    if let Some(extension) = path.extension().and_then(|ext| ext.to_str()) {
        let extension = extension.to_ascii_lowercase();
        if let Some(language) = all()
            .iter()
            .find(|language| language.extensions.contains(&extension.as_str()))
        {
            return Some(language);
        }
    }
    detect_shebang(first_line)
}

fn detect_shebang(first_line: &str) -> Option<&'static LanguageConfig> {
    let command = first_line.strip_prefix("#!")?;
    let mut words = command.split_whitespace();
    let mut program = words.next()?.rsplit('/').next()?;
    if program == "env" {
        program = words.find(|word| !word.starts_with('-'))?;
    }
    let program = program.trim_end_matches(|c: char| c.is_ascii_digit() || c == '.');
    all().iter().find(|language| {
        language
            .shebangs
            .iter()
            .any(|shebang| shebang.trim_end_matches(|c: char| c.is_ascii_digit()) == program)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_highlight_query_compiles() {
        let failures: Vec<String> = all()
            .iter()
            .filter_map(|language| {
                language
                    .compile()
                    .err()
                    .map(|err| format!("{}: {err}", language.name))
            })
            .collect();
        assert!(failures.is_empty(), "{failures:#?}");
    }

    #[test]
    fn languages_are_detected_from_names_and_shebangs() {
        let name = |path: &str, line: &str| detect(Path::new(path), line).map(|l| l.name);
        assert_eq!(name("src/app.tsx", ""), Some("tsx"));
        assert_eq!(name("Makefile", ""), Some("make"));
        assert_eq!(name("gblame", "#!/bin/bash"), Some("bash"));
        assert_eq!(name("tool", "#!/usr/bin/env node"), Some("javascript"));
        assert_eq!(name("tool", "#!/usr/bin/env python3"), Some("python"));
        assert_eq!(name("notes.txt", ""), None);
    }
}
