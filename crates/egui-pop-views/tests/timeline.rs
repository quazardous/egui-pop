//! The `Timeline` widget: its transport keys, the pointer (scrub, range,
//! zoom), its markers, each change of its document a step of the history,
//! its accessible node, its tracks and clips (selected, moved, trimmed), and
//! agents reading it through the `timeline` collector and driving it with
//! the `timeline.*` commands.

use egui::accesskit::Role;
use egui::{Event, Key, Modifiers, PointerButton, Pos2, vec2};
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT as _, Queryable as _};
use egui_pop_api::{App, history};
use egui_pop_core::lock;
use egui_pop_model::commands::Origin;
use egui_pop_views::timeline::{
    Clip, ClipPaint, Edit, Follow, Marker, MarkerKind, NUDGE_GAIN, PlayState, TimeRange, TimeScale,
    Timeline, TimelinePlugin, TimelineState, Timelines, Track, Transport,
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
            _ => "clip",
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

/// A minute at 25 fps: V1 holds "One" (0..10 s) and "Two" (20..30 s), A1
/// "Sound" (5..15 s).
fn edit() -> TimelineState {
    let mut tl = TimelineState::new(TimeScale::frames(25), 60.0);
    tl.playhead = 12.0;
    let v1 = tl.add_track(Track::new(0, "V1"));
    let a1 = tl.add_track(Track::new(0, "A1"));
    tl.add_clip(v1, Clip::new(0, 0.0, 10.0, "One").source_length(12.0))
        .unwrap();
    tl.add_clip(v1, Clip::new(0, 20.0, 10.0, "Two").offset(2.0))
        .unwrap();
    tl.add_clip(a1, Clip::new(0, 5.0, 10.0, "Sound")).unwrap();
    tl
}

fn undo_steps(app: &egui_pop_api::AppHandle) -> Vec<String> {
    lock(&app.store())
        .undo_steps()
        .iter()
        .map(|s| s.label.clone())
        .collect()
}

#[test]
fn clips_are_named_selected_and_driven_by_the_keys() {
    let mut h = harness(edit(), true);
    let app = h.state().app.as_ref().unwrap().handle().clone();
    h.get_by_role_and_label(Role::Button, "Clip One on V1, 00:00:00:00 to 00:00:10:00");
    egui_pop_test::assert_all_named(&h);
    focus(&mut h);
    let (none, shift, ctrl) = (Modifiers::NONE, Modifiers::SHIFT, Modifiers::COMMAND);
    // Page Down: the next clip on the first track, the playhead to it.
    press(&mut h, none, Key::PageDown);
    assert_eq!(h.state().tl.selected_clips, [2]);
    assert_eq!(h.state().tl.playhead, 20.0);
    // Shift+Down: the clip nearest the playhead on the track below.
    press(&mut h, shift, Key::ArrowDown);
    assert_eq!(h.state().tl.selected_clips, [3]);
    // Ctrl+Right moves it a frame; Ctrl+Up to V1 is refused (it would
    // overlap One), so it stays.
    press(&mut h, ctrl, Key::ArrowRight);
    assert_eq!(h.state().tl.clip(3).unwrap().start, 5.04);
    press(&mut h, ctrl, Key::ArrowUp);
    assert_eq!(h.state().tl.clip_track(3).unwrap().name, "A1");
    // ] trims its end to the playhead (20 s); [ its start, to 12 s.
    press(&mut h, none, Key::CloseBracket);
    assert_eq!(h.state().tl.clip(3).unwrap().end(), 20.0);
    h.state_mut().tl.playhead = 12.0;
    press(&mut h, none, Key::OpenBracket);
    let c = h.state().tl.clip(3).unwrap().clone();
    assert_eq!((c.start, c.end()), (12.0, 20.0));
    assert!((c.offset - 6.96).abs() < 1e-9, "{}", c.offset);
    // Delete removes it; Escape lets the selection go.
    press(&mut h, none, Key::Delete);
    assert!(h.state().tl.clip(3).is_none());
    assert_eq!(
        undo_steps(&app),
        ["Move clip", "Trim clip", "Trim clip", "Remove clip"]
    );
    history::undo(&app, &Origin::User);
    h.run();
    assert_eq!(h.state().tl.clip(3).map(|c| c.start), Some(12.0));
    assert!(h.state().edits.contains(&Edit::Restored));
    press(&mut h, none, Key::PageUp);
    assert_eq!(h.state().tl.selected_clips, [1]);
    press(&mut h, none, Key::Escape);
    assert!(h.state().tl.selected_clips.is_empty());
}

#[test]
fn a_clip_dragged_moves_across_tracks_and_snaps_and_its_edge_trims() {
    let mut h = harness(edit(), true);
    let app = h.state().app.as_ref().unwrap().handle().clone();
    let one = h
        .get_by_role_and_label(Role::Button, "Clip One on V1, 00:00:00:00 to 00:00:10:00")
        .rect();
    let sound = h
        .get_by_role_and_label(Role::Button, "Clip Sound on A1, 00:00:05:00 to 00:00:15:00")
        .rect();
    let slider = h.get_by_role_and_label(Role::Slider, "Timeline").rect();
    let x_of = |t: f64| slider.left() + slider.width() * (t / 60.0) as f32;
    // A click selects it, and leaves the playhead.
    h.drag_at(one.center());
    h.run();
    h.drop_at(one.center());
    h.run();
    assert_eq!(h.state().tl.selected_clips, [1]);
    assert_eq!(h.state().tl.playhead, 12.0);
    // Grabbed in its middle (5 s), dropped near 40.2 s on A1: it lands
    // with its start on the grid, on A1.
    let y = sound.center().y;
    h.drag_at(one.center());
    h.run();
    for t in [20.0, 38.0, 40.2] {
        h.hover_at(Pos2::new(x_of(t), y));
        h.run();
    }
    h.drop_at(Pos2::new(x_of(40.2), y));
    h.run();
    let c = h.state().tl.clip(1).unwrap().clone();
    assert_eq!(h.state().tl.clip_track(1).unwrap().name, "A1");
    assert!((c.start - 35.0).abs() < 0.5, "{}", c.start);
    // Over Sound: it stops against it.
    let one = h
        .get_by_role_and_label(
            Role::Button,
            &format!(
                "Clip One on A1, {} to {}",
                h.state().tl.scale.format(c.start),
                h.state().tl.scale.format(c.end())
            ),
        )
        .rect();
    h.drag_at(one.center());
    h.run();
    for t in [30.0, 22.0, 18.0] {
        h.hover_at(Pos2::new(x_of(t), y));
        h.run();
    }
    h.drop_at(Pos2::new(x_of(18.0), y));
    h.run();
    assert_eq!(h.state().tl.clip(1).unwrap().start, 15.0, "against Sound");
    assert_eq!(undo_steps(&app), ["Move clip", "Move clip"]);
    // Its end dragged to 27.1 s: trimmed, but no further than its source
    // (12 s long), so it stops at 27 s.
    let c = h.state().tl.clip(1).unwrap().clone();
    let end = Pos2::new(x_of(c.end()) - 1.0, y);
    h.drag_at(end);
    h.run();
    for t in [26.0, 27.1] {
        h.hover_at(Pos2::new(x_of(t), y));
        h.run();
    }
    h.drop_at(Pos2::new(x_of(27.1), y));
    h.run();
    assert_eq!(h.state().tl.clip(1).unwrap().end(), 27.0);
    assert_eq!(
        undo_steps(&app).last().map(String::as_str),
        Some("Trim clip")
    );
    let trims = h
        .state()
        .edits
        .iter()
        .filter(|e| matches!(e, Edit::ClipTrimmed { .. }))
        .count();
    assert_eq!(trims, 1, "one edit for the whole drag");
    history::undo(&app, &Origin::User);
    h.run();
    assert_eq!(h.state().tl.clip(1).unwrap().end(), 25.0);
}

#[test]
fn an_application_paints_its_clips() {
    let mut seen = Vec::new();
    let mut tl = edit();
    let ctx = egui::Context::default();
    let mut out = ctx.run_ui(egui::RawInput::default(), |ui| {
        let mut painter = |_: &egui::Painter, clip: &ClipPaint<'_>| {
            let (from, to) = clip.source_in_view();
            seen.push((clip.clip.label.clone(), clip.track.name.clone(), from, to));
        };
        Timeline::new(&mut tl).clips(&mut painter).show(ui);
    });
    out.textures_delta.clear();
    let two = seen.iter().find(|s| s.0 == "Two").expect("Two painted");
    assert_eq!(two.1, "V1");
    // Its source from its offset.
    assert!(
        (two.2 - 2.0).abs() < 0.1 && (two.3 - 12.0).abs() < 0.1,
        "{two:?}"
    );
    assert_eq!(seen.len(), 3);
}

