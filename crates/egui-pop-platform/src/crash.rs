//! Crash reports, written on the machine and offered on the next start.
//! Nothing is sent anywhere.
//!
//! [`CrashPlugin`] marks the session as running (`session-<pid>.json` in
//! the crash directory, `<state>/crashes`) and installs a panic hook
//! (chained to the one before it). A panic writes `panic-<pid>.json`:
//! the application's name and version, the OS and its release, the
//! thread, the message, where, a backtrace, the last log lines (from
//! [`crate::logs`] when installed) and the tail of the host's journal.
//! A clean exit (the `app.exit` step) takes the marker and the panics of
//! the session away: a panic a fault boundary caught does not make a
//! report.
//!
//! On the next start, a marker left by a session that is gone means it
//! did not end cleanly: its panics, or a report saying it ended without
//! closing (killed, out of memory, a crash in native code) with the last
//! lines of its log file, become `report-<time>-<pid>.json`, and the
//! step runs (ADR 0011):
//!
//! - `crash.report`: a report to offer, `{ "path", "report" }`. By
//!   default the report (with whatever a before-hook added by
//!   redirecting it) is written back and offered in a toast that stays,
//!   with a button showing the file (`crash.reveal`). An application
//!   adds data with a before-hook, sends it with an after-hook (its own
//!   service, after asking the user), or replaces it with a dialog of
//!   its own.
//!
//! The reports stay until dismissed (`crash.dismiss`, [`Crashes`]).
//!
//! ```
//! use egui_pop_api::App;
//! use egui_pop_api::toasts::{TOASTS, ToastsPlugin};
//! use egui_pop_platform::crash::{self, CrashPlugin, Report};
//!
//! let dir = std::env::temp_dir().join(format!("egui-pop-crash-doc-{}", std::process::id()));
//! // A session that died: its marker and its panic stay.
//! crash::simulate(&dir, 4_000_000, &Report::panic("Demo", "1.0", "index out of bounds"))
//!     .unwrap();
//! let ctx = egui::Context::default();
//! let mut app = App::builder()
//!     .plugin(ToastsPlugin)
//!     .plugin(CrashPlugin::new("Demo", "1.0").dir(&dir).hook(false))
//!     .build(&ctx)
//!     .unwrap();
//! app.logic(&ctx);
//! let toasts = app.handle().service(&TOASTS).unwrap();
//! assert!(toasts.shown()[0].toast.text.contains("closed unexpectedly"));
//! # std::fs::remove_dir_all(&dir).unwrap();
//! ```

use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, RwLock};

use egui_pop_api::app::STEP_EXIT;
use egui_pop_api::hooks::On;
use egui_pop_api::services::ServiceToken;
use egui_pop_api::steps::StepSpec;
use egui_pop_api::toasts::{TOASTS, Toast};
use egui_pop_api::{AppHandle, Ctx, Plugin, PluginError};
use egui_pop_core::lock;
use egui_pop_model::commands::CommandSpec;
use egui_pop_model::ids::{CommandId, PluginId, StepId};
use serde::{Deserialize, Serialize};
use serde_json::{Value as Json, json};

/// The plugin's id.
pub const PLUGIN: PluginId = PluginId::from_static("egui_pop.crash");
/// A report to offer: `{ "path", "report" }`.
pub const STEP_REPORT: StepId = StepId::from_static("crash.report");
/// Shows a report's file (`{ "path" }`).
pub const CMD_REVEAL: CommandId = CommandId::from_static("crash.reveal");
/// Deletes a report (`{ "path" }`).
pub const CMD_DISMISS: CommandId = CommandId::from_static("crash.dismiss");
/// The reports, as a service.
pub const CRASHES: ServiceToken<Crashes> = ServiceToken::new("platform.crashes");
/// How many journal events a report quotes.
pub const JOURNAL_TAIL: usize = 50;
/// How many log lines a report quotes.
pub const LOG_TAIL: usize = 100;

