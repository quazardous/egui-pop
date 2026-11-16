//! Undo across changes made outside the interface (ADR 0014), as the
//! application sees them: an agent's request is a step of its own, the
//! next pass says the history moved, Ctrl+Z takes it back; a request
//! read before an outside change is refused as stale; an agent's dry
//! run answers as its undo would. The store's side is in
//! `egui-pop-model/tests/undo_outside.rs`.

use egui::{Key, Modifiers};
use egui_kittest::Harness;
use egui_pop_api::history::{self, APP, Walk};
use egui_pop_api::{App, AppHandle, Ctx, Plugin, PluginError};
use egui_pop_core::lock;
use egui_pop_model::commands::Origin;
use egui_pop_model::ids::{CommandId, OwnerId, PluginId};
use egui_pop_model::values::{Change, Descriptor, Set, Value};
use serde_json::{Value as Json, json};

const X: &str = "doc/x";
const Y: &str = "doc/y";
const PLACE: &str = "doc/place";
const SET: CommandId = CommandId::from_static("value.set");

fn agent() -> Origin {
    Origin::Agent("t".into())
}

/// `doc/x` the agent may write, `doc/y` it may not, `doc/place` the
/// application keeps for itself.
struct Doc;

impl Plugin for Doc {
    fn id(&self) -> PluginId {
        PluginId::from_static("doc")
    }

    fn connect(&mut self, ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
        let store = ctx.app().store();
        let mut s = lock(&store);
        s.declare(
            &OwnerId::USER,
            X,
            Descriptor::int("X", 0, 99, 0).writers(&[agent().writer()]),
        )
        .unwrap();
        s.declare(&OwnerId::USER, Y, Descriptor::int("Y", 0, 99, 0))
            .unwrap();
        s.declare(&OwnerId::USER, PLACE, Descriptor::int("Place", 0, 99, 0))
            .unwrap();
        Ok(())
    }
}

fn app() -> App {
    App::builder()
        .plugin(Doc)
        .build(&egui::Context::default())
        .unwrap()
}

fn user(a: &AppHandle, path: &str, v: i64) {
    lock(&a.store())
        .set(Set::new(path, Change::To(Value::Int(v)), Origin::User))
        .unwrap();
}

fn int(a: &AppHandle, path: &str) -> Value {
    lock(&a.store()).get(path).unwrap().value.clone()
}

fn events(a: &AppHandle, since: u64, name: &str) -> Vec<Json> {
    a.events_since(since)
        .into_iter()
        .filter(|e| e.name == name)
        .map(|e| e.data)
        .collect()
}

#[test]
fn an_agents_step_is_published_at_the_next_pass_and_ctrl_z_takes_it_back() {
    let app = app();
    let a = app.handle().clone();
    let mut h = Harness::builder()
        .with_size(egui::vec2(400.0, 300.0))
        .build_ui_state(|ui, app: &mut App| app.show(ui), app);
    h.run();
    user(&a, X, 1);
    h.run();
    // The agent's request, run before the next frame: its own step,
    // which the pass after it says moved the history.
    let before = a.last_event();
    a.run_as(SET, json!({ "path": X, "value": 5 }), agent());
    h.run();
    assert_eq!(int(&a, X), Value::Int(5));
    let line = history::line(&a, &APP).unwrap();
    assert_eq!(line.undo.len(), 2);
    let head = line.head().unwrap();
    assert_eq!(head.origin, agent());
    let changed = events(&a, before, "history.changed");
    assert_eq!(changed.len(), 1, "{changed:?}");
    assert_eq!(changed[0]["scope"], "app");
    assert_eq!(changed[0]["head"], head.id);
    assert_eq!(changed[0]["rev"], line.rev);
    // A value written untracked (state the application keeps): no
    // step, and nothing said about the history.
    let before = a.last_event();
    lock(&a.store())
        .set(Set::new(PLACE, Change::To(Value::Int(3)), Origin::User).untracked())
        .unwrap();
    h.run();
    assert!(events(&a, before, "history.changed").is_empty());
    assert_eq!(history::line(&a, &APP).unwrap().undo.len(), 2);
    // The user's Ctrl+Z takes back the agent's step, the newest.
    let before = a.last_event();
    h.key_press_modifiers(Modifiers::COMMAND, Key::Z);
    h.run();
    assert_eq!(int(&a, X), Value::Int(1));
    assert_eq!(int(&a, PLACE), Value::Int(3), "the untracked value stays");
    let undone = events(&a, before, "edit.undone");
    assert_eq!(undone.len(), 1);
    assert_eq!(undone[0]["scope"], "app");
    assert_eq!(undone[0]["label"], "set doc/x");
    let changed = events(&a, before, "history.changed");
    assert_eq!(changed.len(), 1, "said once, not again at the pass");
    assert_eq!(
        changed[0]["head"],
        history::line(&a, &APP).unwrap().undo[0].id
    );
}

#[test]
fn a_request_read_before_an_outside_change_is_refused_as_stale() {
    let mut app = app();
    let a = app.handle().clone();
    user(&a, X, 1);
    let rev = history::line(&a, &APP).unwrap().rev;
    // The agent changes the value after the request was read.
    app.run(&SET, json!({ "path": X, "value": 5 }), agent())
        .now()
        .unwrap()
        .unwrap();
    let before = a.last_event();
    let out = history::ask(&a, Walk::Undo, Some(&APP), Some(rev), false, &Origin::User);
    assert!(out.result().unwrap().is_err());
    assert_eq!(int(&a, X), Value::Int(5), "nothing moved");
    let refused = events(&a, before, "history.refused");
    assert_eq!(refused.len(), 1);
    assert_eq!(refused[0]["kind"], "stale");
    assert_eq!(refused[0]["scope"], "app");
    // Read again, the request names the agent's step.
    let rev = history::line(&a, &APP).unwrap().rev;
    let out = history::ask(&a, Walk::Undo, Some(&APP), Some(rev), false, &Origin::User);
    let r = out.result().unwrap().clone().unwrap();
    assert_eq!(
        r["steps"][0]["origin"],
        json!({ "kind": "agent", "id": "t" })
    );
    assert_eq!(int(&a, X), Value::Int(1));
}

#[test]
fn an_agents_dry_run_answers_as_its_undo_would() {
    let app = app();
    let a = app.handle().clone();
    // A user's step on a value the agent may not write.
    user(&a, Y, 4);
    let rev = history::line(&a, &APP).unwrap().rev;
    for what in [Walk::Undo, Walk::Jump(0)] {
        let dry = history::ask(&a, what, Some(&APP), None, true, &agent());
        let err = dry.result().unwrap().clone().unwrap_err();
        assert!(err.contains("may not write `doc/y`"), "{what:?}: {err}");
        let before = a.last_event();
        let real = history::ask(&a, what, Some(&APP), None, false, &agent());
        assert_eq!(real.result().unwrap().clone().unwrap_err(), err, "{what:?}");
        let refused = events(&a, before, "history.refused");
        assert_eq!(refused.len(), 1, "{what:?}");
        assert_eq!(refused[0]["kind"], "owner");
        assert_eq!(int(&a, Y), Value::Int(4));
        assert_eq!(history::line(&a, &APP).unwrap().rev, rev);
    }
    // The user's dry run goes through, and changes nothing.
    let dry = history::ask(&a, Walk::Undo, Some(&APP), None, true, &Origin::User);
    assert_eq!(dry.result().unwrap().clone().unwrap()["dry_run"], true);
    assert_eq!(int(&a, Y), Value::Int(4));
}
