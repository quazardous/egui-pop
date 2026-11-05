//! The `Timeline` widget: its transport keys, the pointer (scrub, range,
//! zoom), its markers, each change of its document a step of the history,
//! its accessible node, and agents reading it through the `timeline`
//! collector and driving it with the `timeline.*` commands.

use egui::accesskit::Role;
use egui::{Event, Key, Modifiers, PointerButton, Pos2, vec2};
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT as _, Queryable as _};
use egui_pop_api::{App, history};
use egui_pop_core::lock;
use egui_pop_model::commands::Origin;
use egui_pop_views::timeline::{
    Edit, Marker, MarkerKind, PlayState, TimeRange, TimeScale, Timeline, TimelinePlugin,
    TimelineState, Timelines,
};
use serde_json::{Value, json};

/// What a test keeps between passes.
struct Kept {
    tl: TimelineState,
    app: Option<App>,
    /// With an application: its history and the agents' plugin.
    with_app: bool,
    edits: Vec<Edit>,
}

fn song() -> TimelineState {
    let mut tl = TimelineState::new(TimeScale::frames(25), 60.0);
    tl.playhead = 10.0;
    tl.add_marker(Marker::new(0, 5.0, "Verse"));
    tl.add_marker(Marker::new(0, 20.0, "Chorus").kind(MarkerKind::Cue));
    tl
}

fn harness(tl: TimelineState, with_app: bool) -> Harness<'static, Kept> {
    let mut h = Harness::builder()
        .with_size(vec2(800.0, 160.0))
        .build_ui_state(
            |ui, k: &mut Kept| {
                if k.with_app {
                    let app = k.app.get_or_insert_with(|| {
                        App::builder()
                            .plugin(TimelinePlugin)
                            .build(ui.ctx())
                            .unwrap()
                    });
                    app.logic(ui.ctx());
                    let out = Timeline::new(&mut k.tl)
                        .id("song")
                        .app(app.handle())
                        .history("doc/song")
                        .show(ui);
                    k.edits.extend(out.edits);
                } else {
                    let out = Timeline::new(&mut k.tl).id("song").show(ui);
                    k.edits.extend(out.edits);
                }
            },
            Kept {
                tl,
                app: None,
                with_app,
                edits: Vec::new(),
            },
        );
    h.run();
    h
}

fn focus(h: &mut Harness<'_, Kept>) {
    h.get_by_role_and_label(Role::Slider, "Timeline").focus();
    h.run();
}

fn press(h: &mut Harness<'_, Kept>, modifiers: Modifiers, key: Key) {
    h.key_press_modifiers(modifiers, key);
    h.run();
}

#[test]
fn space_and_j_k_l_ask_to_play_and_shuttle() {
    let mut h = harness(song(), false);
    focus(&mut h);
    let none = Modifiers::NONE;
    press(&mut h, none, Key::Space);
    assert_eq!(
        (h.state().tl.play, h.state().tl.speed),
        (PlayState::Playing, 1.0)
    );
    press(&mut h, none, Key::L);
    press(&mut h, none, Key::L);
    assert_eq!(h.state().tl.speed, 4.0);
    press(&mut h, none, Key::J);
    assert_eq!(h.state().tl.speed, -1.0);
    press(&mut h, none, Key::K);
    assert_eq!(
        (h.state().tl.play, h.state().tl.speed),
        (PlayState::Paused, 1.0)
    );
    press(&mut h, none, Key::Space);
    press(&mut h, none, Key::Space);
    assert_eq!(h.state().tl.play, PlayState::Paused);
    // The widget asks; nothing moved the time.
    assert_eq!(h.state().tl.playhead, 10.0);
}

