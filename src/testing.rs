//! End-to-end test harness: runs the real editor in memory, feeds it key
//! presses, mouse clicks and pastes, and compares what is on the screen with
//! text snapshots in `tests/e2e/snapshots/`.
//!
//! Each harness gets its own project, config and state folders in the temp
//! directory, an in-memory clipboard and no terminal. Background work (file
//! index, git status, live grep) is waited for after every step, so a
//! snapshot always shows a settled screen.
//!
//! ```ignore
//! let mut e = Harness::builder().file("app.js", "let a = 1;\n").open("app.js").start();
//! e.keys("end").type_text(" // one");
//! e.snapshot("editing_type_at_end");
//! ```
//!
//! A snapshot that does not match fails the test, prints a line diff and
//! writes the actual screen next to the expected one as `<name>.actual.txt`.
//! `cargo update-snapshots` (or `UPDATE_SNAPSHOTS=1`) accepts new screens.

use crate::app::{App, AppEvent, Start, parse_target};
use crate::clipboard::Clipboard;
use crate::config::Paths;
use crate::ui::surface::{Style, Surface};
use crate::ui::theme::Theme;
use crossterm::event::{Event, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{self, Receiver};
use std::time::{Duration, Instant};

const SETTLE_TIMEOUT: Duration = Duration::from_secs(10);
const SNAPSHOT_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/e2e/snapshots");
const STYLE_SYMBOLS: &str = "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789";

/// Folders removed when the harness is dropped.
struct TempDir(PathBuf);

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn temp_dir() -> TempDir {
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    let dir = std::env::temp_dir().join(format!(
        "yate-e2e-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::SeqCst)
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("create the temp folder of the test");
    TempDir(dir)
}

fn write(path: &Path, content: &str) {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).expect("create folders of the test file");
    }
    std::fs::write(path, content).expect("write the test file");
}

/// Runs git in `dir`, isolated from the environment of the test runner.
fn git(dir: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .args(["-c", "user.name=yate", "-c", "user.email=yate@example.com"])
        .args([
            "-c",
            "commit.gpgsign=false",
            "-c",
            "init.defaultBranch=main",
        ])
        .args(args)
        .current_dir(dir)
        .env_remove("GIT_DIR")
        .env_remove("GIT_WORK_TREE")
        .env_remove("GIT_INDEX_FILE")
        .output()
        .expect("git must be installed for the end-to-end tests");
    assert!(
        output.status.success(),
        "git {} failed: {}",
        args.join(" "),
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).into_owned()
}

/// Describes the project a test starts with.
pub struct Builder {
    files: Vec<(String, String)>,
    config: Vec<(String, String)>,
    working_dir: Option<String>,
    git: bool,
    size: (u16, u16),
    open: Option<String>,
}

impl Builder {
    /// A file of the project, path relative to the project root.
    pub fn file(mut self, path: &str, content: &str) -> Self {
        self.files.push((path.to_string(), content.to_string()));
        self
    }

    /// A file in the yate folder (`~/.yate`), e.g. `yate.json` or
    /// `plugins/sort.js`.
    pub fn config(mut self, path: &str, content: &str) -> Self {
        self.config.push((path.to_string(), content.to_string()));
        self
    }

    /// Runs yate from a folder of the project (default: the project root),
    /// so `yate.json` files on the way to it apply.
    pub fn working_dir(mut self, path: &str) -> Self {
        self.working_dir = Some(path.to_string());
        self
    }

    /// Makes the project a git repository (branch `main`) and commits all
    /// files.
    pub fn git(mut self) -> Self {
        self.git = true;
        self
    }

    /// Terminal size, 80x20 by default.
    pub fn size(mut self, width: u16, height: u16) -> Self {
        self.size = (width, height);
        self
    }

    /// Opens a file like `yate <path>` (`app.js` or `app.js:12`). Without
    /// it, yate starts in the project folder with the file tree.
    pub fn open(mut self, target: &str) -> Self {
        self.open = Some(target.to_string());
        self
    }

    pub fn start(self) -> Harness {
        let temp = temp_dir();
        let root = temp.0.join("project");
        let paths = Paths {
            dir: temp.0.join("home/.yate"),
            home: Some(temp.0.join("home")),
            cwd: root.join(self.working_dir.as_deref().unwrap_or("")),
            ceiling: Some(temp.0.clone()),
        };
        std::fs::create_dir_all(&root).expect("create the test project");
        for (path, content) in &self.files {
            write(&root.join(path), content);
        }
        for (path, content) in &self.config {
            write(&paths.dir.join(path), content);
        }
        std::fs::create_dir_all(&paths.cwd).expect("create the working directory of the test");
        if self.git {
            git(&root, &["init", "-q"]);
            git(&root, &["add", "-A"]);
            git(&root, &["commit", "-q", "--allow-empty", "-m", "initial"]);
        }
        let (file, line) = match &self.open {
            Some(target) => {
                let (path, line) = parse_target(target);
                (Some(root.join(path)), line)
            }
            None => (None, None),
        };
        let (tx, rx) = mpsc::channel();
        let app = App::start(
            Start {
                root: root.clone(),
                file,
                line,
                paths,
                clipboard: Clipboard::in_memory(),
                size: self.size,
            },
            tx,
        );
        let mut harness = Harness {
            app,
            rx,
            root,
            _temp: temp,
            size: self.size,
            steps: Vec::new(),
        };
        harness.step(match &self.open {
            Some(target) => format!("start: yate {target}"),
            None => "start: yate (project folder)".to_string(),
        });
        harness.settle();
        harness
    }
}

/// The editor under test. Every step waits until background work is done.
pub struct Harness {
    app: App,
    rx: Receiver<AppEvent>,
    root: PathBuf,
    _temp: TempDir,
    size: (u16, u16),
    steps: Vec<String>,
}

impl Harness {
    pub fn builder() -> Builder {
        Builder {
            files: Vec::new(),
            config: Vec::new(),
            working_dir: None,
            git: false,
            size: (80, 20),
            open: None,
        }
    }

    fn step(&mut self, description: String) {
        self.steps.push(description);
    }

    /// Handles background events until no job is pending.
    fn settle(&mut self) {
        let started = Instant::now();
        loop {
            while let Ok(event) = self.rx.try_recv() {
                self.app.handle_event(event);
            }
            if self.app.pending_jobs == 0 {
                break;
            }
            assert!(
                started.elapsed() < SETTLE_TIMEOUT,
                "background work did not finish within {SETTLE_TIMEOUT:?} ({} jobs pending) after: {}",
                self.app.pending_jobs,
                self.steps.last().map(String::as_str).unwrap_or("start")
            );
            if let Ok(event) = self.rx.recv_timeout(Duration::from_millis(10)) {
                self.app.handle_event(event);
            }
        }
        self.app.take_terminal_writes();
    }

    /// Sends one terminal event the way the event loop does: after a frame,
    /// so the layout used by mouse clicks is current.
    fn send(&mut self, event: Event) {
        self.app.draw(self.size.0, self.size.1);
        self.app.handle_event(AppEvent::Terminal(event));
        self.settle();
    }

    /// Presses keys, separated by spaces: `"ctrl+p"`, `"down down enter"`,
    /// `"alt+shift+down"`, `"alt+tab"`, `"esc"`. Use `type_text` for text.
    pub fn keys(&mut self, keys: &str) -> &mut Self {
        self.step(format!("keys: {keys}"));
        for name in keys.split_whitespace() {
            let key = crate::keymap::parse_key(name)
                .unwrap_or_else(|err| panic!("keys(\"{keys}\"): {err}"));
            self.send(Event::Key(key));
        }
        self
    }

    /// Types text one character at a time, like a person; `\n` is Enter.
    pub fn type_text(&mut self, text: &str) -> &mut Self {
        self.step(format!("type: {text:?}"));
        for c in text.chars() {
            let name = match c {
                '\n' => "enter".to_string(),
                '\t' => "tab".to_string(),
                ' ' => "space".to_string(),
                c => c.to_string(),
            };
            let key = crate::keymap::parse_key(&name).expect("typed characters are keys");
            let key = if c.is_uppercase() {
                crossterm::event::KeyEvent::new(
                    crossterm::event::KeyCode::Char(c),
                    KeyModifiers::SHIFT,
                )
            } else {
                key
            };
            self.send(Event::Key(key));
        }
        self
    }

    /// Pastes text the way a terminal does with bracketed paste.
    pub fn paste(&mut self, text: &str) -> &mut Self {
        self.step(format!("paste: {text:?}"));
        self.send(Event::Paste(text.to_string()));
        self
    }

    fn mouse(&mut self, kind: MouseEventKind, column: u16, row: u16) {
        self.mouse_with(kind, column, row, KeyModifiers::NONE);
    }

    fn mouse_with(&mut self, kind: MouseEventKind, column: u16, row: u16, modifiers: KeyModifiers) {
        self.send(Event::Mouse(MouseEvent {
            kind,
            column,
            row,
            modifiers,
        }));
    }

    /// Left click on a screen cell (0-based column and row, as in the
    /// snapshot frame).
    pub fn click(&mut self, column: u16, row: u16) -> &mut Self {
        self.step(format!("click: {column},{row}"));
        self.app.last_click = None;
        self.mouse(MouseEventKind::Down(MouseButton::Left), column, row);
        self.mouse(MouseEventKind::Up(MouseButton::Left), column, row);
        self
    }

    /// Left click with Ctrl held (go to definition).
    pub fn ctrl_click(&mut self, column: u16, row: u16) -> &mut Self {
        self.step(format!("ctrl+click: {column},{row}"));
        self.app.last_click = None;
        let ctrl = KeyModifiers::CONTROL;
        self.mouse_with(MouseEventKind::Down(MouseButton::Left), column, row, ctrl);
        self.mouse_with(MouseEventKind::Up(MouseButton::Left), column, row, ctrl);
        self
    }

    pub fn double_click(&mut self, column: u16, row: u16) -> &mut Self {
        self.step(format!("double click: {column},{row}"));
        self.app.last_click = None;
        for _ in 0..2 {
            self.mouse(MouseEventKind::Down(MouseButton::Left), column, row);
            self.mouse(MouseEventKind::Up(MouseButton::Left), column, row);
        }
        self
    }

    /// Mouse drag from one cell to another with the left button.
    pub fn drag(&mut self, from: (u16, u16), to: (u16, u16)) -> &mut Self {
        self.step(format!("drag: {},{} -> {},{}", from.0, from.1, to.0, to.1));
        self.app.last_click = None;
        self.mouse(MouseEventKind::Down(MouseButton::Left), from.0, from.1);
        self.mouse(MouseEventKind::Drag(MouseButton::Left), to.0, to.1);
        self.mouse(MouseEventKind::Up(MouseButton::Left), to.0, to.1);
        self
    }

    /// Mouse wheel over a cell; positive `steps` scroll down.
    pub fn scroll(&mut self, column: u16, row: u16, steps: i32) -> &mut Self {
        self.step(format!("scroll: {column},{row} by {steps}"));
        let kind = if steps > 0 {
            MouseEventKind::ScrollDown
        } else {
            MouseEventKind::ScrollUp
        };
        for _ in 0..steps.unsigned_abs() {
            self.mouse(kind, column, row);
        }
        self
    }

    pub fn resize(&mut self, width: u16, height: u16) -> &mut Self {
        self.step(format!("resize: {width}x{height}"));
        self.size = (width, height);
        self.send(Event::Resize(width, height));
        self
    }

    /// The terminal (or tmux pane) loses focus.
    pub fn focus_lost(&mut self) -> &mut Self {
        self.step("focus lost".to_string());
        self.send(Event::FocusLost);
        self
    }

    /// The terminal gets focus back: yate reloads changed files and
    /// refreshes git.
    pub fn focus_gained(&mut self) -> &mut Self {
        self.step("focus gained".to_string());
        self.send(Event::FocusGained);
        self
    }

    /// Reports the result of the daily update check, which never runs in
    /// tests: `Some(version)` when a newer release exists.
    pub fn update_check(&mut self, latest: Option<&str>) -> &mut Self {
        self.step(format!("update check: {}", latest.unwrap_or("up to date")));
        self.app.draw(self.size.0, self.size.1);
        self.app
            .handle_event(AppEvent::UpdateCheck(latest.map(str::to_string)));
        self.settle();
        self
    }

    /// Runs a command by id, like the command palette does.
    pub fn command(&mut self, id: &str) -> &mut Self {
        self.step(format!("command: {id}"));
        self.app.draw(self.size.0, self.size.1);
        self.app.run_command(id);
        self.settle();
        self
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Reads a project file from disk.
    pub fn read_file(&self, path: &str) -> String {
        std::fs::read_to_string(self.root.join(path))
            .unwrap_or_else(|err| panic!("read {path}: {err}"))
    }

    pub fn file_exists(&self, path: &str) -> bool {
        self.root.join(path).exists()
    }

    /// Changes a project file outside the editor.
    pub fn write_file(&mut self, path: &str, content: &str) -> &mut Self {
        self.step(format!("write file outside the editor: {path}"));
        write(&self.root.join(path), content);
        self
    }

    /// Runs git in the project (e.g. `&["commit", "-am", "x"]`).
    pub fn git(&mut self, args: &[&str]) -> String {
        self.step(format!("git {}", args.join(" ")));
        git(&self.root, args)
    }

    /// What a paste would insert.
    pub fn clipboard(&self) -> String {
        self.app.clipboard.paste().0
    }

    pub fn app(&self) -> &App {
        &self.app
    }

    pub fn app_mut(&mut self) -> &mut App {
        &mut self.app
    }

    /// The text of the file in the focused pane.
    pub fn editor_text(&self) -> String {
        self.app.doc().text()
    }

    /// Draws a frame and returns it.
    pub fn screen(&mut self) -> Screen {
        let (surface, cursor) = self.app.draw(self.size.0, self.size.1);
        Screen { surface, cursor }
    }

    #[track_caller]
    pub fn assert_screen_contains(&mut self, text: &str) -> &mut Self {
        let screen = self.screen();
        assert!(
            screen.contains(text),
            "the screen does not contain {text:?}:\n{}",
            screen.frame()
        );
        self
    }

    #[track_caller]
    pub fn assert_screen_lacks(&mut self, text: &str) -> &mut Self {
        let screen = self.screen();
        assert!(
            !screen.contains(text),
            "the screen contains {text:?}:\n{}",
            screen.frame()
        );
        self
    }

    /// Compares the screen text with `tests/e2e/snapshots/<name>.txt`.
    #[track_caller]
    pub fn snapshot(&mut self, name: &str) -> &mut Self {
        let screen = self.screen();
        let text = self.snapshot_text(name, &screen, false);
        compare(name, &text);
        self
    }

    /// Like `snapshot`, plus a second frame with one letter per style and a
    /// legend of colors, to check highlighting, selection and focus.
    #[track_caller]
    pub fn snapshot_styles(&mut self, name: &str) -> &mut Self {
        let screen = self.screen();
        let text = self.snapshot_text(name, &screen, true);
        compare(name, &text);
        self
    }

    fn snapshot_text(&self, name: &str, screen: &Screen, styles: bool) -> String {
        let mut out = format!("# {name}\n");
        for step in &self.steps {
            out.push_str(&format!("# {step}\n"));
        }
        let cursor = match screen.cursor {
            Some((x, y)) => format!("cursor at column {x}, row {y}"),
            None => "cursor hidden".to_string(),
        };
        out.push_str(&format!(
            "# {}x{}, {cursor}\n",
            screen.surface.width, screen.surface.height
        ));
        out.push_str(&screen.frame());
        if styles {
            out.push_str("\nstyles\n");
            out.push_str(&screen.style_frame(&self.app.theme));
        }
        out
    }
}

/// One drawn frame.
pub struct Screen {
    pub surface: Surface,
    /// The terminal cursor: column and row.
    pub cursor: Option<(u16, u16)>,
}

impl Screen {
    /// The rows as text, without trailing spaces.
    pub fn lines(&self) -> Vec<String> {
        (0..self.surface.height)
            .map(|y| self.row(y).trim_end().to_string())
            .collect()
    }

    fn row(&self, y: u16) -> String {
        let width = self.surface.width as usize;
        let start = y as usize * width;
        self.surface.cells[start..start + width]
            .iter()
            .filter(|cell| !cell.continuation)
            .map(|cell| cell.symbol.as_str())
            .collect()
    }

    pub fn line(&self, y: u16) -> String {
        self.row(y).trim_end().to_string()
    }

    pub fn contains(&self, text: &str) -> bool {
        (0..self.surface.height).any(|y| self.row(y).contains(text))
    }

    /// The screen in a box, so its edges and trailing spaces are visible.
    pub fn frame(&self) -> String {
        let width = self.surface.width as usize;
        let mut out = format!("┌{}┐\n", "─".repeat(width));
        for y in 0..self.surface.height {
            out.push_str(&format!("│{}│\n", self.row(y)));
        }
        out.push_str(&format!("└{}┘\n", "─".repeat(width)));
        out
    }

    /// One letter per distinct style, and the legend. Blank cells only
    /// count their background.
    pub fn style_frame(&self, theme: &Theme) -> String {
        let width = self.surface.width as usize;
        let mut symbols: HashMap<Style, char> = HashMap::new();
        let mut legend: Vec<(char, Style)> = Vec::new();
        let mut out = format!("┌{}┐\n", "─".repeat(width));
        for y in 0..self.surface.height as usize {
            out.push('│');
            for x in 0..width {
                let mut index = y * width + x;
                while self.surface.cells[index].continuation && index > y * width {
                    index -= 1;
                }
                let cell = &self.surface.cells[index];
                let style = if cell.symbol.trim().is_empty() {
                    Style {
                        bg: cell.style.bg,
                        ..Style::default()
                    }
                } else {
                    cell.style
                };
                let next = legend.len();
                let symbol = *symbols.entry(style).or_insert_with(|| {
                    let symbol = STYLE_SYMBOLS.chars().nth(next).unwrap_or('?');
                    legend.push((symbol, style));
                    symbol
                });
                out.push(symbol);
            }
            out.push_str("│\n");
        }
        out.push_str(&format!("└{}┘\n", "─".repeat(width)));
        for (symbol, style) in legend {
            out.push_str(&format!("{symbol}  {}\n", describe(theme, style)));
        }
        out
    }
}

fn describe(theme: &Theme, style: Style) -> String {
    let mut parts = Vec::new();
    if let Some(fg) = style.fg {
        parts.push(format!("fg {}", theme.color_name(fg)));
    }
    if let Some(bg) = style.bg {
        parts.push(format!("bg {}", theme.color_name(bg)));
    }
    if style.bold {
        parts.push("bold".to_string());
    }
    if style.italic {
        parts.push("italic".to_string());
    }
    if style.underline {
        parts.push("underline".to_string());
    }
    if parts.is_empty() {
        "default".to_string()
    } else {
        parts.join(", ")
    }
}

#[track_caller]
fn compare(name: &str, actual: &str) {
    assert!(
        !name.is_empty()
            && name
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '-'),
        "snapshot names use a-z, 0-9, _ and -: {name:?}"
    );
    let dir = Path::new(SNAPSHOT_DIR);
    let expected_path = dir.join(format!("{name}.txt"));
    let actual_path = dir.join(format!("{name}.actual.txt"));
    std::fs::create_dir_all(dir).expect("create the snapshot folder");
    let update = std::env::var_os("UPDATE_SNAPSHOTS").is_some_and(|v| !v.is_empty() && v != "0");
    let expected = std::fs::read_to_string(&expected_path).ok();
    if expected.as_deref() == Some(actual) {
        let _ = std::fs::remove_file(&actual_path);
        return;
    }
    if update {
        std::fs::write(&expected_path, actual).expect("write the snapshot");
        let _ = std::fs::remove_file(&actual_path);
        return;
    }
    std::fs::write(&actual_path, actual).expect("write the actual screen");
    let Some(expected) = expected else {
        panic!(
            "\nno snapshot {name} yet; the screen is in\n  {}\n\n{actual}\n\
             Check it, then accept it with: cargo update-snapshots\n",
            actual_path.display()
        );
    };
    panic!(
        "\nsnapshot {name} does not match\n  expected: {}\n  actual:   {}\n\n{}\n\
         If the new screen is right, accept it with: cargo update-snapshots\n",
        expected_path.display(),
        actual_path.display(),
        line_diff(&expected, actual)
    );
}

/// The whole snapshot with changed lines marked `-` (expected) and `+`
/// (actual).
fn line_diff(expected: &str, actual: &str) -> String {
    use similar::{ChangeTag, TextDiff};
    let diff = TextDiff::from_lines(expected, actual);
    let mut out = String::new();
    for change in diff.iter_all_changes() {
        let marker = match change.tag() {
            ChangeTag::Equal => "  ",
            ChangeTag::Delete => "- ",
            ChangeTag::Insert => "+ ",
        };
        out.push_str(marker);
        out.push_str(change.value().trim_end_matches('\n'));
        out.push('\n');
    }
    out
}
