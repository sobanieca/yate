# Plugins and configuration

yate runs JavaScript in an embedded [QuickJS](https://bellard.org/quickjs/) engine. There is
no Node.js and no npm: a plugin is one `.js` file, evaluated as an ES module (its top-level
names stay private), with a global `yate` object.

A plugin lives in `~/.config/yate/plugins/<name>.js` and loads when the `plugins` section of
`yate.json` switches it on (see [configuration.md](configuration.md#plugins)):

```jsonc
{
  "plugins": {
    "sort-lines": true,   // ~/.config/yate/plugins/sort-lines.js
    "git": false          // a bundled plugin turned off
  }
}
```

yate loads, in this order:

1. the bundled plugins `git` and `format` (`src/plugins/builtin/`), unless set to
   `false`
2. the plugins set to `true`, in name order

`$XDG_CONFIG_HOME` replaces `~/.config` when set. Plugins run after `yate.json`, so they can
override its options and keys. Saving a file in the config folder reloads everything; so
does the command **Reload yate.json and plugins**. Errors show in the status line and in
`~/.local/state/yate/yate.log`.

Plugins are only loaded from your config folder, never from the opened project: a plugin can
run programs with `yate.exec`, and opening a repository must not run its code.

## How a call works

Before a plugin function runs, yate takes a snapshot of the text in the editor. `yate.editor`
reads that snapshot. Changes (`insert`, `setText`, `setCursor`...) are queued and applied in
order when the function returns. Reading after a change in the same call still returns the
text before the change.

A call that runs longer than 10 seconds is interrupted. `async` functions work, and awaited
promises are settled before the call returns.

Lines and columns are 0-based.

## API

```js
// Commands appear in the command palette (Alt+P) while the editor has focus, and can
// be bound to keys.
yate.registerCommand(id, title, fn)
yate.registerCommand(id, fn)                 // the title is the id
yate.runCommand(id)                          // built-in or plugin command

// Key names: "ctrl+p", "alt+up", "f5", "ctrl+/"...
// Contexts: "editor" (default), "tree", "global".
// An empty command removes a binding.
yate.bindKey(key, commandId, context)

// Events: "open", "beforeSave", "save". The listener gets the file path.
// "open" runs each time a file is read from disk to be shown.
// Edits made in "beforeSave" are written by that save.
yate.on(event, fn)

yate.setOption(name, value)
yate.notify(message)
yate.error(message)
yate.prompt(label, (answer) => {}, initial)  // not called when cancelled
yate.openFile(path, line)                    // relative to the project root
yate.root()                                  // project root

// Runs a program (no shell), waits for it. cwd defaults to the project root.
const { code, stdout, stderr } = yate.exec("git", ["status"], { input, cwd })

yate.editor.path()          // absolute path, or null for text without a file
yate.editor.language()      // "typescript", "rust"... or null
yate.editor.text()
yate.editor.lineCount()
yate.editor.line(index)
yate.editor.cursor()        // { line, col }
yate.editor.selection()     // { start: { line, col }, end: { line, col }, text }
yate.editor.insert(text)            // at the cursor
yate.editor.replaceSelection(text)  // replaces the selection (or inserts)
yate.editor.setText(text)           // whole text, one undo step
yate.editor.setCursor(line, col)
yate.editor.select({ line, col }, { line, col })

console.log(...)            // shows in the status line
```

## Options

| Option               | Default | Meaning                                                    |
| -------------------- | ------- | ---------------------------------------------------------- |
| `tabSize`            | `2`     | Width of an indent level and of a tab                      |
| `insertSpaces`       | `true`  | Indent with spaces                                         |
| `scrollOff`          | `999`   | Lines kept around the cursor; large keeps it centered      |
| `indentGuides`       | `true`  | Indent guide lines                                         |
| `autoPairs`          | `true`  | Close brackets and quotes                                  |
| `lineNumbers`        | `true`  | Line numbers                                               |
| `cursorLine`         | `true`  | Highlight the cursor line                                  |
| `colorize`           | `true`  | Show `#rrggbb` colors in their color                       |
| `nerdIcons`          | `false` | Nerd Font icons in the tree and the status line            |
| `showIgnored`        | `false` | Git-ignored files in the tree (also `i` in the tree)       |
| `treeFollow`         | `true`  | The tree follows the current file                          |
| `treeAutoCloseWidth` | `200`   | Opening a file from the tree closes it below this width    |
| `treeWidth`          | `0`     | Columns of the file tree; `0` fits the names               |
| `mouseScrollLines`   | `3`     | Lines per mouse wheel step                                 |
| `jumpChars`          | `10`    | Characters moved by Ctrl+Left / Ctrl+Right                 |
| `jumpLines`          | `5`     | Lines moved by Ctrl+Up / Ctrl+Down                         |
| `updateCheck`        | `true`  | Check once a day for a new release (read at startup)       |
| `keyHintDelay`       | `500`   | Milliseconds after Alt+W before the targets pop up         |

## Examples

See [`examples/plugins/extras.js`](../examples/plugins/extras.js), switched on in
[`examples/yate.jsonc`](../examples/yate.jsonc). The bundled plugins in `src/plugins/builtin/`
(git blame, Prettier) use the same API.
