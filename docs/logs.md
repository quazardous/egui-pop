# Logs and crash reports

`egui-pop-platform` writes the application's logs to a file in the
platform's log directory, rotated by size, at a level the user sets.
The facade re-exports it with the `platform` feature (or
`platform-logs` alone):

```toml
egui-pop = { version = "0.1", features = ["platform-logs"] }
```

## Logs

`LogsPlugin` installs a logger for the `log` facade, which egui-pop and
most crates log through. Each line goes to `<log dir>/<name>.log` and
to a tail kept in memory; when the file passes its size (5 MiB by
default) it becomes `<name>.1.log`, the older ones shift and the oldest
beyond those kept (3) goes.

| | The log directory |
|---|---|
| Linux and BSD | `$XDG_STATE_HOME/<app>/logs` (`~/.local/state/<app>/logs`) |
| macOS | `~/Library/Logs/<app>` |
| Windows | `%LOCALAPPDATA%\<app>\logs` |

The plugin declares the `developer.log_level` setting (`error`, `warn`,
`info`, `debug`, `trace`; `info` by default), the `logs.reveal` command
(opens the folder) and the `logs` debug collector (the file, the level,
the tail):

```rust
use egui_pop::egui;
use egui_pop::platform::logs::{Level, Logs, LogsPlugin, LOGS};
use egui_pop::prelude::*;

let dir = std::env::temp_dir().join(format!("viewer-logs-{}", std::process::id()));
let ctx = egui::Context::default();
let app = App::builder()
    .plugins(DefaultPlugins)
    // `LogsPlugin::new("org.example.Viewer")` uses the platform's
    // directory; here, a temporary one, smaller files, two kept.
    .plugin(LogsPlugin::new("org.example.Viewer").logs(Logs::new(&dir, "viewer").max_size(1 << 20).keep(2)))
    .build(&ctx)
    .unwrap();
let logs = app.handle().service(&LOGS).unwrap();
// What `log::warn!(target: "viewer", ...)` writes, from any thread:
logs.write(Level::Warn, "viewer", "the cache is nearly full");
assert!(logs.tail().last().unwrap().ends_with("WARN  viewer: the cache is nearly full"));
logs.flush();
assert!(std::fs::read_to_string(dir.join("viewer.log")).unwrap().contains("nearly full"));
# std::fs::remove_dir_all(&dir).unwrap();
```

An application that installed another logger first (`env_logger`,
`tracing`'s) gets a warning in the tail, and feeds the lines it wants in
the file with `Logs::write(level, target, message)`. No step runs per
line: lines are written from whatever thread logs them.

## Crash reports

`CrashPlugin` marks the session as running in the crash directory
(`<state dir>/crashes`) and installs a panic hook, chained to the one
before it. A panic writes the session's report: the application's name
and version, the OS and its release, the thread, the message and where,
a backtrace, the last log lines and the tail of the host's journal. A
clean exit (the `app.exit` step) takes the marker and the session's
panics away, so a panic a fault boundary caught makes no report.

On the next start, a marker left by a session that is gone means it
did not end cleanly: its panics, or, without one, a report saying it
ended without closing (killed, out of memory, a crash in native code)
with the last lines of its log file, become a report, offered by the
`crash.report` step. **Nothing is sent anywhere**: the default writes
the report and shows a toast that stays, with a button opening the file.

| Step | Arguments | Default |
|---|---|---|
| `crash.report` | `path`, `report` | the report (with what a before-hook added under `report.extra`) is written, then offered in a toast with *Show the report* (`crash.reveal`) |

An application adds its own data with a before-hook, and sends the
report itself, after asking the user, with an after-hook:

```rust
use egui_pop::egui;
use egui_pop::platform::crash::{self, CrashPlugin, Report, CRASHES, STEP_REPORT};
use egui_pop::prelude::*;

struct Reporter;

impl Plugin for Reporter {
    fn id(&self) -> PluginId {
        PluginId::from_static("reporter")
    }

    fn connect(&mut self, ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
        ctx.before(STEP_REPORT, |_, args| {
            let mut args = args.clone();
            args["report"]["extra"] = serde_json::json!({ "catalog": "photos.db" });
            Before::Redirect(args)
        });
        ctx.after(STEP_REPORT, |app, args, _| {
            // Ask the user, then send `args["report"]` to the
            // application's own service.
            app.publish("reporter", None, "reporter.offered", args["path"].clone());
        });
        Ok(())
    }
}

let dir = std::env::temp_dir().join(format!("viewer-crashes-{}", std::process::id()));
// What a session that panicked leaves behind.
crash::simulate(&dir, 4_100_000, &Report::panic("Viewer", "1.0.0", "index out of bounds")).unwrap();

let ctx = egui::Context::default();
let mut app = App::builder()
    .plugins(DefaultPlugins)
    .plugin(Reporter)
    // `CrashPlugin::new("org.example.Viewer", env!("CARGO_PKG_VERSION"))`
    // uses the platform's directory and installs the hook.
    .plugin(CrashPlugin::new("Viewer", "1.0.0").dir(&dir).hook(false))
    .build(&ctx)
    .unwrap();
app.logic(&ctx);
let crashes = app.handle().service(&CRASHES).unwrap();
let report = crashes.read(&crashes.reports()[0]).unwrap();
assert_eq!(report.extra["catalog"], "photos.db");
assert_eq!(report.summary(), "index out of bounds at src/main.rs:1:1");
# std::fs::remove_dir_all(&dir).unwrap();
```

The reports stay in the crash directory until dismissed
(`crash.dismiss`, `Crashes::dismiss`). The hook never blocks: if the
session's state is held at the moment of the panic, the panic is not
written.

## Platforms

| | Logs | Crash reports |
|---|---|---|
| Linux and BSD | yes | yes; a session is known gone when its process is |
| Windows, macOS | yes | yes; a session left behind is taken as gone (two instances at once should use the single instance) |
| Web | no file system: the tail only | no |

A panic with `panic = "abort"` still runs the hook, so the report is
written; a crash in native code (a segmentation fault in a driver)
leaves only the marker: the next start reports a session that ended
without closing, with its last log lines.
