//! Logs to a file in the platform's log directory, rotated by size, at
//! a level the user sets.
//!
//! [`Logs`] is a sink for the `log` facade (egui-pop and most crates log
//! through it): each line goes to `<dir>/<name>.log` and to a tail kept
//! in memory (what a crash report quotes). When the file passes its
//! size, it becomes `<name>.1.log` (the older ones shift, the oldest
//! beyond [`Logs::keep`] goes) and a new one starts.
//!
//! [`LogsPlugin`] installs it as the process's logger (the router is
//! installed once; another plugin instance takes it over), declares the
//! `developer.log_level` setting (`error`, `warn`, `info`, `debug`,
//! `trace`), the `logs.reveal` command (opens the folder) and the `logs`
//! debug collector (the file, the level, the tail). An application on
//! `tracing` or another logger feeds lines with [`Logs::write`].
//!
//! No step runs per line: a line is written from whatever thread logs
//! it, and steps run on the interface thread. The crash report
//! (`crash`) is where the logs reach the application's hooks.
//!
//! Owned rather than tracing-appender (MIT): it rotates by time, writes
//! through tracing's subscriber, and keeps no tail to quote; this is a
//! hundred lines over `std::fs`.
//!
//! ```
//! use egui_pop_platform::logs::Logs;
//!
//! let dir = std::env::temp_dir().join(format!("egui-pop-logs-doc-{}", std::process::id()));
//! let logs = Logs::new(&dir, "demo").max_size(200).keep(2);
//! for i in 0..20 {
//!     logs.write(log::Level::Info, "demo", &format!("line {i}"));
//! }
//! assert!(logs.tail().last().unwrap().ends_with("INFO  demo: line 19"));
//! assert!(dir.join("demo.1.log").exists());
//! assert!(!dir.join("demo.3.log").exists());
//! # std::fs::remove_dir_all(&dir).unwrap();
//! ```

use std::collections::VecDeque;
use std::fs::{File, OpenOptions};
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, RwLock};

use egui_pop_api::services::ServiceToken;
use egui_pop_api::{Ctx, Plugin, PluginError};
use egui_pop_core::debug::FnCollector;
use egui_pop_core::lock;
use egui_pop_model::commands::CommandSpec;
use egui_pop_model::ids::{CommandId, PluginId, SettingId};
use egui_pop_model::values::{Descriptor, Value};
pub use log::{Level, LevelFilter};
use serde_json::json;

/// The plugin's id.
pub const PLUGIN: PluginId = PluginId::from_static("egui_pop.logs");
/// The level written: `error`, `warn`, `info`, `debug`, `trace`.
pub const SETTING_LEVEL: SettingId = SettingId::from_static("developer.log_level");
/// Opens the log folder.
pub const CMD_REVEAL: CommandId = CommandId::from_static("logs.reveal");
/// The logs, as a service.
pub const LOGS: ServiceToken<Logs> = ServiceToken::new("platform.logs");
/// How many lines the tail keeps.
pub const TAIL: usize = 200;
/// The levels, as the setting stores them.
pub const LEVELS: [&str; 5] = ["error", "warn", "info", "debug", "trace"];

struct Inner {
    path: PathBuf,
    max_size: u64,
    keep: usize,
    file: Mutex<Option<(File, u64)>>,
    tail: Mutex<VecDeque<String>>,
    level: AtomicUsize,
}

/// A log file and its tail. Cheap to clone (shared).
#[derive(Clone)]
pub struct Logs(Arc<Inner>);

impl std::fmt::Debug for Logs {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Logs")
            .field("path", &self.0.path)
            .field("level", &self.level())
            .finish_non_exhaustive()
    }
}

fn level_index(l: LevelFilter) -> usize {
    l as usize
}

fn level_of(i: usize) -> LevelFilter {
    LevelFilter::iter().nth(i).unwrap_or(LevelFilter::Info)
}

/// A level as the setting names it.
pub fn parse_level(text: &str) -> Option<LevelFilter> {
    match text {
        "off" => Some(LevelFilter::Off),
        "error" => Some(LevelFilter::Error),
        "warn" => Some(LevelFilter::Warn),
        "info" => Some(LevelFilter::Info),
        "debug" => Some(LevelFilter::Debug),
        "trace" => Some(LevelFilter::Trace),
        _ => None,
    }
}

/// `2026-10-12T14:03:07.123Z` for a time since the epoch.
pub fn timestamp(since_epoch: std::time::Duration) -> String {
    let secs = since_epoch.as_secs();
    let (days, rest) = (secs / 86_400, secs % 86_400);
    // Days to a civil date (Howard Hinnant's algorithm).
    let z = days as i64 + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!(
        "{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}.{:03}Z",
        rest / 3_600,
        rest / 60 % 60,
        rest % 60,
        since_epoch.subsec_millis()
    )
}