#[test]
fn agents_add_move_trim_select_and_remove_clips() {
    let mut h = harness(edit(), true);
    let app = h.state().app.as_ref().unwrap().handle().clone();
    let call = |h: &mut Harness<'_, Kept>, id: &str, args: Value| {
        let f = app.debug_call("commands", "run", &json!({ "id": id, "args": args }));
        h.run();
        f
    };
    let f = call(
        &mut h,
        "timeline.add_clip",
        json!({ "track": 1, "seconds": 40.0, "length": 5.0, "label": "Three", "source": "three.mov", "color": "#3366cc" }),
    );
    assert!(f.ok, "{f:?}");
    let added = h
        .state()
        .tl
        .clips()
        .find(|(_, c)| c.label == "Three")
        .unwrap()
        .1
        .clone();
    assert_eq!(
        (added.start, added.end(), added.source.as_str()),
        (40.0, 45.0, "three.mov")
    );
    // Refused where it would overlap, and said why.
    let refused = h
        .state_mut()
        .app
        .as_mut()
        .unwrap()
        .run(
            &egui_pop_views::timeline::CMD_MOVE_CLIP,
            json!({ "id": added.id, "seconds": 22.0 }),
            Origin::Agent("t".into()),
        )
        .now();
    let said = format!("{refused:?}");
    assert!(
        matches!(refused, Some(Err(_))) && said.contains("overlap clip 2"),
        "{said}"
    );
    h.run();
    for (id, args) in [
        (
            "timeline.move_clip",
            json!({ "id": added.id, "track": 2, "time": "00:00:30:00" }),
        ),
        (
            "timeline.trim_clip",
            json!({ "id": added.id, "start": 31.0, "end": 33.0 }),
        ),
        ("timeline.select_clips", json!({ "ids": [1, added.id] })),
    ] {
        let f = call(&mut h, id, args);
        assert!(f.ok, "{id}: {f:?}");
    }
    let c = h.state().tl.clip(added.id).unwrap();
    assert_eq!((c.start, c.end(), c.offset), (31.0, 33.0, 1.0));
    assert_eq!(h.state().tl.clip_track(added.id).unwrap().id, 2);
    let snap = app.debug_snapshot();
    let tl = &snap["timeline"]["timelines"]["song"];
    assert_eq!(tl["selected_clips"], json!([1, added.id]), "{tl}");
    assert_eq!(
        tl["tracks"][1]["clips"][1]["text"],
        json!(["00:00:31:00", "00:00:33:00"])
    );
    assert_eq!(tl["tracks"][1]["clips"][1]["color"], "#3366cc");
    let f = call(
        &mut h,
        "timeline.remove_clip",
        json!({ "ids": [1, added.id] }),
    );
    assert!(f.ok, "{f:?}");
    assert!(h.state().tl.clip(1).is_none() && h.state().tl.clip(added.id).is_none());
    assert_eq!(
        undo_steps(&app),
        [
            "Add clip",
            "Move clip",
            "Trim clip",
            "Trim clip",
            "Remove clips"
        ]
    );
}