#[test]
fn the_arrows_home_and_end_step_through_time() {
    let mut h = harness(song(), false);
    focus(&mut h);
    let (none, shift) = (Modifiers::NONE, Modifiers::SHIFT);
    press(&mut h, none, Key::ArrowRight);
    assert_eq!(h.state().tl.playhead, 10.04, "one frame");
    press(&mut h, shift, Key::ArrowRight);
    assert_eq!(h.state().tl.playhead, 11.04, "a timecode second");
    press(&mut h, none, Key::ArrowLeft);
    assert_eq!(h.state().tl.playhead, 11.0);
    press(&mut h, none, Key::End);
    assert_eq!(h.state().tl.playhead, 60.0);
    press(&mut h, none, Key::ArrowRight);
    assert_eq!(h.state().tl.playhead, 60.0, "kept inside the bounds");
    press(&mut h, none, Key::Home);
    assert_eq!(h.state().tl.playhead, 0.0);
    // K held, L steps one frame and does not play.
    h.key_down(Key::K);
    h.run();
    press(&mut h, none, Key::L);
    h.key_up(Key::K);
    h.run();
    assert_eq!(h.state().tl.playhead, 0.04);
    assert_eq!(h.state().tl.play, PlayState::Paused);
    // Stepping while playing pauses.
    press(&mut h, none, Key::Space);
    press(&mut h, none, Key::ArrowRight);
    assert_eq!(h.state().tl.play, PlayState::Paused);
}

#[test]
fn i_and_o_set_a_range_that_loops_and_markers_follow_the_keys() {
    let mut h = harness(song(), false);
    focus(&mut h);
    let (none, shift) = (Modifiers::NONE, Modifiers::SHIFT);
    press(&mut h, none, Key::I);
    for _ in 0..2 {
        press(&mut h, shift, Key::ArrowRight);
    }
    press(&mut h, none, Key::O);
    assert_eq!(h.state().tl.range, Some(TimeRange::new(10.0, 12.0)));
    press(&mut h, Modifiers::COMMAND, Key::L);
    assert!(h.state().tl.looping);
    press(&mut h, shift, Key::I);
    assert_eq!(h.state().tl.playhead, 10.0);
    press(&mut h, Modifiers::ALT, Key::X);
    assert_eq!(h.state().tl.range, None);

    // M adds a marker at the playhead, selected.
    press(&mut h, none, Key::M);
    let added = h.state().tl.selected.unwrap();
    assert_eq!(h.state().tl.marker(added).unwrap().label, "Marker 3");
    // Up and Down walk them, moving the playhead there.
    press(&mut h, none, Key::ArrowDown);
    assert_eq!(h.state().tl.playhead, 20.0);
    press(&mut h, none, Key::ArrowUp);
    press(&mut h, none, Key::ArrowUp);
    assert_eq!(h.state().tl.playhead, 5.0);
    let verse = h.state().tl.selected.unwrap();
    // Ctrl+Right moves the selected one a frame, Ctrl+Shift+Right a second.
    press(&mut h, Modifiers::COMMAND, Key::ArrowRight);
    press(
        &mut h,
        Modifiers::COMMAND | Modifiers::SHIFT,
        Key::ArrowRight,
    );
    assert_eq!(h.state().tl.marker(verse).unwrap().time, 6.04);
    press(&mut h, none, Key::Delete);
    assert!(h.state().tl.marker(verse).is_none());
    assert_eq!(h.state().tl.markers.len(), 2);
    let kinds: Vec<&str> = h
        .state()
        .edits
        .iter()
        .map(|e| match e {
            Edit::RangeSet { to: Some(_), .. } => "range",
            Edit::RangeSet { to: None, .. } => "clear",
            Edit::LoopSet(_) => "loop",
            Edit::MarkerAdded(_) => "add",
            Edit::MarkerMoved { .. } => "move",
            Edit::MarkerRemoved(_) => "remove",
            Edit::Restored => "restored",
        })
        .collect();
    assert_eq!(
        kinds,
        [
            "range", "range", "loop", "clear", "add", "move", "move", "remove"
        ]
    );
}

