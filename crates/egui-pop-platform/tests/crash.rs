//! Crash reports: sessions that died become reports offered at start;
//! a clean exit leaves nothing.

#![cfg(feature = "crash")]

use std::path::PathBuf;

use egui_pop_api::steps::Before;
use egui_pop_api::toasts::{TOASTS, ToastsPlugin};
use egui_pop_api::{App, Ctx, Plugin, PluginError};
use egui_pop_platform::crash::{self, CRASHES, CrashPlugin, Report, STEP_REPORT};
use serde_json::json;

fn dir(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("egui-pop-crash-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    d
}

/// Adds the application's own data to every report.
struct Extra;

impl Plugin for Extra {
    fn id(&self) -> egui_pop_model::ids::PluginId {
        egui_pop_model::ids::PluginId::from_static("extra")
    }

    fn connect(&mut self, ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
        ctx.before(STEP_REPORT, |_, args| {
            let mut args = args.clone();
            args["report"]["extra"] = json!({ "catalog": "photos.db" });
            Before::Redirect(args)
        });
        Ok(())
    }
}

#[test]
fn a_dead_sessions_panic_is_offered_with_the_applications_data() {
    let d = dir("panic");
    crash::simulate(&d, 4_100_001, &Report::panic("Demo", "1.2.3", "boom")).unwrap();
    let ctx = egui::Context::default();
    let mut app = App::builder()
        .plugin(ToastsPlugin)
        .plugin(Extra)
        .plugin(CrashPlugin::new("Demo", "1.2.3").dir(&d).hook(false))
        .build(&ctx)
        .unwrap();
    app.logic(&ctx);
    let crashes = app.handle().service(&CRASHES).unwrap();
    let reports = crashes.reports();
    assert_eq!(reports.len(), 1);
    let report = crashes.read(&reports[0]).unwrap();
    assert_eq!(report.kind, "panic");
    assert_eq!(report.summary(), "boom at src/main.rs:1:1");
    assert_eq!(report.extra, json!({ "catalog": "photos.db" }));
    let toasts = app.handle().service(&TOASTS).unwrap().shown();
    assert_eq!(toasts.len(), 1);
    let action = toasts[0].toast.action.clone().unwrap();
    assert_eq!(action.command, crash::CMD_REVEAL);
    // The dead session's files became the report.
    assert!(!d.join("session-4100001.json").exists());
    assert!(!d.join("panic-4100001.json").exists());
    // Dismissed, it is gone.
    crashes.dismiss(&reports[0]).unwrap();
    assert!(crashes.reports().is_empty());
    std::fs::remove_dir_all(&d).unwrap();
}

#[test]
fn a_session_gone_without_a_panic_says_it_ended_and_a_clean_exit_leaves_nothing() {
    let d = dir("ended");
    let mut gone = Report::new("Demo", "1.0");
    gone.panics.clear();
    crash::simulate(&d, 4_100_002, &gone).unwrap();
    let ctx = egui::Context::default();
    let mut app = App::builder()
        .plugin(ToastsPlugin)
        .plugin(CrashPlugin::new("Demo", "1.0").dir(&d).hook(false))
        .build(&ctx)
        .unwrap();
    app.logic(&ctx);
    let crashes = app.handle().service(&CRASHES).unwrap();
    let report = crashes.read(&crashes.reports()[0]).unwrap();
    assert_eq!(report.kind, "ended");
    assert_eq!(report.summary(), "it ended without closing");
    // This session is marked while it runs, and not after a clean exit.
    let own = d.join(format!("session-{}.json", std::process::id()));
    assert!(own.exists());
    app.exit();
    assert!(!own.exists());
    std::fs::remove_dir_all(&d).unwrap();
}
