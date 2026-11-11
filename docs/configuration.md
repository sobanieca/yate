# Configuration: yate.json

`yate.json` sets the theme, options, key bindings and plugins. It is JSONC: JSON with `//`
and `/* */` comments and trailing commas allowed. The file can also be named `yate.jsonc`,
so other editors expect the comments too.

## Where yate looks

1. `~/yate.json`, the global settings.
2. Every `yate.json` in the folders from `/` down to the working directory (and down to the
   opened project folder when it is below the working directory), skipping folders above
   your home folder.

In each folder, `yate.jsonc` is read after `yate.json`, so it wins when both exist.

Files closer to the working directory override farther ones, key by key: a project can set
`"options": { "tabSize": 4 }` and keep your global theme. A project `yate.json` is data
only, so it is safe to commit: its `plugins` section only picks plugins by name, and their
code comes from `~/.config/yate/plugins`. Plugins run after `yate.json` is applied, so they
can override it.

The command **Open yate.json** (command palette, Ctrl+H) opens the closest file, or creates
`~/yate.jsonc` from a commented template that lists every default key binding with its
command id and title, so keys can be changed in place. Saving a `yate.json` or `yate.jsonc` (or a file in `~/.config/yate`)
applies it at once; **Reload yate.json and plugins** does the same by hand. Mistakes show in the status
line and in `~/.local/state/yate/yate.log`, with the file, line and column.

## Example

```jsonc
{
  "theme": {
    "preset": "tokyonight-night",
    "colors": {
      "bg": "#1e1e2e",
      "bg_visual": "#45475a",
      "orange": "blue"            // a palette color can copy another one
    },
    "syntax": {
      "keyword": "magenta",
      "comment": { "fg": "#6c7086", "italic": true },
      "function.builtin": { "fg": "cyan", "bold": true }
    }
  },
  "options": {
    "tabSize": 2,
    "nerdIcons": true,
    "scrollOff": 999
  },
  "keys": {
    "editor": { "ctrl+y": "tree.toggle", "alt+z": "edit.redo" },
    "tree": { "ctrl+y": "tree.toggle" },
    "global": { "ctrl+q": "" }   // "" or null removes a binding
  },
  "plugins": {
    "sort-lines": true,           // ~/.config/yate/plugins/sort-lines.js
    "format": false               // a bundled plugin turned off
  }
}
```

## theme

`preset` is the starting point: `tokyonight-storm` (default), `tokyonight-night`,
`tokyonight-moon`, `tokyonight-day` (light), `gruvbox-dark`, `gruvbox-light` (light), `nord`
or `solarized-light` (light).

The status line label shows where keys go in the color of that focus area: `blue` for the
editor, `green` for the tree, `yellow` for find, `cyan` for pickers and
`orange` for prompts. Change those palette colors to change the labels.

`colors` overrides palette colors. A value is `#rrggbb`, `#rgb` or the name of another
palette color. The palette and where the views use it:

| Color                  | Used for                                                       |
| ---------------------- | -------------------------------------------------------------- |
| `bg`                   | editor background                                              |
| `bg_dark`              | tree, pane titles, status bar, pickers, prompts                |
| `bg_inactive`          | editor background while the terminal or tmux pane has no focus |
| `bg_highlight`         | cursor line, selected rows in lists                            |
| `bg_visual`            | selection                                                      |
| `bg_search`            | find and grep matches                                          |
| `fg`, `fg_dark`        | text, dimmer text                                              |
| `fg_gutter`            | line numbers, indent guides, separators                        |
| `dark3`, `dark5`       | hints, ignored files, dim labels                               |
| `comment`              | comments                                                       |
| `blue`                 | accents: borders, status bar mode, folders, focused pane title |
| `orange`               | current line number, current match, modified marker            |
| `magenta`              | picker prompt, find bar label                                  |
| `yellow`, `green`, `red`, `cyan`, `green1` | git status of files, messages              |
| `blue1`, `blue5`, `blue6`, `purple`, `red1` | syntax colors                             |
| `git_add`, `git_change`, `git_delete` | gutter signs                                    |

`syntax` styles tree-sitter captures. A key is a capture scope such as `keyword`,
`keyword.return`, `string`, `function.method`, `type`, `variable.parameter`, `property`,
`tag`, `markup.heading`. The longest matching scope wins: a style for `function` applies to
`function.method.call` unless `function.method` has its own. A value is a color (hex or
palette name) or an object with `fg`, `bg`, `bold`, `italic` and `underline`. Default styles
refer to palette colors, so changing `magenta` changes every scope drawn in magenta.

