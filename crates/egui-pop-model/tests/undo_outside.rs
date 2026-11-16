//! Undo across changes made outside the interface (ADR 0014): a write
//! that is not a step (state the application keeps, a settings file
//! read back), an agent's or a script's write, a step in another
//! history, an owner removed (ADR 0028). What each records, merges or
//! leaves, and what undo and redo do after it.

use std::sync::{Arc, Mutex};

use egui_pop_model::clock::ManualClock;
use egui_pop_model::commands::{Commands, Origin};
use egui_pop_model::history::ScopeId;
use egui_pop_model::ids::{CommandId, OwnerId};
use egui_pop_model::values::*;
use egui_pop_model::when::WhenContext;
use serde_json::json;

const X: &str = "doc/x";
const Y: &str = "doc/y";
const PLACE: &str = "doc/place";
const PLUGIN_A: &str = "plugin/a";

fn agent() -> Origin {
    Origin::Agent("a1".into())
}

/// `doc/x` the agent may write, `doc/y` it may not, `doc/place` the
/// application keeps for itself, all at 0.
fn store() -> Store {
    let mut s = Store::new();
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
    s
}

fn to(path: &str, v: i64, origin: Origin) -> Set {
    Set::new(path, Change::To(Value::Int(v)), origin)
}

fn user(s: &mut Store, path: &str, v: i64) {
    s.set(to(path, v, Origin::User)).unwrap();
}

fn int(s: &Store, path: &str) -> i64 {
    s.get(path).unwrap().value.as_f64().unwrap() as i64
}

#[test]
fn a_write_that_is_not_a_step_leaves_the_history_as_it_was() {
    let mut s = store();
    user(&mut s, X, 1);
    s.undo(&Origin::User).unwrap();
    let rev = s.history_rev(&APP);
    // The window's place, written untracked, and a settings file read
    // back: values change, no step is made, the redo list stays.
    s.set(to(PLACE, 4, Origin::User).untracked()).unwrap();
    let doc = SettingsDoc {
        values: [(Y.to_string(), json!(7))].into(),
    };
    assert!(s.import(&doc, &Origin::Remote("file".into())).is_empty());
    assert_eq!((int(&s, PLACE), int(&s, Y)), (4, 7));
    assert!(s.undo_steps().is_empty());
    assert_eq!(s.redo_steps().len(), 1, "the redo list is kept");
    assert_eq!(s.history_rev(&APP), rev, "the history did not move");
    assert_eq!(
        s.get(Y).unwrap().origin,
        Some(Origin::Remote("file".into()))
    );
    // Redo and undo walk the user's step alone: the values changed
    // outside it keep the outside change.
    s.redo(&Origin::User).unwrap();
    assert_eq!((int(&s, X), int(&s, Y), int(&s, PLACE)), (1, 7, 4));
    s.undo(&Origin::User).unwrap();
    assert_eq!((int(&s, X), int(&s, Y), int(&s, PLACE)), (0, 7, 4));
}

#[test]
fn an_agents_write_is_a_step_of_its_own_undone_first_as_the_newest() {
    let store = Arc::new(Mutex::new(store()));
    let mut r = Commands::new();
    register(&mut r, &OwnerId::FRAMEWORK, store.clone()).unwrap();
    let cx = WhenContext::new();
    user(&mut store.lock().unwrap(), X, 1);
    user(&mut store.lock().unwrap(), Y, 1);
    // The agent's request, as the bus or the bridge brings it.
    r.run(
        &CommandId::from_static("value.set"),
        json!({ "path": X, "value": 5 }),
        agent(),
        &cx,
    )
    .unwrap();
    let mut s = store.lock().unwrap();
    let steps = s.undo_steps();
    assert_eq!(steps.len(), 3);
    assert_eq!(steps[2].origin, agent());
    assert_eq!(
        steps[2].changes,
        vec![(X.into(), Value::Int(1), Value::Int(5))]
    );
    assert_eq!(s.get(X).unwrap().writer, Some(agent().writer()));
    // The user's Ctrl+Z takes back the newest step, whoever made it;
    // the user's own steps stay.
    s.undo(&Origin::User).unwrap();
    assert_eq!((int(&s, X), int(&s, Y)), (1, 1));
    assert_eq!(s.undo_steps().len(), 2);
    assert_eq!(
        s.redo_steps()[0].origin,
        agent(),
        "the redo keeps its origin"
    );
    // Redone, it is the agent's step again, not the user's.
    s.redo(&Origin::User).unwrap();
    assert_eq!(int(&s, X), 5);
    assert_eq!(s.undo_steps()[2].origin, agent());
}

