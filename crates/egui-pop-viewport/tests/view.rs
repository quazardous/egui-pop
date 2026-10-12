//! The view model through the widget: pan, cancel, wheel detent, keys,
//! double click, and 1:1 on a window at another scale.

use egui::{Event, Key, Modifiers, MouseWheelUnit, PointerButton, Pos2, Vec2, pos2, vec2};
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT as _, Queryable as _};
use egui_pop_viewport::{
    Cause, Phase, Signal, Verdict, View, ViewChange, Viewport, ViewportState, Zoom,
};

#[derive(Default)]
struct Scene {
    state: ViewportState,
    view: View,
    signals: Vec<Signal>,
    hover: Option<[i64; 2]>,
}

fn harness(ppp: f32, view: View) -> Harness<'static, Scene> {
    harness_with(ppp, view, ViewportState::new())
}

fn harness_with(ppp: f32, view: View, state: ViewportState) -> Harness<'static, Scene> {
    Harness::builder()
        .with_size(vec2(400.0, 300.0))
        .with_pixels_per_point(ppp)
        .build_ui_state(
            |ui, s: &mut Scene| {
                let out =
                    Viewport::new("v")
                        .image_size([1000, 800])
                        .show(ui, &mut s.state, &mut s.view);
                s.signals.extend(out.signals);
                s.hover = out.hover_pixel;
            },
            Scene {
                view,
                state,
                ..Scene::default()
            },
        )
}

fn centre(h: &Harness<'_, Scene>) -> Pos2 {
    h.get_by_label("Image viewport").rect().center()
}

fn press(h: &mut Harness<'_, Scene>, pos: Pos2, pressed: bool, modifiers: Modifiers) {
    h.input_mut().events.push(Event::PointerButton {
        pos,
        button: PointerButton::Primary,
        pressed,
        modifiers,
    });
}

fn phases(s: &Scene) -> Vec<Phase> {
    s.signals
        .iter()
        .filter_map(|s| match s {
            Signal::View { phase, .. } => Some(*phase),
            _ => None,
        })
        .collect()
}

#[test]
fn a_drag_pans_as_one_gesture() {
    let mut h = harness(1.0, View::actual([500.0, 400.0]));
    h.run();
    let c = centre(&h);
    h.hover_at(c);
    press(&mut h, c, true, Modifiers::NONE);
    h.step();
    for i in 1..=4 {
        h.hover_at(c + vec2(10.0 * i as f32, 0.0));
        h.step();
    }
    press(&mut h, c + vec2(40.0, 0.0), false, Modifiers::NONE);
    h.run();
    let s = h.state();
    // The image followed the pointer: 40 points right is 40 image pixels
    // at 100 % on a 1x window.
    assert_eq!(s.view, View::actual([460.0, 400.0]));
    let p = phases(s);
    assert_eq!(p.first(), Some(&Phase::Begin));
    assert_eq!(p.last(), Some(&Phase::End));
    assert!(
        p[1..p.len() - 1].iter().all(|p| *p == Phase::Update),
        "{p:?}"
    );
}

#[test]
fn escape_during_a_pan_puts_the_view_back() {
    let start = View::actual([500.0, 400.0]);
    let mut h = harness(1.0, start);
    h.run();
    let c = centre(&h);
    h.hover_at(c);
    press(&mut h, c, true, Modifiers::NONE);
    h.step();
    h.hover_at(c + vec2(30.0, 20.0));
    h.step();
    assert_ne!(h.state().view, start);
    h.key_press(Key::Escape);
    h.step();
    press(&mut h, c + vec2(30.0, 20.0), false, Modifiers::NONE);
    h.run();
    assert_eq!(h.state().view, start);
    assert_eq!(phases(h.state()).last(), Some(&Phase::Cancel));
}

#[test]
fn the_wheel_zooms_and_stops_at_100_percent() {
    let mut h = harness(1.0, View::at(0.5, [500.0, 400.0]));
    h.run();
    let c = centre(&h);
    h.hover_at(c);
    // One long scroll: continuous, but held on 100 % when it crosses it.
    h.input_mut().events.push(Event::MouseWheel {
        unit: MouseWheelUnit::Point,
        delta: vec2(0.0, 2000.0),
        modifiers: Modifiers::NONE,
        phase: egui::TouchPhase::Move,
    });
    h.run_steps(60);
    assert_eq!(h.state().view.zoom, Zoom::Px(1.0));
    // The next scroll leaves it.
    h.input_mut().events.push(Event::MouseWheel {
        unit: MouseWheelUnit::Point,
        delta: vec2(0.0, 100.0),
        modifiers: Modifiers::NONE,
        phase: egui::TouchPhase::Move,
    });
    h.run_steps(60);
    let Zoom::Px(z) = h.state().view.zoom else {
        panic!("not fitting")
    };
    assert!(z > 1.0, "{z}");
}

#[test]
fn keys_walk_the_ladder_when_focused() {
    let mut h = harness(1.0, View::fit());
    h.run();
    h.get_by_label("Image viewport").click();
    h.run();
    h.key_press(Key::Num1);
    h.run();
    assert_eq!(h.state().view.zoom, Zoom::Px(1.0));
    h.key_press(Key::Plus);
    h.run();
    assert_eq!(h.state().view.zoom, Zoom::Px(2.0));
    h.key_press(Key::Minus);
    h.key_press(Key::Minus);
    h.run();
    assert_eq!(h.state().view.zoom, Zoom::Px(2.0 / 3.0));
    h.key_press(Key::Num0);
    h.run();
    assert!(h.state().view.is_fit());
}