/// One panic.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Panic {
    pub time: String,
    pub thread: String,
    pub message: String,
    /// `file:line:column`.
    pub location: String,
    pub backtrace: String,
}

/// A crash report.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Report {
    pub app: String,
    pub version: String,
    /// `linux`, `windows`, `macos`.
    pub os: String,
    pub arch: String,
    /// The OS's own name for itself (`Fedora Linux 44`), when known.
    pub os_release: String,
    pub pid: u32,
    /// When the session started.
    pub started: String,
    /// `panic`, or `ended` (the session went without closing and without
    /// a panic: killed, out of memory, a crash in native code).
    pub kind: String,
    /// The panics, first to last (a report's message is the last one's).
    pub panics: Vec<Panic>,
    /// The last log lines.
    pub log: Vec<String>,
    /// The tail of the host's journal.
    pub journal: Vec<Json>,
    /// What the application added (a before-hook on `crash.report`).
    #[serde(default)]
    pub extra: Json,
}

impl Report {
    /// A report for this machine, without panics yet.
    pub fn new(app: &str, version: &str) -> Report {
        Report {
            app: app.into(),
            version: version.into(),
            os: std::env::consts::OS.into(),
            arch: std::env::consts::ARCH.into(),
            os_release: os_release(),
            pid: std::process::id(),
            started: crate::logs::now(),
            kind: "ended".into(),
            ..Report::default()
        }
    }

    /// A report of one panic with `message`, for tests and the gallery.
    pub fn panic(app: &str, version: &str, message: &str) -> Report {
        let mut r = Report::new(app, version);
        r.kind = "panic".into();
        r.panics.push(Panic {
            time: crate::logs::now(),
            thread: "main".into(),
            message: message.into(),
            location: "src/main.rs:1:1".into(),
            backtrace: String::new(),
        });
        r
    }

    /// What the report says in one line.
    pub fn summary(&self) -> String {
        match self.panics.last() {
            Some(p) => format!("{} at {}", p.message, p.location),
            None => "it ended without closing".into(),
        }
    }
}

/// The OS's name for itself: `PRETTY_NAME` of `/etc/os-release` on
/// Linux, empty elsewhere.
fn os_release() -> String {
    if cfg!(target_os = "linux") {
        std::fs::read_to_string("/etc/os-release")
            .ok()
            .and_then(|t| {
                t.lines()
                    .find_map(|l| l.strip_prefix("PRETTY_NAME="))
                    .map(|v| v.trim_matches('"').to_string())
            })
            .unwrap_or_default()
    } else {
        String::new()
    }
}

fn session_file(dir: &Path, pid: u32) -> PathBuf {
    dir.join(format!("session-{pid}.json"))
}

fn panic_file(dir: &Path, pid: u32) -> PathBuf {
    dir.join(format!("panic-{pid}.json"))
}

/// Whether a process still runs (Linux; elsewhere a session left is
/// taken as gone).
fn alive(pid: u32) -> bool {
    pid == std::process::id()
        || (cfg!(target_os = "linux") && Path::new(&format!("/proc/{pid}")).exists())
}

fn write_json(path: &Path, value: &impl Serialize) -> std::io::Result<()> {
    let text = serde_json::to_string_pretty(value).map_err(std::io::Error::other)?;
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, text)?;
    std::fs::rename(tmp, path)
}

/// Leaves what a session that died leaves: its marker, and its panic
/// when `report` has one (for tests and the gallery; `pid` should name
/// no running process).
pub fn simulate(dir: &Path, pid: u32, report: &Report) -> std::io::Result<()> {
    std::fs::create_dir_all(dir)?;
    let mut r = report.clone();
    r.pid = pid;
    write_json(&session_file(dir, pid), &r)?;
    if !r.panics.is_empty() {
        write_json(&panic_file(dir, pid), &r)?;
    }
    Ok(())
}

