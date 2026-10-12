//! Desktop notifications with actions, routed to commands, falling back
//! to an in-app toast.
//!
//! A [`Notification`] has a title, a body, a level, actions (each a
//! label and a command with its arguments) and a command for a click on
//! its body. [`show`] shows one now (on the interface thread);
//! [`Notifier::post`] queues one from any thread, shown at the next
//! pass. Both go through the steps (ADR 0011):
//!
//! - `notify.show`: the notification, as JSON (its fields and `id`). By
//!   default it goes to the desktop (notify-rust: the XDG notification
//!   server on Linux and BSD, the notification centre on macOS, toasts
//!   on Windows) as the `notifications.desktop` setting says (`always`,
//!   `unfocused`: only while the window does not have the focus,
//!   `never`), else, and whenever the desktop has no notification
//!   server or fails, to a toast (`TOASTS`, its first action as the
//!   toast's). The result says where: `{ "id", "via": "desktop" |
//!   "toast" }`. Replace it to send notifications elsewhere; veto it to
//!   keep quiet (do-not-disturb).
//! - `notify.action`: the user picked an action, on the desktop or on
//!   the toast: `{ "id", "action", "command", "args" }` (`action`:
//!   the action's id, or `default` for a click on the body). By default
//!   its command runs through the host; `default` also brings the window
//!   to the front.
//!
//! Desktop notifications are shown from a thread of their own (a D-Bus
//! call can block), so a failure arrives at a later pass: it shows the
//! toast then and publishes `notify.failed` (`{ "id", "error" }`).
//!
//! ```
//! use egui_pop_api::App;
//! use egui_pop_api::toasts::{TOASTS, ToastsPlugin};
//! use egui_pop_model::ids::CommandId;
//! use egui_pop_platform::notify::{self, Notification, NotifyPlugin, Simulated};
//!
//! let ctx = egui::Context::default();
//! let desktop = Simulated::unavailable("no notification server");
//! let app = App::builder()
//!     .plugin(ToastsPlugin)
//!     .plugin(NotifyPlugin::new("Demo").backend(desktop))
//!     .build(&ctx)
//!     .unwrap();
//! let n = Notification::new("Export done", "3 images in ~/Pictures")
//!     .action("show", "Show", CommandId::from_static("files.reveal"), serde_json::json!({}));
//! let out = notify::show(app.handle(), n);
//! assert_eq!(out.result().unwrap().as_ref().unwrap()["via"], "toast");
//! assert_eq!(app.handle().service(&TOASTS).unwrap().shown().len(), 1);
//! ```

use std::collections::{BTreeMap, VecDeque};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Mutex};

use egui_pop_api::hooks::On;
use egui_pop_api::services::ServiceToken;
use egui_pop_api::steps::{Outcome, StepSpec};
use egui_pop_api::toasts::{self, TOASTS, Toast};
use egui_pop_api::{AppHandle, Ctx, Plugin, PluginError};
use egui_pop_core::lock;
use egui_pop_model::commands::{CommandSpec, Origin};
use egui_pop_model::ids::{CommandId, PluginId, SettingId, StepId};
use egui_pop_model::values::Descriptor;
use serde::{Deserialize, Serialize};
use serde_json::{Value as Json, json};

/// The plugin's id.
pub const PLUGIN: PluginId = PluginId::from_static("egui_pop.notify");
/// A notification to show: its fields and `id`.
pub const STEP_SHOW: StepId = StepId::from_static("notify.show");
/// An action picked: `{ "id", "action", "command", "args" }`.
pub const STEP_ACTION: StepId = StepId::from_static("notify.action");
/// Where notifications show: `always` on the desktop, `unfocused` (while
/// the window does not have the focus), `never` (toasts only).
pub const SETTING_DESKTOP: SettingId = SettingId::from_static("notifications.desktop");
/// The command a toast's action runs, to go through `notify.action`
/// (`{ "id", "action" }`).
pub const CMD_ACT: CommandId = CommandId::from_static("notify.act");
/// The notifier, as a service.
pub const NOTIFIER: ServiceToken<Notifier> = ServiceToken::new("platform.notify");
/// The signal published when the desktop failed to show one.
pub const FAILED: &str = "notify.failed";
/// How many notifications [`Notifier::history`] keeps.
pub const HISTORY: usize = 32;

