//! The terminal's steps (ADR 0011), its commands (the agent surface),
//! its debug collectors, and the plugin declaring them.
//!
//! | Step | Arguments | Default |
//! |---|---|---|
//! | `terminal.spawn` | `terminal`, `source`, `command` (`{program, args, cwd, env}` or `null`), `rows`, `cols`, `origin` | starts the source with `command` |
//! | `terminal.input` | `terminal`, `text`, `kind` (`keys`, `paste`, `agent`), `origin` | writes `text` to the source |
//! | `terminal.exit` | `terminal`, `reason` (`exited`, `close`), `code`, `origin`, `notice`, `restart` | `close`: stops the source; both: writes `notice`, starts again when `restart` |
//!
//! Collectors: `terminal_sessions` (each terminal's state and its last
//! lines) and `terminal_io` (the last runs of the three steps).

use std::collections::{BTreeMap, VecDeque};
use std::sync::Mutex;

use egui_pop_api::hooks::On;
use egui_pop_api::host::source_of;
use egui_pop_api::services::ServiceToken;
use egui_pop_api::steps::StepSpec;
use egui_pop_api::{Call, Ctx, Plugin, PluginError};
use egui_pop_core::debug::FnCollector;
use egui_pop_core::lock;
use egui_pop_model::commands::CommandSpec;
use egui_pop_model::ids::{CommandId, PluginId, StepId};
use egui_pop_model::keymap::BindOptions;
use serde_json::{Value as Json, json};

use super::state::{InputKind, TerminalState};

pub const TERMINAL_SPAWN: StepId = StepId::from_static("terminal.spawn");
pub const TERMINAL_INPUT: StepId = StepId::from_static("terminal.input");
pub const TERMINAL_EXIT: StepId = StepId::from_static("terminal.exit");

/// The plugin's id.
pub const TERMINAL_PLUGIN: PluginId = PluginId::from_static("egui_pop.terminal");

/// The focus scope every terminal draws in: its commands are bound
/// there.
pub const SURFACE: &str = "terminal";

pub const CMD_READ: CommandId = CommandId::from_static("terminal.read");
pub const CMD_SEND: CommandId = CommandId::from_static("terminal.send");
pub const CMD_CLEAR: CommandId = CommandId::from_static("terminal.clear");
pub const CMD_COPY: CommandId = CommandId::from_static("terminal.copy");
pub const CMD_CLOSE: CommandId = CommandId::from_static("terminal.close");
pub const CMD_RESTART: CommandId = CommandId::from_static("terminal.restart");

/// How many step runs `terminal_io` keeps.
pub const LOG: usize = 64;

/// The terminals shown with an application, by id: what commands and
/// collectors reach. A terminal registers itself when drawn with
/// `.app(...)`; [`Terminals::detach`] forgets one.
#[derive(Debug, Default)]
pub struct Terminals {
    inner: Mutex<Inner>,
}

#[derive(Debug, Default)]
struct Inner {
    terminals: BTreeMap<String, TerminalState>,
    focused: Option<String>,
    log: VecDeque<Json>,
}

/// The registry, a service of [`TerminalPlugin`].
pub const TERMINALS: ServiceToken<Terminals> = ServiceToken::new("egui_pop.terminals");

impl Terminals {
    /// Registers a terminal under its id (a new state under the same id
    /// replaces the old one).
    pub fn attach(&self, state: &TerminalState) {
        let id = state.id();
        let mut i = lock(&self.inner);
        match i.terminals.get(&id) {
            Some(s) if s.same(state) => {}
            _ => {
                i.terminals.insert(id, state.clone());
            }
        }
    }

    pub fn detach(&self, id: &str) {
        let mut i = lock(&self.inner);
        i.terminals.remove(id);
        if i.focused.as_deref() == Some(id) {
            i.focused = None;
        }
    }

    pub fn get(&self, id: &str) -> Option<TerminalState> {
        lock(&self.inner).terminals.get(id).cloned()
    }

    pub fn ids(&self) -> Vec<String> {
        lock(&self.inner).terminals.keys().cloned().collect()
    }

    /// The terminal that last had the keyboard focus.
    pub fn focused(&self) -> Option<TerminalState> {
        let i = lock(&self.inner);
        i.focused.as_ref().and_then(|f| i.terminals.get(f)).cloned()
    }

