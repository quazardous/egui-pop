//! Collector actions and agents: every action says whether agents may
//! call it (none that writes, by default), the bridge's calls are the
//! agent's, and every call goes through the `debug.call` step a plugin
//! may veto, the devtools panel's (the user's) included.

use egui_pop_api::app::STEP_DEBUG_CALL;
use egui_pop_api::contexts::ContextsPlugin;
use egui_pop_api::devtools::{self, DevtoolsPlugin, SETTING_DEVTOOLS};
use egui_pop_api::jobs::JobsPlugin;
use egui_pop_api::steps::{Before, Outcome};
use egui_pop_api::{App, AppBuilder, Ctx, DefaultPlugins, Plugin, PluginError};
use egui_pop_model::commands::Origin;
use egui_pop_model::ids::{PluginId, StepId};
use serde_json::{Value as Json, json};

/// Vetoes every call of the `journal` collector, by anyone.
struct Kiosk;

impl Plugin for Kiosk {
    fn id(&self) -> PluginId {
        PluginId::from_static("kiosk")
    }

    fn connect(&mut self, ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
        ctx.before(STEP_DEBUG_CALL, |_, args| {
            if args["collector"] == "journal" {
                Before::Veto("no journal here".into())
            } else {
                Before::Go
            }
        });
        Ok(())
    }
}

fn builder() -> AppBuilder {
    App::builder()
        .plugins(DefaultPlugins)
        .plugin(DevtoolsPlugin::default())
        .plugin(JobsPlugin::default())
        .plugin(ContextsPlugin::default())
}

fn build(b: AppBuilder) -> App {
    let mut app = b.build(&egui::Context::default()).unwrap();
    app.logic(&egui::Context::default());
    app
}

/// Every action described: (collector, action, its description).
fn actions(app: &App) -> Vec<(String, String, Json)> {
    let mut out = Vec::new();
    for c in app.handle().debug_describe().as_array().unwrap() {
        for a in c["actions"].as_array().unwrap() {
            out.push((
                c["name"].as_str().unwrap().to_string(),
                a["name"].as_str().unwrap().to_string(),
                a.clone(),
            ));
        }
    }
    out
}

fn devtools_on(app: &App) -> bool {
    egui_pop_core::lock(&app.handle().store())
        .get(&format!("settings/{SETTING_DEVTOOLS}"))
        .and_then(|e| e.value.as_bool())
        .unwrap()
}

#[test]
fn every_action_says_whether_agents_may_call_it() {
    let app = build(builder());
    let all = actions(&app);
    assert!(all.len() > 20, "{all:?}");
    for (c, a, spec) in &all {
        let mutates = spec["mutates"].as_bool().unwrap();
        let agent = spec["agent"].as_bool().unwrap();
        if c == "commands" && a == "run" {
            // It checks the command's own `agent` flag.
            assert!(agent && mutates);
        } else {
            assert_eq!(agent, !mutates, "{c}.{a}: {spec}");
        }
    }
}

#[test]
fn an_agent_is_refused_what_writes_and_the_journal_says_so() {
    let mut app = build(builder());
    let h = app.handle().clone();
    let refused: Vec<(String, String, Json)> = actions(&app)
        .into_iter()
        .filter(|(_, _, s)| s["agent"] == false)
        .collect();
    let names: Vec<String> = refused.iter().map(|(c, a, _)| format!("{c}.{a}")).collect();
    // One of each class: the core's, the store, plugins, dialogs, file
    // choices, toasts, the context stack, jobs.
    for expected in [
        "focus.clear",
        "overlays.close_top",
        "announcements.announce",
        "screen.set_interface_zoom",
        "store.set",
        "store.undo",
        "plugins.set_enabled",
        "dialogs.answer",
        "files.answer",
        "toasts.dismiss",
        "contexts.request",
        "contexts.hold",
        "contexts.release",
        "jobs.cancel",
        "jobs.pause",
        "jobs.resume",
        "jobs.retry",
        "jobs.dismiss",
        "jobs.clear",
    ] {
        assert!(
            names.iter().any(|n| n == expected),
            "{expected} in {names:?}"
        );
    }
    for (c, a, spec) in &refused {
        let before = h.last_event();
        let f = app.debug_call(c, a, &spec["args"]);
        assert!(!f.ok, "{c}.{a}: {f:?}");
        assert_eq!(
            f.message,
            format!("`{c}.{a}` refused: it is not offered to agents; only the user may call it")
        );
        let events = h.events_since(before);
        let refusal = events
            .iter()
            .find(|e| e.name == "debug.call.refused")
            .unwrap_or_else(|| panic!("{c}.{a} is journaled: {events:?}"));
        assert_eq!(refusal.source, "agent:debug-bridge");
        assert_eq!(refusal.data["collector"], c.as_str());
        assert_eq!(refusal.data["action"], a.as_str());
        assert!(
            !events.iter().any(|e| e.name == "debug.call"),
            "the step never ran: {events:?}"
        );
        assert_eq!(h.debug_log().last().unwrap(), &f);
    }
    // Nothing happened: the setting only its owner writes stays off, the
    // plugin that would be switched off stays connected.
    let f = app.debug_call(
        "store",
        "set",
        &json!({ "path": format!("settings/{SETTING_DEVTOOLS}"), "value": true }),
    );
    assert!(!f.ok);
    let f = app.debug_call(
        "plugins",
        "set_enabled",
        &json!({ "id": "egui_pop.dialogs", "enabled": false }),
    );
    assert!(!f.ok);
    app.logic(&egui::Context::default());
    assert!(!devtools_on(&app));
    assert!(
        h.plugins()
            .iter()
            .any(|p| p.id.as_str() == "egui_pop.dialogs" && p.connected)
    );
}