/// How much a notification matters: its toast's level, its urgency on
/// the desktop.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Level {
    #[default]
    Info,
    Success,
    Warning,
    Error,
}

impl Level {
    fn toast(self) -> toasts::Level {
        match self {
            Level::Info => toasts::Level::Info,
            Level::Success => toasts::Level::Success,
            Level::Warning => toasts::Level::Warning,
            Level::Error => toasts::Level::Error,
        }
    }
}

/// An action offered with a notification: a button on the desktop.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Action {
    /// Stable, never shown: `show`, `retry`.
    pub id: String,
    pub label: String,
    pub command: CommandId,
    #[serde(default)]
    pub args: Json,
}

/// A notification.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Notification {
    pub title: String,
    #[serde(default)]
    pub body: String,
    #[serde(default)]
    pub level: Level,
    #[serde(default)]
    pub actions: Vec<Action>,
    /// What a click on its body runs (besides bringing the window to the
    /// front).
    #[serde(default)]
    pub default: Option<(CommandId, Json)>,
    /// A themed icon's name or a file path (desktop only).
    #[serde(default)]
    pub icon: Option<String>,
}

impl Notification {
    pub fn new(title: &str, body: &str) -> Self {
        Self {
            title: title.into(),
            body: body.into(),
            ..Self::default()
        }
    }

    pub fn level(mut self, level: Level) -> Self {
        self.level = level;
        self
    }

    /// Offers an action, running `command` with `args` when picked.
    pub fn action(mut self, id: &str, label: &str, command: CommandId, args: Json) -> Self {
        self.actions.push(Action {
            id: id.into(),
            label: label.into(),
            command,
            args,
        });
        self
    }

    /// Runs `command` with `args` when its body is clicked.
    pub fn on_click(mut self, command: CommandId, args: Json) -> Self {
        self.default = Some((command, args));
        self
    }

    pub fn icon(mut self, icon: &str) -> Self {
        self.icon = Some(icon.into());
        self
    }
}

/// What shows notifications on the desktop. [`Desktop`] is the real
/// one; [`Simulated`] records them, for tests and the gallery.
pub trait Backend: Send + Sync + 'static {
    /// A short name: `xdg`, `windows`, `macos`, `simulated`.
    fn name(&self) -> String;

    /// Whether the desktop can show notifications, and through what
    /// (the server's name), or why not. May block: called from a thread.
    fn probe(&self) -> Result<String, String>;

    /// [`Backend::probe`]'s answer when it is known without asking
    /// (taken at connect, and the thread asks nothing).
    fn known(&self) -> Option<Result<String, String>> {
        None
    }

    /// Shows one. `respond` gets the action the user picks (an action's
    /// id, or `default` for its body), at most once. May block: called
    /// from a thread.
    fn show(
        &self,
        id: u64,
        n: &Notification,
        respond: Box<dyn FnOnce(String) + Send>,
    ) -> Result<(), String>;
}

/// The desktop's notifications, through notify-rust.
#[derive(Clone, Debug)]
pub struct Desktop {
    app_name: String,
}

impl Desktop {
    /// `app_name`: the application as the notification server shows it.
    /// On Windows it is the application's AppUserModelID, set by its
    /// installer; on macOS the bundle's identifier (see
    /// `docs/packaging.md`).
    pub fn new(app_name: &str) -> Self {
        Self {
            app_name: app_name.into(),
        }
    }
}