    pub(crate) fn set_focused(&self, id: &str) {
        let mut i = lock(&self.inner);
        if i.focused.as_deref() != Some(id) {
            i.focused = Some(id.to_string());
        }
    }

    /// The last runs of the terminal's steps, oldest first.
    pub fn log(&self) -> Vec<Json> {
        lock(&self.inner).log.iter().cloned().collect()
    }

    fn push(&self, entry: Json) {
        let mut i = lock(&self.inner);
        i.log.push_back(entry);
        while i.log.len() > LOG {
            i.log.pop_front();
        }
    }

    fn all(&self) -> Vec<TerminalState> {
        lock(&self.inner).terminals.values().cloned().collect()
    }
}

/// Declares the terminal's steps, commands and collectors, provides the
/// [`Terminals`] registry, and runs `terminal.exit` for programs that
/// ended, even in terminals not drawn. Connect it when terminals run with
/// an application.
#[derive(Debug, Default)]
pub struct TerminalPlugin;

/// Every step the terminal declares.
pub fn specs() -> Vec<StepSpec> {
    vec![
        StepSpec::new(
            TERMINAL_SPAWN,
            "A terminal starts its source (a shell in a PTY, a log console); a veto keeps it stopped, a redirect changes the command, a replacement starts the program elsewhere.",
        )
        .args(json!({
            "terminal": "shell",
            "source": "pty",
            "command": { "program": "bash", "args": ["-l"], "cwd": null, "env": [] },
            "rows": 24,
            "cols": 80,
            "origin": "user",
        })),
        StepSpec::new(
            TERMINAL_INPUT,
            "Input goes to a terminal's program (keys, a paste, an agent's text); a veto drops it, a redirect rewrites it, a replacement sends it elsewhere.",
        )
        .args(json!({ "terminal": "shell", "text": "ls\r", "kind": "keys", "origin": "user" })),
        StepSpec::new(
            TERMINAL_EXIT,
            "A terminal's session ends: its program exited (`exited`) or it is closed (`close`, a veto keeps it open); a redirect changes the notice or restarts.",
        )
        .args(json!({
            "terminal": "shell",
            "reason": "exited",
            "code": 0,
            "origin": "program",
            "notice": "Process exited with code 0",
            "restart": false,
        })),
    ]
}

/// The terminal a command acts on: `terminal` in its arguments, else the
/// one that last had the focus.
fn target(call: &Call<'_>) -> Result<TerminalState, String> {
    let reg = call
        .app
        .service(&TERMINALS)
        .ok_or("the terminal plugin is not connected")?;
    match call.args.get("terminal").and_then(Json::as_str) {
        Some(id) => reg.get(id).ok_or_else(|| format!("no terminal `{id}`")),
        None => reg
            .focused()
            .ok_or_else(|| "no terminal has had the focus; name one with `terminal`".into()),
    }
}

impl Plugin for TerminalPlugin {
    fn id(&self) -> PluginId {
        TERMINAL_PLUGIN
    }

    fn title(&self) -> String {
        "Terminal".into()
    }

    fn connect(&mut self, ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
        for spec in specs() {
            ctx.step(spec)?;
        }
        ctx.service(&TERMINALS, Terminals::default())?;
        let reg = ctx.app().service(&TERMINALS).expect("just provided");
        commands(ctx)?;
        // Programs that ended, in terminals drawn or not.
        let r = reg.clone();
        ctx.on(On::Pass, move |app, _| {
            for t in r.all() {
                t.poll(Some(app));
            }
        });
        for step in [TERMINAL_SPAWN, TERMINAL_INPUT, TERMINAL_EXIT] {
            let r = reg.clone();
            let name = step.as_str().to_string();
            ctx.after(step, move |_, args, result| {
                let mut e = args.clone();
                e["step"] = json!(&name);
                if let Some(text) = args["text"].as_str() {
                    // Long pastes are cut in the log.
                    e["text"] = json!(text.chars().take(80).collect::<String>());
                }
                e["result"] = match result {
                    Ok(v) => v.clone(),
                    Err(e) => json!({ "error": e }),
                };
                r.push(e);
            });
        }
        let r = reg.clone();
        ctx.collector(
            FnCollector::new(
                "terminal_sessions",
                "The terminals (source, size, session, cursor, scrollback, title) and their last 20 lines.",
            )
            .snapshot(move |_| {
                let all: Vec<Json> = r
                    .all()
                    .iter()
                    .map(|t| {
                        let mut d = t.describe();
                        let n = t.line_count();
                        d["last_lines"] = json!(t.lines(n.saturating_sub(20), n));
                        d
                    })
                    .collect();
                json!({
                    "terminals": all,
                    "focused": r.focused().map(|t| t.id()),
                })
            }),
        )?;
        let r = reg;
        ctx.collector(
            FnCollector::new(
                "terminal_io",
                "The last runs of `terminal.spawn`, `terminal.input` and `terminal.exit`, with their results.",
            )
            .snapshot(move |_| json!({ "log": r.log() })),
        )?;
        Ok(())
    }
}

