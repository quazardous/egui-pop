//! Where a terminal's bytes come from: the [`TerminalSource`] trait, the
//! [`Sink`] a source writes its output into, and [`Console`], a
//! read-only source for logs and agent output.

use std::fmt;
use std::sync::{Arc, Mutex, Weak};

use egui_pop_core::lock;
use serde::{Deserialize, Serialize};
use serde_json::{Value as Json, json};

use super::state::Model;

/// A terminal's size, in cells.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Size {
    pub rows: u16,
    pub cols: u16,
}

impl Size {
    pub fn new(rows: u16, cols: u16) -> Size {
        Size {
            rows: rows.max(1),
            cols: cols.max(1),
        }
    }
}

/// A program to run, as a PTY source spawns it. An empty `program` is
/// the user's shell (`$SHELL` on Unix, `cmd.exe` on Windows).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommandLine {
    #[serde(default)]
    pub program: String,
    #[serde(default)]
    pub args: Vec<String>,
    /// The working directory; the application's own when `None`.
    #[serde(default)]
    pub cwd: Option<String>,
    /// Variables added to the application's environment.
    #[serde(default)]
    pub env: Vec<(String, String)>,
}

impl CommandLine {
    /// Runs `program` (found on `PATH`).
    pub fn new(program: impl Into<String>) -> CommandLine {
        CommandLine {
            program: program.into(),
            ..CommandLine::default()
        }
    }

    /// The user's shell.
    pub fn shell() -> CommandLine {
        CommandLine::default()
    }

    pub fn arg(mut self, arg: impl Into<String>) -> CommandLine {
        self.args.push(arg.into());
        self
    }

    pub fn cwd(mut self, dir: impl Into<String>) -> CommandLine {
        self.cwd = Some(dir.into());
        self
    }

    pub fn env(mut self, key: impl Into<String>, value: impl Into<String>) -> CommandLine {
        self.env.push((key.into(), value.into()));
        self
    }
}

/// What a source starts with, from the `terminal.spawn` step's arguments
/// (a before-hook may have redirected the command).
#[derive(Clone, Debug, PartialEq)]
pub struct Spawn {
    /// The program to run, for sources that run one.
    pub command: Option<CommandLine>,
    pub size: Size,
}

/// Where a terminal's bytes come from and where its input goes: a
/// program in a pseudo-terminal (`PtySource`, feature `pty`), a log the
/// application writes ([`Console`]), or the application's own kernel.
///
/// The application picks the source and owns what runs behind it; the
/// terminal only starts it (`terminal.spawn`), writes the user's input to
/// it (`terminal.input`) and stops it (`terminal.exit`). A source runs
/// its program wherever it likes and gives the output to the [`Sink`]
/// from any thread.
pub trait TerminalSource: Send {
    /// The source's kind, shown in steps and collectors (`pty`,
    /// `console`...).
    fn name(&self) -> String;

    /// Whether the terminal sends input to it: a read-only source keeps
    /// the terminal from sending keys (it still selects, copies and
    /// scrolls).
    fn writable(&self) -> bool {
        false
    }

    /// The program it runs, given to `terminal.spawn` as `command`.
    fn command(&self) -> Option<CommandLine> {
        None
    }

    /// Starts: the output goes to `sink`, from any thread, and the end to
    /// [`Sink::exit`]. Returns what to report (a process id...). A source
    /// started again stops what it ran first.
    fn start(&mut self, spawn: &Spawn, sink: Sink) -> Result<Json, String>;

    /// Writes input (keys, a paste, an agent's text, already encoded).
    fn write(&mut self, bytes: &[u8]) -> Result<(), String> {
        let _ = bytes;
        Err("this terminal is read-only".into())
    }

    /// The terminal's size changed.
    fn resize(&mut self, size: Size) {
        let _ = size;
    }

    /// Stops what runs (the session is closed).
    fn stop(&mut self) -> Result<(), String> {
        Ok(())
    }
}

/// The way into a terminal for a source's output: cheap to clone, usable
/// from any thread. A sink belongs to one session: once the session ends
/// (closed, restarted) or the terminal is dropped, its output goes
/// nowhere and [`Sink::output`] says so.
#[derive(Clone)]
pub struct Sink {
    model: Weak<Mutex<Model>>,
    session: u64,
}

impl fmt::Debug for Sink {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Sink")
            .field("session", &self.session)
            .field("open", &self.is_open())
            .finish()
    }
}

