//! A drag on an image tool goes on to its release whatever its own change
//! does around it: a widget showing up above the tool once the value moved
//! (egui would give the tool another automatic id and end its drag), or the
//! histogram's panel turning stale under the drag (its edit area inert).

use egui::{Color32, Event, Modifiers, PointerButton, Pos2, Vec2, vec2};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable as _;
use egui_pop_core::areas;
use egui_pop_viewport::change::Change;
use egui_pop_viewport::curve::{CurveEditor, identity};
use egui_pop_viewport::histogram::{Channel, Drag, Histogram};
use egui_pop_viewport::wheel::{ColorWheel, WheelValue};

/// Presses at `from`, moves by `d` over four frames, releases.
fn drag<S>(h: &mut Harness<'_, S>, from: Pos2, d: Vec2) {
    h.event(Event::PointerMoved(from));
    h.step();
    h.event(Event::PointerButton {
        pos: from,
        button: PointerButton::Primary,
        pressed: true,
        modifiers: Modifiers::NONE,
    });
    h.step();
    for i in 1..=4 {
        h.event(Event::PointerMoved(from + d * (i as f32 / 4.0)));
        h.step();
    }
    h.event(Event::PointerButton {
        pos: from + d,
        button: PointerButton::Primary,
        pressed: false,
        modifiers: Modifiers::NONE,
    });
    h.run();
}

#[derive(Default)]
struct Curve {
    points: Vec<[f32; 2]>,
    moved: bool,
    done: usize,
}

#[test]
fn a_curve_drag_goes_on_when_a_widget_shows_up_above_it() {
    let mut h = Harness::builder()
        .with_size(vec2(240.0, 280.0))
        .build_ui_state(
            |ui, s: &mut Curve| {
                // Shown once the curve changed, as an application's "modified"
                // mark: every automatic id after it moves.
                if s.moved {
                    ui.label("Modified");
                }
                let out = CurveEditor::new(&mut s.points, "Tone curve")
                    .size(vec2(200.0, 200.0))
                    .show(ui);
                s.moved |= !out.changes.is_empty();
                s.done += out.changes.iter().filter(|c| c.done).count();
            },
            Curve {
                points: identity(),
                ..Curve::default()
            },
        );
    h.run();
    let rect = h.get_by_label("Tone curve").rect();
    drag(&mut h, rect.center(), vec2(0.0, -40.0));
    let pts = h.state().points.clone();
    assert_eq!(pts.len(), 3, "a point added: {pts:?}");
    assert!(pts[1][1] > 0.6, "and lifted by the drag: {pts:?}");
    assert!(h.state().done >= 1, "the drag ends as a finished change");
}

#[derive(Default)]
struct Wheel {
    value: WheelValue,
    moved: bool,
    done: usize,
}

#[test]
fn a_wheel_drag_goes_on_when_a_widget_shows_up_above_it() {
    let mut h = Harness::builder()
        .with_size(vec2(240.0, 300.0))
        .build_ui_state(
            |ui, s: &mut Wheel| {
                if s.moved {
                    ui.label("Modified");
                }
                let out = ColorWheel::offset(&mut s.value, "Lift")
                    .size(160.0)
                    .show(ui);
                s.moved |= !out.changes.is_empty();
                s.done += out.changes.iter().filter(|c| c.done).count();
            },
            Wheel::default(),
        );
    h.run();
    let rect = h.get_by_label("Lift").rect();
    drag(&mut h, rect.center(), vec2(40.0, 0.0));
    assert!(
        h.state().value.chroma > 0.1,
        "the drag moved the colour: {:?}",
        h.state().value
    );
    assert_eq!(h.state().done, 1, "one finished change at the release");
}

#[derive(Default)]
struct Scope {
    stale: bool,
    moved: Vec<f32>,
    done: Vec<Change<Drag>>,
}

#[test]
fn a_histogram_drag_goes_on_when_its_panel_turns_stale_under_it() {
    let bins = vec![1.0_f32; 64];
    let mut h = Harness::builder()
        .with_size(vec2(320.0, 200.0))
        .build_ui_state(
            move |ui, s: &mut Scope| {
                // The panel is stale as soon as the drag changed what it
                // shows, as an application recomputing its picture says.
                areas::scope(ui, "demo.scopes", s.stale, |ui| {
                    let out = Histogram::new([Channel::new("Luma", &bins, Color32::GRAY)])
                        .size(vec2(300.0, 100.0))
                        .on_change(|_| egui_pop_viewport::change::Verdict::Allow)
                        .show(ui);
                    for c in &out.changes {
                        if c.done {
                            s.done.push(c.clone());
                        } else {
                            s.moved.push(c.to.delta);
                            s.stale = true;
                        }
                    }
                });
            },
            Scope::default(),
        );
    h.run();
    let rect = h.get_by_label("Histogram").rect();
    drag(&mut h, rect.center(), vec2(60.0, 0.0));
    let s = h.state();
    assert!(
        s.moved.len() >= 3,
        "every frame of the drag moved: {:?}",
        s.moved
    );
    assert_eq!(s.done.len(), 1, "one finished change: {:?}", s.done);
    // It ends where the drag took it; given up, it would go back to 0.
    assert!(
        s.done[0].to.delta > 0.15,
        "the whole drag kept: {:?}",
        s.done
    );

    // Released, the stale panel's histogram is inert: a new drag edits
    // nothing.
    let before = (s.moved.len(), s.done.len());
    drag(&mut h, rect.center(), vec2(60.0, 0.0));
    let s = h.state();
    assert_eq!((s.moved.len(), s.done.len()), before, "inert once released");
}
