# End-to-end tests

The tests run the real editor in memory: the same `App`, key bindings, plugins, git and
rendering as in a terminal, but without a terminal. A test presses keys, clicks and pastes,
then compares the screen with a text file in `snapshots/`.

```bash
cargo e2e                      # run the end-to-end tests
cargo e2e -- tree              # only tests whose name contains "tree"
cargo update-snapshots         # accept the current screens as the snapshots
cargo verify                   # everything, before committing
```

The whole suite takes about a second. It needs `git` installed.

## Writing a test

Tests are grouped by area in `editing.rs`, `tree.rs`, `pickers.rs`, `find.rs`, `git.rs`,
`split.rs`, `tabs.rs`, `keys.rs`, `definitions.rs`, `plugins.rs`, `settings.rs` and
`update.rs`. A test describes a project, starts yate in it and drives it:

```rust
use yate::testing::Harness;

#[test]
fn comment_toggle() {
    let mut e = Harness::builder()
        .size(60, 12)                            // terminal size, 80x20 by default
        .file("app.js", "let a = 1;\n")          // project files
        .config("plugins/sort.js", "yate.registerCommand('sort', () => {});") // ~/.config/yate files
        .home_file("yate.json", r#"{ "plugins": { "sort": true } }"#) // files in ~
        .working_dir("src")                      // run from a project folder (default: the root)
        .git()                                   // git repository, branch main, all committed
        .open("app.js")                          // like `yate app.js`; omit to start in the folder
        .start();

    e.keys("ctrl+/");                            // key names, separated by spaces
    assert_eq!(e.editor_text(), "// let a = 1;\n");
    e.snapshot("editing_comment_toggle");        // tests/e2e/snapshots/editing_comment_toggle.txt
}
```

Steps (each one waits until background work such as git status or live grep is done):

| Step                                  | Does                                                     |
| ------------------------------------- | -------------------------------------------------------- |
| `keys("ctrl+p down enter")`           | presses keys: `ctrl+`, `alt+`, `shift+`, `enter`, `esc`, `tab`, `up`, `pageup`, `alt+tab`, `space`, a character |
| `type_text("hello\n")`                | types text one character at a time (`\n` is Enter)       |
| `paste("text")`                       | bracketed paste                                          |
| `click(x, y)`, `double_click(x, y)`   | left click on a screen cell (column, row from 0)         |
| `ctrl_click(x, y)`                    | left click with Ctrl held (go to definition)             |
| `drag((x1, y1), (x2, y2))`            | drag with the left button                                |
| `scroll(x, y, steps)`                 | mouse wheel, positive steps scroll down                  |
| `command("file.save")`                | runs a command by id, like the command palette           |
| `write_file(path, text)`              | changes a file outside the editor                        |
| `focus_gained()`, `focus_lost()`      | terminal focus; focus gained reloads files and git       |
| `git(&["commit", "-qam", "x"])`       | runs git in the project                                  |
| `resize(w, h)`                        | resizes the terminal                                     |
| `update_check(Some("9.9.9"))`        | the daily update check found a release (never online in tests) |

Checks:

| Check                                 | Checks                                                   |
| ------------------------------------- | -------------------------------------------------------- |
| `snapshot("name")`                    | the screen text against `snapshots/name.txt`             |
| `snapshot_styles("name")`             | the screen and its colors (one letter per style, and a legend) |
| `assert_screen_contains("text")`      | a text anywhere on the screen (`assert_screen_lacks` too) |
| `editor_text()`, `read_file(path)`    | the text being edited, a file on disk                      |
| `screen()`                            | the frame: `lines()`, `line(y)`, `cursor`                |
| `clipboard()`, `app()`                | what a paste would insert, the whole `App` state         |

Prefer assertions on text and files for behavior, and snapshots for what the person sees.
Keep screens small (60x12 is often enough) so snapshots stay readable. Snapshot names are
`<area>_<what>` in lower case.

## Snapshot files

A snapshot is the screen in a box, with the steps that led to it:

```
# find_replace_bar
# start: yate cart.js
# keys: ctrl+f ctrl+r
# type: "total"
# keys: tab
# type: "sum"
# keys: enter
# 70x10, cursor at column 60, row 8
┌──────────────────────────────────────────────────────────────────────┐
│ cart.js ●                                                            │
│  1  let sum = 0;                                                     │
...
│ FIND  cart.js ●         (5 selected) javascript  utf-8  LF  50%  3:8 │
└──────────────────────────────────────────────────────────────────────┘
```

`snapshot_styles` adds a second box where every cell is a letter, and a legend with the
theme color names, so a selection, a search match or syntax colors are visible as text:

```
styles
┌──────┐
│aadaae│
└──────┘
a  bg BG
d  fg FG_GUTTER, bg BG
e  fg PURPLE, bg BG, italic
```

Blank cells only count their background. Letters are given in order of first appearance,
so a new style shifts the letters after it; read the legend, not the letters.

## When a test fails

A snapshot that does not match fails with a diff of the whole file: unchanged lines start
with two spaces, expected lines with `- `, actual lines with `+ `. The actual screen is also
written next to the snapshot as `snapshots/<name>.actual.txt` (git ignores these), so you
can open both files side by side.

- The new screen is right (an intended UI change): run `cargo update-snapshots` and commit
  the changed `.txt` files. Review them in `git diff` like code.
- The new screen is wrong: fix the code; the `.actual.txt` file disappears when the test
  passes.
- A new snapshot has no file yet: the test fails, prints the screen and writes the
  `.actual.txt` file; check it, then run `cargo update-snapshots`.

Other failures:

- `background work did not finish`: a step started work (git, file index, grep) that never
  reported back; the message names the last step.
- An `assert_screen_contains` failure prints the whole screen.
- To see a screen while writing a test, add `println!("{}", e.screen().frame());` and run
  `cargo e2e -- name --nocapture`.

## How it works

`yate::testing::Harness` (`src/testing.rs`) creates temporary project, config and state
folders, starts `App` with an in-memory clipboard (no tmux, no OSC 52), and feeds it
terminal events the way the event loop does: a frame is drawn before every event, so mouse
clicks hit the current layout. After every step it handles background events until
`App::pending_jobs` is zero. `snapshot` draws a frame with `App::draw`, the same function the
terminal uses, and prints the cells of the `Surface`.

Screens are deterministic: live grep results are sorted by file and line, the git branch is
`main`, the project folder is always called `project`, paths show as relative or `~/...`,
and plugins and `yate.json` files load only from the test's own folders (the search for
`yate.json` stops at the test's temp folder).