/// What a deck test keeps: a timeline with its playhead fixed in the
/// middle, its clips locked to the pointer, nudged while it plays.
struct Deck {
    tl: TimelineState,
    moved: bool,
}

/// Thirty seconds at 120 BPM (a beat is half a second), one clip over all
/// of it, eight seconds in view, the playhead at 10 s.
fn deck() -> Harness<'static, Deck> {
    let mut tl = TimelineState::new(TimeScale::beats(120.0), 30.0);
    let t = tl.add_track(Track::new(0, "Deck"));
    tl.add_clip(t, Clip::new(0, 0.0, 30.0, "Song")).unwrap();
    tl.view = egui_pop_views::timeline::TimeView::new(0.0, 8.0);
    tl.playhead = 10.0;
    let mut h = Harness::builder()
        .with_size(vec2(800.0, 160.0))
        .build_ui_state(
            |ui, k: &mut Deck| {
                let out = Timeline::new(&mut k.tl)
                    .id("deck")
                    .follow_mode(Follow::CENTRE)
                    .scrub_on_drag(true)
                    .nudge_on_drag(true)
                    .show(ui);
                k.moved |= out.playhead_moved;
            },
            Deck { tl, moved: false },
        );
    h.run();
    h
}

/// The timeline's rect and its seconds per point.
fn deck_rect(h: &Harness<'_, Deck>) -> (egui::Rect, f64) {
    let rect = h.get_by_role_and_label(Role::Slider, "Timeline").rect();
    (rect, h.state().tl.view.duration / f64::from(rect.width()))
}

