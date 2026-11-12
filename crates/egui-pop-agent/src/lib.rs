//! Serves egui-pop's debug bridge out of the process: JSON-RPC 2.0 over a
//! private local socket, for agents that drive a running application.
//!
//! Off unless the application turns it on, with
//! [`AgentSocketExt::agent_socket`] or, when the user asks for it,
//! [`AgentSocketExt::agent_socket_from_env`] (`EGUI_POP_AGENT_SOCKET=1`
//! or a path), or [`AgentSocketExt::agent_socket_from_env_or_setting`]:
//! the variable, or the `developer.agentSocket` setting (off by
//! default), the socket starting and stopping as the setting changes.
//! When the application chose no bridge mode, turning the socket on
//! makes it [`BridgeMode::ReadOnly`]: an agent reads and runs read-only
//! commands, and anything that writes is refused.
//!
//! **Transport.** A Unix socket (Linux, macOS) in the user's runtime
//! directory (`$XDG_RUNTIME_DIR`, `$TMPDIR` on macOS), mode 0600, in a
//! directory no one else may write to; a named pipe on Windows. One
//! JSON-RPC request per line, one response per line. Methods:
//!
//! | Method | Params | Result |
//! |---|---|---|
//! | `describe` | none | `bridge.describe`: the capabilities, protocol 1, and the open `windows` (`id`, `title`, `main`, `active`) |
//! | `snapshot` | `{ "collector"?, "window"? }` | every collector's snapshot, or one |
//! | `call` | `{ "collector", "action", "args"?, "window"? }` | the bridge's feedback block (`ok`, `message`, `data`...) |
//! | `wait` | `bridge.wait`'s arguments and `deadline_ms` | `{ "met", "by", "event", "last" }` |
//! | `ui` | `{ "action", "label"?, "role"?, "nth"?, "text"?, "key"?, "up"?, "window"? }` | the accessibility tree, or the node acted on |
//!
//! `ui` reads the interface as a person sees it and drives it: `tree`
//! lists the AccessKit nodes of the frame just drawn; `click`, `type`,
//! `key` and `scroll` queue raw egui events (hover, press, release, text,
//! wheel) into the next frames' input, through an egui plugin, so they
//! work headless as in a window. Reading needs the bridge on or
//! read-only, acting needs it on. With `window`, it reads and acts in
//! that application window (its own OS window, or, where egui embeds it
//! in the main one, its embedded window); without, in the main window.
//!
//! `window` names an application window (its id, `main` for the main
//! window; ADR 0025), the active window without one: a command runs in
//! that window's context (`appWindow`, `modalOpen`), what it opens shows
//! there, and `dialogs.answer` answers the dialog shown there. An unknown
//! window is refused.
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
use egui_pop_model::ids::{PluginId, SettingId};
use egui_pop_model::values::Descriptor;
use serde_json::{Value as Json, json};

#[cfg(feature = "mcp")]
pub mod mcp;
mod rpc;
mod ui;
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
/// The setting that turns the socket on, live
/// ([`AgentSocketExt::agent_socket_from_env_or_setting`]); off by default.
pub const SETTING_AGENT_SOCKET: SettingId = SettingId::from_static("developer.agentSocket");
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

    /// Declares the `developer.agentSocket` setting (off by default) and
    /// serves the bridge on `socket` while it is on: the socket starts
    /// when the setting turns on and stops, its file removed, when it
    /// turns off. The bridge's mode as with
    /// [`AgentSocketExt::agent_socket`].
    fn agent_socket_on_setting(self, socket: AgentSocket) -> Self;

    /// The setting ([`AgentSocketExt::agent_socket_on_setting`], at the
    /// default place), or [`ENV`]: when the variable asks for the socket,
    /// it listens (at the variable's path) whatever the setting says.
    fn agent_socket_from_env_or_setting(self) -> Self;
}

impl AgentSocketExt for AppBuilder {
    fn agent_socket(self, socket: AgentSocket) -> Self {
        read_only_unless_chosen(self).plugin(AgentPlugin::new(socket))
    }