impl Backend for Desktop {
    fn name(&self) -> String {
        if cfg!(windows) {
            "windows"
        } else if cfg!(target_os = "macos") {
            "macos"
        } else if cfg!(target_arch = "wasm32") {
            "none"
        } else {
            "xdg"
        }
        .into()
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn probe(&self) -> Result<String, String> {
        #[cfg(all(unix, not(target_os = "macos")))]
        {
            notify_rust::get_server_information()
                .map(|s| format!("{} {}", s.name, s.version))
                .map_err(|e| format!("no notification server: {e}"))
        }
        #[cfg(not(all(unix, not(target_os = "macos"))))]
        {
            Ok(self.name())
        }
    }

    #[cfg(target_arch = "wasm32")]
    fn probe(&self) -> Result<String, String> {
        Err("no desktop notifications on the web".into())
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn show(
        &self,
        _id: u64,
        n: &Notification,
        respond: Box<dyn FnOnce(String) + Send>,
    ) -> Result<(), String> {
        let mut out = notify_rust::Notification::new();
        out.appname(&self.app_name).summary(&n.title).body(&n.body);
        if let Some(icon) = &n.icon {
            out.icon(icon);
        }
        #[cfg(all(unix, not(target_os = "macos")))]
        out.urgency(match n.level {
            Level::Error => notify_rust::Urgency::Critical,
            Level::Warning => notify_rust::Urgency::Normal,
            Level::Info | Level::Success => notify_rust::Urgency::Low,
        });
        if n.default.is_some() {
            out.action("default", &n.title);
        }
        for a in &n.actions {
            out.action(&a.id, &a.label);
        }
        let handle = out.show().map_err(|e| e.to_string())?;
        let waits = n.default.is_some() || !n.actions.is_empty();
        if waits {
            // Each notification waiting for an answer holds a thread
            // until it is closed (notify-rust's way).
            std::thread::Builder::new()
                .name("egui-pop notification".into())
                .spawn(move || {
                    handle.wait_for_action(|action| {
                        if action != "__closed" {
                            respond(action.to_string());
                        }
                    });
                })
                .map_err(|e| e.to_string())?;
        }
        Ok(())
    }

    #[cfg(target_arch = "wasm32")]
    fn show(
        &self,
        _: u64,
        _: &Notification,
        _: Box<dyn FnOnce(String) + Send>,
    ) -> Result<(), String> {
        Err("no desktop notifications on the web".into())
    }
}

type Respond = Box<dyn FnOnce(String) + Send>;

#[derive(Default)]
struct SimState {
    shown: Vec<(u64, Notification)>,
    responders: BTreeMap<u64, Respond>,
}

/// A desktop that records what it shows; [`Simulated::click`] answers
/// as the user would. For tests and the gallery.
#[derive(Clone)]
pub struct Simulated {
    available: Result<String, String>,
    state: Arc<Mutex<SimState>>,
}

impl std::fmt::Debug for Simulated {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Simulated")
            .field("available", &self.available)
            .finish_non_exhaustive()
    }
}

impl Default for Simulated {
    fn default() -> Self {
        Self {
            available: Ok("simulated".into()),
            state: Arc::default(),
        }
    }
}

impl Simulated {
    /// A desktop without notifications, saying why.
    pub fn unavailable(reason: &str) -> Self {
        Self {
            available: Err(reason.into()),
            state: Arc::default(),
        }
    }

    /// What it showed: id and notification, oldest first.
    pub fn shown(&self) -> Vec<(u64, Notification)> {
        lock(&self.state).shown.clone()
    }

    /// The user picks `action` (an action's id, or `default`) on the
    /// notification `id`; false when it waits for no answer.
    pub fn click(&self, id: u64, action: &str) -> bool {
        let respond = lock(&self.state).responders.remove(&id);
        match respond {
            Some(r) => {
                r(action.into());
                true
            }
            None => false,
        }
    }
}

impl Backend for Simulated {
    fn name(&self) -> String {
        "simulated".into()
    }

    fn probe(&self) -> Result<String, String> {
        self.available.clone()
    }

    fn known(&self) -> Option<Result<String, String>> {
        Some(self.available.clone())
    }

    fn show(&self, id: u64, n: &Notification, respond: Respond) -> Result<(), String> {
        self.available.as_ref().map_err(Clone::clone)?;
        let mut s = lock(&self.state);
        s.shown.push((id, n.clone()));
        s.responders.insert(id, respond);
        Ok(())
    }
}

/// Where a notification went.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Via {
    Desktop,
    Toast,
    /// Not shown: its step was vetoed.
    Nowhere,
}

/// A notification shown, as kept.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Shown {
    pub id: u64,
    pub notification: Notification,
    pub via: Via,
    /// The actions picked, in order.
    pub picked: Vec<String>,
}

/// What the desktop said back, from its threads.
enum Event {
    Probed(Result<String, String>),
    Failed(u64, String),
    Picked(u64, String),
}

