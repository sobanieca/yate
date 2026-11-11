# AGENTS.md

Guidance for AI coding agents (Claude Code and others) working in this repository.

## Project

yate (yet another text editor) is a modeless terminal text editor written in Rust and
shipped as one binary. It behaves like a basic VS Code: a file tree, Ctrl+P, live grep,
split view, go to definition without a language server, and JavaScript plugins
in QuickJS.

The current version is 0.5.0, an early release. The 0.x series is still settling, so prefer
the better design over compatibility: change keys, options or the plugin API when needed,
without shims, and document the change.

## Commands

Development tasks are cargo aliases in `.cargo/config.toml`, backed by the `xtask` crate:

```bash
cargo yate [path]       # run from source (`fast` profile: optimized, no LTO, incremental)
cargo verify            # fmt --check, clippy -D warnings, all tests (run before committing; CI runs it)
cargo e2e               # end-to-end tests only (screen snapshots)
cargo e2e -- tree       # end-to-end tests whose name contains "tree"
cargo update-snapshots  # accept the current screens as the e2e snapshots
cargo dist              # release binary in dist/yate (static musl when the target is installed)
```

Requirements: stable Rust 1.85+ (edition 2024), a C compiler for the grammar crates, and
git (the tests create git repositories).

## Repository layout

| Path                       | Contents                                                          |
| -------------------------- | ----------------------------------------------------------------- |
| `src/`                     | the editor (library in `lib.rs`, binary in `main.rs`)             |
| `tests/e2e/`               | end-to-end tests and their screen snapshots (`snapshots/`)        |
| `queries/`                 | tree-sitter highlight queries for grammars that ship none         |
| `queries/definitions/`     | extra tags queries for go to definition                           |
| `docs/index.html`          | landing page and manual (GitHub Pages)                            |
| `docs/plugins.md`, `docs/configuration.md` | plugin API and `yate.json` reference              |
| `examples/`                | an example `yate.jsonc` and example plugins                       |
| `install.sh`               | the installer served at `sobanieca.github.io/yate/install.sh`     |
| `xtask/`                   | development tasks behind the cargo aliases                        |
| `.github/workflows/`       | `publish.yml` (checks, binaries, release), `pages.yml` (site)     |

## Architecture

| Module                           | Job                                                                 |
| -------------------------------- | ------------------------------------------------------------------- |
| `src/main.rs`, `src/help.txt`    | arguments, `--help`, `--version`, `yate update`                     |
| `src/lib.rs`                     | the library, used by `main.rs` and the end-to-end tests             |
| `src/testing.rs`                 | the end-to-end test harness (`Harness`) and snapshot comparison     |
| `src/app/mod.rs`                 | `App` state, the event loop, plugin calls, opening files            |
| `src/app/commands.rs`            | the command table (`COMMANDS`), the command palette, the key menus and every built-in command |
| `src/app/input.rs`               | keys, mouse and paste, per focus (editor, find, tree) and overlay |
| `src/app/goto.rs`                | go to definition / file under the cursor                            |
| `src/app/panes.rs`               | the pane tree of a split: groups side by side or stacked, even sizes, resizes |
| `src/app/split.rs`               | split view commands (split, close, focus, resize), scroll sync and the recent file switcher |
| `src/editor/document.rs`         | `Document`: rope, syntax tree, history, selection, scroll, git signs |
| `src/editor/ops.rs`              | editing commands and cursor motions                                 |
| `src/editor/selection.rs`        | cursor and selection as char indices in the rope                    |
| `src/editor/history.rs`          | undo/redo transactions                                              |
| `src/syntax/`                    | languages (`languages.rs`), incremental parsing and highlighting, definitions and ES imports/exports (`definitions.rs`) |
| `src/workspace/`                 | git (child process), file index, file tree, live grep, glob filters, gutter diffs, path and module resolution (`resolve.rs`), project definition search |
| `src/picker.rs`                  | picker model and fuzzy matching (nucleo)                            |
| `src/plugins/`                   | QuickJS host, `prelude.js` (the `yate` API), bundled plugins in `builtin/` |
| `src/keymap.rs`                  | key names, default bindings, per-context lookup                     |
| `src/config.rs`                  | editor options and the directories yate reads and writes            |
| `src/settings.rs`, `src/json.rs` | `yate.json`/`yate.jsonc` lookup and merge; the JSONC parser         |
| `src/bookmarks.rs`               | line bookmarks, per project, in `~/.local/state/yate/bookmarks`     |
| `src/clipboard.rs`               | tmux buffers, OSC 52                                                |
| `src/update.rs`                  | `yate update` (self-update from GitHub releases), daily update check |
| `src/log.rs`                     | append-only error log in `~/.local/state/yate/yate.log`             |
| `src/text.rs`                    | display widths, grapheme boundaries and words                       |
| `src/ui/theme.rs`                | `Theme`: palette, presets, capture styles, `yate.json` theme section |
| `src/ui/`                        | `Surface` (cell grid), theme, one module per view                   |
| `src/terminal.rs`                | raw mode, alternate screen, diffed frame flush                      |