/// The last lines of a text file.
fn last_lines(path: &Path, n: usize) -> Vec<String> {
    std::fs::read_to_string(path)
        .map(|t| {
            let lines: Vec<&str> = t.lines().collect();
            lines[lines.len().saturating_sub(n)..]
                .iter()
                .map(|l| l.to_string())
                .collect()
        })
        .unwrap_or_default()
}

/// Turns the sessions that died into reports: returns the reports'
/// files, newest last.
pub fn collect(dir: &Path, log_file: Option<&Path>) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for e in entries.flatten() {
        let name = e.file_name().to_string_lossy().into_owned();
        let Some(pid) = name
            .strip_prefix("session-")
            .and_then(|r| r.strip_suffix(".json"))
            .and_then(|p| p.parse::<u32>().ok())
        else {
            continue;
        };
        if alive(pid) {
            continue;
        }
        let session: Option<Report> = std::fs::read_to_string(e.path())
            .ok()
            .and_then(|t| serde_json::from_str(&t).ok());
        let panicked: Option<Report> = std::fs::read_to_string(panic_file(dir, pid))
            .ok()
            .and_then(|t| serde_json::from_str(&t).ok());
        let report = match (panicked, session) {
            (Some(r), _) => r,
            (None, Some(mut r)) => {
                r.kind = "ended".into();
                if let Some(log) = log_file {
                    r.log = last_lines(log, LOG_TAIL);
                }
                r
            }
            (None, None) => continue,
        };
        let stamp = crate::logs::now().replace([':', '.'], "-");
        let path = dir.join(format!("report-{stamp}-{pid}.json"));
        if write_json(&path, &report).is_ok() {
            let _ = std::fs::remove_file(e.path());
            let _ = std::fs::remove_file(panic_file(dir, pid));
            out.push(path);
        }
    }
    out.sort();
    out
}

/// What the panic hook reads: this session's report so far and where to
/// write it. Shared with the hook (process-wide).
#[derive(Debug, Default)]
struct HookState {
    dir: Option<PathBuf>,
    report: Report,
    journal: VecDeque<Json>,
}

static HOOK: RwLock<Option<Arc<Mutex<HookState>>>> = RwLock::new(None);

/// Installs the panic hook once per process (chained to the one before).
fn install_hook() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        let previous = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            write_panic(info);
            previous(info);
        }));
    });
}

/// Writes the panic into this session's panic file. Never blocks: a
/// state held elsewhere is skipped.
fn write_panic(info: &std::panic::PanicHookInfo<'_>) {
    let Some(state) = HOOK.try_read().ok().and_then(|h| h.clone()) else {
        return;
    };
    let Ok(mut s) = state.try_lock() else {
        return;
    };
    let Some(dir) = s.dir.clone() else {
        return;
    };
    let message = if let Some(m) = info.payload().downcast_ref::<&str>() {
        (*m).to_string()
    } else if let Some(m) = info.payload().downcast_ref::<String>() {
        m.clone()
    } else {
        "a panic without a message".into()
    };
    let panic = Panic {
        time: crate::logs::now(),
        thread: std::thread::current()
            .name()
            .unwrap_or("unnamed")
            .to_string(),
        message,
        location: info
            .location()
            .map(|l| format!("{}:{}:{}", l.file(), l.line(), l.column()))
            .unwrap_or_default(),
        backtrace: std::backtrace::Backtrace::force_capture().to_string(),
    };
    s.report.kind = "panic".into();
    s.report.panics.push(panic);
    #[cfg(feature = "logs")]
    if let Some(l) = crate::logs::installed() {
        let tail = l.tail();
        s.report.log = tail[tail.len().saturating_sub(LOG_TAIL)..].to_vec();
        l.flush();
    }
    s.report.journal = s.journal.iter().cloned().collect();
    let _ = std::fs::create_dir_all(&dir);
    let _ = write_json(&panic_file(&dir, s.report.pid), &s.report);
}