#[test]
fn an_allowed_read_still_works_and_stays_off_the_journal() {
    let mut app = build(builder());
    let h = app.handle().clone();
    let before = h.last_event();
    let f = app.debug_call(
        "store",
        "get",
        &json!({ "path": format!("settings/{SETTING_DEVTOOLS}") }),
    );
    assert!(f.ok && !f.mutates, "{f:?}");
    assert_eq!(f.data["value"], false);
    let f = app.debug_call("journal", "since", &json!({ "seq": before }));
    assert!(f.ok, "{f:?}");
    // Polling does not fill the journal nor the steps' traces.
    assert!(f.data.as_array().unwrap().is_empty(), "{}", f.data);
    assert!(h.events_since(before).is_empty());
    assert!(!h.step_traces().iter().any(|t| t.step == STEP_DEBUG_CALL));
    // `commands.run` stays the agent's door, the command's flag checked.
    let f = app.debug_call("commands", "run", &json!({ "id": "app.palette" }));
    assert!(f.ok, "{f:?}");
    let f = app.debug_call("commands", "run", &json!({ "id": "app.quit" }));
    assert!(
        !f.ok && f.message.contains("not offered to agents"),
        "{f:?}"
    );
    // A mutating call that ran is journaled through the step.
    let ran: Vec<_> = h
        .events_since(before)
        .into_iter()
        .filter(|e| e.name == STEP_DEBUG_CALL.as_str())
        .collect();
    assert_eq!(ran.len(), 2, "{ran:?}");
    assert_eq!(ran[0].data["args"]["origin"], "agent:debug-bridge");
}

#[test]
fn a_plugins_veto_refuses_even_an_allowed_action() {
    let mut app = build(builder().plugin(Kiosk));
    let h = app.handle().clone();
    let before = h.last_event();
    let f = app.debug_call("journal", "since", &json!({ "seq": 0 }));
    assert!(!f.ok);
    assert_eq!(
        f.message,
        "`journal.since` vetoed by kiosk: no journal here"
    );
    assert_eq!(h.debug_log().last().unwrap(), &f);
    let vetoed = h
        .events_since(before)
        .into_iter()
        .find(|e| e.name == "debug.call.vetoed")
        .unwrap();
    assert_eq!(vetoed.data["by"], "kiosk");
    assert_eq!(vetoed.data["args"]["origin"], "agent:debug-bridge");
    let t = h.step_traces().pop().unwrap();
    assert_eq!(t.step, STEP_DEBUG_CALL);
    assert_eq!(t.vetoed_by.unwrap().1, "no journal here");
    // The user is vetoed too.
    let f = app.debug_call_as("journal", "since", &json!({ "seq": 0 }), Origin::User);
    assert!(!f.ok && f.message.contains("vetoed by kiosk"), "{f:?}");
    // Other collectors go on.
    assert!(
        app.debug_call("mounts", "check", &json!({ "component": "x", "host": "y" }))
            .collector
            == "mounts"
    );
}

#[test]
fn the_devtools_panel_is_the_user_and_honours_the_veto() {
    let app = build(builder().plugin(Kiosk));
    let h = app.handle().clone();
    // The user may write what an agent may not.
    let out = devtools::call(
        &h,
        "store",
        "set",
        json!({ "path": format!("settings/{SETTING_DEVTOOLS}"), "value": true }),
    );
    assert!(out.ok(), "{out:?}");
    assert!(devtools_on(&app));
    let t = h
        .step_traces()
        .into_iter()
        .rfind(|t| t.step == STEP_DEBUG_CALL)
        .unwrap();
    assert_eq!(t.args["origin"], "user");
    // A veto on `debug.call` covers the panel.
    let out = devtools::call(&h, "journal", "since", json!({ "seq": 0 }));
    match out {
        Outcome::Done(Err(e)) => assert!(e.contains("vetoed by kiosk"), "{e}"),
        other => panic!("vetoed: {other:?}"),
    }
}

#[test]
fn an_application_offers_an_action_to_agents() {
    let mut app = build(
        builder()
            .offer_to_agents("toasts.dismiss")
            .offer_to_agents("toasts.nothing"),
    );
    let all = actions(&app);
    let dismiss = all
        .iter()
        .find(|(c, a, _)| c == "toasts" && a == "dismiss")
        .unwrap();
    assert_eq!(dismiss.2["agent"], true);
    // Offered: the toast's own check answers, not the refusal.
    let f = app.debug_call("toasts", "dismiss", &json!({ "id": 99 }));
    assert!(!f.ok && !f.message.contains("not offered"), "{f:?}");
    // A name that matches nothing is said.
    let d = app.handle().diagnostics();
    assert!(
        d.iter()
            .any(|d| d.code == "agent-offer-unknown" && d.subject == "toasts.nothing"),
        "{d:?}"
    );
}

#[test]
fn the_step_is_declared() {
    let app = build(builder());
    let steps = app.handle().steps();
    let step = steps.iter().find(|s| s.spec.id == STEP_DEBUG_CALL).unwrap();
    assert_eq!(step.owner, egui_pop_model::ids::OwnerId::FRAMEWORK);
    assert_eq!(STEP_DEBUG_CALL, StepId::from_static("debug.call"));
}
