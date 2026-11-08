//! Serves egui-pop's debug bridge out of the process: JSON-RPC 2.0 over a
//! private local socket, for agents that drive a running application.
//!
//! Off unless the application turns it on, with
//! [`AgentSocketExt::agent_socket`] or, when the user asks for it,
//! [`AgentSocketExt::agent_socket_from_env`] (`EGUI_POP_AGENT_SOCKET=1`
//! or a path). When the application chose no bridge mode, turning the
//! socket on makes it [`BridgeMode::ReadOnly`]: an agent reads and runs
//! read-only commands, and anything that writes is refused.
//!
//! **Transport.** A Unix socket (Linux, macOS) in the user's runtime
//! directory (`$XDG_RUNTIME_DIR`, `$TMPDIR` on macOS), mode 0600, in a
//! directory no one else may write to; a named pipe on Windows. One
//! JSON-RPC request per line, one response per line. Methods:
//!
//! | Method | Params | Result |
//! |---|---|---|
//! | `describe` | none | `bridge.describe`: the capabilities, protocol 1 |
//! | `snapshot` | `{ "collector"? }` | every collector's snapshot, or one |
//! | `call` | `{ "collector", "action", "args"? }` | the bridge's feedback block (`ok`, `message`, `data`...) |
//! | `wait` | `bridge.wait`'s arguments and `deadline_ms` | `{ "met", "by", "event", "last" }` |
//!
//! Every call is made as `Origin::Agent("agent-socket")`, so the bridge's
//! mode, the `agent` flags and the `debug.call` step apply as to any
//! agent.
//!
//! **Threading.** The socket's threads never touch the application: a
//! request goes to the UI thread through a channel, drained at each pass
//! of the application's logic ([`On::Pass`]), and the reply comes back on
//! a one-shot channel. A `wait` is held by the transport: the condition is
//! checked again after each frame until it holds or the deadline passes,
//! and a repaint is asked for meanwhile. The UI thread never blocks.

use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, SyncSender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use egui_pop_api::hooks::On;
use egui_pop_api::wait::WaitFor;
use egui_pop_api::{AppBuilder, AppHandle, Ctx, Plugin, PluginError};
use egui_pop_core::debug::BridgeMode;
use egui_pop_model::commands::Origin;
use egui_pop_model::ids::PluginId;
use serde_json::{Value as Json, json};

mod rpc;
#[cfg(unix)]
mod unix;
#[cfg(windows)]
mod windows;

pub use rpc::RpcError;

/// The environment variable that turns the socket on
/// ([`AgentSocket::from_env`]): `1` for the default path, or a path.
pub const ENV: &str = "EGUI_POP_AGENT_SOCKET";
/// The agent's name in the calls' origin: `Origin::Agent("agent-socket")`.
pub const AGENT: &str = "agent-socket";
/// The plugin's id.
pub const AGENT_PLUGIN: PluginId = PluginId::from_static("egui_pop.agent_socket");
/// How long a `wait` is held when it gives no `deadline_ms`.
pub const DEFAULT_DEADLINE: Duration = Duration::from_secs(5);
/// The longest a `wait` is held, whatever it asks.
pub const MAX_DEADLINE: Duration = Duration::from_secs(120);

/// Where the socket listens.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AgentSocket {
    path: Option<PathBuf>,
}

impl AgentSocket {
    /// The default place: `egui-pop-agent-<app id>.sock` in the user's
    /// runtime directory (`$XDG_RUNTIME_DIR`; `$TMPDIR` on macOS), or the
    /// pipe `\\.\pipe\egui-pop-agent-<app id>` on Windows.
    pub fn new() -> Self {
        Self::default()
    }

    /// A socket at `path` (on Windows, a pipe name).
    pub fn at(path: impl Into<PathBuf>) -> Self {
        Self {
            path: Some(path.into()),
        }
    }

    /// From [`ENV`]: `1` or `true` for the default place, a path for that
    /// path; `None` when unset, empty, `0` or `false`.
    pub fn from_env() -> Option<Self> {
        Self::from_value(&std::env::var(ENV).ok()?)
    }

    fn from_value(v: &str) -> Option<Self> {
        match v.trim() {
            "" | "0" | "false" => None,
            "1" | "true" => Some(Self::new()),
            p => Some(Self::at(p)),
        }
    }

    /// The path given, `None` for the default place.
    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }

    /// Where it listens for the application `app_id`.
    pub fn resolve(&self, app_id: &str) -> Result<PathBuf, String> {
        match &self.path {
            Some(p) => Ok(p.clone()),
            None => default_path(app_id),
        }
    }
}

/// The file name's part from the application's id.
fn stem(app_id: &str) -> String {
    let s: String = app_id
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '.' || c == '-' {
                c
            } else {
                '_'
            }
        })
        .collect();
    if s.is_empty() { "app".into() } else { s }
}