/// The reports on this machine, as a service.
#[derive(Clone, Debug)]
pub struct Crashes {
    dir: PathBuf,
    log_file: Option<PathBuf>,
    pending: Arc<Mutex<Vec<PathBuf>>>,
}

impl Crashes {
    /// The crash directory.
    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// The reports kept, oldest first.
    pub fn reports(&self) -> Vec<PathBuf> {
        let mut out: Vec<PathBuf> = std::fs::read_dir(&self.dir)
            .map(|d| {
                d.flatten()
                    .map(|e| e.path())
                    .filter(|p| {
                        p.file_name()
                            .is_some_and(|n| n.to_string_lossy().starts_with("report-"))
                    })
                    .collect()
            })
            .unwrap_or_default();
        out.sort();
        out
    }

    /// A report, read.
    pub fn read(&self, path: &Path) -> Option<Report> {
        serde_json::from_str(&std::fs::read_to_string(path).ok()?).ok()
    }

    /// Deletes a report.
    pub fn dismiss(&self, path: &Path) -> Result<(), String> {
        if path.parent() != Some(self.dir.as_path()) {
            return Err(format!("{} is not a report", path.display()));
        }
        std::fs::remove_file(path).map_err(|e| e.to_string())
    }

    /// Looks for sessions that died again (the plugin does at start);
    /// their reports are offered at the next pass.
    pub fn scan(&self) -> usize {
        let found = collect(&self.dir, self.log_file.as_deref());
        let n = found.len();
        lock(&self.pending).extend(found);
        n
    }
}

/// Runs `crash.report` for a report's file.
fn offer(app: &AppHandle, crashes: &Crashes, path: &Path) {
    let Some(report) = crashes.read(path) else {
        return;
    };
    let args = json!({ "path": path, "report": report });
    app.step(&STEP_REPORT, args, |app, args| {
        let path = PathBuf::from(args["path"].as_str().unwrap_or_default());
        // What a before-hook added stays with the report.
        if let Ok(report) = serde_json::from_value::<Report>(args["report"].clone()) {
            write_json(&path, &report).map_err(|e| e.to_string())?;
        }
        let toasts = app
            .service(&TOASTS)
            .ok_or("no toasts to offer the report in")?;
        let app_name = args["report"]["app"].as_str().unwrap_or("The application");
        toasts.notify(
            Toast::warning(&format!(
                "{app_name} closed unexpectedly last time. A report was saved on this computer; nothing was sent."
            ))
            .sticky()
            .action("Show the report", CMD_REVEAL, json!({ "path": path })),
        );
        Ok(json!({ "offered": true, "path": path }))
    });
}

/// Crash reports, as a plugin (see the module).
#[derive(Debug)]
pub struct CrashPlugin {
    app: String,
    version: String,
    dir: Option<PathBuf>,
    hook: bool,
    state: Option<Arc<Mutex<HookState>>>,
}

impl CrashPlugin {
    /// For the application `app` (its name or id) at `version`
    /// (`env!("CARGO_PKG_VERSION")`), in [`crate::dirs::crash_dir`].
    pub fn new(app: &str, version: &str) -> Self {
        Self {
            app: app.into(),
            version: version.into(),
            dir: None,
            hook: true,
            state: None,
        }
    }

    /// Another crash directory.
    pub fn dir(mut self, dir: impl Into<PathBuf>) -> Self {
        self.dir = Some(dir.into());
        self
    }

    /// Whether to install the panic hook (on by default; tests turn it
    /// off, their panics being failures).
    pub fn hook(mut self, on: bool) -> Self {
        self.hook = on;
        self
    }
}

impl Plugin for CrashPlugin {
    fn id(&self) -> PluginId {
        PLUGIN
    }

    fn title(&self) -> String {
        "Crash reports".into()
    }

    fn priority(&self) -> i32 {
        // Early, after the logs: the others' connecting is covered.
        190
    }

