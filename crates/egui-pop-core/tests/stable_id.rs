//! A value widget keeps its drag when a widget appears before it
//! mid-drag (a "modified" mark, a reset button shown once the value
//! moves): it senses under an id of its own, not egui's automatic id,
//! which follows how many widgets came before.

use egui::{Event, Modifiers, PointerButton, Pos2, Rect, Vec2, vec2};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable as _;
use egui_pop_core::knob::Knob;
use egui_pop_core::slider::{RangeSlider, Slider};

/// A mark put before the widget once `modified`, without moving the
/// widget: it takes an automatic id, as any widget shown there would.
fn mark(ui: &mut egui::Ui, modified: bool) {
    if modified {
        let at = Rect::from_min_size(
            ui.max_rect().right_top() - vec2(12.0, 0.0),
            Vec2::splat(10.0),
        );
        ui.put(at, egui::Label::new("*"));
    }
}

/// Presses at `from`, moves by `by` in four frames, releases.
fn drag<S>(h: &mut Harness<'_, S>, from: Pos2, by: Vec2) {
    h.event(Event::PointerMoved(from));
    h.event(Event::PointerButton {
        pos: from,
        button: PointerButton::Primary,
        pressed: true,
        modifiers: Modifiers::NONE,
    });
    h.step();
    for i in 1..=4 {
        h.event(Event::PointerMoved(from + by * i as f32 / 4.0));
        h.step();
    }
    h.event(Event::PointerButton {
        pos: from + by,
        button: PointerButton::Primary,
        pressed: false,
        modifiers: Modifiers::NONE,
    });
    h.run();
}

#[derive(Default)]
struct One {
    value: f64,
    done: usize,
}

#[test]
fn a_slider_keeps_its_drag_when_a_mark_appears_before_it() {
    let mut h = Harness::builder()
        .with_size(vec2(420.0, 80.0))
        .build_ui_state(
            |ui, s: &mut One| {
                mark(ui, s.value != 0.0);
                let out = Slider::new(&mut s.value, "Exposure", -2.0..=2.0)
                    .step(0.01)
                    .default(0.0)
                    .show(ui);
                s.done += out.changes.iter().filter(|c| c.done).count();
            },
            One::default(),
        );
    h.run();
    let bar = h.get_by_label("Exposure").rect();
    drag(&mut h, bar.center(), vec2(bar.width() / 4.0, 0.0));
    let v = h.state().value;
    assert!(
        (v - 1.0).abs() < 0.05,
        "the whole drag, a quarter of 4 EV: {v}"
    );
    assert_eq!(h.state().done, 1, "the drag ends as one done change");
}

/// A scrub field over -2 to 2 dragged by `by`; the mark shows once the
/// value moved, or from the start when `always`.
fn scrub_drag(always: bool, by: f32) -> f64 {
    let mut h = Harness::builder()
        .with_size(vec2(420.0, 80.0))
        .build_ui_state(
            move |ui, s: &mut One| {
                mark(ui, always || s.value != 0.0);
                Slider::new(&mut s.value, "Exposure", -2.0..=2.0)
                    .step(0.01)
                    .scrub()
                    .show(ui);
            },
            One::default(),
        );
    h.run();
    let bar = h.get_by_label("Exposure").rect();
    drag(&mut h, bar.center(), vec2(by, 0.0));
    h.state().value
}

#[test]
fn a_scrub_field_keeps_its_drag_when_a_mark_appears_before_it() {
    let reference = scrub_drag(true, 60.0);
    let v = scrub_drag(false, 60.0);
    assert!(reference != 0.0, "the reference scrub moved");
    assert!(
        (v - reference).abs() < 0.02,
        "the drag goes as far as with the mark there from the start: {v} vs {reference}"
    );
}

#[derive(Default)]
struct Two {
    low: f64,
    high: f64,
}

#[test]
fn a_range_slider_keeps_its_drag_when_a_mark_appears_before_it() {
    let mut h = Harness::builder()
        .with_size(vec2(420.0, 80.0))
        .build_ui_state(
            |ui, s: &mut Two| {
                mark(ui, s.high != 80.0);
                ui.add(
                    RangeSlider::new(&mut s.low, &mut s.high, "Focal", 0.0..=100.0)
                        .step(1.0)
                        .default(20.0, 80.0),
                );
            },
            Two {
                low: 20.0,
                high: 80.0,
            },
        );
    h.run();
    let bar = h
        .get_by_label("Focal, minimum")
        .rect()
        .union(h.get_by_label("Focal, maximum").rect());
    let high = egui::pos2(bar.left() + bar.width() * 0.8, bar.center().y);
    drag(&mut h, high, vec2(-bar.width() * 0.2, 0.0));
    let s = h.state();
    assert!(
        (s.high - 60.0).abs() < 3.0,
        "the whole drag, 80 to 60: {}",
        s.high
    );
    assert_eq!(s.low, 20.0);
}

#[test]
fn a_knob_keeps_its_drag_when_a_mark_appears_before_it() {
    let mut h = Harness::builder()
        .with_size(vec2(200.0, 200.0))
        .build_ui_state(
            |ui, s: &mut One| {
                mark(ui, s.value != 50.0);
                Knob::new(&mut s.value, "Gain", 0.0..=100.0)
                    .default(50.0)
                    .size(Vec2::splat(120.0))
                    .show(ui);
            },
            One {
                value: 50.0,
                done: 0,
            },
        );
    h.run();
    let at = h.get_by_label("Gain").rect().center();
    // Each frame's move steps the knob; a lost drag stops after the
    // first.
    drag(&mut h, at, vec2(0.0, -80.0));
    let once = {
        let mut h1 = Harness::builder()
            .with_size(vec2(200.0, 200.0))
            .build_ui_state(
                |ui, s: &mut One| {
                    // The same knob, the mark always there: no id change.
                    mark(ui, true);
                    Knob::new(&mut s.value, "Gain", 0.0..=100.0)
                        .default(50.0)
                        .size(Vec2::splat(120.0))
                        .show(ui);
                },
                One {
                    value: 50.0,
                    done: 0,
                },
            );
        h1.run();
        drag(&mut h1, at, vec2(0.0, -80.0));
        h1.state().value
    };
    let v = h.state().value;
    assert!(once != 50.0, "the reference knob moved");
    assert!(
        (v - once).abs() < 0.5,
        "the drag goes as far as with the mark there from the start: {v} vs {once}"
    );
}
