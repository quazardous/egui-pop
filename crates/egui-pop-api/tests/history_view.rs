//! The history's projection (ADR 0014): the `history.*` commands agents
//! drive the histories with, and the history panel.

use egui_kittest::Harness;
use egui_kittest::kittest::Queryable as _;
use egui_pop_api::App;
use egui_pop_api::history::{self, APP};
use egui_pop_api::history_view::{self, HistoryPlugin, PANEL_HISTORY};
use egui_pop_core::lock;
use egui_pop_model::commands::Origin;
use egui_pop_model::ids::{CommandId, OwnerId};
use egui_pop_model::values::{Change, Descriptor, Set, Value};
use serde_json::{Value as Json, json};

const N: &str = "doc/n";

fn agent() -> Origin {
    Origin::Agent("helper".into())
}

/// An application with the plugin and three steps on `doc/n` (1, 2, 3).
fn app() -> App {
    let app = App::builder()
        .plugin(HistoryPlugin::default())
        .build(&egui::Context::default())
        .unwrap();
    let store = app.handle().store();
    let mut s = lock(&store);
    s.declare(&OwnerId::USER, N, Descriptor::int("N", 0, 9, 0))
        .unwrap();
    for n in 1..=3 {
        s.set(Set::new(N, Change::To(Value::Int(n)), Origin::User))
            .unwrap();
        // Three steps, not one merged.
        s.seal();
    }
    drop(s);
    app
}

fn run(app: &mut App, id: &'static str, args: Json) -> Result<Json, String> {
    app.run(&CommandId::from_static(id), args, agent())
        .now()
        .expect("answered now")
}

fn n(app: &App) -> Value {
    lock(&app.handle().store()).get(N).unwrap().value.clone()
}

#[test]
fn an_agent_reads_and_lists_the_histories() {
    let mut app = app();
    let read = run(&mut app, "history.read", json!({ "scope": "app" })).unwrap();
    assert_eq!(read["scope"], "app");
    assert_eq!(read["undo"].as_array().unwrap().len(), 3);
    assert_eq!(read["redo"].as_array().unwrap().len(), 0);
    assert_eq!(read["head"], read["undo"][2]["id"]);
    assert!(read["undo"][0]["text"].is_string());
    // Without a scope: where Undo goes.
    let routed = run(&mut app, "history.read", json!({})).unwrap();
    assert_eq!(routed["scope"], "app");
    let list = run(&mut app, "history.list", Json::Null).unwrap();
    assert_eq!(list["undo_scope"], "app");
    assert!(list["can_undo"].as_bool().unwrap());
    assert!(
        run(&mut app, "history.read", json!({ "scope": "nowhere" })).is_err(),
        "an unknown scope is refused"
    );
}

#[test]
fn an_agent_undoes_redoes_and_jumps_with_if_rev_and_dry_run() {
    let mut app = app();
    let read = run(&mut app, "history.read", json!({ "scope": "app" })).unwrap();
    let rev = read["rev"].as_u64().unwrap();
    // A dry run says what it would undo and changes nothing.
    let dry = run(
        &mut app,
        "history.undo",
        json!({ "scope": "app", "if_rev": rev, "dry_run": true }),
    )
    .unwrap();
    assert_eq!(dry["steps"][0]["id"], read["head"]);
    assert_eq!(dry["dry_run"], true);
    assert_eq!(n(&app), Value::Int(3));
    // A stale revision is refused.
    let stale = run(
        &mut app,
        "history.undo",
        json!({ "scope": "app", "if_rev": rev + 1 }),
    );
    assert!(stale.is_err(), "{stale:?}");
    assert_eq!(n(&app), Value::Int(3));
    // The undo itself.
    run(
        &mut app,
        "history.undo",
        json!({ "scope": "app", "if_rev": rev }),
    )
    .unwrap();
    assert_eq!(n(&app), Value::Int(2));
    run(&mut app, "history.redo", json!({ "scope": "app" })).unwrap();
    assert_eq!(n(&app), Value::Int(3));
    // A jump to the start, all at once; then to the first step.
    let first = read["undo"][0]["id"].as_u64().unwrap();
    let walked = run(&mut app, "history.jump", json!({ "scope": "app", "to": 0 })).unwrap();
    assert_eq!(walked["steps"].as_array().unwrap().len(), 3);
    assert_eq!(n(&app), Value::Int(0));
    run(
        &mut app,
        "history.jump",
        json!({ "scope": "app", "to": first }),
    )
    .unwrap();
    assert_eq!(n(&app), Value::Int(1));
    assert!(run(&mut app, "history.jump", json!({ "scope": "app" })).is_err());
    assert!(
        run(
            &mut app,
            "history.jump",
            json!({ "scope": "app", "to": 999 })
        )
        .is_err(),
        "no such step"
    );
    // The agent's walks went through the same steps as the keys.
    let line = history::line(app.handle(), &APP).unwrap();
    assert_eq!(line.undo.len(), 1);
    assert_eq!(line.redo.len(), 2);
}

#[test]
fn the_panel_lists_the_steps_and_a_click_jumps_there() {
    let app = app();
    let handle = app.handle().clone();
    handle.set_panel_visible(&PANEL_HISTORY, true);
    let mut h = Harness::builder()
        .with_size(egui::vec2(900.0, 600.0))
        .build_ui_state(|ui, app: &mut App| app.show(ui), app);
    h.run();
    h.run();
    assert!(handle.panel_visible(&PANEL_HISTORY));
    assert!(h.query_by_label("Start").is_some());
    assert_eq!(history_view::shown_scope(&handle), Some(APP));
    h.get_by_label("Start").click();
    h.run();
    assert_eq!(
        lock(&handle.store()).get(N).unwrap().value,
        Value::Int(0),
        "the start row undoes everything"
    );
    let line = history::line(&handle, &APP).unwrap();
    assert_eq!(line.redo.len(), 3);
}