fn surface() -> BindOptions {
    BindOptions::surface(SURFACE)
}

/// The terminal's commands.
fn commands(ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
    const CAT: &str = "Terminal";
    let terminal = json!({ "terminal": "shell (optional: the terminal that last had the focus)" });
    ctx.command(
        CommandSpec::new(CMD_READ, "Read the terminal", CAT)
            .description("The terminal's text: its scrollback and screen (or lines `from_line` to `to_line`, from 0, end excluded; `screen` alone for the screen), with the cursor, the size and the session's state.")
            .hidden()
            .args(json!({ "terminal": "shell", "from_line": 0, "to_line": 50, "screen": false })),
        |call| {
            let t = target(call)?;
            let mut out = t.describe();
            let from = call.args.get("from_line").and_then(Json::as_u64);
            let to = call.args.get("to_line").and_then(Json::as_u64);
            let screen = call.args.get("screen").and_then(Json::as_bool) == Some(true);
            out["lines"] = json!(t.line_count());
            out["text"] = json!(match (screen, from, to) {
                (true, _, _) => t.screen_text(),
                (false, None, None) => t.text(),
                (false, f, to) => t
                    .lines(
                        f.unwrap_or(0) as usize,
                        to.map_or(usize::MAX, |n| n as usize)
                    )
                    .join("\n"),
            });
            Ok(out)
        },
    )?;
    ctx.command(
        CommandSpec::new(CMD_SEND, "Send input", CAT)
            .description("Sends `text` to the program through `terminal.input` (with `enter`, followed by Enter).")
            .hidden()
            .args(json!({ "terminal": "shell", "text": "ls -l", "enter": true })),
        |call| {
            let t = target(call)?;
            let mut text = call.args["text"].as_str().ok_or("`text` is missing")?.to_string();
            if call.args.get("enter").and_then(Json::as_bool) == Some(true) {
                text.push('\r');
            }
            let n = t.send(Some(call.app), &text, InputKind::Agent, &source_of(call.origin))?;
            Ok(json!({ "terminal": t.id(), "bytes": n }))
        },
    )?;
    ctx.command(
        CommandSpec::new(CMD_CLEAR, "Clear the terminal", CAT)
            .description("Clears the screen and the scrollback (the program goes on).")
            .key_with("Ctrl+Shift+K", surface())
            .args(terminal.clone()),
        |call| {
            let t = target(call)?;
            t.clear();
            Ok(json!({ "terminal": t.id() }))
        },
    )?;
    ctx.command(
        CommandSpec::new(CMD_COPY, "Copy the selection", CAT)
            .description("Copies the terminal's selection to the clipboard.")
            .args(terminal.clone()),
        |call| {
            let t = target(call)?;
            let text = t.selection_text().ok_or("nothing is selected")?;
            call.app.egui().copy_text(text.clone());
            Ok(json!({ "terminal": t.id(), "copied": text }))
        },
    )?;
    ctx.command(
        CommandSpec::new(CMD_CLOSE, "Close the session", CAT)
            .description(
                "Ends the terminal's session through `terminal.exit` (the program is stopped).",
            )
            .args(terminal.clone()),
        |call| {
            let t = target(call)?;
            t.close(Some(call.app), &source_of(call.origin))?;
            Ok(t.describe())
        },
    )?;
    ctx.command(
        CommandSpec::new(CMD_RESTART, "Restart", CAT)
            .description("Starts the terminal's program again through `terminal.spawn` (closing the running one first).")
            .args(terminal),
        |call| {
            let t = target(call)?;
            t.restart(Some(call.app), &source_of(call.origin))?;
            Ok(t.describe())
        },
    )?;
    Ok(())
}