#[test]
fn a_fixed_playhead_stays_in_the_middle_and_a_drag_moves_the_content() {
    let mut h = deck();
    let (rect, per_point) = deck_rect(&h);
    // The playhead in the middle, the view around it.
    assert_eq!(h.state().tl.view.start, 6.0);
    // A drag on the (locked) clip, 100 points to the left: the content
    // follows the pointer, the playhead moves on with it.
    let y = rect.bottom() - 10.0;
    let x = rect.center().x + 50.0;
    h.drag_at(Pos2::new(x, y));
    h.run();
    assert_eq!(h.state().tl.playhead, 10.0, "a press does not move it");
    for dx in [-50.0, -100.0] {
        h.hover_at(Pos2::new(x + dx, y));
        h.run();
    }
    h.drop_at(Pos2::new(x - 100.0, y));
    h.run();
    let want = 10.0 + 100.0 * per_point;
    let t = h.state().tl.playhead;
    assert!((t - want).abs() < 0.1, "{t} vs {want}");
    assert_eq!(h.state().tl.view.start, t - 4.0, "still in the middle");
    // The clip did not move.
    let c = h.state().tl.clip(1).unwrap();
    assert_eq!((c.start, c.length), (0.0, 30.0));
    // A click on the ruler moves the playhead there.
    let at = h.state().tl.view.start + 2.0;
    let p = Pos2::new(rect.left() + (2.0 / per_point) as f32, rect.top() + 4.0);
    h.drag_at(p);
    h.run();
    h.drop_at(p);
    h.run();
    assert!((h.state().tl.playhead - at).abs() < 0.1);
}

#[test]
fn a_drag_while_playing_nudges_and_lets_the_speed_be() {
    let mut h = deck();
    h.state_mut().tl.play_at(1.0);
    h.run();
    let (rect, per_point) = deck_rect(&h);
    let (y, x) = (rect.bottom() - 10.0, rect.center().x);
    h.drag_at(Pos2::new(x, y));
    h.run();
    assert_eq!(h.state().tl.playhead, 10.0);
    for dx in [-40.0, -80.0] {
        h.hover_at(Pos2::new(x + dx, y));
        h.run();
    }
    // Dragged forward: a quarter of the drag added to the playing.
    let nudged = 10.0 + 80.0 * per_point * NUDGE_GAIN;
    assert!((h.state().tl.playhead - nudged).abs() < 1e-6);
    // Dragged back: slowed down.
    h.hover_at(Pos2::new(x, y));
    h.run();
    assert!((h.state().tl.playhead - 10.0).abs() < 1e-6);
    h.drop_at(Pos2::new(x, y));
    h.run();
    let tl = &h.state().tl;
    assert_eq!((tl.play, tl.speed), (PlayState::Playing, 1.0));
    assert!(h.state().moved);
}

#[test]
fn the_wheel_steps_a_paused_fixed_playhead_by_beats() {
    let mut h = deck();
    h.state_mut().tl.playhead = 10.2;
    h.run();
    let (rect, _) = deck_rect(&h);
    h.hover_at(rect.center());
    h.run();
    let wheel = |h: &mut Harness<'_, Deck>, lines: f32| {
        h.event(Event::MouseWheel {
            unit: egui::MouseWheelUnit::Line,
            delta: vec2(0.0, lines),
            modifiers: Modifiers::NONE,
            phase: egui::TouchPhase::Move,
            source: egui::MouseWheelSource::Wheel,
        });
        h.run();
    };
    // Down a notch: onto the beat, then the next one.
    wheel(&mut h, -1.0);
    assert_eq!(h.state().tl.playhead, 10.5);
    wheel(&mut h, 2.0);
    assert_eq!(h.state().tl.playhead, 9.5);
    // Playing, the wheel leaves it.
    h.state_mut().tl.play_at(1.0);
    wheel(&mut h, -1.0);
    assert_eq!(h.state().tl.playhead, 9.5);
}

