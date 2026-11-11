// Defines the global `yate` object on top of the natives from plugins/mod.rs.
// Lines and columns are 0-based in the API.
(() => {
  const native = globalThis.__yateNative;
  delete globalThis.__yateNative;

  const commands = new Map();
  const listeners = new Map();
  const prompts = new Map();
  let nextPrompt = 1;

  const position = ([line, col]) => ({ line, col });

  const editor = {
    path: () => native.path(),
    language: () => native.language(),
    text: () => native.text(),
    lineCount: () => native.lineCount(),
    line: (index) => native.line(index),
    cursor: () => position(native.cursor()),
    selection() {
      const [startLine, startCol, endLine, endCol] = native.selection();
      return {
        start: { line: startLine, col: startCol },
        end: { line: endLine, col: endCol },
        text: native.selectedText(),
      };
    },
    insert: (text) => native.insert(String(text)),
    replaceSelection: (text) => native.replaceSelection(String(text)),
    setText: (text) => native.setText(String(text)),
    setCursor: (line, col = 0) => native.setCursor(line, col),
    select: (start, end) => native.select(start.line, start.col, end.line, end.col),
  };

  const yate = {
    editor,
    root: () => native.root(),
    registerCommand(id, title, run) {
      if (typeof title === "function") {
        run = title;
        title = id;
      }
      if (typeof run !== "function") throw new TypeError("registerCommand needs a function");
      commands.set(id, run);
      native.registerCommand(String(id), String(title));
    },
    bindKey(key, command, context = "editor") {
      native.bindKey(String(key), String(command), String(context));
    },
    on(event, listener) {
      if (!listeners.has(event)) {
        listeners.set(event, []);
        native.listen(String(event));
      }
      listeners.get(event).push(listener);
    },
    setOption: (name, value) => native.setOption(String(name), value),
    notify: (message) => native.notify(String(message), false),
    error: (message) => native.notify(String(message), true),
    show: (title, text) => native.show(String(title), String(text)),
    exec: (command, args = [], options = {}) =>
      native.exec(String(command), args.map(String), options.input ?? null, options.cwd ?? null),
    openFile: (path, line) => native.openFile(String(path), line ?? -1),
    runCommand: (id) => native.runCommand(String(id)),
    prompt(label, callback, initial = "") {
      const id = nextPrompt++;
      prompts.set(id, callback);
      native.prompt(String(label), String(initial), id);
    },
  };

  const format = (args) =>
    args.map((arg) => (typeof arg === "string" ? arg : JSON.stringify(arg))).join(" ");
  globalThis.console = {
    log: (...args) => native.log(format(args)),
    error: (...args) => native.log(format(args)),
  };
  globalThis.yate = yate;

  globalThis.__yateDispatch = (kind, name, arg) => {
    if (kind === "command") {
      const run = commands.get(name);
      if (!run) throw new Error(`unknown command ${name}`);
      return run();
    }
    if (kind === "event") {
      for (const listener of listeners.get(name) ?? []) listener(arg);
      return;
    }
    if (kind === "prompt") {
      const id = Number(name);
      const callback = prompts.get(id);
      prompts.delete(id);
      if (callback && arg !== null) return callback(arg);
    }
  };
})();
