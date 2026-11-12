//! JavaScript plugins in an embedded QuickJS engine.
//!
//! Plugins see a snapshot of the editor taken before each call (`yate.editor`
//! reads it) and queue actions (edits, notifications, prompts...) that the
//! app applies when the call returns. Commands, key bindings and event
//! listeners registered by plugins are collected the same way.

use crate::config::OptionValue;
use ropey::Rope;
use rquickjs::{CatchResultExt, CaughtError, Context, Ctx, Function, Object, Runtime, Value};
use std::cell::RefCell;
use std::path::Path;
use std::process::{Command, Stdio};
use std::rc::Rc;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

const PRELUDE: &str = include_str!("prelude.js");
pub const BUNDLED: &[(&str, &str)] = &[
    ("git", include_str!("builtin/git.js")),
    ("format", include_str!("builtin/format.js")),
];
const CALL_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Default, Clone)]
pub struct Snapshot {
    pub root: String,
    pub path: Option<String>,
    pub language: Option<String>,
    pub rope: Rope,
    pub anchor: usize,
    pub head: usize,
}

impl Snapshot {
    fn line_col(&self, pos: usize) -> (i32, i32) {
        let pos = pos.min(self.rope.len_chars());
        let line = self.rope.char_to_line(pos);
        (line as i32, (pos - self.rope.line_to_char(line)) as i32)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Action {
    Notify(String, bool),
    /// A dialog: title and text.
    Show(String, String),
    Insert(String),
    ReplaceSelection(String),
    SetText(String),
    SetCursor(usize, usize),
    Select(usize, usize, usize, usize),
    OpenFile(String, Option<usize>),
    RunCommand(String),
    Prompt {
        label: String,
        initial: String,
        id: u32,
    },
}

#[derive(Default)]
pub struct Bridge {
    pub snapshot: Snapshot,
    pub actions: Vec<Action>,
    /// Newly registered commands: id and title.
    pub commands: Vec<(String, String)>,
    /// Newly bound keys: key, command, context.
    pub bindings: Vec<(String, String, String)>,
    pub options: Vec<(String, OptionValue)>,
    pub events: Vec<String>,
}

pub struct PluginHost {
    _runtime: Runtime,
    context: Context,
    pub bridge: Rc<RefCell<Bridge>>,
    deadline: Arc<Mutex<Option<Instant>>>,
    /// How long one call may run before it is interrupted.
    pub timeout: Duration,
}

impl PluginHost {
    pub fn new() -> Result<PluginHost, String> {
        let runtime = Runtime::new().map_err(|err| err.to_string())?;
        runtime.set_memory_limit(256 * 1024 * 1024);
        runtime.set_max_stack_size(1024 * 1024);
        let deadline: Arc<Mutex<Option<Instant>>> = Arc::new(Mutex::new(None));
        let watched = deadline.clone();
        runtime.set_interrupt_handler(Some(Box::new(move || {
            watched
                .lock()
                .ok()
                .and_then(|deadline| *deadline)
                .is_some_and(|deadline| Instant::now() > deadline)
        })));
        let context = Context::full(&runtime).map_err(|err| err.to_string())?;
        let bridge = Rc::new(RefCell::new(Bridge::default()));
        let host = PluginHost {
            _runtime: runtime,
            context,
            bridge,
            deadline,
            timeout: CALL_TIMEOUT,
        };
        host.context.with(|ctx| -> Result<(), String> {
            install_natives(&ctx, host.bridge.clone()).map_err(|err| err.to_string())?;
            ctx.eval::<(), _>(PRELUDE)
                .catch(&ctx)
                .map_err(|err| caught_message(&err))
        })?;
        Ok(host)
    }

    /// Loads the plugins that `yate.json` switches on: the bundled ones
    /// unless set to `false`, then `plugins/<name>.js` from the yate
    /// folder for every name set to `true`, in name order. Returns the
    /// errors of plugins that failed.
    pub fn load_all(&mut self, yate_dir: &Path, switches: &[(String, bool)]) -> Vec<String> {
        let enabled = |name: &str| {
            switches
                .iter()
                .rev()
                .find(|(switched, _)| switched == name)
                .map(|(_, on)| *on)
        };
        let mut errors = Vec::new();
        for (name, source) in BUNDLED {
            if enabled(name) == Some(false) {
                continue;
            }
            if let Err(err) = self.load(&format!("builtin/{name}.js"), source) {
                errors.push(format!("{name}: {err}"));
            }
        }
        let mut names: Vec<&str> = switches
            .iter()
            .map(|(name, _)| name.as_str())
            .filter(|name| enabled(name) == Some(true))
            .filter(|name| !BUNDLED.iter().any(|(bundled, _)| bundled == name))
            .collect();
        names.sort();
        names.dedup();
        for name in names {
            let file = format!("plugins/{name}.js");
            let result = std::fs::read_to_string(yate_dir.join(&file))
                .map_err(|err| match err.kind() {
                    std::io::ErrorKind::NotFound => {
                        format!("{file} not found in ~/.yate")
                    }
                    _ => err.to_string(),
                })
                .and_then(|source| self.load(&file, &source));
            if let Err(err) = result {
                errors.push(format!("{name}: {err}"));
            }
        }
        errors
    }

    /// Runs a plugin as an ES module, so its top-level names stay private.
    pub fn load(&mut self, name: &str, source: &str) -> Result<(), String> {
        self.with_deadline(|ctx| {
            let promise = rquickjs::Module::evaluate(ctx.clone(), name, source)
                .catch(ctx)
                .map_err(|err| caught_message(&err))?;
            promise
                .finish::<()>()
                .catch(ctx)
                .map_err(|err| caught_message(&err))
        })
    }

    pub fn run_command(&mut self, id: &str) -> Result<(), String> {
        self.dispatch("command", id, None)
    }

    pub fn emit(&mut self, event: &str, arg: Option<String>) -> Result<(), String> {
        if !self.bridge.borrow().events.iter().any(|e| e == event) {
            return Ok(());
        }
        self.dispatch("event", event, arg)
    }

    pub fn answer_prompt(&mut self, id: u32, answer: Option<String>) -> Result<(), String> {
        self.dispatch("prompt", &id.to_string(), answer)
    }

    #[cfg(test)]
    pub fn eval(&mut self, source: &str) -> Result<String, String> {
        self.with_deadline(|ctx| {
            let value: Value = ctx
                .eval(source)
                .catch(ctx)
                .map_err(|err| caught_message(&err))?;
            Ok(display_value(&value))
        })
    }

    fn dispatch(&mut self, kind: &str, name: &str, arg: Option<String>) -> Result<(), String> {
        self.with_deadline(|ctx| {
            let dispatch: Function = ctx
                .globals()
                .get("__yateDispatch")
                .map_err(|err| err.to_string())?;
            let result: Value = dispatch
                .call((kind, name, arg))
                .catch(ctx)
                .map_err(|err| caught_message(&err))?;
            if let Some(promise) = result.as_promise() {
                promise
                    .finish::<Value>()
                    .catch(ctx)
                    .map_err(|err| caught_message(&err))?;
            }
            Ok(())
        })
    }

    fn with_deadline<T>(
        &mut self,
        run: impl FnOnce(&Ctx<'_>) -> Result<T, String>,
    ) -> Result<T, String> {
        if let Ok(mut deadline) = self.deadline.lock() {
            *deadline = Some(Instant::now() + self.timeout);
        }
        let result = self.context.with(|ctx| run(&ctx));
        if let Ok(mut deadline) = self.deadline.lock() {
            *deadline = None;
        }
        result
    }

    pub fn take_actions(&mut self) -> Vec<Action> {
        std::mem::take(&mut self.bridge.borrow_mut().actions)
    }
}

fn display_value(value: &Value) -> String {
    if value.is_undefined() {
        return "undefined".to_string();
    }
    value
        .get::<rquickjs::convert::Coerced<String>>()
        .map(|s| s.0)
        .unwrap_or_default()
}

fn caught_message(err: &CaughtError<'_>) -> String {
    match err {
        CaughtError::Exception(exception) => {
            let message = exception.message().unwrap_or_default();
            match exception.stack() {
                Some(stack) => match stack_location(&stack) {
                    Some(location) => format!("{message} ({location})"),
                    None => message,
                },
                _ => message,
            }
        }
        CaughtError::Value(value) => display_value(value),
        CaughtError::Error(error) => error.to_string(),
    }
}

/// `file.js:2:5` from the first stack frame that names a file.
fn stack_location(stack: &str) -> Option<String> {
    stack.lines().find_map(|line| {
        let inner = line.rsplit_once('(')?.1.strip_suffix(')')?;
        (inner.contains(':') && !inner.starts_with('<')).then(|| inner.to_string())
    })
}

fn to_option_value(value: &Value) -> OptionValue {
    if let Some(flag) = value.as_bool() {
        OptionValue::Bool(flag)
    } else if let Some(number) = value.as_number() {
        OptionValue::Number(number)
    } else {
        OptionValue::Text(display_value(value))
    }
}

fn index(value: i32) -> usize {
    value.max(0) as usize
}

fn install_natives<'js>(ctx: &Ctx<'js>, bridge: Rc<RefCell<Bridge>>) -> rquickjs::Result<()> {
    let native = Object::new(ctx.clone())?;

    macro_rules! read {
        ($name:literal, |$snapshot:ident $(, $arg:ident: $ty:ty)*| -> $ret:ty $body:block) => {{
            let bridge = bridge.clone();
            native.set(
                $name,
                Function::new(ctx.clone(), move |$($arg: $ty),*| -> $ret {
                    let bridge = bridge.borrow();
                    let $snapshot = &bridge.snapshot;
                    $body
                })?,
            )?;
        }};
    }
    macro_rules! act {
        ($name:literal, |$($arg:ident: $ty:ty),*| $action:expr) => {{
            let bridge = bridge.clone();
            native.set(
                $name,
                Function::new(ctx.clone(), move |$($arg: $ty),*| {
                    bridge.borrow_mut().actions.push($action);
                })?,
            )?;
        }};
    }

    read!("root", |s| -> String { s.root.clone() });
    read!("path", |s| -> Option<String> { s.path.clone() });
    read!("language", |s| -> Option<String> { s.language.clone() });
    read!("text", |s| -> String { s.rope.to_string() });
    read!("lineCount", |s| -> i32 { s.rope.len_lines() as i32 });
    read!("line", |s, line: i32| -> String {
        let line = index(line);
        if line >= s.rope.len_lines() {
            return String::new();
        }
        s.rope
            .line(line)
            .to_string()
            .trim_end_matches('\n')
            .to_string()
    });
    read!("cursor", |s| -> Vec<i32> {
        let (line, col) = s.line_col(s.head);
        vec![line, col]
    });
    read!("selection", |s| -> Vec<i32> {
        let (start_line, start_col) = s.line_col(s.anchor.min(s.head));
        let (end_line, end_col) = s.line_col(s.anchor.max(s.head));
        vec![start_line, start_col, end_line, end_col]
    });
    read!("selectedText", |s| -> String {
        let len = s.rope.len_chars();
        let (start, end) = (s.anchor.min(s.head).min(len), s.anchor.max(s.head).min(len));
        s.rope.slice(start..end).to_string()
    });

    act!("insert", |text: String| Action::Insert(text));
    act!("replaceSelection", |text: String| Action::ReplaceSelection(
        text
    ));
    act!("setText", |text: String| Action::SetText(text));
    act!("setCursor", |line: i32, col: i32| Action::SetCursor(
        index(line),
        index(col)
    ));
    act!("select", |a: i32, b: i32, c: i32, d: i32| Action::Select(
        index(a),
        index(b),
        index(c),
        index(d)
    ));
    act!("notify", |message: String, error: bool| Action::Notify(
        message, error
    ));
    act!("log", |message: String| Action::Notify(message, false));
    act!("show", |title: String, text: String| Action::Show(
        title, text
    ));
    act!("openFile", |path: String, line: i32| Action::OpenFile(
        path,
        (line >= 0).then_some(line as usize)
    ));
    act!("runCommand", |id: String| Action::RunCommand(id));
    act!("prompt", |label: String, initial: String, id: u32| {
        Action::Prompt { label, initial, id }
    });

    {
        let bridge = bridge.clone();
        native.set(
            "registerCommand",
            Function::new(ctx.clone(), move |id: String, title: String| {
                bridge.borrow_mut().commands.push((id, title));
            })?,
        )?;
    }
    {
        let bridge = bridge.clone();
        native.set(
            "bindKey",
            Function::new(
                ctx.clone(),
                move |key: String, command: String, context: String| {
                    bridge.borrow_mut().bindings.push((key, command, context));
                },
            )?,
        )?;
    }
    {
        let bridge = bridge.clone();
        native.set(
            "listen",
            Function::new(ctx.clone(), move |event: String| {
                bridge.borrow_mut().events.push(event);
            })?,
        )?;
    }
    {
        let bridge = bridge.clone();
        native.set(
            "setOption",
            Function::new(ctx.clone(), move |name: String, value: Value<'js>| {
                bridge
                    .borrow_mut()
                    .options
                    .push((name, to_option_value(&value)));
            })?,
        )?;
    }
    {
        let bridge = bridge.clone();
        native.set(
            "exec",
            Function::new(
                ctx.clone(),
                move |ctx: Ctx<'js>,
                      command: String,
                      args: Vec<String>,
                      input: Option<String>,
                      cwd: Option<String>|
                      -> rquickjs::Result<Object<'js>> {
                    let cwd = cwd.unwrap_or_else(|| bridge.borrow().snapshot.root.clone());
                    let result = Object::new(ctx.clone())?;
                    match run_process(&command, &args, input.as_deref(), &cwd) {
                        Ok((code, stdout, stderr)) => {
                            result.set("code", code)?;
                            result.set("stdout", stdout)?;
                            result.set("stderr", stderr)?;
                        }
                        Err(err) => {
                            result.set("code", -1)?;
                            result.set("stdout", "")?;
                            result.set("stderr", err)?;
                        }
                    }
                    Ok(result)
                },
            )?,
        )?;
    }

