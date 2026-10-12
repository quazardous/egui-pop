//! Logs: the `log` facade written to a rotating file, at the level the
//! setting says.

#![cfg(feature = "logs")]

use egui_pop_api::App;
use egui_pop_core::lock;
use egui_pop_model::commands::Origin;
use egui_pop_model::values::{Change, Set, Value};
use egui_pop_platform::logs::{self, LOGS, Logs, LogsPlugin, SETTING_LEVEL};

#[test]
fn the_plugin_writes_the_log_facade_at_the_settings_level() {
    let dir = std::env::temp_dir().join(format!("egui-pop-logs-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let ctx = egui::Context::default();
    let app = App::builder()
        .plugin(LogsPlugin::new("test").logs(Logs::new(&dir, "test")))
        .build(&ctx)
        .unwrap();
    let logs = app.handle().service(&LOGS).unwrap();
    log::info!(target: "demo", "kept");
    log::debug!(target: "demo", "dropped");
    lock(&app.handle().store())
        .set(Set::new(
            &format!("settings/{SETTING_LEVEL}"),
            Change::To(Value::Choice("debug".into())),
            Origin::User,
        ))
        .unwrap();
    log::debug!(target: "demo", "now kept");
    logs.flush();
    let text = std::fs::read_to_string(dir.join("test.log")).unwrap();
    assert!(text.contains("INFO  demo: kept"), "{text}");
    assert!(!text.contains("dropped"));
    assert!(text.contains("DEBUG demo: now kept"));
    assert!(logs.tail().iter().any(|l| l.contains("logging to")));
    let snap = app.handle().debug_snapshot();
    assert_eq!(snap["logs"]["level"], "debug");
    assert!(logs::installed().is_some());
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn timestamps_are_utc_dates() {
    use std::time::Duration;
    assert_eq!(logs::timestamp(Duration::ZERO), "1970-01-01T00:00:00.000Z");
    assert_eq!(
        logs::timestamp(Duration::from_millis(1_791_814_987_123)),
        "2026-10-12T14:23:07.123Z"
    );
    assert_eq!(
        logs::timestamp(Duration::from_secs(951_782_400)),
        "2000-02-29T00:00:00.000Z"
    );
}
