//! The value store: ranges, steps, gestures, guards, undo, listeners,
//! ownership, settings and the commands over it.

use std::sync::{Arc, Mutex};

use egui_pop_model::commands::{Commands, Origin};
use egui_pop_model::ids::{CommandId, OwnerId};
use egui_pop_model::values::*;
use egui_pop_model::when::WhenContext;
use serde_json::json;

const GAIN: &str = "mixer/gain";
const FW: OwnerId = OwnerId::FRAMEWORK;

fn agent() -> Origin {
    Origin::Agent("a1".into())
}

/// A mixer's values: a gain in dB with a soft range, a mute, a limit.
fn mixer() -> Store {
    let mut store = Store::new();
    store
        .declare(
            &FW,
            GAIN,
            Descriptor::float("Gain", -60.0, 12.0, 0.0)
                .soft(-24.0, 6.0)
                .step(0.1)
                .coarse(1.0)
                .unit("dB"),
        )
        .unwrap();
    store
        .declare(&FW, "mixer/mute", Descriptor::bool("Mute", false))
        .unwrap();
    store
        .declare(
            &FW,
            "mixer/limit",
            Descriptor::float("Limit", 0.0, 1.0, 0.0),
        )
        .unwrap();
    store
}

fn gain(store: &Store) -> f64 {
    store.get(GAIN).unwrap().value.as_f64().unwrap()
}

fn to(v: f64) -> Change {
    Change::To(Value::Float(v))
}

#[test]
fn descriptors_say_what_a_value_is() {
    let store = mixer();
    let d = &store.get(GAIN).unwrap().descriptor;
    assert_eq!(d.ty, Type::Float);
    assert_eq!(d.hard, Some((-60.0, 12.0)));
    assert_eq!(d.soft, Some((-24.0, 6.0)));
    assert_eq!((d.step, d.coarse), (0.1, 1.0));
    assert_eq!(d.unit.as_deref(), Some("dB"));
    assert_eq!(d.default, Value::Float(0.0));
    let paths: Vec<&str> = store.list("mixer/").iter().map(|(p, _)| *p).collect();
    assert_eq!(paths, ["mixer/gain", "mixer/limit", "mixer/mute"]);
    let ordered: Vec<&str> = store
        .list_in_order("mixer/")
        .iter()
        .map(|(p, _)| *p)
        .collect();
    assert_eq!(ordered, ["mixer/gain", "mixer/mute", "mixer/limit"]);
}

#[test]
fn declaring_checks_the_path_the_default_and_the_owner() {
    let mut store = mixer();
    assert!(matches!(
        store.declare(&FW, "a//b", Descriptor::bool("x", false)),
        Err(DeclareError::BadPath { .. })
    ));
    assert!(matches!(
        store.declare(&FW, "a/b", Descriptor::float("x", 0.0, 1.0, 5.0)),
        Err(DeclareError::BadDefault { .. })
    ));
    assert!(matches!(
        store.declare(&FW, "a/c", Descriptor::choice("x", &["p", "q"], "r")),
        Err(DeclareError::BadDefault { .. })
    ));
    let other = OwnerId::from_static("other");
    let e = store
        .declare(&other, GAIN, Descriptor::bool("x", false))
        .unwrap_err();
    assert_eq!(
        e.to_string(),
        "`mixer/gain` is already declared by `framework`"
    );
    // The owner may declare again: the value is kept when it fits.
    store.set(Set::new(GAIN, to(3.0), Origin::User)).unwrap();
    store
        .declare(&FW, GAIN, Descriptor::float("Gain", -60.0, 12.0, 0.0))
        .unwrap();
    assert_eq!(gain(&store), 3.0);
}

