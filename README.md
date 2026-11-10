# yate

**Yet another text editor**: a modeless terminal editor that works like a basic VS Code,
not like vim. No modes and no language servers. It is made for quick, comfortable edits in a
terminal or a tmux pane.

Website and manual: https://sobanieca.github.io/yate/

> **Status:** version 0.1.0, an early release. Keys, options and the plugin API may still
> change between versions.

## Features

- **One binary, no runtime dependencies.** Syntax grammars and the JavaScript engine are
  compiled in. yate uses git when it is installed.
- **Project tools:** a file tree with git status and filters for ignored and changed files,
  a fuzzy file finder (Ctrl+P), live grep with glob filters, a review of git changes against
  HEAD, diffs between two files or two folders (also `yate diff <a> <b>` from the shell),
  bookmarks and a jump list.
- **Split view** (Ctrl+\\): two panes side by side, showing the same file or two files. Scroll
  sync (Ctrl+J) scrolls both together.
- **Ctrl keys with hints on screen:** the status line shows the keys for what you are doing,
  and Ctrl+H lists every binding. Alt+W followed by a digit focuses the file tree (1) or an
  open file (2-9); the numbers pop up when you wait. No function keys are needed.
- **Go to definition** (Alt+D, Ctrl+click) without a language server. tree-sitter finds the
  definition in the file, follows ES imports and re-exports to the module that defines a
  name, or searches the project. On a path, an import or `file:line:col` it opens that file.
  Imports resolve like Node and TypeScript (`tsconfig.json` paths, `package.json` exports,
  `node_modules`).
- **Syntax highlighting** with tree-sitter, parsed incrementally, for 36 languages:
  JavaScript, TypeScript, TSX, JSON, CSS, HTML, Svelte, Markdown, YAML, TOML, Rust, Go,
  Python, Bash, C, C++, Java, C#, Lua, Ruby, PHP, SQL, Zig, Swift, Elixir, Haskell, Scala,
  Dart, Gleam, Nix, GraphQL, Prisma, HCL/Terraform, XML, Makefile and diff.
- **JavaScript plugins** run in an embedded QuickJS engine: commands, key bindings, events
  and running programs. See [docs/plugins.md](docs/plugins.md).
- **tmux friendly:** copies go to the tmux paste buffer (and to the system clipboard with
  `set-clipboard on`), paste reads from it, and losing focus dims the editor like an inactive
  tmux pane.
- **Themes:** Tokyo Night by default (storm, night, moon and day presets), plus gruvbox (dark,
  light), nord and solarized-light presets. Every color can be changed in `yate.json`. The
  status line label and the focused panel title take the color of the focus area: blue for
  the editor, green for the tree, yellow for find, magenta for review.
- **UTF-8 only.** LF and CRLF line endings are kept. Files that are not valid UTF-8 open
  read-only.

## Install

Linux and macOS (x64, arm64):

```bash
curl -fsSL sobanieca.github.io/yate/install.sh | bash
```

This installs yate to `/usr/local/bin`. If you can't write there, run the script with `sudo`
(`... | sudo bash`), or install it to another folder on your `PATH` with
`... | INSTALL_DIR=~/.local/bin bash`.