/// The default socket for `app_id` (see [`AgentSocket::new`]).
pub fn default_path(app_id: &str) -> Result<PathBuf, String> {
    let name = format!("egui-pop-agent-{}", stem(app_id));
    if cfg!(windows) {
        return Ok(PathBuf::from(format!(r"\\.\pipe\{name}")));
    }
    let dir = std::env::var_os("XDG_RUNTIME_DIR")
        .filter(|d| !d.is_empty())
        .or_else(|| {
            cfg!(target_os = "macos")
                .then(|| std::env::var_os("TMPDIR"))
                .flatten()
        })
        .ok_or("no runtime directory (`XDG_RUNTIME_DIR` is not set): give a path")?;
    Ok(PathBuf::from(dir).join(format!("{name}.sock")))
}

/// Turning the socket on from the application's builder.
pub trait AgentSocketExt: Sized {
    /// Serves the bridge on `socket`. When the application chose no
    /// bridge mode (`AppBuilder::debug_bridge`), the bridge becomes
    /// read-only; an application that offers more says so with
    /// `debug_bridge(BridgeMode::On)`.
    fn agent_socket(self, socket: AgentSocket) -> Self;

    /// As [`AgentSocketExt::agent_socket`] when [`ENV`] asks for it
    /// ([`AgentSocket::from_env`]); nothing otherwise.
    fn agent_socket_from_env(self) -> Self {
        match AgentSocket::from_env() {
            Some(s) => self.agent_socket(s),
            None => self,
        }
    }
}

impl AgentSocketExt for AppBuilder {
    fn agent_socket(self, socket: AgentSocket) -> Self {
        let b = if self.chosen_debug_bridge().is_none() {
            self.debug_bridge(BridgeMode::ReadOnly)
        } else {
            self
        };
        b.plugin(AgentPlugin::new(socket))
    }
}

/// The plugin serving the socket ([`AgentSocketExt::agent_socket`] adds
/// it). It fails to listen, without failing the application, when the
/// socket's directory may be written by others, when another instance
/// listens on it, or when the socket cannot be made private; the reason
/// is logged.
#[derive(Debug)]
pub struct AgentPlugin {
    socket: AgentSocket,
    server: Option<Server>,
}

impl AgentPlugin {
    pub fn new(socket: AgentSocket) -> Self {
        Self {
            socket,
            server: None,
        }
    }
}

impl Plugin for AgentPlugin {
    fn id(&self) -> PluginId {
        AGENT_PLUGIN
    }

    fn title(&self) -> String {
        "Agent socket".into()
    }

    fn connect(&mut self, ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
        let app = ctx.app();
        let path = match self.socket.resolve(&app.app_id()) {
            Ok(p) => p,
            Err(e) => {
                log::error!(target: "egui_pop_agent", "agent socket not started: {e}");
                return Ok(());
            }
        };
        let (tx, rx) = mpsc::channel();
        let wake = Waker::default();
        match Server::start(&path, tx, wake.clone()) {
            Ok(server) => {
                log::info!(target: "egui_pop_agent", "agent socket listening at {}", path.display());
                self.server = Some(server);
            }
            Err(e) => {
                log::error!(target: "egui_pop_agent", "agent socket not started at {}: {e}", path.display());
                return Ok(());
            }
        }
        let mut side = UiSide {
            rx,
            waits: Vec::new(),
            wake,
        };
        ctx.on(On::Pass, move |app, _| side.pass(app));
        Ok(())
    }

    fn disconnect(&mut self, _ctx: &mut Ctx<'_>) {
        self.server = None;
    }
}

/// The context the application draws on, for the socket's threads to
/// wake it: the application attaches to its window's context on its
/// first frame, so it is taken at each pass.
#[derive(Clone, Default)]
pub(crate) struct Waker(Arc<Mutex<Option<egui::Context>>>);

impl Waker {
    fn set(&self, ctx: egui::Context) {
        if let Ok(mut c) = self.0.lock() {
            *c = Some(ctx);
        }
    }

    /// Asks for a pass soon. Not at once: an immediate repaint would
    /// count as "a repaint asked for" during the next frame, and a wait
    /// for the application to settle would never end.
    pub(crate) fn wake(&self) {
        if let Ok(c) = self.0.lock()
            && let Some(c) = c.as_ref()
        {
            c.request_repaint_after(Duration::from_millis(1));
        }
    }
}

impl std::fmt::Debug for Waker {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Waker")
    }
}

/// The listening side, stopped when dropped.
#[derive(Debug)]
struct Server {
    #[cfg(unix)]
    _listening: unix::Listening,
    #[cfg(windows)]
    _listening: windows::Listening,
}

impl Server {
    #[cfg(unix)]
    fn start(path: &Path, tx: mpsc::Sender<Job>, wake: Waker) -> std::io::Result<Self> {
        Ok(Self {
            _listening: unix::Listening::start(path, tx, wake)?,
        })
    }

    #[cfg(windows)]
    fn start(path: &Path, tx: mpsc::Sender<Job>, wake: Waker) -> std::io::Result<Self> {
        Ok(Self {
            _listening: windows::Listening::start(path, tx, wake)?,
        })
    }

