//! Overlays and their handles, before/after, and the pixel readout,
//! through the widget.

use egui::{Event, Key, Modifiers, MouseWheelUnit, PointerButton, Pos2, vec2};
use egui_kittest::Harness;
use egui_pop_test::viewport::TestSource;
use egui_pop_viewport::overlay::{HandleKind, Overlay, Shape};
use egui_pop_viewport::{Compare, Phase, Signal, Verdict, View, Viewport, ViewportState};

struct Scene {
    state: ViewportState,
    view: View,
    after: TestSource,
    before: TestSource,
    overlays: Vec<Overlay>,
    /// The shapes when the current handle gesture began.
    starts: Vec<Shape>,
    compare: Compare,
    signals: Vec<Signal>,
    readout: Option<egui_pop_viewport::source::Readout>,
}

fn scene() -> Scene {
    let mut state = ViewportState::new();
    state.set_settle(0.0);
    let after = TestSource::immediate(state.sink(), [1000, 800]);
    let before = TestSource::immediate(state.before_sink(), [1000, 800]);
    let overlays = vec![Overlay::new(
        "crop",
        Shape::Rect([400.0, 300.0, 600.0, 500.0]),
    )];
    Scene {
        state,
        view: View::actual([500.0, 400.0]),
        after,
        before,
        starts: overlays.iter().map(|o| o.shape.clone()).collect(),
        overlays,
        compare: Compare::Off,
        signals: Vec::new(),
        readout: None,
    }
}

fn draw(ui: &mut egui::Ui, s: &mut Scene) {
    let out = Viewport::new("o")
        .source(&s.after)
        .before(&s.before)
        .compare(s.compare)
        .overlays(&s.overlays)
        .readout(true)
        .show(ui, &mut s.state, &mut s.view);
    for sig in &out.signals {
        match sig {
            Signal::Handle {
                phase: Phase::Begin,
                ..
            } => {
                s.starts = s.overlays.iter().map(|o| o.shape.clone()).collect();
            }
            Signal::Handle { .. } => {
                for (o, start) in s.overlays.iter_mut().zip(&s.starts) {
                    o.apply(start, sig);
                }
            }
            Signal::Compare { compare, phase }
                if *phase != Phase::Begin && !matches!(compare, Compare::Before) =>
            {
                s.compare = *compare;
            }
            _ => {}
        }
    }
    s.signals.extend(out.signals);
    s.readout = out.readout;
}

fn harness(s: Scene) -> Harness<'static, Scene> {
    Harness::builder()
        .with_size(vec2(400.0, 300.0))
        .build_ui_state(draw, s)
}

fn screen(h: &Harness<'_, Scene>, p: [f64; 2]) -> Pos2 {
    h.state().state.mapping().unwrap().image_to_screen(p)
}

fn button(h: &mut Harness<'_, Scene>, pos: Pos2, pressed: bool) {
    h.input_mut().events.push(Event::PointerButton {
        pos,
        button: PointerButton::Primary,
        pressed,
        modifiers: Modifiers::NONE,
    });
}

fn drag(h: &mut Harness<'_, Scene>, from: Pos2, to: Pos2, cancel: bool) {
    h.hover_at(from);
    h.step();
    button(h, from, true);
    h.step();
    for k in 1..=4 {
        h.hover_at(from + (to - from) * (k as f32 / 4.0));
        h.step();
    }
    if cancel {
        h.key_press(Key::Escape);
        h.step();
    }
    button(h, to, false);
    h.run();
}

fn handle_phases(s: &Scene) -> Vec<Phase> {
    s.signals
        .iter()
        .filter_map(|x| match x {
            Signal::Handle { phase, .. } => Some(*phase),
            _ => None,
        })
        .collect()
}

#[test]
fn dragging_a_corner_is_one_gesture_and_does_not_pan() {
    let mut h = harness(scene());
    h.run();
    let corner = screen(&h, [600.0, 500.0]);
    drag(&mut h, corner, corner + vec2(20.0, 10.0), false);
    let s = h.state();
    assert_eq!(s.view, View::actual([500.0, 400.0]), "the view did not pan");
    assert_eq!(
        s.overlays[0].shape,
        Shape::Rect([400.0, 300.0, 620.0, 510.0])
    );
    let p = handle_phases(s);
    assert_eq!(p.first(), Some(&Phase::Begin));
    assert_eq!(p.last(), Some(&Phase::End));
}

#[test]
fn escape_puts_the_shape_back() {
    let mut h = harness(scene());
    h.run();
    let body = screen(&h, [500.0, 400.0]);
    drag(&mut h, body, body + vec2(30.0, 30.0), true);
    assert_eq!(
        h.state().overlays[0].shape,
        Shape::Rect([400.0, 300.0, 600.0, 500.0])
    );
    assert_eq!(handle_phases(h.state()).last(), Some(&Phase::Cancel));
}

#[test]
fn a_before_hook_snaps_handles() {
    let mut s = scene();
    // Snap to tens of image pixels.
    s.state.hooks_mut().before_handle = Some(Box::new(|mv, _| {
        Verdict::Redirect(mv.to.map(|v| (v / 10.0).round() * 10.0))
    }));
    let mut h = harness(s);
    h.run();
    let corner = screen(&h, [400.0, 300.0]);
    drag(&mut h, corner, corner + vec2(-13.0, 7.0), false);
    assert_eq!(
        h.state().overlays[0].shape,
        Shape::Rect([390.0, 310.0, 600.0, 500.0])
    );
}

