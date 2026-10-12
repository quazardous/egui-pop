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