#[test]
fn writes_are_held_to_the_soft_range_unless_forced_and_say_so() {
    let mut store = mixer();
    let a = store.set(Set::new(GAIN, to(9.0), Origin::User)).unwrap();
    assert_eq!(
        (a.value.clone(), a.clamped),
        (Value::Float(6.0), Clamped::Soft)
    );
    let a = store
        .set(Set::new(GAIN, to(9.0), Origin::User).force())
        .unwrap();
    assert_eq!((a.value, a.clamped), (Value::Float(9.0), Clamped::None));
    let a = store
        .set(Set::new(GAIN, to(40.0), Origin::User).force())
        .unwrap();
    assert_eq!((a.value, a.clamped), (Value::Float(12.0), Clamped::Hard));
    // Top and bottom go to the soft ends, or the hard ones when forced.
    store
        .set(Set::new(GAIN, Change::Bottom, Origin::User))
        .unwrap();
    assert_eq!(gain(&store), -24.0);
    store
        .set(Set::new(GAIN, Change::Top, Origin::User).force())
        .unwrap();
    assert_eq!(gain(&store), 12.0);
    // A wrong type is refused, with the value it stays at.
    let r = store
        .set(Set::new(GAIN, Change::To(Value::Bool(true)), Origin::User))
        .unwrap_err();
    assert!(
        matches!(r, Refused::Invalid { current: Value::Float(v), .. } if v == 12.0),
        "{r:?}"
    );
    assert!(r.to_string().contains("the value stays"));
    assert_eq!(
        store.set(Set::new("nope", Change::Reset, Origin::User)),
        Err(Refused::Unknown {
            path: "nope".into()
        })
    );
}

#[test]
fn steps_move_by_fine_or_coarse_amounts() {
    let mut store = mixer();
    store
        .set(Set::new(
            GAIN,
            Change::Step(5),
            Origin::Device("fader".into()),
        ))
        .unwrap();
    assert!((gain(&store) - 0.5).abs() < 1e-12, "{}", gain(&store));
    store
        .set(Set::new(
            GAIN,
            Change::Coarse(-2),
            Origin::Device("fader".into()),
        ))
        .unwrap();
    assert!((gain(&store) + 1.5).abs() < 1e-12, "{}", gain(&store));
    store
        .set(Set::new(GAIN, Change::Reset, Origin::User))
        .unwrap();
    assert_eq!(gain(&store), 0.0);
    // A bool steps by toggling.
    store
        .set(Set::new("mixer/mute", Change::Step(1), Origin::User))
        .unwrap();
    assert_eq!(store.get("mixer/mute").unwrap().value, Value::Bool(true));
    // A choice steps through its choices, held at the ends.
    store
        .declare(
            &FW,
            "mixer/mode",
            Descriptor::choice("Mode", &["mono", "stereo", "surround"], "mono"),
        )
        .unwrap();
    let a = store
        .set(Set::new("mixer/mode", Change::Step(5), Origin::User))
        .unwrap();
    assert_eq!(
        (a.value, a.clamped),
        (Value::Choice("surround".into()), Clamped::Hard)
    );
}

#[test]
fn an_agent_cannot_move_what_a_hand_is_dragging() {
    let mut store = mixer();
    let drag = store.begin_gesture(Origin::User, "drag gain");
    for v in [0.5, 0.8, 1.0] {
        store
            .set(Set::new(GAIN, to(v), Origin::User).gesture(drag))
            .unwrap();
    }
    assert_eq!(store.held_by(GAIN), Some(&Origin::User));
    let r = store.set(Set::new(GAIN, to(-1.0), agent())).unwrap_err();
    assert_eq!(
        r,
        Refused::Held {
            current: Value::Float(1.0),
            by: Origin::User
        }
    );
    // Other values stay free.
    store
        .set(Set::new("mixer/limit", to(0.5), agent()))
        .unwrap();
    assert!(store.end(drag));
    assert!(!store.end(drag), "ending twice is reported");
    store.set(Set::new(GAIN, to(-1.0), agent())).unwrap();
    assert_eq!(gain(&store), -1.0);
}

#[test]
fn a_writer_can_ask_that_nothing_changed_since_it_read() {
    let mut store = mixer();
    let read = store.get(GAIN).unwrap().seq;
    store.set(Set::new(GAIN, to(1.0), Origin::User)).unwrap();
    let r = store
        .set(Set::new(GAIN, to(2.0), agent()).if_seq(read))
        .unwrap_err();
    assert!(
        matches!(r, Refused::Stale { current: Value::Float(v), .. } if v == 1.0),
        "{r:?}"
    );
    let now = store.get(GAIN).unwrap().seq;
    let a = store
        .set(Set::new(GAIN, to(2.0), agent()).if_seq(now))
        .unwrap();
    assert!(a.changed && a.seq > now);
}

