//! yate, yet another text editor: a modeless terminal editor with a file
//! tree, fuzzy file search, live grep, go to definition and JavaScript plugins.
//!
//! The binary is `src/main.rs`; the library exists so the end-to-end tests
//! in `tests/e2e` can drive the editor through [`testing::Harness`].

pub mod app;
pub mod bookmarks;
pub mod clipboard;
pub mod config;
pub mod editor;
pub mod json;
pub mod keymap;
pub mod log;
pub mod picker;
pub mod plugins;
pub mod settings;
pub mod syntax;
pub mod terminal;
pub mod testing;
pub mod text;
pub mod ui;
pub mod update;
pub mod workspace;

pub const HELP: &str = include_str!("help.txt");
