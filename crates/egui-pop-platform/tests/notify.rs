//! Notifications: to the desktop or a toast, actions routed to commands.

#![cfg(feature = "notify")]

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use egui_pop_api::toasts::{TOASTS, ToastsPlugin};
use egui_pop_api::{App, Ctx, Plugin, PluginError};
use egui_pop_core::lock;
use egui_pop_model::commands::{CommandSpec, Origin};
use egui_pop_model::ids::{CommandId, PluginId};
use egui_pop_model::values::{Change, Set, Value};
use egui_pop_platform::notify::{
    self, CMD_ACT, FAILED, NOTIFIER, Notification, NotifyPlugin, SETTING_DESKTOP, STEP_ACTION,
    Simulated, Via,
};
use serde_json::{Value as Json, json};

const SHOW: CommandId = CommandId::from_static("files.reveal");

/// Records the runs of `files.reveal`.
struct Files(Arc<Mutex<Vec<Json>>>);

impl Plugin for Files {
    fn id(&self) -> PluginId {
        PluginId::from_static("files")
    }

    fn connect(&mut self, ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
        let seen = self.0.clone();
        ctx.command(CommandSpec::new(SHOW, "Reveal", "Files"), move |call| {
            seen.lock().unwrap().push(call.args.clone());
            Ok(Json::Null)
        })?;
        Ok(())
    }
}

fn app(desktop: Simulated) -> (egui::Context, App, Arc<Mutex<Vec<Json>>>) {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let ctx = egui::Context::default();
    let app = App::builder()
        .plugin(ToastsPlugin)
        .plugin(Files(seen.clone()))
        .plugin(NotifyPlugin::new("test").backend(desktop))
        .build(&ctx)
        .unwrap();
    (ctx, app, seen)
}

fn exported() -> Notification {
    Notification::new("Export done", "3 images").action(
        "show",
        "Show",
        SHOW,
        json!({ "path": "out" }),
    )
}

/// Runs passes until `done` holds (the desktop answers from threads).
fn settle(ctx: &egui::Context, app: &mut App, done: impl Fn(&App) -> bool) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while !done(app) {
        assert!(Instant::now() < deadline, "timed out");
        std::thread::sleep(Duration::from_millis(5));
        app.logic(ctx);
    }
}

#[test]
fn a_desktop_notification_routes_its_action_to_its_command() {
    let desktop = Simulated::default();
    let (ctx, mut app, seen) = app(desktop.clone());
    let out = notify::show(app.handle(), exported());
    let id = out.result().unwrap().as_ref().unwrap()["id"]
        .as_u64()
        .unwrap();
    assert_eq!(out.result().unwrap().as_ref().unwrap()["via"], "desktop");
    settle(&ctx, &mut app, |_| desktop.shown().len() == 1);
    assert!(desktop.click(id, "show"));
    settle(&ctx, &mut app, |_| !seen.lock().unwrap().is_empty());
    assert_eq!(*seen.lock().unwrap(), vec![json!({ "path": "out" })]);
    let trace = app
        .handle()
        .step_traces()
        .into_iter()
        .find(|t| t.step == STEP_ACTION)
        .unwrap();
    assert_eq!(trace.args["action"], "show");
    let history = app.handle().service(&NOTIFIER).unwrap().history();
    assert_eq!(history[0].via, Via::Desktop);
    assert_eq!(history[0].picked, ["show"]);
    // No toast: the desktop showed it.
    assert!(app.handle().service(&TOASTS).unwrap().shown().is_empty());
}

#[test]
fn without_a_desktop_it_falls_back_to_a_toast_whose_action_goes_through_the_step() {
    let (ctx, mut app, seen) = app(Simulated::unavailable("no notification server"));
    let out = notify::show(app.handle(), exported());
    let r = out.result().unwrap().as_ref().unwrap().clone();
    assert_eq!(r["via"], "toast");
    assert_eq!(r["reason"], "no notification server");
    let toasts = app.handle().service(&TOASTS).unwrap();
    let shown = toasts.shown();
    assert_eq!(shown.len(), 1);
    assert_eq!(shown[0].toast.text, "Export done: 3 images");
    let action = shown[0].toast.action.clone().unwrap();
    assert_eq!(action.command, CMD_ACT);
    // The toast's button runs the action command, as a click does.
    app.run(&action.command, action.args, Origin::User);
    app.logic(&ctx);
    assert_eq!(*seen.lock().unwrap(), vec![json!({ "path": "out" })]);
    assert!(
        app.handle()
            .step_traces()
            .iter()
            .any(|t| t.step == STEP_ACTION)
    );
}

#[test]
fn the_setting_keeps_notifications_in_the_window() {
    let desktop = Simulated::default();
    let (_ctx, app, _) = app(desktop.clone());
    lock(&app.handle().store())
        .set(Set::new(
            &format!("settings/{SETTING_DESKTOP}"),
            Change::To(Value::Choice("never".into())),
            Origin::User,
        ))
        .unwrap();
    let out = notify::show(app.handle(), exported());
    assert_eq!(out.result().unwrap().as_ref().unwrap()["via"], "toast");
    assert!(desktop.shown().is_empty());
}

#[test]
fn notifications_posted_from_a_thread_show_at_the_next_pass() {
    let desktop = Simulated::default();
    let (ctx, mut app, _) = app(desktop.clone());
    let notifier = app.handle().service(&NOTIFIER).unwrap();
    let n = (*notifier).clone();
    std::thread::spawn(move || n.post(Notification::new("Done", "")))
        .join()
        .unwrap();
    app.logic(&ctx);
    settle(&ctx, &mut app, |_| desktop.shown().len() == 1);
    assert_eq!(desktop.shown()[0].1.title, "Done");
}

/// A desktop that says it is there, then fails.
#[derive(Clone)]
struct Broken;

impl notify::Backend for Broken {
    fn name(&self) -> String {
        "broken".into()
    }

    fn probe(&self) -> Result<String, String> {
        Ok("broken".into())
    }

    fn show(
        &self,
        _: u64,
        _: &Notification,
        _: Box<dyn FnOnce(String) + Send>,
    ) -> Result<(), String> {
        Err("the server went away".into())
    }
}

#[test]
fn a_desktop_failure_shows_the_toast_and_says_so() {
    let ctx = egui::Context::default();
    let mut app = App::builder()
        .plugin(ToastsPlugin)
        .plugin(NotifyPlugin::new("test").backend(Broken))
        .build(&ctx)
        .unwrap();
    notify::show(app.handle(), Notification::new("Saved", ""));
    let toasts = app.handle().service(&TOASTS).unwrap();
    settle(&ctx, &mut app, |_| toasts.shown().len() == 1);
    let failed = app
        .handle()
        .events_since(0)
        .into_iter()
        .find(|e| e.name == FAILED)
        .unwrap();
    assert_eq!(failed.data["error"], "the server went away");
}
