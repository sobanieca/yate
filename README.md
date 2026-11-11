# yate

**Yet another text editor**: a terminal editor somewhere between nano and neovim. Simple to
use like nano: no modes, type and the text goes in, and the keys you need are on screen. Enough
to work on a project like neovim: a file tree, a fuzzy file finder, live grep, split view, go
to definition and JavaScript plugins, with no language servers and nothing to set up. It is
made for quick, comfortable edits in a terminal or a tmux pane.

Website and manual: https://sobanieca.github.io/yate/

> **Status:** version 0.6.0, an early release. Keys, options and the plugin API may still
> change between versions.

## Features

- **One binary, no runtime dependencies.** Syntax grammars and the JavaScript engine are
  compiled in. yate uses git when it is installed.
- **Project tools:** a file tree with git status, checkboxes for ignored and changed files,
  sideways scrolling for deep names and a width you set (`[` / `]` or drag its edge), a fuzzy file finder (Ctrl+P), live grep with glob
  filters, git signs in the gutter, bookmarks and a jump list.
- **Tabs without hidden buffers.** Every file you open gets a tab above the editor; click
  one to show it, click its `×` to close it, or press Ctrl+O and its number. Only the files
  on screen are loaded, so nothing unsaved hides behind a tab: showing another tab with
  unsaved changes asks first (save, discard or stay). A file opens again where the cursor
  was, and Ctrl+P lists the files you had last on top.
- **Split view** (Ctrl+W): as many panes as you need, side by side or one above the other,
  showing the same file or different files. Panes share the space evenly; resize one by
  typing the columns and rows to add. Scroll sync (Ctrl+W Y) scrolls all of them together.
- **Ctrl keys only, with hints on screen:** a few direct keys for what you do all the time
  (save, find, copy, undo), and six keys that pop up a menu of related commands: Ctrl+G go
  to, Ctrl+K bookmarks, Ctrl+W panes and tabs, Ctrl+T file tree, Ctrl+L lines and Ctrl+O
  switch. A letter or an arrow then picks the command, with Ctrl still held or not, and the
  menu shows only when you wait. The command palette (Ctrl+H) lists what the focused panel
  can do right now with the keys for it. No Alt, function or Ctrl+Shift keys are needed.
- **Go to definition** (Ctrl+G D, Ctrl+click) without a language server. tree-sitter finds the
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
  the editor, green for the tree, yellow for find.
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
yate update           install the latest release
yate --help           list the keys
yate --version        show the version
```

Ctrl+H inside yate lists the commands with their keys and runs the one you pick. All of them:

| Keys                          | Action                                                       |
| ----------------------------- | ------------------------------------------------------------ |
| Ctrl+H                        | Command palette and keys (searchable, Enter runs)            |
| Ctrl+P                        | Find file (recent files first)                               |
| Ctrl+R                        | Live grep                                                    |
| Tab in Ctrl+P / Ctrl+R        | Glob filter: `*.ts, src/**, !*.test.ts` (`!` excludes)       |
| Up in Ctrl+P / Ctrl+R         | The last search again, before typing                         |
| `f` / `g` in the tree         | Find file / live grep in the selected folder                 |
| Ctrl+S, Ctrl+N, Ctrl+Q        | Save, new file, quit                                         |
| Ctrl+F                        | Find in file; Enter, ↓ / ↑ next / previous match             |
| Tab in the find bar           | Next field: `[ ] Aa` case, `[ ] .*` regex, `[ ] replace`; ← → move along a row, Space or Enter ticks a checkbox or presses Replace / Replace all |
| Ctrl+C / X / V                | Copy / cut / paste (whole line without a selection)          |
| Ctrl+Z / Ctrl+Y               | Undo / redo                                                  |
| Ctrl+A / Ctrl+D               | Select all / select word, then its next occurrence           |
| Ctrl+/                        | Toggle comment                                               |
| Tab / Shift+Tab               | Indent / outdent                                             |
| Ctrl+Left/Right, Ctrl+Up/Down | 10 characters / 5 lines (`jumpChars`, `jumpLines`)           |
| Ctrl+click                    | Go to definition, or to the file under the cursor            |
| `[` / `]` in the tree         | Narrower / wider file tree (or drag its right edge)          |

Six keys open a menu; the second key picks the command:

| Menu                | Keys in the menu                                                         |
| ------------------- | ------------------------------------------------------------------------ |
| Ctrl+G go to        | `l` line, `d` definition or file under the cursor, ← back, → forward     |
| Ctrl+K bookmarks    | `t` toggle, ↓ next, ↑ previous, `l` list, `c` clear in this file         |
| Ctrl+W panes, tabs  | `v` split side by side, `s` split stacked, `h` `j` `k` `l` or arrows focus the pane on that side, `w` next pane, `r` resize, `=` equal sizes, `c` close, `o` close the others, `y` scroll sync, `x` close the tab, `t` close the tabs not on screen |
| Ctrl+T file tree    | `t` toggle, `r` reveal current file, `f` focus, `l` lock                 |
| Ctrl+L lines        | ↑ ↓ move, ← → outdent / indent, `d` duplicate, `k` delete                |
| Ctrl+O switch       | `1` file tree, `2`-`9` the tabs; Ctrl+O again: the next tab              |

- The second key works with Ctrl still held: Ctrl+W Ctrl+V is Ctrl+W V. Only Ctrl+C differs:
  it closes the menu, like Esc.
- The key of a menu twice runs its usual command: Ctrl+T Ctrl+T toggles the tree, Ctrl+W
  Ctrl+W focuses the next pane, Ctrl+O Ctrl+O shows the next tab.
- The arrow commands repeat: right after one, Ctrl and an arrow run it again without the
  menu (Ctrl+G ←, then Ctrl+← Ctrl+← goes back twice more). Any other key, or a pause of a
  second and a half, ends that.
- The menu pops up only when you wait (`keyHintDelay`, 500 ms), so keys you know show
  nothing. A click picks a command too.

Commands without a key are in the command palette (Ctrl+H). It lists what the focused panel
can do right now, in sections: with the editor focused File, Edit, Search, Go to, Bookmarks,
View, File tree, Plugins and yate; with the file tree focused the commands for the selected
file or folder, the tree, search and yate; and last Keys, the keys that are not commands.
Commands with nothing to work on are left out (the pane commands without a split, "Find
next" without a search, bookmark navigation without bookmarks). Typing filters by section
and name ("tree toggle"). `yate.json` can give any command a key of its own.

Every default key is Ctrl and a letter, so it works in every terminal, through tmux and in
Termux: no Alt keys (terminals and window managers take many of them), no Ctrl+Shift (sent
only with the kitty keyboard protocol, and not through tmux), no Ctrl+B (the tmux prefix)
and no Ctrl+I or Ctrl+M, which terminals send as Tab and Enter. Shift+Tab is never the only
way: Ctrl+L ← outdents.

Bars and dialogs add no keys: they are forms. In the find bar Tab moves from the text to the
checkboxes for case, regex and replace and to the Replace buttons, ← and → move along a
row, and Space or Enter ticks or presses. Questions (unsaved changes, delete, quit) show
buttons: ← → and Enter, or the first letter of a button. A message too long for the status
line (an error with several lines, git blame) opens in a dialog: ↑ ↓ scroll it, Esc or Enter
closes it. The file tree has two checkboxes
above the files, `[ ] changed` (only changed files) and `[ ] ignored` (git-ignored files
too): ↑ from the first file reaches them, Space or Enter ticks, ↓ goes back. Clicks work
everywhere.

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