    fn agent_socket_on_setting(self, socket: AgentSocket) -> Self {
        read_only_unless_chosen(self).plugin(AgentPlugin::on_setting(socket))
    }

    fn agent_socket_from_env_or_setting(self) -> Self {
        let plugin = match AgentSocket::from_env() {
            Some(s) => AgentPlugin::on_setting(s).forced(),
            None => AgentPlugin::on_setting(AgentSocket::new()),
        };
        read_only_unless_chosen(self).plugin(plugin)
    }
}

/// The bridge `ReadOnly` when the application chose no mode.
fn read_only_unless_chosen(b: AppBuilder) -> AppBuilder {
    if b.chosen_debug_bridge().is_none() {
        b.debug_bridge(BridgeMode::ReadOnly)
    } else {
        b
    }
}

/// Whether the `developer.agentSocket` setting is on.
pub fn setting_on(app: &AppHandle) -> bool {
    app.setting::<bool>(&SETTING_AGENT_SOCKET).unwrap_or(false)
}

/// The plugin serving the socket ([`AgentSocketExt::agent_socket`] adds
/// it). It fails to listen, without failing the application, when the
/// socket's directory may be written by others, when another instance
/// listens on it, or when the socket cannot be made private; the reason
/// is logged.
#[derive(Debug)]
pub struct AgentPlugin {
    socket: AgentSocket,
    /// Declares `developer.agentSocket` and follows it.
    setting: bool,
    /// Listens whatever the setting says.
    forced: bool,
    server: Arc<Mutex<Option<Server>>>,
}

impl AgentPlugin {
    /// Always listening.
    pub fn new(socket: AgentSocket) -> Self {
        Self {
            socket,
            setting: false,
            forced: true,
            server: Arc::default(),
        }
    }

    /// Listening while the `developer.agentSocket` setting is on, which
    /// it declares.
    pub fn on_setting(socket: AgentSocket) -> Self {
        Self {
            socket,
            setting: true,
            forced: false,
            server: Arc::default(),
        }
    }

    /// Listening whatever the setting says (it is still declared).
    #[must_use]
    pub fn forced(mut self) -> Self {
        self.forced = true;
        self
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
        if self.setting {
            ctx.setting(
                SETTING_AGENT_SOCKET,
                Descriptor::bool("Agent socket", false).description(
                    "Lets local programs drive this app (read-only unless the app allows more).",
                ),
            )?;
        }
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
        let mut listen = Listen {
            path,
            tx,
            wake: wake.clone(),
            server: self.server.clone(),
            follows: self.setting && !self.forced,
            tried: false,
        };
        let _ = listen.follow(&app);
        let mut side = UiSide {
            rx,
            waits: Vec::new(),
            ui_jobs: Vec::new(),
            wake,
        };
        ctx.on(On::Pass, move |app, _| {
            if listen.follow(app) {
                side.pass(app);
            } else {
                side.refuse_all();
            }
        });
        Ok(())
    }

    fn disconnect(&mut self, _ctx: &mut Ctx<'_>) {
        if let Ok(mut s) = self.server.lock() {
            *s = None;
        }
    }
}

/// Starts and stops the server as it should listen.
struct Listen {
    path: PathBuf,
    tx: mpsc::Sender<Job>,
    wake: Waker,
    server: Arc<Mutex<Option<Server>>>,
    /// Follows the setting; listens always otherwise.
    follows: bool,
    /// Tried to start since it should listen: a failure is logged once,
    /// not at every pass.
    tried: bool,
}