How the pieces fit:

- **One thread owns the state.** Background threads (input, file index, git status, live
  grep) send `AppEvent`s over one channel. The loop renders a frame, waits for an event and
  handles it.
- **One file per pane, no buffers.** `App::docs` holds only the documents on screen: one
  per file the panes of a split show (`App::panes`; the cursor and scroll of the focused pane
  live in its document, the other panes keep theirs in `Pane`). `App::open` shows a file in place of the one in the
  focused pane; a document no pane shows is dropped (`drop_hidden_docs`), and only its path,
  cursor and scroll stay in `App::recent` (Ctrl+P order, the Ctrl+O switcher). Anything that
  takes a file off the screen first calls `App::may_leave` (or `may_drop`), which asks to
  save or discard unsaved changes and then runs the action again.
- **All text changes go through `Document::apply`**, which records undo history and feeds
  tree-sitter `InputEdit`s, so the syntax tree is reparsed incrementally before the next frame.
- **Highlighting is computed per frame for the visible bytes only** with a `QueryCursor`
  byte range. For overlapping captures, inner nodes win, and the earlier pattern wins for the
  same node (the tree-sitter-highlight convention), so query files from grammar crates work.
- **Everything a key does is a command id** (`file.save`, `picker.grep`...). Key bindings,
  the command palette and plugins all run commands by id. Cursor motions and typing are
  handled directly in `app/input.rs`. Default bindings are `keymap::DEFAULTS`; the `yate.json`
  template (`settings::template`) and the keys in the Ctrl+H palette are generated from them.
- **Rendering** draws into a `Surface`; `Terminal::flush` writes only the cells that changed.
  Views read `App`; `App::prepare_frame` updates caches (syntax, git signs, find matches,
  picker preview) before drawing.
- **Configuration** loads in `App::load_config`: defaults, then the merged `yate.json` files
  (theme, options, keys, plugins), then the JavaScript plugins they switch on. Views take
  every color from `app.theme`; never hard-code a color. Syntax highlighting gets per-capture
  styles from `Theme::capture_styles` at draw time, so a theme change needs no reparse.
- **Plugins** see a snapshot of the text in the editor and queue `Action`s that the app applies after the
  call. Plugins are loaded only from `~/.config/yate/plugins`, never from the opened project;
  `yate.json` only names them.

### Adding a language

Add the grammar crate to `Cargo.toml` and an entry in `src/syntax/languages.rs`. When the
crate ships no highlight query, write one in `queries/`. For go to definition, list its tags
query (and extras in `queries/definitions/`) in `sources` in `src/syntax/definitions.rs`. The
tests `every_highlight_query_compiles` and `every_definition_query_compiles` check every
query. Update the language count and list in `README.md` and `docs/index.html` (`LANGS`).

## Testing

- Unit tests live next to the code (`#[cfg(test)]`).
- End-to-end tests live in `tests/e2e/` and drive the real editor in memory: keys, clicks,
  pastes, then a text snapshot of the screen in `tests/e2e/snapshots/<name>.txt`. Read
  `tests/e2e/README.md` before writing one.
- Every user-visible change gets an end-to-end test or an updated snapshot. Review changed
  snapshots in `git diff`; accept intended changes with `cargo update-snapshots`.
- A failing snapshot prints a diff and writes `<name>.actual.txt` next to the snapshot
  (git-ignored).
- Background work must report back through `AppEvent` and count in `App::pending_jobs`,
  otherwise the harness cannot wait for it.
- Screens must be deterministic: sort results that come from threads, show paths relative to
  the project root.
- Tests never go online: the update check is started only by `app::run`, and tests fake it
  with `update_check(...)`.

## Conventions

