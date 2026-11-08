//! The bridge's capabilities and `bridge.wait`, and the test kit's
//! frame-stepping wait.

use egui_pop_api::about::{AboutPlugin, AppInfo};
use egui_pop_api::wait::{WaitFor, Waited};
use egui_pop_api::{App, DefaultPlugins};
use egui_pop_test::wait::{Deadline, settle, wait_until};
use serde_json::json;

fn harness() -> egui_kittest::Harness<'static, App> {
    let mut h = egui_pop_test::app(
        App::builder()
            .plugins(DefaultPlugins)
            .plugin(AboutPlugin::new(AppInfo::new("Demo", "1.0.0"))),
        egui::vec2(800.0, 600.0),
    );
    h.run_ok();
    h
}

#[test]
fn describe_lists_actions_and_commands_under_protocol_1() {
    let h = harness();
    let f = h
        .state()
        .handle()
        .debug_call("bridge", "describe", &serde_json::Value::Null);
    assert!(f.ok, "{}", f.message);
    assert_eq!(f.data["protocol"], 1);
    let caps = f.data["capabilities"].as_array().unwrap();
    let named = |n: &str| caps.iter().find(|c| c["name"] == n).cloned();
    let wait = named("bridge.wait").expect("bridge.wait is listed");
    assert_eq!(wait["kind"], "action");
    assert_eq!(wait["agent"], true);
    assert_eq!(wait["args"]["type"], "object");
    assert_eq!(wait["result"]["required"], json!(["met", "last"]));
    let about = named("about.read").expect("a command is listed");
    assert_eq!(about["kind"], "command");
    assert_eq!(about["call"]["action"], "run");
}

#[test]
fn bridge_wait_answers_at_once() {
    let mut h = harness();
    let since = h.state().handle().last_event();
    let f = h
        .state_mut()
        .debug_call("commands", "run", &json!({ "id": "about.read" }));
    assert!(f.ok, "{}", f.message);
    let f = h.state().handle().debug_call(
        "bridge",
        "wait",
        &json!({ "any": [{ "event": "command.failed" }, { "event": "command.done" }], "since": since }),
    );
    assert!(f.ok, "{}", f.message);
    assert_eq!(f.data["met"], true, "{}", f.data);
    assert_eq!(f.data["by"], "event");
    assert_eq!(f.data["event"]["data"]["command"], "about.read");
    let f = h
        .state()
        .handle()
        .debug_call("bridge", "wait", &json!({ "event": "no.such" }));
    assert_eq!(f.data["met"], false, "{}", f.data);
    let f = h.state().handle().debug_call("bridge", "wait", &json!({}));
    assert!(!f.ok, "{}", f.message);
}

#[test]
fn wait_until_steps_frames_until_the_event() {
    let mut h = harness();
    let since = h.state().handle().last_event();
    // Queued: it runs before the next frame.
    let f = h
        .state()
        .handle()
        .debug_call("commands", "run", &json!({ "id": "about.read" }));
    assert!(f.ok, "{}", f.message);
    assert_eq!(
        h.state()
            .handle()
            .wait_met(&WaitFor::event("command.done"), since),
        None
    );
    let done = wait_until(
        &mut h,
        &WaitFor::event("command.done"),
        since,
        Deadline::default(),
    );
    assert!(
        matches!(done, Ok(Waited::Event(ref e)) if e.data["command"] == "about.read"),
        "{done:?}"
    );
    settle(&mut h, Deadline::default()).unwrap();
    let never = wait_until(
        &mut h,
        &WaitFor::event("no.such"),
        since,
        Deadline {
            frames: 3,
            ..Deadline::default()
        },
    );
    let e = never.unwrap_err();
    assert!(
        e.contains("not met after 3 frames") && e.contains("no.such"),
        "{e}"
    );
}