impl Listen {
    /// Whether it listens now.
    fn follow(&mut self, app: &AppHandle) -> bool {
        let want = !self.follows || setting_on(app);
        let Ok(mut server) = self.server.lock() else {
            return false;
        };
        if !want {
            self.tried = false;
            if server.take().is_some() {
                log::info!(target: "egui_pop_agent", "agent socket stopped at {}", self.path.display());
            }
            return false;
        }
        if server.is_some() || self.tried {
            return server.is_some();
        }
        self.tried = true;
        match Server::start(&self.path, self.tx.clone(), self.wake.clone()) {
            Ok(s) => {
                log::info!(target: "egui_pop_agent", "agent socket listening at {}", self.path.display());
                *server = Some(s);
            }
            Err(e) => {
                log::error!(target: "egui_pop_agent", "agent socket not started at {}: {e}", self.path.display());
            }
        }
        server.is_some()
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
        /// The application window seen from (the active one without).
        window: Option<String>,
        reply: Reply,
    },
    Call {
        collector: String,
        action: String,
        args: Json,
        /// The application window the call is for (the active one
        /// without).
        window: Option<String>,
        reply: Reply,
    },
    Wait {
        args: Json,
        deadline: Instant,
        reply: Reply,
    },
    Ui {
        params: Json,
        deadline: Instant,
        reply: Reply,
    },
}

/// A `ui` request in progress.
struct UiJob {
    request: ui::Request,
    /// Where it reads and acts.
    scope: ui::Scope,
    /// Answered from a tree kept at this pass or later.
    from_pass: u64,
    /// Once its events went in, this answer.
    answer: Option<Json>,
    deadline: Instant,
    reply: Reply,
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
    ui_jobs: Vec<UiJob>,
    wake: Waker,
}

fn origin() -> Origin {
    Origin::Agent(AGENT.into())
}

impl UiSide {
    /// The socket is off: what clients still connected ask is refused,
    /// and the waits held end.
    fn refuse_all(&mut self) {
        while let Ok(job) = self.rx.try_recv() {
            let reply = match job {
                Job::Describe { reply }
                | Job::Snapshot { reply, .. }
                | Job::Call { reply, .. }
                | Job::Wait { reply, .. }
                | Job::Ui { reply, .. } => reply,
            };
            let _ = reply.send(Err(RpcError::stopped()));
        }
        for w in self.waits.drain(..) {
            let _ = w.reply.send(Err(RpcError::stopped()));
        }
        for j in self.ui_jobs.drain(..) {
            let _ = j.reply.send(Err(RpcError::stopped()));
        }
    }

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
        self.ui_pass(&ctx, now);
        // After the checks: a repaint asked for now is not one the
        // application asked for.
        if !self.waits.is_empty() || !self.ui_jobs.is_empty() {
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
            Job::Snapshot {
                collector,
                window,
                reply,
            } => {
                // Snapshots are the bridge's too: refused when it is off,
                // as its calls are.
                if app.debug_bridge_mode() == BridgeMode::Off {
                    let _ = reply.send(Err(RpcError::off()));
                    return;
                }
                let all = match app.debug_snapshot_in(window.as_deref()) {
                    Ok(all) => all,
                    Err(e) => {
                        let _ = reply.send(Err(RpcError::params(e)));
                        return;
                    }
                };
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
                window,
                reply,
            } => {
                let f = app.debug_call_in(window.as_deref(), &collector, &action, &args, origin());
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
            Job::Ui {
                params,
                deadline,
                reply,
            } => {
                let request = match ui::Request::from_params(&params) {
                    Ok(r) => r,
                    Err(e) => {
                        let _ = reply.send(Err(RpcError::params(e)));
                        return;
                    }
                };
                // The bridge's mode, as for its calls: reading the tree
                // needs it on or read-only, acting needs it on.
                let mode = app.debug_bridge_mode();
                if mode == BridgeMode::Off {
                    let _ = reply.send(Err(RpcError::off()));
                    return;
                }
                if request.writes() && mode != BridgeMode::On {
                    let _ = reply.send(Err(RpcError::new(
                        RpcError::REFUSED,
                        "the debug bridge is read-only: `ui` may read the tree, not act",
                    )));
                    return;
                }
                let scope = match ui_scope(app, request.window.as_deref()) {
                    Ok(s) => s,
                    Err(e) => {
                        let _ = reply.send(Err(RpcError::params(e)));
                        return;
                    }
                };
                let ctx = app.egui();
                let shared = ui::install(&ctx);
                if let Ok(mut s) = shared.lock() {
                    s.want_tree = true;
                }
                ctx.request_repaint_of(scope.viewport);
                self.ui_jobs.push(UiJob {
                    from_pass: ctx.cumulative_pass_nr_for(scope.viewport),
                    request,
                    scope,
                    answer: None,
                    deadline,
                    reply,
                });
            }
        }
    }

