//! End-to-end tests: the real editor driven by key presses and mouse
//! clicks, checked against text snapshots of the screen in `snapshots/`.
//! How to write and troubleshoot them: `tests/e2e/README.md`.

mod compare;
mod definitions;
mod editing;
mod find;
mod git;
mod keys;
mod pickers;
mod plugins;
mod settings;
mod split;
mod tree;
mod update;