/// A transport over the timeline `song`, through the commands with an
/// application, else directly.
fn transport(with_app: bool) -> Harness<'static, Kept> {
    let mut h = Harness::builder()
        .with_size(vec2(800.0, 200.0))
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
                    Transport::new(&mut k.tl)
                        .app(app.handle())
                        .timeline("song")
                        .show(ui);
                    Timeline::new(&mut k.tl)
                        .id("song")
                        .app(app.handle())
                        .history("doc/song")
                        .show(ui);
                } else {
                    Transport::new(&mut k.tl).show(ui);
                    Timeline::new(&mut k.tl).id("song").show(ui);
                }
            },
            Kept {
                tl: song(),
                app: None,
                with_app,
                edits: Vec::new(),
            },
        );
    h.run();
    h
}

fn click(h: &mut Harness<'_, Kept>, button: &str) {
    h.get_by_role_and_label(Role::Button, button).click();
    h.run();
    h.run();
}

#[test]
fn each_transport_button_runs_its_keys_command() {
    for with_app in [true, false] {
        let mut h = transport(with_app);
        let tl = |h: &Harness<'_, Kept>| {
            let t = &h.state().tl;
            (t.playhead, t.play, t.speed)
        };
        click(&mut h, "Step forward");
        assert_eq!(tl(&h).0, 10.04, "a frame on");
        click(&mut h, "Step back");
        click(&mut h, "Step back");
        assert_eq!(tl(&h).0, 9.96);
        click(&mut h, "Go to end");
        assert_eq!(tl(&h).0, 60.0);
        click(&mut h, "Go to start");
        assert_eq!(tl(&h).0, 0.0);
        click(&mut h, "Play");
        assert_eq!(tl(&h), (0.0, PlayState::Playing, 1.0));
        // Playing, the button pauses.
        click(&mut h, "Pause");
        assert_eq!(tl(&h).1, PlayState::Paused);
        click(&mut h, "Play backward");
        click(&mut h, "Play backward");
        assert_eq!(tl(&h), (0.0, PlayState::Playing, -2.0));
        assert!(
            h.get_by_role_and_label(Role::Button, "Play backward")
                .accesskit_node()
                .toggled()
                == Some(egui::accesskit::Toggled::True)
        );
        click(&mut h, "Stop");
        assert_eq!(tl(&h), (0.0, PlayState::Paused, 1.0));
        click(&mut h, "Loop");
        assert!(h.state().tl.looping);
        if with_app {
            // Ctrl+L's step of the history.
            let app = h.state().app.as_ref().unwrap().handle().clone();
            assert_eq!(undo_steps(&app), ["Loop the range"]);
        }
        click(&mut h, "Loop");
        assert!(!h.state().tl.looping);
    }
}

#[test]
fn the_transport_commands_are_the_keys() {
    let mut h = harness(song(), true);
    let app = h.state().app.as_ref().unwrap().handle().clone();
    let run = |h: &mut Harness<'_, Kept>, id: &str| {
        let args = json!({ "timeline": "song" });
        let f = app.debug_call("commands", "run", &json!({ "id": id, "args": args }));
        assert!(f.ok, "{id}: {f:?}");
        h.run();
    };
    run(&mut h, "timeline.shuttle_forward");
    run(&mut h, "timeline.shuttle_forward");
    assert_eq!(h.state().tl.speed, 2.0);
    run(&mut h, "timeline.toggle_play");
    assert_eq!(h.state().tl.play, PlayState::Paused);
    run(&mut h, "timeline.step_backward");
    assert_eq!(h.state().tl.playhead, 9.96);
    run(&mut h, "timeline.go_to_end");
    assert_eq!(h.state().tl.playhead, 60.0);
    run(&mut h, "timeline.toggle_loop");
    assert!(h.state().tl.looping);
}
