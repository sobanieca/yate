//! File icons for the tree: Nerd Font glyphs when `nerdIcons` is on, plain
//! markers otherwise.

use super::surface::Rgb;
use super::theme::Theme;
use std::path::Path;

pub fn folder(open: bool, nerd: bool) -> &'static str {
    match (nerd, open) {
        (true, true) => "\u{f07c}",
        (true, false) => "\u{f07b}",
        (false, true) => "▾",
        (false, false) => "▸",
    }
}

pub fn file(t: &Theme, path: &Path, nerd: bool) -> (&'static str, Rgb) {
    if !nerd {
        return (" ", t.dark3);
    }
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
    let extension = path.extension().and_then(|e| e.to_str()).unwrap_or("");
    match (name, extension) {
        (_, "rs") => ("\u{e7a8}", t.orange),
        (_, "js" | "mjs" | "cjs") => ("\u{e74e}", t.yellow),
        (_, "jsx" | "tsx") => ("\u{e7ba}", t.cyan),
        (_, "ts" | "mts" | "cts") => ("\u{e628}", t.blue),
        (_, "json" | "jsonc") => ("\u{e60b}", t.yellow),
        (_, "md" | "markdown") => ("\u{e609}", t.fg_dark),
        (_, "html" | "htm") => ("\u{e736}", t.orange),
        (_, "css" | "scss" | "less") => ("\u{e749}", t.blue),
        (_, "py") => ("\u{e606}", t.yellow),
        (_, "go") => ("\u{e626}", t.cyan),
        (_, "sh" | "bash" | "zsh") => ("\u{f489}", t.green),
        (_, "lua") => ("\u{e620}", t.blue),
        (_, "toml" | "yaml" | "yml" | "ini") => ("\u{e615}", t.dark5),
        (_, "svelte") => ("\u{e697}", t.orange),
        (_, "vue") => ("\u{e6a0}", t.green),
        (_, "lock") => ("\u{f023}", t.dark5),
        ("Dockerfile", _) => ("\u{f308}", t.blue),
        (".gitignore" | ".gitattributes", _) => ("\u{e702}", t.orange),
        _ => ("\u{f15b}", t.dark5),
    }
}
