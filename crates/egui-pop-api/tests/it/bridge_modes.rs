//! The bridge's modes (on, read-only, off), the commands' read-only
//! flag, and a refused `commands.run` on the journal.

use egui_pop_api::devtools::{self, DevtoolsPlugin, SETTING_DEVTOOLS};
use egui_pop_api::{App, AppBuilder, Ctx, DefaultPlugins, Plugin, PluginError};
use egui_pop_core::debug::BridgeMode;
use egui_pop_model::commands::{CommandSpec, Origin};
use egui_pop_model::ids::{CommandId, PluginId};
use serde_json::{Value as Json, json};

/// A read-only command kept from agents, and one that writes.
struct Probe;

impl Plugin for Probe {
    fn id(&self) -> PluginId {
        PluginId::from_static("probe")
    }

    fn connect(&mut self, ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
        ctx.command(
            CommandSpec::new(CommandId::from_static("probe.secret"), "Secret", "Probe")
                .read_only()
                .no_agent(),
            |_| Ok(json!("secret")),
        )?;
        ctx.command(
            CommandSpec::new(CommandId::from_static("probe.touch"), "Touch", "Probe"),
            |_| Ok(json!("touched")),
        )?;
        Ok(())
    }
}

fn build(b: AppBuilder) -> App {
    let mut app = b
        .plugins(DefaultPlugins)
        .plugin(DevtoolsPlugin::default())
        .plugin(Probe)
        .offer_to_agents("store.set")
        .build(&egui::Context::default())
        .unwrap();
    app.logic(&egui::Context::default());
    app
}

fn refused(app: &App, before: u64) -> Vec<Json> {
    app.handle()
        .events_since(before)
        .into_iter()
        .filter(|e| e.name == "debug.call.refused")
        .map(|e| e.data)
        .collect()
}

fn devtools_path() -> String {
    format!("settings/{SETTING_DEVTOOLS}")
}

#[test]
fn commands_say_whether_they_only_read() {
    let mut app = build(App::builder());
    let snap = app.handle().debug_snapshot();
    let flag = |id: &str| {
        snap["commands"]["commands"]
            .as_array()
            .unwrap()
            .iter()
            .find(|c| c["id"] == id)
            .unwrap_or_else(|| panic!("{id}"))["read_only"]
            .clone()
    };
    for reader in ["value.get", "value.list", "links.check", "probe.secret"] {
        assert_eq!(flag(reader), true, "{reader}");
    }
    for writer in ["value.set", "app.palette", "probe.touch"] {
        assert_eq!(flag(writer), false, "{writer}");
    }
    // `commands.run` reports it.
    let f = app.debug_call(
        "commands",
        "run",
        &json!({ "id": "value.get", "args": { "path": devtools_path() } }),
    );
    assert!(f.ok && !f.mutates, "{f:?}");
    let f = app.debug_call("commands", "run", &json!({ "id": "probe.touch" }));
    assert!(f.ok && f.mutates, "{f:?}");
}

#[test]
fn on_by_default_the_agent_calls_what_is_offered() {
    let mut app = build(App::builder());
    let f = app.debug_call(
        "store",
        "set",
        &json!({ "path": "settings/dialogs.closable", "value": true }),
    );
    // Offered; the writers' check answers, not the mode.
    assert!(!f.message.contains("debug bridge is"), "{f:?}");
    let f = app.debug_call("commands", "run", &json!({ "id": "probe.touch" }));
    assert!(f.ok, "{f:?}");
}

#[test]
fn read_only_the_agent_reads_and_nothing_else() {
    let mut app = build(App::builder().debug_bridge(BridgeMode::ReadOnly));
    let h = app.handle().clone();
    let before = h.last_event();
    // Reads.
    assert!(
        app.debug_call("store", "get", &json!({ "path": devtools_path() }))
            .ok
    );
    assert!(app.debug_call("journal", "since", &json!({ "seq": 0 })).ok);
    let f = app.debug_call(
        "commands",
        "run",
        &json!({ "id": "value.get", "args": { "path": devtools_path() } }),
    );
    assert!(f.ok && !f.mutates, "{f:?}");
    // Writes, even offered: refused, journaled.
    let f = app.debug_call("commands", "run", &json!({ "id": "probe.touch" }));
    assert!(!f.ok && f.message.contains("read-only"), "{f:?}");
    let f = app.debug_call(
        "store",
        "set",
        &json!({ "path": devtools_path(), "value": true }),
    );
    assert!(!f.ok && f.message.contains("read-only"), "{f:?}");
    let r = refused(&app, before);
    assert_eq!(r.len(), 2, "{r:?}");
    assert_eq!(r[0]["args"]["id"], "probe.touch");
    // The user is not held to it.
    let f = app.debug_call_as(
        "commands",
        "run",
        &json!({ "id": "probe.touch" }),
        Origin::User,
    );
    assert!(f.ok, "{f:?}");
}

#[test]
fn off_every_call_but_the_users_is_refused() {
    let mut app = build(App::builder().debug_bridge(BridgeMode::Off));
    let h = app.handle().clone();
    let before = h.last_event();
    for (c, a, args) in [
        ("store", "get", json!({ "path": devtools_path() })),
        ("journal", "since", json!({ "seq": 0 })),
        (
            "commands",
            "run",
            json!({ "id": "value.get", "args": { "path": devtools_path() } }),
        ),
    ] {
        let f = app.debug_call(c, a, &args);
        assert!(
            !f.ok && f.message.contains("the debug bridge is off"),
            "{f:?}"
        );
    }
    assert_eq!(refused(&app, before).len(), 3);
    // The user's devtools panel still works.
    let out = devtools::call(
        &h,
        "store",
        "set",
        json!({ "path": devtools_path(), "value": true }),
    );
    assert!(out.ok(), "{out:?}");
    assert!(
        app.debug_call_as(
            "store",
            "get",
            &json!({ "path": devtools_path() }),
            Origin::User
        )
        .ok
    );
}

#[test]
fn a_refused_commands_run_is_on_the_journal_once() {
    let mut app = build(App::builder());
    let h = app.handle().clone();
    for id in ["app.quit", "probe.secret", "no.such.command"] {
        let before = h.last_event();
        let f = app.debug_call("commands", "run", &json!({ "id": id }));
        assert!(!f.ok, "{f:?}");
        let events = h.events_since(before);
        let about: Vec<_> = events
            .iter()
            .filter(|e| e.data.to_string().contains(id))
            .collect();
        assert_eq!(about.len(), 1, "{id}: {events:?}");
        assert_eq!(about[0].name, "debug.call");
        assert!(
            about[0].data["error"]
                .as_str()
                .unwrap()
                .contains("not offered to agents"),
            "{:?}",
            about[0]
        );
        assert_eq!(about[0].data["args"]["origin"], "agent:debug-bridge");
    }
    // The user's run of the read-only command kept from agents works.
    let f = app.debug_call_as(
        "commands",
        "run",
        &json!({ "id": "probe.secret" }),
        Origin::User,
    );
    assert_eq!(f.data, json!("secret"), "{f:?}");
}
