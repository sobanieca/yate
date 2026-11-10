// Example plugin: ~/.config/yate/plugins/extras.js
// Switch it on with "plugins": { "extras": true } in yate.json.

// Format TypeScript and JavaScript with Prettier on save.
const prettierLanguages = new Set(["typescript", "tsx", "javascript", "json", "css", "markdown"]);
yate.on("beforeSave", () => {
  if (prettierLanguages.has(yate.editor.language())) {
    yate.runCommand("format.prettier");
  }
});

// Insert today's date.
yate.registerCommand("date.insert", "Insert today's date", () => {
  yate.editor.replaceSelection(new Date().toISOString().slice(0, 10));
});

// Sort the selected lines.
yate.registerCommand("lines.sort", "Sort selected lines", () => {
  const { text } = yate.editor.selection();
  if (!text) return yate.error("Select some lines first");
  const trailing = text.endsWith("\n");
  const lines = (trailing ? text.slice(0, -1) : text).split("\n").sort();
  yate.editor.replaceSelection(lines.join("\n") + (trailing ? "\n" : ""));
});

// Show the file at HEAD a number of days ago (GitShow in the nvim setup).
yate.registerCommand("git.showDaysAgo", "Git: show file N days ago in a notification", () => {
  const path = yate.editor.path();
  if (!path) return;
  yate.prompt("Days ago", (days) => {
    const relative = path.slice(yate.root().length + 1);
    const date = new Date(Date.now() - Number(days) * 86400000).toISOString().slice(0, 10);
    const result = yate.exec("git", ["log", "-1", "--format=%h %an %ar: %s", `--before=${date}`, "--", relative]);
    yate.notify(result.stdout.trim() || `No commit of ${relative} before ${date}`);
  });
});