- Don't add comments to the code; use self-documenting names (module doc comments are fine).
- Keep dependencies few and modern: crossterm, ropey, tree-sitter and grammar crates,
  rquickjs, ignore, nucleo-matcher, similar, regex, unicode-width/segmentation, ureq (rustls,
  for updates only), flate2 (unpacking update downloads). git runs as a child process; never link a git library.
- UTF-8 only. No other encodings.
- Default keys are Ctrl and a letter only, so they work in every terminal, inside tmux and
  in Termux: no Alt, no Ctrl+Shift, no function keys, no Ctrl+B (tmux prefix), no Ctrl+I or
  Ctrl+M (Tab and Enter to a terminal), and never Ctrl+Tab or Shift+Tab as the only way to
  do something. A test in `src/keymap.rs` checks it.
- Keep the direct keys few: only what is pressed all the time (save, find, clipboard, undo)
  gets one. Everything else goes into a menu (`MenuSpec` in `src/app/commands.rs`: Ctrl+G go
  to, Ctrl+K bookmarks, Ctrl+W panes, Ctrl+T file tree, Ctrl+L lines), where a letter or an
  arrow picks the command, with Ctrl held or not. Do not use `i`, `m` or a digit as a menu
  key (Ctrl+I and Ctrl+M are Tab and Enter, terminals send no Ctrl+digit). Only commands on
  the arrow keys repeat (`repeats`): right after one, Ctrl and an arrow run it again, so a
  letter there would shadow a direct key.
- Bars and dialogs are forms, not key maps: an option is a checkbox, an action is a button,
  and Tab, the arrows, Space, Enter and clicks work them (the find bar: `FindField` and
  `ui::find_controls`; confirmations: `Prompt::choices` and `ui::prompt_buttons`; the tree
  filters: `TreeFilter` and `sidebar::filter_controls`). Do not
  give an option or a button of a form a key of its own.
- Messages go through `App::notify` and `App::error`. One that fits shows in the status
  line (the file info makes room for it); one that does not, or has several lines, opens in
  a dialog (`App::dialog`, `ui::dialog_view`) that Esc closes, so never shorten a message to
  make it fit. Messages from background work use `App::notify_quietly`, which never opens
  a dialog, so nothing takes the keys while the person types.
- Every command has a `Group` (its section in the palette; titles do not repeat it) and a
  `Scope` in `COMMANDS`. The command palette lists only what the focused panel can do right
  now: a command with nothing to work on gets a condition in `LISTED_WHEN`, and one that only
  makes sense as a key goes in `NOT_IN_PALETTE`. Keep the palette short.
- yate has no diff, compare or git review views, on purpose: git tools and coding agents do
  that better. Git support is the status colors in the tree and the signs in the gutter.
- When adding or changing keys or commands, update `src/help.txt`, `README.md` and, for the
  plugin API, `docs/plugins.md`; for `yate.json`, `docs/configuration.md`.

## Landing page

The landing page and manual live in `docs/index.html`, served at `sobanieca.github.io/yate/`.
Every time you add a feature or change keys, commands, options or the plugin API, update the
landing page too (the key reference table is the `REF` list in its script), so it always
reflects the current feature set.

The page supports a light and a dark theme: it follows `prefers-color-scheme`, and the header
toggle overrides it (`data-theme` on `<html>`, saved in `localStorage`). Every color is a CSS
custom property on `:root`, redefined for light in both the `prefers-color-scheme: light`
block and the `:root[data-theme="light"]` block; keep the two blocks identical. Never
hard-code a color in a rule; add a token for both themes. The theme preset previews are the
exception: they show each preset's own colors on purpose. Check new sections in both themes.

## Releases

The repository is `sobanieca/yate` on GitHub. Every push to `main` runs `cargo verify`,
builds binaries for Linux (musl, static; x64 and arm64) and macOS (x64 and arm64) and
publishes them as the GitHub release `v<version>`, with the version from `Cargo.toml`. To ship
a new release, bump `version` in `Cargo.toml` (and `Cargo.lock`, by building) and the version
pill in `docs/index.html`.

`.github/workflows/pages.yml` publishes `docs/` (the landing page), `install.sh` and
`src/help.txt` to `sobanieca.github.io/yate/`. Each release has gzipped binaries
(`yate-linux-x64.gz`, `yate-linux-arm64.gz`, `yate-macos-x64.gz`, `yate-macos-arm64.gz`, about
7 MB each instead of 55 MB). `install.sh` and `yate update` download them, so their names must
not change.