#[test]
fn an_agent_may_not_take_back_what_it_may_not_write() {
    let mut s = store();
    user(&mut s, Y, 4);
    let rev = s.history_rev(&APP);
    let refused = s.undo_by(&agent(), &agent().writer()).unwrap_err();
    assert!(
        matches!(&refused, Refused::NotWriter { path, .. } if path == Y),
        "{refused:?}"
    );
    // Nothing moved: the value, the step, the revision.
    assert_eq!(int(&s, Y), 4);
    assert_eq!(s.undo_steps().len(), 1);
    assert!(s.redo_steps().is_empty());
    assert_eq!(s.history_rev(&APP), rev);
    // The user (the framework writes for a person) may.
    s.undo(&Origin::User).unwrap();
    assert_eq!(int(&s, Y), 0);
    // An agent's undo of a value it may write goes through.
    s.set(to(X, 3, agent()).by(&agent().writer())).unwrap();
    assert_eq!(
        s.undo_by(&agent(), &agent().writer()).unwrap().as_deref(),
        Some("set doc/x")
    );
    assert_eq!(int(&s, X), 0);
}

#[test]
fn an_outside_step_after_an_undo_replaces_the_redo_list() {
    let mut s = store();
    user(&mut s, X, 1);
    user(&mut s, X, 2);
    s.undo(&Origin::User).unwrap();
    let undone = s.redo_steps()[0].id;
    // The agent changes the value meanwhile: a new step, the user's
    // redo is gone from the list, kept as a replaced line.
    s.set(to(X, 9, agent()).by(&agent().writer())).unwrap();
    assert!(s.redo_steps().is_empty());
    assert_eq!(s.redo(&Origin::User).unwrap(), None);
    assert_eq!(int(&s, X), 9, "a redo never brings the old line back");
    let replaced = s.replaced_in(&APP);
    assert_eq!(replaced.len(), 1);
    assert_eq!(replaced[0].1.id, undone);
    assert_eq!(replaced[0].0, s.undo_steps()[0].id, "left after the first");
    // A jump to it goes back to where it left, then along it; the
    // agent's step becomes the replaced one.
    s.jump_in(&APP, undone, &Origin::User).unwrap();
    assert_eq!(int(&s, X), 2);
    let replaced = s.replaced_in(&APP);
    assert_eq!(replaced.len(), 1);
    assert_eq!(replaced[0].1.origin, agent());
}

#[test]
fn writes_from_another_origin_never_merge_into_a_run() {
    let mut s = store();
    let clock = ManualClock::new(10.0);
    s.set_clock(Some(clock.shared()));
    let bump = |s: &mut Store, v: i64, origin: Origin| {
        let by = origin.writer();
        s.set(to(X, v, origin).merge(X).by(&by)).unwrap();
    };
    bump(&mut s, 1, Origin::User);
    clock.advance(0.2);
    bump(&mut s, 2, Origin::User);
    assert_eq!(s.undo_steps().len(), 1, "the user's repeats merge");
    // The agent's write in the middle, with the same key, less than a
    // second later: its own step, and the user's run starts again.
    clock.advance(0.2);
    bump(&mut s, 5, agent());
    clock.advance(0.2);
    bump(&mut s, 3, Origin::User);
    let steps = s.undo_steps();
    assert_eq!(steps.len(), 3);
    assert_eq!(
        steps.iter().map(|st| st.origin.clone()).collect::<Vec<_>>(),
        [Origin::User, agent(), Origin::User]
    );
    // Each undo takes back one origin's part.
    for want in [5, 2, 0] {
        s.undo(&Origin::User).unwrap();
        assert_eq!(int(&s, X), want);
    }
}

#[test]
fn a_step_in_another_history_moves_that_history_alone() {
    let mut s = store();
    user(&mut s, X, 1);
    let app_rev = s.history_rev(&APP);
    {
        // The settings window (another history) changes another value.
        let _settings = enter_history(&SETTINGS);
        user(&mut s, Y, 2);
    }
    assert_eq!(s.history_rev(&APP), app_rev, "the application's is unmoved");
    assert_eq!(s.undo_steps().len(), 1);
    assert_eq!(s.undo_steps_in(&SETTINGS).len(), 1);
    // Undone there, the application's value stays; undone in the
    // application, the settings' value stays.
    s.undo_in(&SETTINGS, &Origin::User).unwrap();
    assert_eq!((int(&s, X), int(&s, Y)), (1, 0));
    assert_eq!(s.history_rev(&APP), app_rev);
    s.redo_in(&SETTINGS, &Origin::User).unwrap();
    s.undo(&Origin::User).unwrap();
    assert_eq!((int(&s, X), int(&s, Y)), (0, 2));
    assert_eq!(s.redo_steps_in(&SETTINGS).len(), 0);
    assert_eq!(s.undo_steps_in(&SETTINGS).len(), 1);
}

