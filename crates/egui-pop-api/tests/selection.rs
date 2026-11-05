//! The application-wide selection as agents see and drive it: its
//! commands, its collector, its context keys and its journal events.

use egui_pop_api::App;
use egui_pop_api::selection::{
    CMD_ADD, CMD_BACK, CMD_CLEAR, CMD_FORWARD, CMD_HOVER, CMD_SET, SELECTION, SelectionPlugin,
};
use egui_pop_model::commands::Origin;
use egui_pop_model::ids::CommandId;
use serde_json::{Value as Json, json};

fn app() -> App {
    let app = App::builder()
        .plugin(SelectionPlugin)
        .build(&egui::Context::default())
        .unwrap();
    assert!(
        app.handle().diagnostics().is_empty(),
        "{:?}",
        app.handle().diagnostics()
    );
    app
}

fn run(app: &mut App, id: CommandId, args: Json) -> Result<Json, String> {
    app.run(&id, args, Origin::Agent("helper".into()))
        .now()
        .expect("answered now")
}

fn key(app: &App, k: &str) -> String {
    app.handle().context().get(k).to_string()
}

#[test]
fn agents_set_add_hover_and_clear_and_read_it_back() {
    let mut app = app();
    assert_eq!(key(&app, "hasSelection"), "false");
    assert_eq!(key(&app, "selectionCount"), "0");
    let out = run(&mut app, CMD_SET, json!({ "items": ["a", "b"] })).unwrap();
    assert_eq!(out["items"], json!(["a", "b"]));
    assert_eq!(out["changed"], json!(true));
    let out = run(&mut app, CMD_ADD, json!({ "items": ["b", "c"] })).unwrap();
    assert_eq!(out["items"], json!(["a", "b", "c"]));
    assert_eq!(key(&app, "hasSelection"), "true");
    assert_eq!(key(&app, "selectionCount"), "3");
    run(&mut app, CMD_HOVER, json!({ "item": "b" })).unwrap();
    let snap = app.handle().debug_snapshot();
    let s = &snap["selection"];
    assert_eq!(s["items"], json!(["a", "b", "c"]));
    assert_eq!(s["count"], json!(3));
    assert_eq!(s["hovered"], json!("b"));
    assert_eq!(s["hovered_by"], json!("command"));
    assert_eq!(s["back"], json!(2));
    run(&mut app, CMD_HOVER, json!({ "item": null })).unwrap();
    assert_eq!(
        app.handle().debug_snapshot()["selection"]["hovered"],
        Json::Null
    );
    run(&mut app, CMD_CLEAR, Json::Null).unwrap();
    assert_eq!(key(&app, "hasSelection"), "false");
    assert!(app.handle().service(&SELECTION).unwrap().items().is_empty());
    // Each change is on the journal.
    let changes = app
        .handle()
        .events_since(0)
        .into_iter()
        .filter(|e| e.name == "selection.changed")
        .count();
    assert!(changes >= 4, "{changes}");
    // A bad argument says why.
    assert!(run(&mut app, CMD_SET, json!({ "items": [1] })).is_err());
    assert!(run(&mut app, CMD_SET, json!({})).is_err());
}

#[test]
fn back_and_forward_walk_the_past_selections() {
    let mut app = app();
    for items in [json!(["a"]), json!(["b"]), json!(["c"])] {
        run(&mut app, CMD_SET, json!({ "items": items })).unwrap();
    }
    assert_eq!(key(&app, "selectionCanBack"), "true");
    assert_eq!(key(&app, "selectionCanForward"), "false");
    // Not available while there is nowhere to go: the clause refuses it.
    assert!(run(&mut app, CMD_FORWARD, Json::Null).is_err());
    let out = run(&mut app, CMD_BACK, Json::Null).unwrap();
    assert_eq!(out["items"], json!(["b"]));
    let out = run(&mut app, CMD_BACK, Json::Null).unwrap();
    assert_eq!(out["items"], json!(["a"]));
    assert_eq!(key(&app, "selectionCanForward"), "true");
    let out = run(&mut app, CMD_FORWARD, Json::Null).unwrap();
    assert_eq!(out["items"], json!(["b"]));
    // A new selection drops what was ahead.
    run(&mut app, CMD_SET, json!({ "items": ["d"] })).unwrap();
    assert_eq!(key(&app, "selectionCanForward"), "false");
    let out = run(&mut app, CMD_BACK, Json::Null).unwrap();
    assert_eq!(out["items"], json!(["b"]));
}

#[test]
fn the_keys_are_on_the_selection_surface() {
    let app = app();
    let keys = app.handle().keys_for(&CMD_BACK);
    assert_eq!(keys, ["Alt+Left"]);
    assert_eq!(app.handle().keys_for(&CMD_FORWARD), ["Alt+Right"]);
}
