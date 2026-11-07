---
egui-pop-platform: minor
egui-pop: minor
---

Several apps in one process (tests, a simulated desktop) keep their logs and crash reports apart.

- **`LogsPlugin::install(false)`** keeps the logs the app's own: they are not made the `log` facade's logger, so they hold only what is written to them (`Logs::write`) and no line logged by another app of the process. On by default, as before.
- **Crash reports** quote the app's own logs (its `LOGS` service: their path, and their tail in a panic), no longer the logs the process installed last. A `CrashPlugin` with `hook(false)` no longer receives the panics the process's hook catches for another app.