impl Sink {
    pub(crate) fn new(model: &Arc<Mutex<Model>>, session: u64) -> Sink {
        Sink {
            model: Arc::downgrade(model),
            session,
        }
    }

    /// Feeds the program's output to the terminal (wakes the interface).
    /// False once nobody reads it any more: stop reading then.
    pub fn output(&self, bytes: &[u8]) -> bool {
        let Some(m) = self.model.upgrade() else {
            return false;
        };
        let mut m = {
            profiling::scope!("pop.terminal.sink_lock");
            lock(&m)
        };
        if m.session != self.session {
            return false;
        }
        m.process(bytes);
        true
    }

    /// The program ended, with its exit code when known: the terminal
    /// runs `terminal.exit` at its next pass.
    pub fn exit(&self, code: Option<i32>) {
        if let Some(m) = self.model.upgrade() {
            let mut m = lock(&m);
            if m.session == self.session {
                m.pending_exit = Some(code);
                m.wake();
            }
        }
    }

    /// Whether the terminal still reads this session's output.
    pub fn is_open(&self) -> bool {
        self.model
            .upgrade()
            .is_some_and(|m| lock(&m).session == self.session)
    }
}

/// A read-only source the application writes to: build logs, a job's
/// output, an agent's transcript. Cheap to clone, written from any
/// thread; what is written before a terminal starts it is kept and shown
/// then. A lone `\n` becomes `\r\n`, as a terminal's line discipline
/// does, so log lines start at the left edge; ANSI escapes (colours,
/// bold, underline) are drawn.
///
/// ```
/// use egui_pop_editors::terminal::{Console, TerminalState};
///
/// let log = Console::new();
/// log.line("\x1b[32mCompiling\x1b[0m demo v0.1.0");
/// let term = TerminalState::new("build-log", log.clone());
/// term.spawn(None, "user").unwrap();
/// log.line("Finished.");
/// assert_eq!(term.text(), "Compiling demo v0.1.0\nFinished.");
/// ```
#[derive(Clone, Debug, Default)]
pub struct Console(Arc<Mutex<ConsoleInner>>);

#[derive(Debug, Default)]
struct ConsoleInner {
    /// Written before a terminal started the console.
    pending: Vec<u8>,
    sink: Option<Sink>,
    /// The last byte written was `\r`.
    after_cr: bool,
    /// The work behind it ended, with its code when known.
    finished: Option<Option<i32>>,
}

impl Console {
    pub fn new() -> Console {
        Console::default()
    }

    /// Writes text, ANSI escapes included.
    pub fn write(&self, text: &str) {
        self.write_bytes(text.as_bytes());
    }

    /// Writes a line: the text and a line break.
    pub fn line(&self, text: &str) {
        let mut s = String::with_capacity(text.len() + 1);
        s.push_str(text);
        s.push('\n');
        self.write(&s);
    }

    /// Writes bytes, ANSI escapes included.
    pub fn write_bytes(&self, bytes: &[u8]) {
        let mut i = lock(&self.0);
        let mut out = Vec::with_capacity(bytes.len() + 8);
        for &b in bytes {
            if b == b'\n' && !i.after_cr {
                out.push(b'\r');
            }
            out.push(b);
            i.after_cr = b == b'\r';
        }
        // The console's lock is held while the terminal takes the bytes:
        // two writers never interleave out of order.
        let delivered = i.sink.as_ref().is_some_and(|sink| sink.output(&out));
        if !delivered {
            i.sink = None;
            i.pending.extend_from_slice(&out);
        }
    }

    /// The work behind the console ended (`code` when it has one): the
    /// terminal runs `terminal.exit`.
    pub fn finish(&self, code: Option<i32>) {
        let mut i = lock(&self.0);
        i.finished = Some(code);
        if let Some(sink) = &i.sink {
            sink.exit(code);
        }
    }

    /// Whether `finish` was called.
    pub fn is_finished(&self) -> bool {
        lock(&self.0).finished.is_some()
    }
}

impl TerminalSource for Console {
    fn name(&self) -> String {
        "console".into()
    }

    fn start(&mut self, _spawn: &Spawn, sink: Sink) -> Result<Json, String> {
        let mut i = lock(&self.0);
        let pending = std::mem::take(&mut i.pending);
        if !pending.is_empty() {
            sink.output(&pending);
        }
        if let Some(code) = i.finished {
            sink.exit(code);
        }
        i.sink = Some(sink);
        Ok(json!({ "replayed": pending.len() }))
    }

    fn stop(&mut self) -> Result<(), String> {
        lock(&self.0).sink = None;
        Ok(())
    }
}