#[test]
fn a_gesture_or_a_group_is_one_undo_step_and_a_write_another() {
    let mut store = mixer();
    let drag = store.begin_gesture(Origin::User, "drag gain");
    for v in [0.5, 0.8, 1.0] {
        store
            .set(Set::new(GAIN, to(v), Origin::User).gesture(drag))
            .unwrap();
    }
    store.end(drag);
    let group = store.begin_group(agent(), "louder and limited");
    store
        .set(Set::new(GAIN, Change::Step(10), agent()).gesture(group))
        .unwrap();
    store
        .set(Set::new("mixer/limit", to(0.2), agent()).gesture(group))
        .unwrap();
    store.end(group);
    store.set(Set::new(GAIN, to(3.0), Origin::User)).unwrap();
    let labels: Vec<&str> = store
        .undo_steps()
        .iter()
        .map(|s| s.label.as_str())
        .collect();
    assert_eq!(
        labels,
        ["drag gain", "louder and limited", "set mixer/gain"]
    );
    store.undo(&Origin::User).unwrap();
    store.undo(&Origin::User).unwrap();
    assert_eq!(gain(&store), 1.0);
    assert_eq!(store.get("mixer/limit").unwrap().value, Value::Float(0.0));
    // Undo is a write: it carries who asked for it.
    assert_eq!(store.get(GAIN).unwrap().origin, Some(Origin::User));
    assert_eq!(
        store.undo(&Origin::User).unwrap().as_deref(),
        Some("drag gain")
    );
    assert_eq!(gain(&store), 0.0);
    store.redo(&Origin::User).unwrap();
    assert_eq!(gain(&store), 1.0);
}

#[test]
fn every_change_is_told_once_and_no_change_is_not() {
    let mut store = mixer();
    let seen = Arc::new(Mutex::new(Vec::new()));
    let s = seen.clone();
    let listener = store.listen(&FW, "mixer/", move |c| s.lock().unwrap().push(c.clone()));
    let start = store.seq();
    store.set(Set::new(GAIN, to(1.0), agent())).unwrap();
    let a = store.set(Set::new(GAIN, to(1.0), Origin::User)).unwrap();
    assert!(!a.changed);
    let events = seen.lock().unwrap().clone();
    assert_eq!(events.len(), 1, "{events:?}");
    assert_eq!(
        serde_json::to_value(&events[0]).unwrap(),
        json!({
            "path": GAIN,
            "old": { "float": 0.0 },
            "new": { "float": 1.0 },
            "origin": { "kind": "agent", "id": "a1" },
            "seq": a.seq,
        })
    );
    assert_eq!(store.changes_since(start).len(), 1);
    assert!(store.unlisten(listener));
    store.set(Set::new(GAIN, to(2.0), agent())).unwrap();
    assert_eq!(seen.lock().unwrap().len(), 1);
}

#[test]
fn an_owner_takes_its_paths_and_listeners_away_and_gets_its_values_back() {
    let mut store = mixer();
    let plugin = OwnerId::from_static("plugin.eq");
    store
        .declare(
            &plugin,
            "settings/eq/bass",
            Descriptor::float("Bass", -12.0, 12.0, 0.0),
        )
        .unwrap();
    store.listen(&plugin, "settings/eq/", |_| {});
    store
        .set(Set::new("settings/eq/bass", to(4.0), Origin::User))
        .unwrap();
    assert_eq!(store.remove_owner(&plugin), ["settings/eq/bass"]);
    assert!(store.get("settings/eq/bass").is_none());
    assert!(store.listeners().is_empty());
    assert!(store.undo_steps().is_empty(), "its undo steps went with it");
    assert_eq!(
        store.export("settings/").values["settings/eq/bass"],
        json!(4.0)
    );
    // Declared again (the plugin is back): the value comes back.
    store
        .declare(
            &plugin,
            "settings/eq/bass",
            Descriptor::float("Bass", -12.0, 12.0, 0.0),
        )
        .unwrap();
    assert_eq!(
        store.get("settings/eq/bass").unwrap().value,
        Value::Float(4.0)
    );
}