#[test]
fn an_owner_removed_takes_its_steps_and_undo_walks_the_rest() {
    let plugin = OwnerId::new("plugin").unwrap();
    let mut s = store();
    s.declare(&plugin, PLUGIN_A, Descriptor::int("A", 0, 99, 0))
        .unwrap();
    user(&mut s, X, 1);
    // One step mixing the plugin's value with the user's.
    let g = s.begin_group(agent(), "both");
    s.set(to(X, 2, agent()).gesture(g)).unwrap();
    s.set(to(PLUGIN_A, 1, agent()).gesture(g)).unwrap();
    s.end(g);
    user(&mut s, PLUGIN_A, 2);
    user(&mut s, X, 3);
    s.undo(&Origin::User).unwrap();
    assert_eq!(s.undo_steps().len(), 3);
    assert_eq!(s.redo_steps().len(), 1);
    let rev = s.history_rev(&APP);

    let removal = s.remove_owner_report(&plugin);
    assert_eq!(removal.paths, [PLUGIN_A]);
    assert_eq!(
        removal.dropped,
        [DroppedSteps {
            history: APP,
            steps: 2,
            mixed: 1,
        }]
    );
    assert!(s.history_rev(&APP) > rev, "the history moved");
    // The user's first step is left to undo, and the redo of the third.
    // The mixed step's change of `doc/x` (1 to 2) is in no history now:
    // like any change made outside, undo and redo keep it.
    assert_eq!(s.undo_steps().len(), 1);
    assert_eq!(s.redo_steps().len(), 1);
    assert_eq!(int(&s, X), 2);
    s.take_skipped();
    s.undo(&Origin::User).unwrap();
    assert_eq!(int(&s, X), 2, "kept, said skipped");
    assert_eq!(s.take_skipped().len(), 1);
    s.redo(&Origin::User).unwrap();
    s.redo(&Origin::User).unwrap();
    assert_eq!(int(&s, X), 3, "the third step starts from it");
    assert_eq!(s.redo(&Origin::User).unwrap(), None);
    // The plugin back: its value comes back as it was, and no undo
    // touches it.
    s.declare(&plugin, PLUGIN_A, Descriptor::int("A", 0, 99, 0))
        .unwrap();
    assert_eq!(int(&s, PLUGIN_A), 2);
    while s.undo(&Origin::User).unwrap().is_some() {}
    assert_eq!((int(&s, X), int(&s, PLUGIN_A)), (2, 2));
}

fn skipped(s: &mut Store) -> Vec<(ScopeId, String)> {
    s.take_skipped()
        .into_iter()
        .map(|k| (k.history, k.path))
        .collect()
}

#[test]
fn undo_and_redo_keep_a_value_changed_outside_the_history_since() {
    let mut s = store();
    user(&mut s, X, 1);
    user(&mut s, Y, 1);
    // A settings file read back changes `doc/x` (no step).
    let doc = SettingsDoc {
        values: [(X.to_string(), json!(5))].into(),
    };
    s.import(&doc, &Origin::Remote("file".into()));
    // Undo walks both steps: `doc/y` goes back, `doc/x` keeps the
    // file's value and is named as skipped.
    s.undo(&Origin::User).unwrap();
    s.undo(&Origin::User).unwrap();
    assert_eq!((int(&s, X), int(&s, Y)), (5, 0));
    assert_eq!(skipped(&mut s), [(APP, X.to_string())]);
    assert!(skipped(&mut s).is_empty(), "taken once");
    // Redo leaves it too: the outside change is never lost.
    s.redo(&Origin::User).unwrap();
    s.redo(&Origin::User).unwrap();
    assert_eq!((int(&s, X), int(&s, Y)), (5, 1));
    assert_eq!(skipped(&mut s), [(APP, X.to_string())]);
    // A jump to the start skips it the same way.
    s.jump_in(&APP, 0, &Origin::User).unwrap();
    assert_eq!((int(&s, X), int(&s, Y)), (5, 0));
    assert_eq!(skipped(&mut s), [(APP, X.to_string())]);
}

#[test]
fn a_write_in_another_history_is_kept_by_an_undo_here() {
    let mut s = store();
    user(&mut s, X, 1);
    {
        let _settings = enter_history(&SETTINGS);
        user(&mut s, X, 2);
    }
    s.undo(&Origin::User).unwrap();
    assert_eq!(int(&s, X), 2, "the settings' change stays");
    assert_eq!(skipped(&mut s), [(APP, X.to_string())]);
    // Undone in its own history, it goes back to what it found.
    s.undo_in(&SETTINGS, &Origin::User).unwrap();
    assert_eq!(int(&s, X), 1);
    assert!(skipped(&mut s).is_empty());
}

