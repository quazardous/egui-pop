//! A program in a pseudo-terminal (feature `pty`), through portable-pty:
//! Unix PTYs, ConPTY on Windows. Not for the web.

use std::fmt;
use std::io::{Read, Write};
use std::sync::mpsc;
use std::time::Duration;

use portable_pty::{ChildKiller, CommandBuilder, MasterPty, PtySize, native_pty_system};
use serde_json::{Value as Json, json};

use super::source::{CommandLine, Sink, Size, Spawn, TerminalSource};

/// Runs a program (the user's shell by default) in a pseudo-terminal:
/// the terminal's input is its keyboard, its size the terminal's.
/// Dropping the source (or closing the session) kills the program.
///
/// ```no_run
/// use egui_pop_editors::terminal::{CommandLine, PtySource, TerminalState};
///
/// let shell = TerminalState::new("shell", PtySource::shell());
/// let build = TerminalState::new(
///     "build",
///     PtySource::new(CommandLine::new("cargo").arg("build").cwd("/path/to/project")),
/// );
/// # let _ = (shell, build);
/// ```
pub struct PtySource {
    command: CommandLine,
    session: Option<Session>,
}

struct Session {
    master: Box<dyn MasterPty + Send>,
    writer: mpsc::Sender<Vec<u8>>,
    killer: Box<dyn ChildKiller + Send + Sync>,
    pid: Option<u32>,
}

impl fmt::Debug for PtySource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PtySource")
            .field("command", &self.command)
            .field("pid", &self.session.as_ref().and_then(|s| s.pid))
            .finish()
    }
}

impl PtySource {
    /// Runs `command` (redirected by `terminal.spawn` hooks when they
    /// change its `command`).
    pub fn new(command: CommandLine) -> PtySource {
        PtySource {
            command,
            session: None,
        }
    }

    /// Runs the user's shell (`$SHELL` on Unix, `cmd.exe` on Windows).
    pub fn shell() -> PtySource {
        PtySource::new(CommandLine::shell())
    }

    /// The running program's process id.
    pub fn pid(&self) -> Option<u32> {
        self.session.as_ref().and_then(|s| s.pid)
    }

    fn end_session(&mut self) {
        if let Some(mut s) = self.session.take() {
            // Already gone when it exited by itself.
            let _ = s.killer.kill();
        }
    }
}

impl Drop for PtySource {
    fn drop(&mut self) {
        self.end_session();
    }
}

fn pty_size(size: Size) -> PtySize {
    PtySize {
        rows: size.rows,
        cols: size.cols,
        pixel_width: 0,
        pixel_height: 0,
    }
}

fn builder(command: &CommandLine) -> Result<CommandBuilder, String> {
    let mut cmd = if command.program.is_empty() {
        if !command.args.is_empty() {
            return Err("arguments need a `program`".into());
        }
        CommandBuilder::new_default_prog()
    } else {
        let mut c = CommandBuilder::new(&command.program);
        c.args(&command.args);
        c
    };
    if let Some(dir) = &command.cwd {
        cmd.cwd(dir);
    }
    if !command.env.iter().any(|(k, _)| k == "TERM") {
        cmd.env("TERM", "xterm-256color");
    }
    for (k, v) in &command.env {
        cmd.env(k, v);
    }
    Ok(cmd)
}

impl TerminalSource for PtySource {
    fn name(&self) -> String {
        "pty".into()
    }

    fn writable(&self) -> bool {
        true
    }

    fn command(&self) -> Option<CommandLine> {
        Some(self.command.clone())
    }

    fn start(&mut self, spawn: &Spawn, sink: Sink) -> Result<Json, String> {
        self.end_session();
        let command = spawn
            .command
            .clone()
            .unwrap_or_else(|| self.command.clone());
        let cmd = builder(&command)?;
        let pair = native_pty_system()
            .openpty(pty_size(spawn.size))
            .map_err(|e| format!("cannot open a pseudo-terminal: {e}"))?;
        let mut child = pair
            .slave
            .spawn_command(cmd)
            .map_err(|e| format!("cannot start `{}`: {e}", command.program))?;
        drop(pair.slave);
        let pid = child.process_id();
        let killer = child.clone_killer();
        let mut reader = pair.master.try_clone_reader().map_err(|e| e.to_string())?;
        let mut writer = pair.master.take_writer().map_err(|e| e.to_string())?;
        let (tx, rx) = mpsc::channel::<Vec<u8>>();
        // Input is written on a thread of its own: a program not reading
        // never blocks the interface.
        std::thread::Builder::new()
            .name("egui-pop terminal input".into())
            .spawn(move || {
                for bytes in rx {
                    if writer
                        .write_all(&bytes)
                        .and_then(|()| writer.flush())
                        .is_err()
                    {
                        break;
                    }
                }
            })
            .map_err(|e| e.to_string())?;
        let (done_tx, done_rx) = mpsc::channel::<()>();
        let out = sink.clone();
        std::thread::Builder::new()
            .name("egui-pop terminal output".into())
            .spawn(move || {
                let mut buf = [0u8; 16 * 1024];
                loop {
                    match reader.read(&mut buf) {
                        Ok(0) | Err(_) => break,
                        Ok(n) => {
                            if !out.output(&buf[..n]) {
                                break;
                            }
                        }
                    }
                }
                let _ = done_tx.send(());
            })
            .map_err(|e| e.to_string())?;
        std::thread::Builder::new()
            .name("egui-pop terminal wait".into())
            .spawn(move || {
                let code = child.wait().ok().map(|s| s.exit_code() as i32);
                // The output still in the pseudo-terminal first (on
                // Windows the reader ends only once the session is
                // dropped: it is not waited for long).
                let _ = done_rx.recv_timeout(Duration::from_millis(500));
                sink.exit(code);
            })
            .map_err(|e| e.to_string())?;
        self.session = Some(Session {
            master: pair.master,
            writer: tx,
            killer,
            pid,
        });
        Ok(json!({ "pid": pid, "program": command.program, "args": command.args }))
    }

    fn write(&mut self, bytes: &[u8]) -> Result<(), String> {
        let s = self.session.as_ref().ok_or("the program is not running")?;
        s.writer
            .send(bytes.to_vec())
            .map_err(|_| "the program's input is closed".to_string())
    }

    fn resize(&mut self, size: Size) {
        if let Some(s) = &self.session {
            let _ = s.master.resize(pty_size(size));
        }
    }

    fn stop(&mut self) -> Result<(), String> {
        self.end_session();
        Ok(())
    }
}