#[derive(Default)]
struct State {
    next: u64,
    posted: VecDeque<Notification>,
    history: VecDeque<Shown>,
    desktop: Option<Result<String, String>>,
    waker: Option<egui::Context>,
}

/// The notifications, as a service: post from any thread, read what was
/// shown and where. Cheap to clone (shared).
#[derive(Clone, Default)]
pub struct Notifier(Arc<Mutex<State>>);

impl std::fmt::Debug for Notifier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Notifier").finish_non_exhaustive()
    }
}

impl Notifier {
    /// Queues a notification, shown at the next pass (from any thread).
    pub fn post(&self, n: Notification) {
        let waker = {
            let mut s = lock(&self.0);
            s.posted.push_back(n);
            s.waker.clone()
        };
        if let Some(ctx) = waker {
            ctx.request_repaint();
        }
    }

    /// The notifications shown, oldest first.
    pub fn history(&self) -> Vec<Shown> {
        lock(&self.0).history.iter().cloned().collect()
    }

    /// Whether the desktop shows notifications (and through what), or
    /// why not; none while it is being asked.
    pub fn desktop(&self) -> Option<Result<String, String>> {
        lock(&self.0).desktop.clone()
    }

    fn next_id(&self) -> u64 {
        let mut s = lock(&self.0);
        s.next += 1;
        s.next
    }

    fn record(&self, shown: Shown) {
        let mut s = lock(&self.0);
        s.history.retain(|h| h.id != shown.id);
        s.history.push_back(shown);
        while s.history.len() > HISTORY {
            s.history.pop_front();
        }
    }

    fn find(&self, id: u64) -> Option<Shown> {
        lock(&self.0).history.iter().find(|h| h.id == id).cloned()
    }

    fn update(&self, id: u64, f: impl FnOnce(&mut Shown)) {
        if let Some(h) = lock(&self.0).history.iter_mut().find(|h| h.id == id) {
            f(h);
        }
    }
}

/// What the plugin keeps between its closures.
struct Shared {
    notifier: Notifier,
    jobs: Mutex<Option<Sender<(u64, Notification)>>>,
}

/// Shows a notification now, through `notify.show` (on the interface
/// thread; [`Notifier::post`] from others). Without the plugin, the
/// step runs nowhere and says so.
pub fn show(app: &AppHandle, n: Notification) -> Outcome {
    let Some(notifier) = app.service(&NOTIFIER) else {
        return Outcome::Done(Err("the notifications plugin is not connected".into()));
    };
    let id = notifier.next_id();
    run_show(app, &notifier, id, n)
}

fn args_of(id: u64, n: &Notification) -> Json {
    let mut args = serde_json::to_value(n).unwrap_or_default();
    args["id"] = json!(id);
    args
}

fn run_show(app: &AppHandle, notifier: &Notifier, id: u64, n: Notification) -> Outcome {
    let args = args_of(id, &n);
    let n_default = n.clone();
    let out = app.step(&STEP_SHOW, args, move |app, args| {
        let n: Notification = serde_json::from_value(args.clone()).unwrap_or(n_default);
        let shared = app
            .service(&SHARED)
            .ok_or("the notifications plugin is not connected")?;
        let wanted = desktop_wanted(app);
        let desktop = lock(&shared.notifier.0).desktop.clone();
        let reason = match (&wanted, &desktop) {
            (Err(why), _) => why.clone(),
            (Ok(()), Some(Err(why))) => why.clone(),
            (Ok(()), _) => {
                let sent = lock(&shared.jobs)
                    .as_ref()
                    .is_some_and(|tx| tx.send((id, n.clone())).is_ok());
                if sent {
                    shared.notifier.record(Shown {
                        id,
                        notification: n,
                        via: Via::Desktop,
                        picked: Vec::new(),
                    });
                    return Ok(json!({ "id": id, "via": "desktop" }));
                }
                "the notification thread is gone".into()
            }
        };
        toast(app, &shared.notifier, id, &n)?;
        Ok(json!({ "id": id, "via": "toast", "reason": reason }))
    });
    if out.vetoed().is_some() {
        notifier.record(Shown {
            id,
            notification: n,
            via: Via::Nowhere,
            picked: Vec::new(),
        });
    }
    out
}