#[test]
fn a_group_begun_before_a_step_is_undone_after_it() {
    let mut s = store();
    // An agent's group writes `doc/x`; the user writes it meanwhile;
    // the group ends after the user's step.
    let g = s.begin_group(agent(), "agent edit");
    s.set(to(X, 1, agent()).gesture(g)).unwrap();
    user(&mut s, X, 2);
    s.end(g);
    let labels: Vec<_> = s.undo_steps().iter().map(|st| st.label.clone()).collect();
    assert_eq!(
        labels,
        ["agent edit", "set doc/x"],
        "ordered by first write"
    );
    // Undoing everything comes back to the start.
    s.undo(&Origin::User).unwrap();
    assert_eq!(int(&s, X), 1);
    s.undo(&Origin::User).unwrap();
    assert_eq!(int(&s, X), 0);
    assert!(skipped(&mut s).is_empty());
    s.redo(&Origin::User).unwrap();
    s.redo(&Origin::User).unwrap();
    assert_eq!(int(&s, X), 2);
}

#[test]
fn a_group_cancelled_keeps_a_change_made_outside_it_since() {
    let mut s = store();
    s.set(to(Y, 3, Origin::User).untracked()).unwrap();
    let g = s.begin_group(agent(), "agent edit");
    s.set(to(X, 1, agent()).gesture(g)).unwrap();
    s.set(to(Y, 4, agent()).gesture(g)).unwrap();
    // The user changes `doc/x` meanwhile (a step).
    user(&mut s, X, 2);
    s.cancel(g);
    // `doc/y` goes back; `doc/x` keeps the user's 2.
    assert_eq!((int(&s, X), int(&s, Y)), (2, 3));
    assert_eq!(skipped(&mut s), [(APP, X.to_string())]);
    // The user's step now starts from before the group: undoing it
    // comes back to the start, never to the group's 1.
    assert_eq!(
        s.undo_steps()[0].changes,
        [(X.to_string(), Value::Int(0), Value::Int(2))]
    );
    s.undo(&Origin::User).unwrap();
    assert_eq!(int(&s, X), 0);
}

#[test]
fn undoing_one_step_out_of_order_keeps_a_value_changed_outside_since() {
    let mut s = store();
    let g = s.begin_group(Origin::User, "both");
    s.set(to(X, 1, Origin::User).gesture(g)).unwrap();
    s.set(to(Y, 1, Origin::User).gesture(g)).unwrap();
    s.end(g);
    user(&mut s, PLACE, 1);
    s.set(to(X, 7, Origin::User).untracked()).unwrap();
    let first = s.undo_steps()[0].id;
    s.undo_step_in(&APP, first, &Origin::User, false).unwrap();
    assert_eq!((int(&s, X), int(&s, Y), int(&s, PLACE)), (7, 0, 1));
    assert_eq!(skipped(&mut s), [(APP, X.to_string())]);
    // A step whose every value changed outside since: refused, named.
    user(&mut s, X, 8);
    user(&mut s, PLACE, 2);
    s.set(to(X, 9, Origin::User).untracked()).unwrap();
    let id = s.undo_steps()[s.undo_steps().len() - 2].id;
    let r = s.undo_step_in(&APP, id, &Origin::User, false).unwrap_err();
    assert!(r.reason.contains("`doc/x`"), "{r:?}");
    assert_eq!(int(&s, X), 9);
}

#[test]
fn an_agent_may_not_undo_one_step_or_restore_what_it_may_not_write() {
    let mut s = store();
    let snap = s
        .take_snapshot(&APP, "start", &Origin::User, false)
        .unwrap();
    user(&mut s, Y, 4);
    user(&mut s, X, 1);
    let first = s.undo_steps()[0].id;
    for dry_run in [true, false] {
        let r = s.undo_step_in(&APP, first, &agent(), dry_run).unwrap_err();
        assert!(r.reason.contains("may not write `doc/y`"), "{r:?}");
        let r = s
            .restore_snapshot(&APP, snap.id, &agent(), dry_run)
            .unwrap_err();
        assert!(r.reason.contains("may not write"), "{r:?}");
    }
    assert_eq!((int(&s, X), int(&s, Y)), (1, 4));
    // The head step too, dry run or not.
    user(&mut s, Y, 5);
    let head = s.undo_steps().last().unwrap().id;
    for dry_run in [true, false] {
        let r = s.undo_step_in(&APP, head, &agent(), dry_run).unwrap_err();
        assert!(r.reason.contains("may not write `doc/y`"), "{r:?}");
    }
    assert_eq!(int(&s, Y), 5);
    // The user may.
    s.restore_snapshot(&APP, snap.id, &Origin::User, false)
        .unwrap();
    assert_eq!((int(&s, X), int(&s, Y)), (0, 0));
}