#[test]
fn the_pointer_scrubs_selects_a_range_and_zooms() {
    let mut tl = TimelineState::new(TimeScale::Seconds, 100.0);
    tl.markers.clear();
    let mut h = harness(tl, false);
    let rect = h.get_by_role_and_label(Role::Slider, "Timeline").rect();
    let x_of = |t: f64| rect.left() + rect.width() * (t / 100.0) as f32;
    let y = rect.top() + 4.0;
    // A press moves the playhead (snapped to the grid), a drag scrubs.
    h.drag_at(Pos2::new(x_of(30.2), y));
    h.run();
    assert_eq!(h.state().tl.playhead.round(), 30.0);
    h.hover_at(Pos2::new(x_of(45.0), y));
    h.run();
    h.hover_at(Pos2::new(x_of(50.1), y));
    h.run();
    h.drop_at(Pos2::new(x_of(50.1), y));
    h.run();
    assert_eq!(h.state().tl.playhead.round(), 50.0);
    assert!(
        h.get_by_role_and_label(Role::Slider, "Timeline")
            .is_focused()
    );
    // Shift+drag: a range, the playhead left where it was.
    let shift = Modifiers::SHIFT;
    h.event_modifiers(
        Event::PointerButton {
            pos: Pos2::new(x_of(10.0), y),
            button: PointerButton::Primary,
            pressed: true,
            modifiers: shift,
        },
        shift,
    );
    h.run();
    for t in [15.0, 20.0] {
        h.event_modifiers(Event::PointerMoved(Pos2::new(x_of(t), y)), shift);
        h.run();
    }
    h.event_modifiers(
        Event::PointerButton {
            pos: Pos2::new(x_of(20.0), y),
            button: PointerButton::Primary,
            pressed: false,
            modifiers: shift,
        },
        shift,
    );
    h.run();
    let r = h.state().tl.range.expect("a range");
    assert_eq!((r.start.round(), r.end.round()), (10.0, 20.0));
    assert_eq!(h.state().tl.playhead.round(), 50.0);
    // Ctrl+wheel zooms in around the pointer.
    let before = h.state().tl.view;
    h.hover_at(Pos2::new(x_of(50.0), y));
    h.run();
    h.event_modifiers(
        Event::MouseWheel {
            unit: egui::MouseWheelUnit::Line,
            delta: vec2(0.0, 3.0),
            modifiers: Modifiers::COMMAND,
            phase: egui::TouchPhase::Move,
            source: egui::MouseWheelSource::Wheel,
        },
        Modifiers::COMMAND,
    );
    h.run();
    let after = h.state().tl.view;
    assert!(after.duration < before.duration, "{before:?} -> {after:?}");
    assert!(after.contains(50.0));
}

#[test]
fn each_change_of_the_document_is_one_undo_step() {
    let mut h = harness(song(), true);
    focus(&mut h);
    let none = Modifiers::NONE;
    let app = h.state().app.as_ref().unwrap().handle().clone();
    let steps = |app: &egui_pop_api::AppHandle| -> Vec<String> {
        lock(&app.store())
            .undo_steps()
            .iter()
            .map(|s| s.label.clone())
            .collect()
    };
    // The playhead and playing are not steps.
    press(&mut h, none, Key::ArrowRight);
    press(&mut h, none, Key::Space);
    press(&mut h, none, Key::K);
    assert!(steps(&app).is_empty(), "{:?}", steps(&app));
    press(&mut h, none, Key::M);
    press(&mut h, none, Key::I);
    press(&mut h, Modifiers::COMMAND, Key::L);
    assert_eq!(steps(&app), ["Add marker", "Set range", "Loop the range"]);
    assert_eq!(h.state().tl.markers.len(), 3);

    // Undo puts the document back, one step at a time.
    for _ in 0..2 {
        history::undo(&app, &Origin::User);
        h.run();
    }
    assert_eq!(h.state().tl.range, None);
    assert!(!h.state().tl.looping);
    assert_eq!(h.state().tl.markers.len(), 3);
    history::undo(&app, &Origin::User);
    h.run();
    assert_eq!(h.state().tl.markers.len(), 2);
    assert!(h.state().edits.contains(&Edit::Restored));
    history::redo(&app, &Origin::User);
    h.run();
    assert_eq!(h.state().tl.markers.len(), 3);

    // A marker dragged is one step, from where it was to where it lands.
    let verse = h.state().tl.markers[0].clone();
    let flag = h
        .get_by_role_and_label(Role::Button, "Marker Verse at 00:00:05:00")
        .rect();
    let rect = h.get_by_role_and_label(Role::Slider, "Timeline").rect();
    let x_of = |t: f64| rect.left() + rect.width() * (t / 60.0) as f32;
    let at = |x: f32| Pos2::new(x, flag.center().y);
    h.drag_at(at(flag.left() + 5.0));
    h.run();
    for t in [7.0, 8.0] {
        h.hover_at(at(x_of(t)));
        h.run();
    }
    h.drop_at(at(x_of(8.0)));
    h.run();
    let moved = h.state().tl.marker(verse.id).unwrap().time;
    assert!((moved - 8.0).abs() < 0.5, "{moved}");
    assert_eq!(steps(&app).last().map(String::as_str), Some("Move marker"));
    history::undo(&app, &Origin::User);
    h.run();
    assert_eq!(h.state().tl.marker(verse.id).unwrap().time, 5.0);
}