/// Whether the setting sends this one to the desktop, or why not.
fn desktop_wanted(app: &AppHandle) -> Result<(), String> {
    let setting = lock(&app.store())
        .get(&format!("settings/{SETTING_DESKTOP}"))
        .and_then(|e| e.value.as_choice().map(str::to_string))
        .unwrap_or_else(|| "always".into());
    match setting.as_str() {
        "never" => Err("notifications show in the window (setting)".into()),
        "unfocused" => {
            let focused = app.egui().input(|i| i.viewport().focused.unwrap_or(true));
            if focused {
                Err("the window has the focus (setting)".into())
            } else {
                Ok(())
            }
        }
        _ => Ok(()),
    }
}

/// Shows it as a toast, its first action as the toast's.
fn toast(app: &AppHandle, notifier: &Notifier, id: u64, n: &Notification) -> Result<(), String> {
    let toasts = app
        .service(&TOASTS)
        .ok_or("no desktop notifications and no toasts")?;
    let text = if n.body.is_empty() {
        n.title.clone()
    } else {
        format!("{}: {}", n.title, n.body)
    };
    let mut t = Toast::new(n.level.toast(), &text);
    if let Some(a) = n.actions.first() {
        t = t.action(&a.label, CMD_ACT, json!({ "id": id, "action": a.id }));
    }
    toasts.notify(t);
    notifier.record(Shown {
        id,
        notification: n.clone(),
        via: Via::Toast,
        picked: Vec::new(),
    });
    Ok(())
}

/// Runs `notify.action` for an action picked.
fn picked(app: &AppHandle, notifier: &Notifier, id: u64, action: &str) -> Outcome {
    let Some(shown) = notifier.find(id) else {
        return Outcome::Done(Err(format!("no notification {id}")));
    };
    notifier.update(id, |h| h.picked.push(action.into()));
    let n = &shown.notification;
    let (command, args) = if action == "default" {
        match &n.default {
            Some((c, a)) => (Some(c.clone()), a.clone()),
            None => (None, Json::Null),
        }
    } else {
        match n.actions.iter().find(|a| a.id == action) {
            Some(a) => (Some(a.command.clone()), a.args.clone()),
            None => return Outcome::Done(Err(format!("no action `{action}` on {id}"))),
        }
    };
    app.step(
        &STEP_ACTION,
        json!({ "id": id, "action": action, "command": command, "args": args }),
        |app, args| {
            if args["action"] == "default" {
                crate::window::raise(&app.egui());
            }
            let Some(command) = args["command"]
                .as_str()
                .and_then(|c| CommandId::new(c).ok())
            else {
                return Ok(json!({ "ran": null }));
            };
            app.run_as(command.clone(), args["args"].clone(), Origin::User);
            Ok(json!({ "ran": command }))
        },
    )
}

const SHARED: ServiceToken<Shared> = ServiceToken::new("platform.notify.shared");

/// Desktop notifications, as a plugin (see the module).
pub struct NotifyPlugin {
    backend: Arc<dyn Backend>,
}

impl std::fmt::Debug for NotifyPlugin {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NotifyPlugin")
            .field("backend", &self.backend.name())
            .finish_non_exhaustive()
    }
}

impl NotifyPlugin {
    /// With the desktop's notifications ([`Desktop`]), the application
    /// named `app_name`.
    pub fn new(app_name: &str) -> Self {
        Self {
            backend: Arc::new(Desktop::new(app_name)),
        }
    }

    /// Another backend ([`Simulated`] in tests).
    pub fn backend(mut self, backend: impl Backend) -> Self {
        self.backend = Arc::new(backend);
        self
    }
}

impl Plugin for NotifyPlugin {
    fn id(&self) -> PluginId {
        PLUGIN
    }

    fn title(&self) -> String {
        "Notifications".into()
    }