    #[cfg(not(any(unix, windows)))]
    fn start(_: &Path, _: mpsc::Sender<Job>, _: Waker) -> std::io::Result<Self> {
        Err(std::io::Error::new(
            std::io::ErrorKind::Unsupported,
            "no local socket on this platform",
        ))
    }
}

/// What a reply carries back to the socket.
pub(crate) type Reply = SyncSender<Result<Json, RpcError>>;

/// A request for the UI thread.
#[derive(Debug)]
pub(crate) enum Job {
    Describe {
        reply: Reply,
    },
    Snapshot {
        collector: Option<String>,
        reply: Reply,
    },
    Call {
        collector: String,
        action: String,
        args: Json,
        reply: Reply,
    },
    Wait {
        args: Json,
        deadline: Instant,
        reply: Reply,
    },
}

/// A wait held until it is met or its deadline passes.
struct Held {
    until: WaitFor,
    since: u64,
    deadline: Instant,
    reply: Reply,
}

/// What the UI thread keeps: the requests' channel and the waits held.
struct UiSide {
    rx: Receiver<Job>,
    waits: Vec<Held>,
    wake: Waker,
}

fn origin() -> Origin {
    Origin::Agent(AGENT.into())
}

impl UiSide {
    fn pass(&mut self, app: &AppHandle) {
        let ctx = app.egui();
        self.wake.set(ctx.clone());
        while let Ok(job) = self.rx.try_recv() {
            self.start(app, job);
        }
        let now = Instant::now();
        self.waits.retain(|w| {
            if let Some(answer) = check(app, &w.until, w.since) {
                let _ = w.reply.send(Ok(answer));
                return false;
            }
            if now >= w.deadline {
                let last = app.last_event();
                let _ = w.reply.send(Ok(
                    json!({ "met": false, "by": null, "event": null, "last": last }),
                ));
                return false;
            }
            true
        });
        // After the checks: a repaint asked for now is not one the
        // application asked for.
        if !self.waits.is_empty() {
            ctx.request_repaint_after(Duration::from_millis(10));
        }
    }

    fn start(&mut self, app: &AppHandle, job: Job) {
        match job {
            Job::Describe { reply } => {
                let f = app.debug_call_as("bridge", "describe", &json!({}), origin());
                let _ = reply.send(if f.ok {
                    Ok(f.data)
                } else {
                    Err(RpcError::refused(&f))
                });
            }
            Job::Snapshot { collector, reply } => {
                let all = app.debug_snapshot();
                let _ = reply.send(match collector {
                    None => Ok(all),
                    Some(c) => all
                        .get(&c)
                        .cloned()
                        .ok_or_else(|| RpcError::params(format!("no collector `{c}`"))),
                });
            }
            Job::Call {
                collector,
                action,
                args,
                reply,
            } => {
                let f = app.debug_call_as(&collector, &action, &args, origin());
                let _ = reply.send(Ok(serde_json::to_value(&f).unwrap_or(Json::Null)));
            }
            Job::Wait {
                args,
                deadline,
                reply,
            } => {
                let until = match WaitFor::from_args(&args) {
                    Ok(u) => u,
                    Err(e) => {
                        let _ = reply.send(Err(RpcError::params(e)));
                        return;
                    }
                };
                // Through the bridge, so the mode and the step apply.
                let f = app.debug_call_as("bridge", "wait", &args, origin());
                if !f.ok {
                    let _ = reply.send(Err(RpcError::refused(&f)));
                    return;
                }
                if f.data["met"] == true {
                    let _ = reply.send(Ok(f.data));
                    return;
                }
                let since = args
                    .get("since")
                    .and_then(Json::as_u64)
                    .or_else(|| f.data["last"].as_u64())
                    .unwrap_or_else(|| app.last_event());
                self.waits.push(Held {
                    until,
                    since,
                    deadline,
                    reply,
                });
            }
        }
    }
}

/// `bridge.wait`'s answer when `until` holds now.
fn check(app: &AppHandle, until: &WaitFor, since: u64) -> Option<Json> {
    use egui_pop_api::wait::Waited;
    let last = app.last_event();
    app.wait_met(until, since).map(|w| match w {
        Waited::Event(e) => json!({ "met": true, "by": "event", "event": e, "last": last }),
        Waited::Settled => json!({ "met": true, "by": "settled", "event": null, "last": last }),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn env_values() {
        assert_eq!(AgentSocket::from_value(""), None);
        assert_eq!(AgentSocket::from_value("0"), None);
        assert_eq!(AgentSocket::from_value("1"), Some(AgentSocket::new()));
        assert_eq!(
            AgentSocket::from_value("/run/x.sock"),
            Some(AgentSocket::at("/run/x.sock"))
        );
    }

    #[test]
    fn stems() {
        assert_eq!(stem("org.example.App"), "org.example.App");
        assert_eq!(stem("a b/c"), "a_b_c");
        assert_eq!(stem(""), "app");
    }
}