#[test]
fn settings_are_saved_as_their_differences_and_read_back() {
    let mut store = Store::new();
    store
        .declare(
            &FW,
            "settings/theme",
            Descriptor::choice("Theme", &["dark", "light"], "dark"),
        )
        .unwrap();
    store
        .declare(&FW, "settings/size", Descriptor::int("Size", 8, 32, 12))
        .unwrap();
    store
        .set(Set::new(
            "settings/theme",
            Change::To(Value::Choice("light".into())),
            Origin::User,
        ))
        .unwrap();
    let doc = store.export("settings/");
    assert_eq!(doc.values.len(), 1);
    let text = settings_schema().save(&doc).unwrap();
    let back: SettingsDoc = settings_schema().load(&text).unwrap().value;
    let mut fresh = Store::new();
    fresh
        .declare(
            &FW,
            "settings/theme",
            Descriptor::choice("Theme", &["dark", "light"], "dark"),
        )
        .unwrap();
    fresh
        .declare(&FW, "settings/size", Descriptor::int("Size", 8, 32, 12))
        .unwrap();
    let mut bad = back.clone();
    bad.values.insert("settings/size".into(), json!("huge"));
    bad.values.insert("settings/later".into(), json!(true));
    let d = fresh.import(&bad, &Origin::User);
    assert_eq!(d.codes(), ["bad-setting"]);
    assert_eq!(
        fresh.get("settings/theme").unwrap().value,
        Value::Choice("light".into())
    );
    assert_eq!(fresh.get("settings/size").unwrap().value, Value::Int(12));
    assert_eq!(fresh.pending().len(), 1, "kept for a path declared later");
    fresh
        .declare(&FW, "settings/later", Descriptor::bool("Later", false))
        .unwrap();
    assert_eq!(
        fresh.get("settings/later").unwrap().value,
        Value::Bool(true)
    );
}

#[test]
fn the_commands_drive_the_store_with_their_origin() {
    let store = Arc::new(Mutex::new(mixer()));
    let mut r = Commands::new();
    register(&mut r, &FW, store.clone()).unwrap();
    let cx = WhenContext::new();
    let id = CommandId::from_static;
    let a = r
        .run(
            &id("value.set"),
            json!({ "path": GAIN, "value": 9 }),
            agent(),
            &cx,
        )
        .unwrap();
    assert_eq!(a["value"], json!(6.0));
    assert_eq!(a["clamped"], json!("soft"));
    let got = r
        .run(&id("value.get"), json!({ "path": GAIN }), agent(), &cx)
        .unwrap();
    assert_eq!(got["origin"], json!({ "kind": "agent", "id": "a1" }));
    assert_eq!(got["changed"], json!(true));
    assert_eq!(got["descriptor"]["unit"], json!("dB"));
    let stale = r
        .run(
            &id("value.set"),
            json!({ "path": GAIN, "step": 1, "if_seq": 0 }),
            agent(),
            &cx,
        )
        .unwrap_err();
    assert!(stale.contains("\"refused\":\"stale\""), "{stale}");
    r.run(
        &id("value.reset"),
        json!({ "path": GAIN }),
        Origin::User,
        &cx,
    )
    .unwrap();
    assert_eq!(gain(&store.lock().unwrap()), 0.0);
    let list = r
        .run(
            &id("value.list"),
            json!({ "prefix": "mixer/" }),
            agent(),
            &cx,
        )
        .unwrap();
    assert_eq!(list.as_array().unwrap().len(), 3);
    let undone = r
        .run(&id("value.undo"), json!(null), Origin::User, &cx)
        .unwrap();
    assert_eq!(undone["undone"], json!("set mixer/gain"));
    assert_eq!(gain(&store.lock().unwrap()), 6.0);
}