#[test]
fn space_held_pans_even_over_a_handle() {
    let mut h = harness(scene());
    h.run();
    let corner = screen(&h, [600.0, 500.0]);
    h.hover_at(corner);
    h.key_down(Key::Space);
    h.step();
    drag(&mut h, corner, corner + vec2(20.0, 0.0), false);
    h.key_up(Key::Space);
    h.run();
    // The image followed the pointer (less the drag threshold).
    assert!(h.state().view.center[0] < 490.0, "{:?}", h.state().view);
    assert_eq!(
        h.state().overlays[0].shape,
        Shape::Rect([400.0, 300.0, 600.0, 500.0])
    );
}

#[test]
fn a_double_click_on_a_handle_resets_it_and_the_wheel_adjusts_it() {
    let mut h = harness(scene());
    h.run();
    let edge = screen(&h, [600.0, 400.0]);
    egui_pop_test::double_click_at(&mut h, edge);
    h.run();
    assert!(h.state().signals.iter().any(|s| matches!(
        s,
        Signal::HandleReset {
            kind: HandleKind::Edge,
            handle: 5,
            ..
        }
    )));
    assert!(
        !h.state().view.is_fit(),
        "the canvas's double click did not run"
    );
    h.hover_at(edge);
    h.input_mut().events.push(Event::MouseWheel {
        unit: MouseWheelUnit::Point,
        delta: vec2(0.0, 50.0),
        modifiers: Modifiers::NONE,
        phase: egui::TouchPhase::Move,
    });
    h.run_steps(10);
    assert!(
        h.state()
            .signals
            .iter()
            .any(|s| matches!(s, Signal::HandleWheel { handle: 5, .. }))
    );
    assert_eq!(h.state().view, View::actual([500.0, 400.0]), "no zoom");
}

#[test]
fn split_shows_before_on_the_left_and_its_divider_moves() {
    let mut s = scene();
    s.overlays.clear();
    s.compare = Compare::Split(0.5);
    let mut h = harness(s);
    h.run();
    assert!(
        h.state().before.requested() > 0,
        "the before image was asked for"
    );
    assert!(h.state().state.tiles_complete());
    let rect = h.state().state.mapping().unwrap().rect;
    let mid = rect.center();
    drag(&mut h, mid, mid + vec2(-100.0, 0.0), false);
    let Compare::Split(t) = h.state().compare else {
        panic!("still splitting")
    };
    assert!((t - (0.5 - 100.0 / rect.width())).abs() < 0.01, "{t}");
}

#[test]
fn holding_the_before_key_shows_before() {
    let mut s = scene();
    s.overlays.clear();
    let mut h = harness(s);
    h.run();
    assert_eq!(h.state().before.requested(), 0);
    h.hover_at(screen(&h, [500.0, 400.0]));
    h.key_down(Key::Backslash);
    h.run();
    assert!(h.state().before.requested() > 0);
    h.key_up(Key::Backslash);
    h.run();
    let c: Vec<_> = h
        .state()
        .signals
        .iter()
        .filter_map(|s| match s {
            Signal::Compare { compare, phase } => Some((*compare, *phase)),
            _ => None,
        })
        .collect();
    assert_eq!(
        c,
        vec![(Compare::Before, Phase::Begin), (Compare::Off, Phase::End)]
    );
}

#[test]
fn the_readout_comes_from_the_source_for_the_pixel_under_the_pointer() {
    let mut s = scene();
    s.overlays.clear();
    let mut h = harness(s);
    h.run();
    let p = screen(&h, [423.5, 345.5]);
    h.hover_at(p);
    h.run();
    let r = h.state().readout.clone().expect("a readout");
    assert_eq!(r.pixel, [423, 345]);
    let [red, green, ..] = TestSource::pixel(423, 345, 0);
    assert_eq!(r.values[0], ("R".to_string(), f32::from(red)));
    assert_eq!(r.values[1], ("G".to_string(), f32::from(green)));
    assert!(
        h.state()
            .signals
            .iter()
            .any(|s| matches!(s, Signal::Readout(_)))
    );
    assert_eq!(h.state().after.readouts(), vec![[423, 345]]);
}

/// Every stroke width drawn, in points.
fn stroke_widths(shapes: &[egui::epaint::ClippedShape]) -> Vec<f32> {
    fn walk(s: &egui::Shape, out: &mut Vec<f32>) {
        match s {
            egui::Shape::Vec(v) => v.iter().for_each(|s| walk(s, out)),
            egui::Shape::LineSegment { stroke, .. } => out.push(stroke.width),
            egui::Shape::Path(p) => out.push(p.stroke.width),
            egui::Shape::Circle(c) => out.push(c.stroke.width),
            egui::Shape::Rect(r) => out.push(r.stroke.width),
            _ => {}
        }
    }
    let mut out = Vec::new();
    for c in shapes {
        walk(&c.shape, &mut out);
    }
    out.retain(|w| *w > 0.0);
    out
}

#[test]
fn overlay_strokes_and_handle_outlines_are_whole_pixels_at_any_scale() {
    for ppp in [1.0, 1.25, 1.5, 2.0] {
        let mut s = scene();
        s.overlays.push(Overlay::new(
            "spot",
            Shape::Circle {
                center: [500.0, 400.0],
                radius: 40.0,
            },
        ));
        s.compare = Compare::Split(0.5);
        let mut h = Harness::builder()
            .with_size(vec2(400.0, 300.0))
            .with_pixels_per_point(ppp)
            .build_ui_state(draw, s);
        h.run();
        let widths = stroke_widths(&h.output().shapes);
        assert!(widths.len() > 10, "{ppp}: {widths:?}");
        for w in widths {
            let px = w * ppp;
            assert!(
                (px - px.round()).abs() < 1e-3,
                "a {w}-point stroke is {px} pixels at {ppp}x"
            );
        }
    }
}
