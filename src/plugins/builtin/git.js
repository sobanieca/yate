// Git helpers from the nvim setup: GitBlame for the cursor line.

const ago = (seconds) => {
  const diff = Math.floor(Date.now() / 1000) - seconds;
  if (diff >= 86400) return `${Math.floor(diff / 86400)} days ago`;
  if (diff >= 3600) return `${Math.floor(diff / 3600)} hours ago`;
  if (diff >= 60) return `${Math.floor(diff / 60)} minutes ago`;
  return `${diff} seconds ago`;
};

yate.registerCommand("git.blame", "Git: Blame current line", () => {
  const path = yate.editor.path();
  if (!path) return yate.error("The buffer has no file");
  const line = yate.editor.cursor().line + 1;
  const result = yate.exec("git", ["blame", "--porcelain", "-L", `${line},${line}`, path], {
    cwd: yate.root(),
  });
  if (result.code !== 0) return yate.error(result.stderr.trim() || "git blame failed");
  const lines = result.stdout.split("\n");
  const field = (name) => {
    const found = lines.find((l) => l.startsWith(`${name} `));
    return found ? found.slice(name.length + 1) : "";
  };
  const hash = lines[0].split(" ")[0];
  if (/^0+$/.test(hash)) return yate.notify("Not committed yet");
  const time = Number(field("author-time"));
  yate.notify(`${hash.slice(0, 8)} ${field("author")}, ${ago(time)}: ${field("summary")}`);
});