#[test]
fn a_path_hint_says_what_a_text_names() {
    let d = Descriptor::path("config", PathKind::File, &["JSON"]);
    assert_eq!(d.ty, Type::Text);
    assert_eq!(d.default, Value::Text(String::new()));
    let hint = d.path.as_ref().unwrap();
    assert!(hint.accepts(std::path::Path::new("a/b.json")));
    assert!(hint.accepts(std::path::Path::new("b.JSON")));
    assert!(!hint.accepts(std::path::Path::new("b.txt")));
    let j = serde_json::to_value(&d).unwrap();
    assert_eq!(j["path"], json!({ "kind": "file", "extensions": ["json"] }));
    let dir = Descriptor::path("workspace", PathKind::Dir, &[]);
    assert_eq!(
        serde_json::to_value(&dir).unwrap()["path"],
        json!({ "kind": "dir" })
    );
    let text = serde_json::to_value(Descriptor::text("name", "")).unwrap();
    assert!(text.get("path").is_none());
}

#[test]
fn an_undo_never_writes_into_a_drag_still_going() {
    let mut store = mixer();
    store.set(Set::new(GAIN, to(1.0), Origin::User)).unwrap();
    let drag = store.begin_gesture(Origin::User, "drag gain");
    store
        .set(Set::new(GAIN, to(2.0), Origin::User).gesture(drag))
        .unwrap();
    // The user's own Ctrl+Z, in the middle of the user's own drag.
    let r = store.undo(&Origin::User).unwrap_err();
    assert_eq!(
        r,
        Refused::Held {
            current: Value::Float(2.0),
            by: Origin::User
        }
    );
    assert_eq!(gain(&store), 2.0, "the drag keeps its value");
    assert_eq!(store.undo_steps().len(), 1, "the step is still there");
    // Once the drag ends, its step is the one undone.
    assert!(store.end(drag));
    assert_eq!(
        store.undo(&Origin::User).unwrap().as_deref(),
        Some("drag gain")
    );
    assert_eq!(gain(&store), 1.0);
    // A redo waits for a new drag on the same value too.
    let drag = store.begin_gesture(Origin::User, "drag gain");
    store
        .set(Set::new(GAIN, to(0.5), Origin::User).gesture(drag))
        .unwrap();
    assert!(matches!(
        store.redo(&Origin::User),
        Err(Refused::Held { .. })
    ));
    assert_eq!(gain(&store), 0.5);
    assert!(store.cancel(drag));
    assert_eq!(
        store.redo(&Origin::User).unwrap().as_deref(),
        Some("drag gain")
    );
    assert_eq!(gain(&store), 2.0);
}

#[test]
fn the_undo_history_keeps_its_newest_five_hundred_steps_or_the_hosts_cap() {
    let mut store = mixer();
    for i in 0..510 {
        store
            .set(Set::new(GAIN, to(f64::from(i) / 100.0), Origin::User))
            .unwrap();
    }
    let steps = store.undo_steps();
    assert_eq!(steps.len(), 500, "the oldest nine dropped");
    // The oldest kept is the eleventh write: from 0.09 to 0.10.
    assert_eq!(steps[0].changes[0].1, Value::Float(0.09));
    assert_eq!(steps[499].changes[0].2, Value::Float(5.09));
    assert_eq!(store.history_cap(), HISTORY_CAP);
    assert_eq!(store.dropped_steps(), (9, 0));
    // A host keeps fewer: the oldest go now, the redo list as well.
    store.undo(&Origin::User).unwrap();
    store.set_history_cap(3);
    assert_eq!(store.history_cap(), 3);
    assert_eq!(store.undo_steps().len(), 3);
    assert_eq!(store.redo_steps().len(), 1);
    assert_eq!(store.dropped_steps(), (505, 0));
    assert_eq!(store.undo_steps()[2].changes[0].2, Value::Float(5.08));
    for _ in 0..3 {
        assert!(store.undo(&Origin::User).unwrap().is_some());
    }
    assert_eq!(store.undo(&Origin::User).unwrap(), None);
    assert_eq!(gain(&store), 5.05);
    // And new steps keep to it.
    for v in [1.0, 2.0, 3.0, 4.0] {
        store.set(Set::new(GAIN, to(v), Origin::User)).unwrap();
    }
    assert_eq!(store.undo_steps().len(), 3);
    assert!(store.redo_steps().is_empty());
}
