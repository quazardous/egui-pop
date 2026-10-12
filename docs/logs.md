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
