//! The knob's drag modes, headless.

use egui::{Event, Modifiers, PointerButton, Pos2, Vec2, vec2};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable as _;
use egui_pop_core::knob::{Knob, KnobDrag};

/// A 120-point knob over 0 to 100, at 50, in `mode` (the profile's when
/// `None`).
fn knob(mode: Option<KnobDrag>) -> Harness<'static, f64> {
    let mut h = Harness::builder()
        .with_size(vec2(200.0, 200.0))
        .build_ui_state(
            move |ui, v: &mut f64| {
                let mut k = Knob::new(v, "Gain", 0.0..=100.0)
                    .default(50.0)
                    .size(Vec2::splat(120.0));
                if let Some(mode) = mode {
                    k = k.drag(mode);
                }
                k.show(ui);
            },
            50.0,
        );
    h.run();
    h
}

fn centre(h: &Harness<'_, f64>) -> Pos2 {
    h.get_by_label("Gain").rect().center()
}

/// Presses at the first point, moves through the others a frame each,
/// and releases at the last.
fn drag(h: &mut Harness<'_, f64>, path: &[Pos2], modifiers: Modifiers) {
    let from = path[0];
    h.event(Event::PointerMoved(from));
    h.event(Event::PointerButton {
        pos: from,
        button: PointerButton::Primary,
        pressed: true,
        modifiers,
    });
    h.step();
    for at in &path[1..] {
        h.event(Event::PointerMoved(*at));
        h.step();
    }
    h.event(Event::PointerButton {
        pos: *path.last().unwrap(),
        button: PointerButton::Primary,
        pressed: false,
        modifiers,
    });
    h.run();
}

/// A straight drag from `from` by `by`, in four moves.
fn line(from: Pos2, by: Vec2) -> Vec<Pos2> {
    (0..=4).map(|i| from + by * i as f32 / 4.0).collect()
}

fn near(v: f64, want: f64, what: &str) {
    assert!((v - want).abs() < 0.5, "{what}: {v}, not {want}");
}

#[test]
fn vertical_is_up_for_more_wherever_the_press_lands() {
    let mut h = knob(None);
    let c = centre(&h);
    // 50 points up, a quarter of the 200-point travel.
    drag(&mut h, &line(c, vec2(0.0, -50.0)), Modifiers::NONE);
    near(*h.state(), 75.0, "up from the centre");
    // Sideways does nothing.
    drag(&mut h, &line(c, vec2(60.0, 0.0)), Modifiers::NONE);
    near(*h.state(), 75.0, "sideways");
    // Pressed on the rim, the drag is still vertical.
    let rim = c + vec2(50.0, 0.0);
    drag(&mut h, &line(rim, vec2(0.0, 50.0)), Modifiers::NONE);
    near(*h.state(), 50.0, "down from the rim");
}

#[test]
fn horizontal_is_right_for_more() {
    let mut h = knob(Some(KnobDrag::Horizontal));
    let c = centre(&h);
    drag(&mut h, &line(c, vec2(40.0, 0.0)), Modifiers::NONE);
    near(*h.state(), 70.0, "right");
    drag(&mut h, &line(c, vec2(0.0, -60.0)), Modifiers::NONE);
    near(*h.state(), 70.0, "up does nothing");
    drag(&mut h, &line(c, vec2(-80.0, 0.0)), Modifiers::NONE);
    near(*h.state(), 30.0, "left");
}

#[test]
fn both_adds_right_and_up() {
    let mut h = knob(Some(KnobDrag::Both));
    let c = centre(&h);
    drag(&mut h, &line(c, vec2(20.0, 0.0)), Modifiers::NONE);
    near(*h.state(), 60.0, "right");
    drag(&mut h, &line(c, vec2(0.0, -20.0)), Modifiers::NONE);
    near(*h.state(), 70.0, "up");
    drag(&mut h, &line(c, vec2(20.0, -20.0)), Modifiers::NONE);
    near(*h.state(), 90.0, "right and up");
    drag(&mut h, &line(c, vec2(20.0, 20.0)), Modifiers::NONE);
    near(*h.state(), 90.0, "right and down cancel");
}

/// Points on a circle around `c` of radius `r`, from angle `a` to `b`
/// (screen radians, y down), in eight moves.
fn arc(c: Pos2, r: f32, a: f32, b: f32) -> Vec<Pos2> {
    (0..=8)
        .map(|i| {
            let t = a + (b - a) * i as f32 / 8.0;
            c + r * vec2(t.cos(), t.sin())
        })
        .collect()
}