## options

The same options as `yate.setOption` in JavaScript (see [plugins.md](plugins.md#options)):
`tabSize`, `insertSpaces`, `scrollOff`, `indentGuides`, `autoPairs`, `lineNumbers`,
`cursorLine`, `colorize`, `nerdIcons`, `showIgnored`, `treeFollow`, `treeAutoCloseWidth`,
`treeWidth`, `mouseScrollLines`, `jumpChars`, `jumpLines`, `updateCheck`, `keyHintDelay`.

`jumpChars` (default `10`) is how many characters Ctrl+Left and Ctrl+Right move, within the
line; `jumpLines` (default `5`) is how many lines Ctrl+Up and Ctrl+Down move.

`treeWidth` (default `0`) is the width of the file tree in columns; `0` fits the names. `[`
and `]` in the tree, or dragging its right edge, change it until yate closes.

`keyHintDelay` (default `500`) is the time in milliseconds before the popup of a menu
(Ctrl+G, Ctrl+K, Ctrl+W, Ctrl+T, Ctrl+L) or of the Ctrl+O switch targets shows; `0` shows it
at once. The keys work before it shows.

`updateCheck` (default `true`) checks once a day for a new release of yate and shows a
message when there is one; `yate update` installs it. The environment variable
`YATE_NO_UPDATE_CHECK=1` turns the check off too.

## keys

Bindings per context: `global` (everywhere), `editor` and `tree`. A context
binding wins over a global one. Values are command ids: the command palette (Ctrl+H) shows
the commands of the focused panel with their keys, and the template of **Open yate.json**
lists all default bindings and the commands without a key. Key names are like
`ctrl+p`, `alt+shift+down`, `alt+tab`, `ctrl+\\`, `ctrl+/`; a single letter like `"c"` binds
a key of the tree. A key can run a plugin command too.

Every default key is Ctrl and a letter: no Alt, no Ctrl+Shift. Five keys open a menu of
related commands, and a letter or an arrow then runs one:

| Key    | Command         | Commands in the menu                                              |
| ------ | --------------- | ----------------------------------------------------------------- |
| Ctrl+G | `goto.menu`     | `goto.line`, `goto.definition`, `jump.back`, `jump.forward`       |
| Ctrl+K | `bookmark.menu` | `bookmark.toggle`, `bookmark.next`, `bookmark.previous`, `picker.bookmarks`, `bookmark.clear` |
| Ctrl+W | `view.paneMenu` | `view.splitVertical`, `view.splitHorizontal`, `view.focusPaneLeft` / `Below` / `Above` / `Right`, `view.focusNextPane`, `view.resizePane`, `view.equalPanes`, `view.closePane`, `view.closeOtherPanes`, `view.toggleScrollSync` |
| Ctrl+T | `tree.menu`     | `tree.toggle`, `tree.reveal`, `tree.focus`, `tree.lock`           |
| Ctrl+L | `edit.lineMenu` | `edit.moveLineUp`, `edit.moveLineDown`, `edit.outdent`, `edit.indent`, `edit.duplicateLine`, `edit.deleteLine` |

The key of a menu can be changed, the keys inside it are fixed. The commands behind a menu
have no key of their own; bind one to skip the menu:

```jsonc
{ "keys": { "editor": { "ctrl+e": "goto.definition" }, "global": { "ctrl+j": "bookmark.next" } } }
```

Ctrl+O (`switch.next`) waits for a digit: `1` focuses the file tree, `2`-`9` show a recent
file, the latest first. Pressing it again shows the next recent file instead;
`switch.previous` goes the other way and has no key.

Ctrl+I, Ctrl+M and Ctrl+[ cannot be bound: terminals send them as Tab, Enter and Esc.

## plugins

Plugins by name, `true` to load and `false` to skip. A name is the file name of a plugin in
`~/.config/yate/plugins` without `.js`: `"sort-lines": true` loads
`~/.config/yate/plugins/sort-lines.js`. A plugin file that no `yate.json` switches on is not
loaded. The bundled plugins `git` and `format` load unless set to `false`. Like
every other section, closer files win, so a project can switch a plugin you installed on or
off. See [plugins.md](plugins.md) for writing one.