Prebuilt binaries are also attached to each
[GitHub release](https://github.com/sobanieca/yate/releases).

### Update

```bash
yate update
```

This downloads the latest release and replaces the binary (use `sudo yate update` when it is
in a system folder). yate checks for a new version once a day and shows a message when one is
available, also after you quit. Turn the check off with `"updateCheck": false` in the
`options` of `yate.json`, or with `YATE_NO_UPDATE_CHECK=1`.

## Usage

```text
yate                  open the current folder (file tree)
yate <folder>         open a folder
yate <file>[:line]    open a file; the project is its git repository
yate diff <a> <b>     compare two files or two folders (a is the old side)
yate update           install the latest release
yate --help           list the keys
yate --version        show the version
```

Ctrl+H inside yate lists every binding and runs the one you pick. The important ones:

| Keys                            | Action                                                     |
| ------------------------------- | ---------------------------------------------------------- |
| Ctrl+H                          | Key bindings (searchable, Enter runs)                      |
| Ctrl+P                          | Find file                                                  |
| Alt+F (Ctrl+Shift+F)            | Live grep                                                  |
| Ctrl+F in Ctrl+P / Alt+F        | Glob filter: `*.ts, src/**, !*.test.ts` (`!` excludes)     |
| `f` / `g` in the tree           | Find file / live grep in the selected folder               |
| Alt+P (Ctrl+Shift+P)            | Command palette                                            |
| Alt+W (Alt+Tab), then 1-9       | 1 file tree, 2-9 open files; Alt+W again: next file        |
| Alt+E / Ctrl+T                  | Toggle file tree / reveal current file                     |
| Ctrl+\\ / Ctrl+O / Ctrl+J       | Split / other pane / scroll sync                           |
| Alt+G                           | Review git changes                                         |
| `c` in the tree                 | Compare: mark a file or folder, then `c` on another        |
| Ctrl+S, Ctrl+W, Ctrl+Q          | Save, close buffer, quit                                   |
| Ctrl+F / Ctrl+R                 | Find / replace in file                                     |
| Ctrl+C / X / V                  | Copy / cut / paste (whole line without a selection)        |
| Ctrl+Z / Ctrl+Y                 | Undo / redo                                                |
| Ctrl+/                          | Toggle comment                                             |
| Alt+Up / Alt+Down               | Move lines                                                 |
| Ctrl+Left/Right, Ctrl+Up/Down   | 10 characters / 5 lines (`jumpChars`, `jumpLines`)         |
| Tab / Shift+Tab (Alt+I / Alt+O) | Indent / outdent                                           |
| Alt+Left / Alt+Right            | Jump back / forward                                        |
| Alt+D, Ctrl+click               | Go to definition, or to the file under the cursor          |
| Alt+M, Alt+N, Ctrl+L            | Bookmark toggle, next, list                                |

Terminals send Ctrl+Shift combinations only with the kitty keyboard protocol (kitty, WezTerm,
foot, Ghostty), and not through tmux, so every Ctrl+Shift binding also has an Alt binding:
Alt+P for the palette, Alt+G for review. Keys avoid Ctrl+B (the tmux prefix), Ctrl+Tab and
Shift+Tab, which many terminals (Termux, terminals over SSH) do not send: Alt+W switches
files, Alt+E toggles the tree and Alt+I / Alt+O indent and outdent.

## Configuration

`yate.json` (or `yate.jsonc`) holds the theme, options, key bindings and plugins.
`~/yate.json` applies everywhere, and any `yate.json` on the way down to the working folder
overrides it, the closest one winning. Comments and trailing commas are allowed. The command
**Open yate.json** creates `~/yate.jsonc` with every key binding listed, ready to edit. See
[docs/configuration.md](docs/configuration.md) and [examples/yate.jsonc](examples/yate.jsonc).

```jsonc
{
  "theme": { "preset": "tokyonight-night", "colors": { "bg": "#1e1e2e" }, "syntax": { "keyword": "magenta" } },
  "options": { "tabSize": 4 },
  "keys": { "editor": { "ctrl+y": "tree.toggle", "alt+d": "date.insert" } },
  "plugins": { "date": true }   // ~/.config/yate/plugins/date.js
}
```

## Plugins

Plugins are JavaScript files in `~/.config/yate/plugins`, loaded when `yate.json` switches
them on by name. yate never loads plugins from the opened project.

```js
// ~/.config/yate/plugins/date.js
yate.registerCommand("date.insert", "Insert today's date", () =>
  yate.editor.replaceSelection(new Date().toISOString().slice(0, 10)),
);
```

See [docs/plugins.md](docs/plugins.md) for the API and
[examples/plugins](examples/plugins) for more plugins.

## Building from source

You need a stable Rust toolchain (1.85 or newer, for the 2024 edition) and a C compiler for
the tree-sitter grammars. git is needed for the tests and for the git features.

```bash
git clone https://github.com/sobanieca/yate.git
cd yate
cargo yate              # run from source (optimized, quick rebuilds) in the current folder
cargo yate src/main.rs  # open a file
cargo install --path .  # install yate to ~/.cargo/bin
cargo dist              # release binary in dist/yate (static with the musl target installed)
```

For a static Linux binary, add the musl target first, for example
`rustup target add x86_64-unknown-linux-musl`.

## Development

```bash
cargo verify            # fmt --check, clippy -D warnings, all tests (run before committing)
cargo e2e               # end-to-end tests: screen snapshots in tests/e2e/snapshots
cargo update-snapshots  # accept changed screens as the new snapshots
```

The architecture and the project conventions are described in [AGENTS.md](AGENTS.md), the
end-to-end tests in [tests/e2e/README.md](tests/e2e/README.md).

Every push to `main` runs the checks, builds the Linux and macOS binaries and publishes them
as the GitHub release `v<version>`, using the version in `Cargo.toml`.

## License

[MIT](LICENSE)