#[test]
fn circular_follows_the_turn_from_the_press_never_jumping() {
    use std::f32::consts::PI;
    let mut h = knob(Some(KnobDrag::Circular));
    let c = centre(&h);
    // A click anywhere changes nothing.
    h.get_by_label("Gain").click();
    h.run();
    near(*h.state(), 50.0, "a click");
    // A quarter turn clockwise from 9 o'clock to 12: 90° of the 270°
    // sweep, a third of the range, though the pointer ends where the
    // value 50 is drawn.
    drag(&mut h, &arc(c, 40.0, PI, 1.5 * PI), Modifiers::NONE);
    near(*h.state(), 50.0 + 100.0 / 3.0, "a quarter turn");
    // Back the other way, at another radius.
    drag(&mut h, &arc(c, 25.0, 0.0, -0.5 * PI), Modifiers::NONE);
    near(*h.state(), 50.0, "a quarter turn back");
}

#[test]
fn circular_waits_near_the_centre() {
    let mut h = knob(Some(KnobDrag::Circular));
    let c = centre(&h);
    // Through the centre and out the other side: the half turn the
    // angle makes on the way is not counted.
    let path = [
        c + vec2(-40.0, 0.0),
        c + vec2(-20.0, 0.0),
        c + vec2(-2.0, 1.0),
        c + vec2(2.0, -1.0),
        c + vec2(20.0, 0.0),
        c + vec2(40.0, 0.0),
    ];
    drag(&mut h, &path, Modifiers::NONE);
    near(*h.state(), 50.0, "through the centre");
}

#[test]
fn the_knob_mode_comes_from_the_profile_unless_the_knob_says() {
    let mut h = knob(None);
    let mut g = egui_pop_core::slider::gestures(&h.ctx);
    assert_eq!(g.knob_drag, KnobDrag::Vertical);
    g.knob_drag = KnobDrag::Horizontal;
    egui_pop_core::slider::set_gestures(&h.ctx, g);
    h.run();
    let c = centre(&h);
    drag(&mut h, &line(c, vec2(40.0, 0.0)), Modifiers::NONE);
    near(*h.state(), 70.0, "the profile's horizontal");
    // The knob's own mode wins.
    let mut h = knob(Some(KnobDrag::Vertical));
    egui_pop_core::slider::set_gestures(&h.ctx, g);
    h.run();
    let c = centre(&h);
    drag(&mut h, &line(c, vec2(40.0, 0.0)), Modifiers::NONE);
    near(*h.state(), 50.0, "the knob's vertical");
}

#[test]
fn the_travel_is_in_points_whatever_the_size() {
    let mut h = Harness::builder()
        .with_size(vec2(300.0, 300.0))
        .build_ui_state(
            |ui, v: &mut f64| {
                Knob::new(v, "Gain", 0.0..=100.0)
                    .size(Vec2::splat(240.0))
                    .travel(100.0)
                    .show(ui);
            },
            0.0,
        );
    h.run();
    let c = centre(&h);
    drag(&mut h, &line(c, vec2(0.0, -50.0)), Modifiers::NONE);
    near(*h.state(), 50.0, "half the travel");
}

/// A drag with `modifiers` held from the press to the end.
fn drag_held(h: &mut Harness<'_, f64>, path: &[Pos2], modifiers: Modifiers) {
    h.event(Event::ModifiersChanged(modifiers));
    drag(h, path, modifiers);
    h.event(Event::ModifiersChanged(Modifiers::NONE));
    h.run();
}

#[test]
fn the_profiles_fine_and_coarse_modifiers_scale_a_drag() {
    // The default profile: Ctrl finer, Shift coarser.
    let mut h = knob(None);
    let c = centre(&h);
    drag_held(&mut h, &line(c, vec2(0.0, -40.0)), Modifiers::COMMAND);
    near(*h.state(), 52.0, "Ctrl: ten times finer");
    drag_held(&mut h, &line(c, vec2(0.0, 8.0)), Modifiers::SHIFT);
    near(*h.state(), 12.0, "Shift: ten times coarser");
    // Lightroom Classic's: Shift finer.
    let mut h = knob(None);
    let lr = egui_pop_model::pointer::profile("lightroom").unwrap();
    egui_pop_core::pointer::set_map(
        &h.ctx,
        egui_pop_model::pointer::PointerMap::from_profile(lr),
    );
    h.run();
    let c = centre(&h);
    drag_held(&mut h, &line(c, vec2(0.0, -40.0)), Modifiers::SHIFT);
    near(*h.state(), 52.0, "Shift: finer under Lightroom's profile");
}
