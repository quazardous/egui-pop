//! The debug bridge over a running application: every collector
//! describes itself, snapshots as JSON, and every call returns feedback.

use egui_pop_api::zones::{self, Block};
use egui_pop_api::{App, Ctx, Plugin, PluginError};
use egui_pop_model::commands::CommandSpec;
use egui_pop_model::ids::{BlockId, CommandId, PluginId, SettingId};
use egui_pop_model::values::Descriptor;
use serde_json::json;

struct Demo;

impl Plugin for Demo {
    fn id(&self) -> PluginId {
        PluginId::from_static("demo")
    }

    fn connect(&mut self, ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
        ctx.command(
            CommandSpec::new(CommandId::from_static("demo.double"), "Double", "Demo")
                .key("Ctrl+D")
                .args(json!({ "n": 2 })),
            |call| Ok(json!(call.args["n"].as_i64().unwrap_or(0) * 2)),
        )?
        .setting(
            SettingId::from_static("demo.level"),
            Descriptor::int("Level", 0, 10, 3),
        )?
        .block(
            zones::MAIN,
            Block::add(BlockId::from_static("demo.hello"), 0, |ui, _| {
                ui.label("Hello");
            }),
        );
        Ok(())
    }
}

#[test]
fn every_collector_describes_and_snapshots_itself() {
    let mut h = egui_pop_test::app(App::builder().plugin(Demo), egui::vec2(800.0, 500.0));
    h.run();
    let app = h.state().handle().clone();
    let names: Vec<String> = app
        .debug_describe()
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["name"].as_str().unwrap().to_string())
        .collect();
    for expected in [
        "focus",
        "overlays",
        "faults",
        "announcements",
        "theme",
        "commands",
        "keys",
        "zones",
        "plugins",
        "diagnostics",
        "store",
    ] {
        assert!(
            names.iter().any(|n| n == expected),
            "{expected} in {names:?}"
        );
    }
    let snap = app.debug_snapshot();
    assert!(
        snap["commands"]["commands"]
            .as_array()
            .unwrap()
            .iter()
            .any(|c| c["id"] == "demo.double" && c["keys"] == json!(["Ctrl+D"]))
    );
    let main = snap["zones"]
        .as_array()
        .unwrap()
        .iter()
        .find(|z| z["id"] == "app.main")
        .unwrap()
        .clone();
    assert_eq!(main["blocks"][0]["id"], "demo.hello");
    assert_eq!(snap["plugins"]["plugins"][0]["connected"], true);
    assert!(snap["diagnostics"].as_array().unwrap().is_empty());
    // The whole snapshot is JSON an agent can read.
    serde_json::to_string(&snap).unwrap();
}

#[test]
fn calls_return_feedback_and_say_whether_they_change_things() {
    let mut h = egui_pop_test::app(App::builder().plugin(Demo), egui::vec2(800.0, 500.0));
    h.run();
    let app = h.state_mut();
    let f = app.debug_call("nope", "x", &json!(null));
    assert!(
        !f.ok && f.message.contains("there are: ") && f.message.contains("focus"),
        "{}",
        f.message
    );
    let f = app.debug_call("store", "fly", &json!(null));
    assert!(
        !f.ok && f.message.contains("it has: get, set, undo"),
        "{}",
        f.message
    );
    let f = app.debug_call("store", "get", &json!({ "path": "settings/demo.level" }));
    assert!(f.ok && !f.mutates);
    assert_eq!(f.data["value"], 3);
    let f = app.debug_call(
        "store",
        "set",
        &json!({ "path": "settings/demo.level", "value": 99 }),
    );
    assert!(f.ok && f.mutates, "{}", f.message);
    assert_eq!(f.data["value"], 10);
    assert_eq!(f.data["clamped"], "hard");
    let f = app.debug_call(
        "commands",
        "run",
        &json!({ "id": "demo.double", "args": { "n": 21 } }),
    );
    assert!(f.ok, "{}", f.message);
    assert_eq!(f.data, json!(42));
    let f = app.debug_call("commands", "run", &json!({ "id": "app.quit" }));
    assert!(
        !f.ok && f.message.contains("not offered to agents"),
        "{}",
        f.message
    );
    // Disconnecting through the bridge takes the plugin's block away.
    let f = app.debug_call(
        "plugins",
        "set_enabled",
        &json!({ "id": "demo", "enabled": false }),
    );
    assert!(f.ok && f.mutates);
    h.run();
    assert!(egui_kittest::kittest::Queryable::query_by_label(&h, "Hello").is_none());
    assert_eq!(h.state().handle().debug_log().len(), 7);
}

#[test]
fn the_keys_collector_explains_the_last_key() {
    let mut h = egui_pop_test::app(App::builder().plugin(Demo), egui::vec2(800.0, 500.0));
    h.run();
    h.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::D);
    h.run();
    let snap = h.state().handle().debug_snapshot();
    let trace = snap["keys"]["traces"]
        .as_array()
        .unwrap()
        .last()
        .unwrap()
        .clone();
    assert_eq!(trace["keys"], "Ctrl+D");
    assert_eq!(
        trace["resolution"],
        json!({ "kind": "run", "command": "demo.double" })
    );
    assert_eq!(
        trace["candidates"][0]["verdict"],
        json!({ "verdict": "won" })
    );
    let history = snap["commands"]["history"].as_array().unwrap();
    assert_eq!(history.last().unwrap()["id"], "demo.double");
}
