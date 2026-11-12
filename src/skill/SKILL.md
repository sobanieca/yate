---
name: yate
description: Configure the yate terminal text editor in yate.json - theme preset, palette colors, syntax styles, options, key bindings and plugins. Use when asked to change yate settings, colors, keys or plugins, or to write a yate plugin.
---

# yate

yate reads its settings from `yate.json` files. They are JSONC: `//` and `/* */`
comments and trailing commas are allowed, and the file can be named `yate.jsonc` too.

## Workflow

1. Run `yate skill` once to read the full reference: where yate looks for settings,
   every palette color, syntax scope, option, key binding and command id with its
   default, and the plugin API.
2. Pick the file to change:
   - Personal settings, for every project: `~/.yate/yate.json`, or
     `~/.yate/yate.jsonc` when that is the one that exists. Create
     `~/.yate/yate.jsonc` when neither exists.
   - Settings for one project (its `tabSize`, say): `yate.json` or `yate.jsonc` in
     the project root. It is data only and safe to commit.
   - Ask when the request does not say which.
3. When the file exists, read it and change only the keys asked for. Keep the
   person's comments and other settings. Files merge key by key, so a file needs
   only the settings it changes.
4. Use only names from the reference: palette colors, syntax scopes, options, key
   names and command ids. Do not invent them.
5. yate applies a saved settings file at once when it is open in yate, otherwise at
   the next start or with the command "Reload yate.json and plugins". Mistakes show
   in the status line and in `~/.yate/yate.log`, with file, line and
   column.

## Rules

- Prefer Ctrl and a letter for new keys: they work in every terminal and in tmux.
  Ctrl+I, Ctrl+M and Ctrl+[ cannot be bound (terminals send Tab, Enter and Esc),
  and Ctrl+B is the tmux prefix. `""` or `null` removes a binding.
- A plugin is `~/.yate/plugins/<name>.js` and loads only when a settings file has `"plugins": { "<name>": true }`.
  Never put plugin code in a project.