#[test]
fn the_timeline_is_a_slider_over_the_time_with_named_markers() {
    let mut h = harness(song(), false);
    let node = h.get_by_role_and_label(Role::Slider, "Timeline");
    assert_eq!(node.value().as_deref(), Some("00:00:10:00"));
    assert_eq!(node.accesskit_node().numeric_value(), Some(10.0));
    assert_eq!(node.accesskit_node().max_numeric_value(), Some(60.0));
    node.increment_accesskit();
    h.run();
    assert_eq!(h.state().tl.playhead, 10.04);
    h.get_by_role_and_label(Role::Button, "Marker Verse at 00:00:05:00");
    h.get_by_role_and_label(Role::Button, "Cue Chorus at 00:00:20:00");
    egui_pop_test::assert_all_named(&h);
}

#[test]
fn agents_read_timelines_and_drive_them_with_commands() {
    let mut h = harness(song(), true);
    let app = h.state().app.as_ref().unwrap().handle().clone();
    let snap = app.debug_snapshot();
    let song = &snap["timeline"]["timelines"]["song"];
    assert_eq!(song["playhead"]["text"], "00:00:10:00", "{snap}");
    assert_eq!(song["scale"]["kind"], "frames");
    assert_eq!(song["markers"][1]["label"], "Chorus");
    assert_eq!(song["markers"][1]["kind"], "cue");

    let run = |h: &mut Harness<'_, Kept>, id: &str, args: Value| {
        let f = app.debug_call("commands", "run", &json!({ "id": id, "args": args }));
        assert!(f.ok, "{id}: {f:?}");
        h.run();
    };
    run(
        &mut h,
        "timeline.add_marker",
        json!({ "timeline": "song", "seconds": 30.0, "label": "Drop", "kind": "cue", "color": "#ff8800" }),
    );
    run(
        &mut h,
        "timeline.set_time",
        json!({ "time": "00:00:12:00" }),
    );
    run(
        &mut h,
        "timeline.set_range",
        json!({ "start": 4.0, "end": 8.0, "loop": true }),
    );
    run(&mut h, "timeline.play", json!({ "speed": -2.0 }));
    let tl = &h.state().tl;
    let cue = tl.markers.iter().find(|m| m.label == "Drop").unwrap();
    assert_eq!((cue.time, cue.kind), (30.0, MarkerKind::Cue));
    assert_eq!(cue.color, Some(egui::Color32::from_rgb(255, 136, 0)));
    assert_eq!(tl.playhead, 12.0);
    assert_eq!(
        (tl.range, tl.looping),
        (Some(TimeRange::new(4.0, 8.0)), true)
    );
    assert_eq!((tl.play, tl.speed), (PlayState::Playing, -2.0));
    let dropped = cue.id;
    run(&mut h, "timeline.pause", json!({}));
    run(&mut h, "timeline.remove_marker", json!({ "id": dropped }));
    assert_eq!(h.state().tl.play, PlayState::Paused);
    assert!(h.state().tl.marker(dropped).is_none());
    // The agent's changes are steps of the history, named as the agent's.
    let store = app.store();
    let origins: Vec<bool> = lock(&store)
        .undo_steps()
        .iter()
        .map(|s| matches!(s.origin, Origin::Agent(_)))
        .collect();
    assert_eq!(origins, [true, true, true, true]);
    drop(store);
    let snap = app.debug_snapshot();
    let song = &snap["timeline"]["timelines"]["song"];
    assert_eq!(song["range"]["text"], json!(["00:00:04:00", "00:00:08:00"]));
    assert_eq!(song["play"], "paused");
    // A timeline nobody drew: refused, and said so.
    assert!(Timelines::of(&h.ctx).get(Some("nope")).is_err());
}
