//! A single instance: a second launch hands over through the socket, and
//! the plugin turns launches into steps.

#![cfg(feature = "instance")]

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use egui_pop_api::steps::Before;
use egui_pop_api::{App, Ctx, Plugin, PluginError};
use egui_pop_model::commands::CommandSpec;
use egui_pop_model::ids::{CommandId, PluginId};
use egui_pop_platform::instance::{
    self, Instance, InstancePlugin, LAUNCHES, Launch, STEP_SECOND_LAUNCH, Single,
};
use serde_json::{Value as Json, json};

#[test]
fn a_second_launch_hands_its_arguments_to_the_first() {
    let id = format!("org.egui-pop.test-{}", std::process::id());
    let first = Launch {
        args: vec!["one".into()],
        ..Launch::default()
    };
    let Single::First(running) = instance::single_with(&id, first).unwrap() else {
        panic!("the first launch is the first");
    };
    assert!(running.socket().is_some());
    let second = Launch {
        args: vec!["--open".into(), "demo://x".into()],
        cwd: Some("/tmp".into()),
        activation: Some("token".into()),
    };
    assert!(matches!(
        instance::single_with(&id, second.clone()).unwrap(),
        Single::Handed
    ));
    let deadline = Instant::now() + Duration::from_secs(5);
    while running.launches().history().is_empty() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(running.launches().history(), vec![second]);
}

/// Opens links by recording them.
struct Opener(Arc<Mutex<Vec<Json>>>);

impl Plugin for Opener {
    fn id(&self) -> PluginId {
        PluginId::from_static("opener")
    }

    fn connect(&mut self, ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
        let seen = self.0.clone();
        ctx.command(
            CommandSpec::new(CommandId::from_static("demo.open"), "Open", "Demo"),
            move |call| {
                seen.lock().unwrap().push(call.args.clone());
                Ok(Json::Null)
            },
        )?;
        Ok(())
    }
}

#[test]
fn launches_become_steps_and_links_run_the_links_command() {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let first = Instance::detached(Launch {
        args: vec!["DEMO:first".into(), "other:no".into()],
        ..Launch::default()
    });
    let ctx = egui::Context::default();
    let mut app = App::builder()
        .plugin(Opener(seen.clone()))
        .plugin(
            InstancePlugin::new(first)
                .scheme("demo")
                .links(CommandId::from_static("demo.open")),
        )
        .build(&ctx)
        .unwrap();
    app.logic(&ctx);
    assert_eq!(*seen.lock().unwrap(), vec![json!({ "url": "DEMO:first" })]);
    let launches = app.handle().service(&LAUNCHES).unwrap();
    launches.deliver(Launch {
        args: vec!["demo://two".into(), "file.txt".into()],
        ..Launch::default()
    });
    app.logic(&ctx);
    app.logic(&ctx);
    assert_eq!(seen.lock().unwrap().len(), 2);
    let trace = app
        .handle()
        .step_traces()
        .into_iter()
        .find(|t| t.step == STEP_SECOND_LAUNCH)
        .unwrap();
    assert_eq!(trace.args["links"], json!(["demo://two"]));
    assert_eq!(
        trace.result,
        Some(Ok(json!({ "raised": true, "links": 1 })))
    );
}

/// Keeps the application in the background.
struct Quiet;

impl Plugin for Quiet {
    fn id(&self) -> PluginId {
        PluginId::from_static("quiet")
    }

    fn connect(&mut self, ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
        ctx.before(STEP_SECOND_LAUNCH, |_, _| Before::Veto("busy".into()));
        Ok(())
    }
}

#[test]
fn a_vetoed_second_launch_opens_nothing() {
    let ctx = egui::Context::default();
    let mut app = App::builder()
        .plugin(InstancePlugin::new(Instance::detached(Launch::default())).scheme("demo"))
        .plugin(Quiet)
        .build(&ctx)
        .unwrap();
    let launches = app.handle().service(&LAUNCHES).unwrap();
    launches.deliver(Launch {
        args: vec!["demo://x".into()],
        ..Launch::default()
    });
    app.logic(&ctx);
    let traces = app.handle().step_traces();
    assert!(
        traces
            .iter()
            .any(|t| t.step == STEP_SECOND_LAUNCH && t.vetoed_by.is_some())
    );
    assert!(!traces.iter().any(|t| t.step.as_str() == "deeplink.open"));
}

#[test]
fn inside_flatpak_the_socket_is_in_the_apps_shared_runtime_directory() {
    let env = |v: &str| match v {
        "FLATPAK_ID" => Some("org.example.Viewer".to_string()),
        "XDG_RUNTIME_DIR" => Some("/run/user/1000".to_string()),
        _ => None,
    };
    assert_eq!(
        instance::flatpak_socket_with("x.instance", env),
        Some("/run/user/1000/app/org.example.Viewer/x.instance.sock".into())
    );
    assert_eq!(instance::flatpak_socket_with("x", |_| None), None);
}

/// Records who each link was opened for.
struct Witness(Arc<Mutex<Vec<egui_pop_model::commands::Origin>>>);

impl Plugin for Witness {
    fn id(&self) -> PluginId {
        PluginId::from_static("witness")
    }