/// Now, as [`timestamp`] writes it.
pub fn now() -> String {
    timestamp(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default(),
    )
}

impl Logs {
    /// Logs to `<dir>/<name>.log` (the directory is made on the first
    /// line), at `info`, 5 MiB per file, 3 older files kept.
    pub fn new(dir: impl AsRef<Path>, name: &str) -> Logs {
        Logs(Arc::new(Inner {
            path: dir.as_ref().join(format!("{name}.log")),
            max_size: 5 * 1024 * 1024,
            keep: 3,
            file: Mutex::new(None),
            tail: Mutex::new(VecDeque::new()),
            level: AtomicUsize::new(level_index(LevelFilter::Info)),
        }))
    }

    fn with(self, f: impl FnOnce(&mut Inner)) -> Logs {
        let mut inner = Arc::try_unwrap(self.0).unwrap_or_else(|shared| Inner {
            path: shared.path.clone(),
            max_size: shared.max_size,
            keep: shared.keep,
            file: Mutex::new(None),
            tail: Mutex::new(lock(&shared.tail).clone()),
            level: AtomicUsize::new(shared.level.load(Ordering::Relaxed)),
        });
        f(&mut inner);
        Logs(Arc::new(inner))
    }

    /// The size a file grows to before it is rotated.
    pub fn max_size(self, bytes: u64) -> Logs {
        self.with(|i| i.max_size = bytes.max(1))
    }

    /// How many older files are kept.
    pub fn keep(self, files: usize) -> Logs {
        self.with(|i| i.keep = files)
    }

    /// The file written now.
    pub fn path(&self) -> &Path {
        &self.0.path
    }

    /// The directory of the files.
    pub fn dir(&self) -> &Path {
        self.0.path.parent().unwrap_or(Path::new("."))
    }

    pub fn level(&self) -> LevelFilter {
        level_of(self.0.level.load(Ordering::Relaxed))
    }

    /// Sets the level written (and the `log` facade's, when installed).
    pub fn set_level(&self, level: LevelFilter) {
        self.0.level.store(level_index(level), Ordering::Relaxed);
        if installed().is_some_and(|l| Arc::ptr_eq(&l.0, &self.0)) {
            log::set_max_level(level);
        }
    }

    /// The last lines written, oldest first.
    pub fn tail(&self) -> Vec<String> {
        lock(&self.0.tail).iter().cloned().collect()
    }

    /// Writes a line, if its level passes: to the file (rotated when
    /// full) and the tail. A failing file is given up silently (the
    /// tail stays).
    pub fn write(&self, level: Level, target: &str, message: &str) {
        if level > self.level() {
            return;
        }
        let line = format!("{} {:<5} {target}: {message}", now(), level);
        {
            let mut tail = lock(&self.0.tail);
            tail.push_back(line.clone());
            while tail.len() > TAIL {
                tail.pop_front();
            }
        }
        let mut file = lock(&self.0.file);
        if file
            .as_ref()
            .is_some_and(|(_, size)| *size + line.len() as u64 + 1 > self.0.max_size)
        {
            *file = None;
            self.rotate();
        }
        if file.is_none() {
            let _ = std::fs::create_dir_all(self.dir());
            *file = OpenOptions::new()
                .create(true)
                .append(true)
                .open(&self.0.path)
                .ok()
                .map(|f| {
                    let size = f.metadata().map(|m| m.len()).unwrap_or(0);
                    (f, size)
                });
        }
        if let Some((f, size)) = file.as_mut()
            && writeln!(f, "{line}").is_ok()
        {
            *size += line.len() as u64 + 1;
        }
    }

    /// `<name>.log` becomes `<name>.1.log`, the older ones shift.
    fn rotate(&self) {
        let numbered = |n: usize| {
            let stem = self
                .0
                .path
                .file_stem()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_default();
            self.dir().join(format!("{stem}.{n}.log"))
        };
        if self.0.keep == 0 {
            let _ = std::fs::remove_file(&self.0.path);
            return;
        }
        let _ = std::fs::remove_file(numbered(self.0.keep));
        for n in (1..self.0.keep).rev() {
            let _ = std::fs::rename(numbered(n), numbered(n + 1));
        }
        let _ = std::fs::rename(&self.0.path, numbered(1));
    }