#[test]
fn a_double_click_fits() {
    let mut h = harness(1.0, View::at(3.0, [10.0, 10.0]));
    h.run();
    let c = centre(&h);
    egui_pop_test::double_click_at(&mut h, c);
    h.run();
    assert!(h.state().view.is_fit());
    assert_eq!(phases(h.state()).last(), Some(&Phase::Once));
}

#[test]
fn space_held_pans_too() {
    let mut h = harness(1.0, View::actual([500.0, 400.0]));
    h.run();
    let c = centre(&h);
    h.hover_at(c);
    h.key_down(Key::Space);
    h.step();
    press(&mut h, c, true, Modifiers::NONE);
    h.step();
    h.hover_at(c + vec2(0.0, 25.0));
    h.step();
    press(&mut h, c + vec2(0.0, 25.0), false, Modifiers::NONE);
    h.key_up(Key::Space);
    h.run();
    assert_eq!(h.state().view, View::actual([500.0, 375.0]));
}

#[test]
fn hundred_percent_is_one_image_pixel_per_physical_pixel_at_any_scale() {
    for ppp in [1.0, 1.25, 1.5, 2.0] {
        let mut h = harness(ppp, View::actual([500.0, 400.0]));
        h.run();
        let c = centre(&h);
        // Walk physical pixels across the centre: the readout moves one
        // image pixel per physical pixel.
        let mut last: Option<[i64; 2]> = None;
        for n in 0..12 {
            let x = ((c.x * ppp).floor() + n as f32 + 0.5) / ppp;
            h.hover_at(pos2(x, c.y));
            h.step();
            let px = h.state().hover.expect("over the image");
            if let Some(l) = last {
                assert_eq!(px[0], l[0] + 1, "ppp {ppp}");
                assert_eq!(px[1], l[1]);
            }
            last = Some(px);
        }
        // The stored zoom is still exactly 1.
        assert_eq!(h.state().view.zoom, Zoom::Px(1.0));
    }
}

#[test]
fn the_viewport_is_named_with_its_zoom() {
    let mut h = harness(2.0, View::actual([500.0, 400.0]));
    h.run();
    let node = h.get_by_label("Image viewport");
    assert_eq!(node.accesskit_node().numeric_value(), Some(100.0));
    let _ = Vec2::ZERO;
}

#[test]
fn a_before_hook_clamps_or_vetoes_the_zoom() {
    let mut state = ViewportState::new();
    // Never above 200 %, and no change by the keys at all.
    state.hooks_mut().before_view = Some(Box::new(|change: &ViewChange, _m| {
        if change.cause == Cause::Key {
            return Verdict::Veto;
        }
        match change.to.zoom {
            Zoom::Px(z) if z > 2.0 => Verdict::Redirect(View::at(2.0, change.to.center)),
            _ => Verdict::Allow,
        }
    }));
    let mut h = harness_with(1.0, View::at(1.0, [500.0, 400.0]), state);
    h.run();
    let c = centre(&h);
    h.hover_at(c);
    h.input_mut().events.push(Event::MouseWheel {
        unit: MouseWheelUnit::Point,
        delta: vec2(0.0, 3000.0),
        modifiers: Modifiers::NONE,
        phase: egui::TouchPhase::Move,
    });
    h.run_steps(60);
    assert_eq!(h.state().view.zoom, Zoom::Px(2.0));
    h.get_by_label("Image viewport").click();
    h.run();
    h.key_press(Key::Num1);
    h.run();
    assert_eq!(h.state().view.zoom, Zoom::Px(2.0));
    let verdicts: Vec<_> = h.state().state.trace().map(|t| t.verdict).collect();
    assert!(
        verdicts.contains(&"redirect") && verdicts.contains(&"veto"),
        "{verdicts:?}"
    );
}

#[test]
fn the_debug_bridge_sees_and_moves_the_viewport() {
    use egui_pop_core::debug::DebugBridge;
    let mut h = harness(1.0, View::fit());
    h.run();
    let mut bridge = DebugBridge::new();
    bridge
        .register(egui_pop_viewport::debug::collector())
        .unwrap();
    let snap = bridge.snapshot_of(&h.ctx, "viewport").unwrap();
    assert_eq!(snap["viewports"]["v"]["view"]["fit"], true, "{snap}");
    let f = bridge.call(
        &h.ctx,
        "viewport",
        "set_view",
        &serde_json::json!({ "name": "v", "zoom": 1.0, "center": [10.0, 20.0] }),
    );
    assert!(f.ok && f.mutates, "{f:?}");
    h.run();
    assert_eq!(h.state().view, View::actual([10.0, 20.0]));
    let snap = bridge.snapshot_of(&h.ctx, "viewport").unwrap();
    let signals = snap["viewports"]["v"]["signals"]
        .as_array()
        .unwrap()
        .clone();
    assert!(
        signals.iter().any(|s| s["cause"] == "command"),
        "{signals:?}"
    );
}

#[test]
fn a_new_display_target_is_signalled() {
    use egui_pop_viewport::display::{self, DisplayTarget, Primaries, Transfer};
    let mut h = harness(1.0, View::fit());
    h.run();
    assert_eq!(
        h.state().state.display_target(),
        Some(DisplayTarget::srgb())
    );
    let wide = DisplayTarget {
        primaries: Primaries::DisplayP3,
        transfer: Transfer::SrgbPiecewise,
        ..DisplayTarget::srgb()
    };
    display::set_target(&h.ctx, egui::ViewportId::ROOT, wide);
    h.run();
    let last = h.state().signals.last().cloned();
    assert_eq!(last, Some(Signal::Display { target: wide }));
    // Not supported yet: drawn as sRGB.
    assert!(!wide.is_supported());
}