    ctx.globals().set("__yateNative", native)?;
    Ok(())
}

fn run_process(
    command: &str,
    args: &[String],
    input: Option<&str>,
    cwd: &str,
) -> Result<(i32, String, String), String> {
    use std::io::Write;
    let mut child = Command::new(command)
        .args(args)
        .current_dir(cwd)
        .stdin(if input.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|err| format!("{command}: {err}"))?;
    if let (Some(input), Some(mut stdin)) = (input, child.stdin.take()) {
        let input = input.to_string();
        std::thread::spawn(move || {
            let _ = stdin.write_all(input.as_bytes());
        });
    }
    let output = child.wait_with_output().map_err(|err| err.to_string())?;
    Ok((
        output.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&output.stdout).into_owned(),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn host() -> PluginHost {
        let mut host = PluginHost::new().unwrap();
        let errors = host.load_all(Path::new("/nonexistent"), &[]);
        assert!(errors.is_empty(), "{errors:?}");
        host
    }

    #[test]
    fn builtin_plugins_register_commands() {
        let host = host();
        let bridge = host.bridge.borrow();
        assert!(bridge.commands.iter().any(|(id, _)| id == "git.blame"));
        assert!(
            bridge
                .commands
                .iter()
                .any(|(id, _)| id == "format.prettier")
        );
    }

    #[test]
    fn commands_read_the_snapshot_and_queue_actions() {
        let mut host = host();
        host.bridge.borrow_mut().snapshot = Snapshot {
            rope: Rope::from_str("hello\nworld"),
            anchor: 6,
            head: 11,
            ..Snapshot::default()
        };
        host.load(
            "test.js",
            r#"yate.registerCommand("t.upper", () => {
                const sel = yate.editor.selection();
                yate.editor.replaceSelection(sel.text.toUpperCase() + sel.start.line);
            });"#,
        )
        .unwrap();
        host.run_command("t.upper").unwrap();
        assert_eq!(
            host.take_actions(),
            vec![Action::ReplaceSelection("WORLD1".to_string())]
        );
    }

    #[test]
    fn yate_json_switches_bundled_and_user_plugins() {
        let dir = std::env::temp_dir().join(format!("yate-plugins-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("plugins")).unwrap();
        std::fs::write(
            dir.join("plugins/hello.js"),
            "yate.registerCommand('hello', () => {});",
        )
        .unwrap();
        std::fs::write(
            dir.join("plugins/unlisted.js"),
            "yate.registerCommand('unlisted', () => {});",
        )
        .unwrap();
        let mut host = PluginHost::new().unwrap();
        let switches = [
            ("git".to_string(), false),
            ("hello".to_string(), true),
            ("missing".to_string(), true),
        ];
        let errors = host.load_all(&dir, &switches);
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(errors, ["missing: plugins/missing.js not found in ~/.yate"]);
        let bridge = host.bridge.borrow();
        let ids: Vec<&str> = bridge.commands.iter().map(|(id, _)| id.as_str()).collect();
        assert!(
            ids.contains(&"hello") && ids.contains(&"format.prettier"),
            "{ids:?}"
        );
        assert!(
            !ids.contains(&"git.blame") && !ids.contains(&"unlisted"),
            "{ids:?}"
        );
    }

    #[test]
    fn errors_and_endless_loops_are_reported() {
        let mut host = host();
        let err = host.load("bad.js", "throw new Error('boom')").unwrap_err();
        assert!(err.contains("boom"), "{err}");
        host.load(
            "loop.js",
            "yate.registerCommand('t.loop', () => { while (true) {} });",
        )
        .unwrap();
        host.timeout = Duration::from_millis(200);
        let started = Instant::now();
        assert!(host.run_command("t.loop").is_err());
        assert!(started.elapsed() < Duration::from_secs(3));
    }

    #[test]
    fn the_example_plugin_loads() {
        let mut host = host();
        host.load(
            "plugins/extras.js",
            include_str!("../../examples/plugins/extras.js"),
        )
        .unwrap();
        let bridge = host.bridge.borrow();
        assert!(bridge.commands.iter().any(|(id, _)| id == "lines.sort"));
        assert!(bridge.events.iter().any(|e| e == "beforeSave"));
    }

    #[test]
    fn exec_runs_processes() {
        let mut host = host();
        let out = host
            .eval("JSON.stringify(yate.exec('sh', ['-c', 'cat; echo err >&2'], { input: 'hi', cwd: '/' }))")
            .unwrap();
        assert_eq!(out, r#"{"code":0,"stdout":"hi","stderr":"err\n"}"#);
    }
}