    /// Flushes the file.
    pub fn flush(&self) {
        if let Some((f, _)) = lock(&self.0.file).as_mut() {
            let _ = f.flush();
        }
    }

    /// Makes these the process's logs: the `log` facade writes here from
    /// now on. Fails when another logger was installed first.
    pub fn install(&self) -> Result<(), String> {
        static SET: std::sync::OnceLock<Result<(), String>> = std::sync::OnceLock::new();
        SET.get_or_init(|| {
            log::set_logger(&ROUTER)
                .map_err(|_| "another logger is installed: feed it to Logs::write".to_string())
        })
        .clone()?;
        *ROUTER
            .0
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(self.clone());
        log::set_max_level(self.level());
        Ok(())
    }
}

/// The `log` facade's logger: hands each record to the installed
/// [`Logs`].
struct Router(RwLock<Option<Logs>>);

static ROUTER: Router = Router(RwLock::new(None));

impl log::Log for Router {
    fn enabled(&self, m: &log::Metadata<'_>) -> bool {
        installed().is_some_and(|l| m.level() <= l.level())
    }

    fn log(&self, r: &log::Record<'_>) {
        if let Some(l) = installed() {
            l.write(r.level(), r.target(), &r.args().to_string());
        }
    }

    fn flush(&self) {
        if let Some(l) = installed() {
            l.flush();
        }
    }
}

/// The logs the `log` facade writes to, when installed.
pub fn installed() -> Option<Logs> {
    ROUTER.0.try_read().ok().and_then(|l| l.as_ref().cloned())
}

/// Logs to a file, as a plugin (see the module).
#[derive(Debug)]
pub struct LogsPlugin {
    logs: Logs,
}

impl LogsPlugin {
    /// Logs of the application `app` (its name or id) in the platform's
    /// log directory ([`crate::dirs::log_dir`]; the temporary directory
    /// when the environment names none).
    pub fn new(app: &str) -> Self {
        let dir = crate::dirs::log_dir(app).unwrap_or_else(|| std::env::temp_dir().join(app));
        let name = app.rsplit('.').next().unwrap_or(app).to_lowercase();
        Self {
            logs: Logs::new(dir, &name),
        }
    }

    /// Other logs (another directory, size, files kept).
    pub fn logs(mut self, logs: Logs) -> Self {
        self.logs = logs;
        self
    }
}

impl Plugin for LogsPlugin {
    fn id(&self) -> PluginId {
        PLUGIN
    }

    fn title(&self) -> String {
        "Logs".into()
    }

    fn priority(&self) -> i32 {
        // First, so the others' connecting is logged.
        200
    }

    fn connect(&mut self, ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
        let logs = self.logs.clone();
        if let Err(e) = logs.install() {
            // Still kept: the application may feed it.
            logs.write(Level::Warn, "egui_pop", &e);
        }
        ctx.setting(
            SETTING_LEVEL,
            Descriptor::choice("Log level", &LEVELS, "info").description(
                "How much the application writes to its log file: errors only, up to every detail.",
            ),
        )?;
        let path = format!("settings/{SETTING_LEVEL}");
        if let Some(level) = lock(&ctx.app().store())
            .get(&path)
            .and_then(|e| e.value.as_choice().and_then(parse_level))
        {
            logs.set_level(level);
        }
        let l = logs.clone();
        ctx.on_change(&path, move |c| {
            if let Value::Choice(v) = &c.new
                && let Some(level) = parse_level(v)
            {
                l.set_level(level);
            }
        });
        let dir = logs.dir().to_path_buf();
        ctx.command(
            CommandSpec::new(CMD_REVEAL, "Show the log folder", "Help")
                .description("Opens the folder holding the application's log files."),
            move |_| {
                crate::dirs::reveal(&dir)?;
                Ok(json!({ "dir": dir }))
            },
        )?;
        let snap = logs.clone();
        ctx.collector(
            FnCollector::new("logs", "The log file, its level and its last lines.").snapshot(
                move |_| {
                    json!({
                        "path": snap.path(),
                        "level": snap.level().to_string().to_lowercase(),
                        "tail": snap.tail(),
                    })
                },
            ),
        )?;
        ctx.service(&LOGS, logs.clone())?;
        log::info!(target: "egui_pop", "logging to {}", logs.path().display());
        Ok(())
    }

    fn disconnect(&mut self, _ctx: &mut Ctx<'_>) {
        if installed().is_some_and(|l| Arc::ptr_eq(&l.0, &self.logs.0)) {
            self.logs.flush();
            *ROUTER
                .0
                .write()
                .unwrap_or_else(std::sync::PoisonError::into_inner) = None;
        }
    }
}
