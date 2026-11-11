// Formatting through external tools, like :Prettier in the nvim setup, and
// :TsNoCheck.

yate.registerCommand("format.prettier", "Format document with Prettier", () => {
  const path = yate.editor.path();
  if (!path) return yate.error("Save the file first");
  const result = yate.exec("npx", ["--no-install", "prettier", "--stdin-filepath", path], {
    input: yate.editor.text(),
    cwd: yate.root(),
  });
  if (result.code !== 0) return yate.error(result.stderr.trim() || "prettier failed");
  yate.editor.setText(result.stdout);
  yate.notify("Formatted with Prettier");
});

yate.registerCommand("ts.noCheck", "TypeScript: Add // @ts-nocheck", () => {
  const cursor = yate.editor.cursor();
  yate.editor.setCursor(0, 0);
  yate.editor.insert("// @ts-nocheck\n");
  yate.editor.setCursor(cursor.line + 1, cursor.col);
});
