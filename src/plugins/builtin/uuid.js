// Inserts a random UUID v4 (lower case, no quotes), like uuid-nvim on Ctrl+U.

const hex = (count) =>
  Array.from({ length: count }, () => Math.floor(Math.random() * 16).toString(16)).join("");

const variant = () => "89ab"[Math.floor(Math.random() * 4)];

yate.registerCommand("uuid.insert", "Insert UUID v4", () => {
  const uuid = `${hex(8)}-${hex(4)}-4${hex(3)}-${variant()}${hex(3)}-${hex(12)}`;
  yate.editor.replaceSelection(uuid);
});

yate.bindKey("ctrl+u", "uuid.insert");