    fn connect(&mut self, ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
        let seen = self.0.clone();
        ctx.command(
            CommandSpec::new(CommandId::from_static("demo.open"), "Open", "Demo"),
            move |call| {
                seen.lock().unwrap().push(call.origin.clone());
                // What the handler starts goes on as the link's origin;
                // it may not claim the user's.
                call.app
                    .run(CommandId::from_static("demo.after"), Json::Null);
                call.app.run_as(
                    CommandId::from_static("demo.after"),
                    Json::Null,
                    egui_pop_model::commands::Origin::User,
                );
                Ok(Json::Null)
            },
        )?;
        let seen = self.0.clone();
        ctx.command(
            CommandSpec::new(CommandId::from_static("demo.after"), "After", "Demo"),
            move |call| {
                seen.lock().unwrap().push(call.origin.clone());
                Ok(Json::Null)
            },
        )?;
        Ok(())
    }
}

#[test]
fn a_link_from_a_launch_is_not_the_users_gesture() {
    use egui_pop_model::commands::Origin;
    let seen = Arc::new(Mutex::new(Vec::new()));
    let first = Instance::detached(Launch {
        args: vec!["demo://first".into()],
        ..Launch::default()
    });
    let ctx = egui::Context::default();
    let mut app = App::builder()
        .plugin(Witness(seen.clone()))
        .plugin(
            InstancePlugin::new(first)
                .scheme("demo")
                .links(CommandId::from_static("demo.open")),
        )
        .build(&ctx)
        .unwrap();
    app.logic(&ctx);
    app.logic(&ctx);
    let remote = instance::deeplink_origin();
    assert_eq!(remote, Origin::Remote("deeplink".into()));
    // The link, then the command it started; the claim to be the user
    // was refused.
    assert_eq!(*seen.lock().unwrap(), vec![remote.clone(), remote.clone()]);
    let h = app.handle().clone();
    let trace = h
        .step_traces()
        .into_iter()
        .find(|t| t.step.as_str() == "deeplink.open")
        .unwrap();
    assert_eq!(trace.args["origin"], "remote:deeplink");
    let refused = h
        .events_since(0)
        .into_iter()
        .find(|e| e.name == "command.refused")
        .unwrap();
    assert_eq!(refused.source, "remote:deeplink");
    assert_eq!(refused.data["origin"], "user");
    // A later launch's link, the same.
    seen.lock().unwrap().clear();
    h.service(&LAUNCHES).unwrap().deliver(Launch {
        args: vec!["demo://two".into()],
        ..Launch::default()
    });
    app.logic(&ctx);
    app.logic(&ctx);
    assert_eq!(seen.lock().unwrap().first(), Some(&remote));
    // A link of the scheme run by the user (a click in the application)
    // runs as the user.
    seen.lock().unwrap().clear();
    let _ = app.run(
        &instance::CMD_DEEPLINK,
        json!({ "url": "demo://three" }),
        Origin::User,
    );
    app.logic(&ctx);
    app.logic(&ctx);
    let seen = seen.lock().unwrap();
    assert_eq!(seen[0], Origin::User);
    assert!(seen[1..].iter().all(|o| *o == Origin::User), "{seen:?}");
}

/// Counts the repaints `ctx` asks for.
fn repaints(ctx: &egui::Context) -> Arc<std::sync::atomic::AtomicUsize> {
    let count = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let c = count.clone();
    ctx.set_request_repaint_callback(move |_| {
        c.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    });
    count
}

#[test]
fn without_app_the_launches_are_taken_and_wake_the_window() {
    let launches = Instance::detached(Launch::default()).launches();
    let ctx = egui::Context::default();
    let woken = repaints(&ctx);
    launches.set_waker(Some(ctx.clone()));
    let later = Launch {
        args: vec!["photo.jpg".into()],
        ..Launch::default()
    };
    launches.deliver(later.clone());
    assert!(woken.load(std::sync::atomic::Ordering::SeqCst) > 0);
    assert_eq!(launches.take(), std::slice::from_ref(&later));
    assert!(launches.take().is_empty(), "each launch is taken once");
    assert_eq!(launches.history(), [later]);

    launches.set_waker(None);
    let before = woken.load(std::sync::atomic::Ordering::SeqCst);
    launches.deliver(Launch::default());
    assert_eq!(woken.load(std::sync::atomic::Ordering::SeqCst), before);
    assert_eq!(launches.take().len(), 1);
}

#[test]
fn without_app_a_launch_handed_through_the_socket_is_taken() {
    let id = format!("org.egui-pop.test-noapp-{}", std::process::id());
    let Single::First(running) = instance::single_with(&id, Launch::default()).unwrap() else {
        panic!("the first launch is the first");
    };
    let launches = running.launches();
    let ctx = egui::Context::default();
    let woken = repaints(&ctx);
    launches.set_waker(Some(ctx));
    let second = Launch {
        args: vec!["--open".into(), "photo.jpg".into()],
        cwd: Some("/tmp".into()),
        activation: None,
    };
    assert!(matches!(
        instance::single_with(&id, second.clone()).unwrap(),
        Single::Handed
    ));
    // The running instance answers once it has delivered the launch.
    let deadline = Instant::now() + Duration::from_secs(5);
    let mut taken = Vec::new();
    while taken.is_empty() && Instant::now() < deadline {
        taken = launches.take();
        if taken.is_empty() {
            std::thread::sleep(Duration::from_millis(10));
        }
    }
    assert_eq!(taken, [second]);
    assert!(woken.load(std::sync::atomic::Ordering::SeqCst) > 0);
}