    fn connect(&mut self, ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
        ctx.setting(
            SETTING_DESKTOP,
            Descriptor::choice(
                "Desktop notifications",
                &["always", "unfocused", "never"],
                "always",
            )
            .description(
                "Where notifications show: on the desktop, on the desktop only while the window does not have the focus, or in the window.",
            ),
        )?;
        ctx.step(
            StepSpec::new(
                STEP_SHOW,
                "A notification is shown (`id`, `title`, `body`, `level`, `actions`, `default`); the result says where (`via`: desktop or toast).",
            )
            .args(json!({ "id": 1, "title": "Export done", "body": "3 images", "level": "success", "actions": [] })),
        )?
        .step(
            StepSpec::new(
                STEP_ACTION,
                "The user picked a notification's action (`id`, `action`, `command`, `args`); by default its command runs.",
            )
            .args(json!({ "id": 1, "action": "show", "command": "files.reveal", "args": {} })),
        )?;
        ctx.command(
            CommandSpec::new(CMD_ACT, "Act on a notification", "Notifications")
                .description(
                    "Runs a notification's action (`id`, `action`), as a click on it does.",
                )
                .args(json!({ "id": 1, "action": "show" }))
                .hidden(),
            |call| {
                let id = call.args["id"].as_u64().ok_or("`id` is missing")?;
                let action = call.args["action"].as_str().ok_or("`action` is missing")?;
                let notifier = call.app.service(&NOTIFIER).ok_or("no notifier")?;
                match picked(call.app, &notifier, id, action) {
                    Outcome::Done(r) => r,
                    Outcome::Vetoed { by, reason } => Err(format!("vetoed by {by}: {reason}")),
                }
            },
        )?;
        let notifier = Notifier::default();
        let known = self.backend.known();
        {
            let mut s = lock(&notifier.0);
            s.waker = Some(ctx.app().egui());
            s.desktop = known.clone();
        }
        let (events_tx, events_rx) = channel::<Event>();
        let events_rx: Mutex<Receiver<Event>> = Mutex::new(events_rx);
        // The desktop's thread: asks once whether it shows notifications,
        // then shows them in order.
        let (jobs_tx, jobs_rx) = channel::<(u64, Notification)>();
        let backend = self.backend.clone();
        let waker = ctx.app().egui();
        let tx = events_tx;
        std::thread::Builder::new()
            .name("egui-pop notifications".into())
            .spawn(move || {
                let wake = |tx: &Sender<Event>, e: Event| {
                    let _ = tx.send(e);
                    waker.request_repaint();
                };
                if known.is_none() {
                    wake(&tx, Event::Probed(backend.probe()));
                }
                for (id, n) in jobs_rx {
                    let (rtx, rwaker) = (tx.clone(), waker.clone());
                    let respond: Respond = Box::new(move |action| {
                        let _ = rtx.send(Event::Picked(id, action));
                        rwaker.request_repaint();
                    });
                    if let Err(e) = backend.show(id, &n, respond) {
                        wake(&tx, Event::Failed(id, e));
                    }
                }
            })
            .map_err(|e| PluginError::Failed(e.to_string()))?;
        let shared = Shared {
            notifier: notifier.clone(),
            jobs: Mutex::new(Some(jobs_tx)),
        };
        ctx.service(&SHARED, shared)?;
        ctx.service(&NOTIFIER, notifier.clone())?;
        ctx.on(On::Pass, move |app, _| {
            let events: Vec<Event> = lock(&events_rx).try_iter().collect();
            for e in events {
                match e {
                    Event::Probed(r) => lock(&notifier.0).desktop = Some(r),
                    Event::Failed(id, error) => {
                        app.publish(
                            "egui_pop",
                            None,
                            FAILED,
                            json!({ "id": id, "error": error }),
                        );
                        if let Some(shown) = notifier.find(id) {
                            let _ = toast(app, &notifier, id, &shown.notification);
                        }
                    }
                    Event::Picked(id, action) => {
                        picked(app, &notifier, id, &action);
                    }
                }
            }
            let posted: Vec<Notification> = lock(&notifier.0).posted.drain(..).collect();
            for n in posted {
                let id = notifier.next_id();
                run_show(app, &notifier, id, n);
            }
        });
        Ok(())
    }

    fn disconnect(&mut self, ctx: &mut Ctx<'_>) {
        if let Some(shared) = ctx.app().service(&SHARED) {
            // The desktop's thread ends with its queue.
            lock(&shared.jobs).take();
            lock(&shared.notifier.0).waker = None;
        }
    }
}