    /// The `ui` requests: answered from the tree kept at the end of a
    /// pass, then once their events went in.
    fn ui_pass(&mut self, ctx: &egui::Context, now: Instant) {
        if self.ui_jobs.is_empty() {
            return;
        }
        let shared = ui::shared(ctx);
        let Ok(mut s) = shared.lock() else {
            return;
        };
        let mut left = Vec::new();
        // One action at a time: a second one's node is found once the
        // first one's events went in.
        let mut busy = s.queued();
        for mut j in self.ui_jobs.drain(..) {
            if let Some(answer) = j.answer.take() {
                if !s.queued() {
                    let _ = j.reply.send(Ok(answer));
                    busy = false;
                    continue;
                }
                j.answer = Some(answer);
                left.push(j);
                continue;
            }
            let fresh = s
                .trees
                .get(&j.scope.viewport)
                .filter(|(pass, _)| *pass >= j.from_pass);
            match fresh {
                Some((_, tree)) if !busy => match j.request.on_tree(tree, &j.scope) {
                    Ok((batches, answer)) if batches.is_empty() => {
                        let _ = j.reply.send(Ok(answer));
                    }
                    Ok((batches, answer)) => {
                        s.queues
                            .entry(j.scope.viewport)
                            .or_default()
                            .extend(batches);
                        busy = true;
                        j.answer = Some(answer);
                        left.push(j);
                    }
                    Err(e) => {
                        let _ = j.reply.send(Err(RpcError::new(RpcError::UI, e)));
                    }
                },
                _ if now >= j.deadline => {
                    let _ = j.reply.send(Err(RpcError::new(
                        RpcError::NO_ANSWER,
                        "no frame drew the accessibility tree in time",
                    )));
                }
                _ => left.push(j),
            }
        }
        self.ui_jobs = left;
        s.want_tree = self.ui_jobs.iter().any(|j| j.answer.is_none());
        let queued: Vec<egui::ViewportId> = s
            .queues
            .iter()
            .filter(|(_, q)| !q.is_empty())
            .map(|(v, _)| *v)
            .collect();
        drop(s);
        for v in queued
            .into_iter()
            .chain(self.ui_jobs.iter().map(|j| j.scope.viewport))
        {
            ctx.request_repaint_of(v);
        }
    }
}

/// Where a `ui` request for `window` reads and acts (ADR 0025): the main
/// window's viewport without one; an application window's own viewport,
/// or, where egui embeds the windows in the main one, the main viewport
/// within (or, for the main window, without) the embedded windows.
fn ui_scope(app: &AppHandle, window: Option<&str>) -> Result<ui::Scope, String> {
    let root = ui::Scope::viewport(egui::ViewportId::ROOT);
    let Some(name) = window else {
        return Ok(root);
    };
    let windows = app.windows();
    let Some(w) = windows.iter().find(|w| w.id.as_str() == name) else {
        let open: Vec<&str> = windows.iter().map(|w| w.id.as_str()).collect();
        return Err(format!("no window `{name}`: the open ones are {open:?}"));
    };
    if !app.egui().embed_viewports() {
        return Ok(ui::Scope::viewport(w.viewport));
    }
    Ok(if w.main {
        ui::Scope {
            without: windows
                .iter()
                .filter(|o| !o.main)
                .map(|o| o.title.clone())
                .collect(),
            ..root
        }
    } else {
        ui::Scope {
            within: Some(w.title.clone()),
            ..root
        }
    })
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