    fn connect(&mut self, ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
        let dir = self
            .dir
            .clone()
            .or_else(|| crate::dirs::crash_dir(&self.app))
            .unwrap_or_else(|| std::env::temp_dir().join(&self.app).join("crashes"));
        ctx.step(
            StepSpec::new(
                STEP_REPORT,
                "A crash report from the last session is offered (`path`, `report`); a before-hook may add to `report.extra`, an after-hook may send it.",
            )
            .args(json!({ "path": "report.json", "report": { "app": "Demo", "kind": "panic" } })),
        )?;
        let log_file = None::<PathBuf>;
        #[cfg(feature = "logs")]
        let log_file =
            log_file.or_else(|| crate::logs::installed().map(|l| l.path().to_path_buf()));
        let crashes = Crashes {
            dir: dir.clone(),
            log_file,
            pending: Arc::default(),
        };
        crashes.scan();
        ctx.service(&CRASHES, crashes.clone())?;
        let reveal = crashes.clone();
        ctx.command(
            CommandSpec::new(CMD_REVEAL, "Show a crash report", "Help")
                .description("Opens a crash report's file (`path`).")
                .args(json!({ "path": "report.json" }))
                .hidden(),
            move |call| {
                let path = PathBuf::from(call.args["path"].as_str().ok_or("`path` is missing")?);
                if path.parent() != Some(reveal.dir()) {
                    return Err(format!("{} is not a report", path.display()));
                }
                crate::dirs::reveal(&path)?;
                Ok(json!({ "path": path }))
            },
        )?;
        let dismiss = crashes.clone();
        ctx.command(
            CommandSpec::new(CMD_DISMISS, "Delete a crash report", "Help")
                .description("Deletes a crash report (`path`).")
                .args(json!({ "path": "report.json" }))
                .hidden(),
            move |call| {
                let path = PathBuf::from(call.args["path"].as_str().ok_or("`path` is missing")?);
                dismiss.dismiss(&path)?;
                Ok(json!({ "dismissed": path }))
            },
        )?;
        // This session runs: its marker says so until a clean exit.
        let report = Report::new(&self.app, &self.version);
        std::fs::create_dir_all(&dir)
            .and_then(|()| write_json(&session_file(&dir, report.pid), &report))
            .map_err(|e| PluginError::Failed(format!("{}: {e}", dir.display())))?;
        let state = Arc::new(Mutex::new(HookState {
            dir: Some(dir.clone()),
            report,
            journal: VecDeque::new(),
        }));
        *HOOK
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(state.clone());
        if self.hook {
            install_hook();
        }
        self.state = Some(state.clone());
        let mut seen = 0;
        ctx.on(On::Pass, move |app, _| {
            // The journal's tail, for the hook.
            let events = app.events_since(seen);
            if let Some(last) = events.last() {
                seen = last.seq;
                let mut s = lock(&state);
                for e in events {
                    s.journal.push_back(json!({
                        "seq": e.seq, "source": e.source, "name": e.name, "data": e.data,
                    }));
                }
                while s.journal.len() > JOURNAL_TAIL {
                    s.journal.pop_front();
                }
            }
            let pending: Vec<PathBuf> = lock(&crashes.pending).drain(..).collect();
            for path in pending {
                offer(app, &crashes, &path);
            }
        });
        let pid = std::process::id();
        ctx.after(STEP_EXIT, move |_, _, _| {
            let _ = std::fs::remove_file(session_file(&dir, pid));
            let _ = std::fs::remove_file(panic_file(&dir, pid));
        });
        Ok(())
    }

    fn disconnect(&mut self, _ctx: &mut Ctx<'_>) {
        if let Some(state) = self.state.take() {
            let mut hook = HOOK
                .write()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if hook.as_ref().is_some_and(|h| Arc::ptr_eq(h, &state)) {
                *hook = None;
            }
            let s = lock(&state);
            if let Some(dir) = &s.dir {
                let _ = std::fs::remove_file(session_file(dir, s.report.pid));
                let _ = std::fs::remove_file(panic_file(dir, s.report.pid));
            }
        }
    }
}
