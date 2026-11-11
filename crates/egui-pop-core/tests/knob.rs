//! The knob's drag modes, headless.

use egui::{Event, Modifiers, PointerButton, Pos2, Vec2, vec2};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable as _;
use egui_pop_core::knob::{Knob, KnobDrag, KnobWheel};

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

struct Strip {
    values: Vec<f64>,
}

/// Twelve knobs in a scrolling panel; the odd ones turn under the wheel
/// on hover.
fn strip() -> Harness<'static, Strip> {
    let mut h = Harness::builder()
        .with_size(vec2(240.0, 160.0))
        // A touchpad's stream comes at the display's pace: a pause drops
        // what is left of a step.
        .with_step_dt(1.0 / 60.0)
        .with_max_steps(120)
        .build_ui_state(
            |ui, s: &mut Strip| {
                egui::ScrollArea::vertical().show(ui, |ui| {
                    for (i, v) in s.values.iter_mut().enumerate() {
                        let wheel = if i % 2 == 1 {
                            KnobWheel::Hover
                        } else {
                            KnobWheel::Profile
                        };
                        Knob::new(v, &format!("Knob {i}"), 0.0..=10.0)
                            .step(1.0)
                            .wheel(wheel)
                            .show(ui);
                    }
                });
            },
            Strip {
                values: vec![5.0; 12],
            },
        );
    h.run();
    h
}

fn wheel_event(h: &mut Harness<'_, Strip>, at: Pos2, unit: egui::MouseWheelUnit, dy: f32) {
    h.event(Event::PointerMoved(at));
    h.event(Event::MouseWheel {
        unit,
        phase: egui::TouchPhase::Move,
        delta: vec2(0.0, dy),
        modifiers: Modifiers::NONE,
        source: egui::MouseWheelSource::Wheel,
    });
    h.run();
}

#[test]
fn a_hover_wheel_knob_turns_where_the_others_let_the_panel_scroll() {
    let mut h = strip();
    // The default profile is guarded: over knob 0, the panel scrolls.
    let at = h.get_by_label("Knob 0").rect().center();
    wheel_event(&mut h, at, egui::MouseWheelUnit::Line, -1.0);
    assert!(h.state().values.iter().all(|v| *v == 5.0));
    assert!(
        h.get_by_label("Knob 0").rect().center().y < at.y,
        "scrolled"
    );
    // Over a hover knob, a notch is a step, and the panel stays.
    let at = h.get_by_label("Knob 3").rect().center();
    wheel_event(&mut h, at, egui::MouseWheelUnit::Line, 1.0);
    assert_eq!(h.state().values[3], 6.0);
    assert_eq!(h.get_by_label("Knob 3").rect().center(), at, "stayed");
}

#[test]
fn a_touchpads_scroll_is_summed_into_steps() {
    let mut h = strip();
    let at = h.get_by_label("Knob 1").rect().center();
    let line = h.ctx.options(|o| o.input_options.line_scroll_speed);
    // A quarter of a line a frame: one step every four frames, not one a
    // frame.
    for _ in 0..8 {
        wheel_event(&mut h, at, egui::MouseWheelUnit::Point, line / 4.0);
    }
    assert_eq!(h.state().values[1], 7.0, "two lines, two steps");
    for _ in 0..3 {
        wheel_event(&mut h, at, egui::MouseWheelUnit::Point, -line / 4.0);
    }
    assert_eq!(h.state().values[1], 7.0, "under a line back: no step yet");
}

/// Types `text` into the open typed entry, then Enter.
fn type_in(h: &mut Harness<'_, f64>, text: &str) {
    for _ in 0..8 {
        h.key_press(egui::Key::Backspace);
    }
    h.get_by_role(egui::accesskit::Role::TextInput)
        .type_text(text);
    h.run();
    h.key_press(egui::Key::Enter);
    h.run();
}

#[test]
fn the_right_click_menu_types_a_value_and_resets() {
    let mut h = knob(None);
    h.get_by_label("Gain").click_secondary();
    h.run();
    h.get_by_label("Type a value").click();
    h.run();
    assert!(
        h.query_by_label("Gain, typed value").is_some(),
        "the typed entry opens"
    );
    type_in(&mut h, "30");
    near(*h.state(), 30.0, "typed");
    h.get_by_label("Gain").click_secondary();
    h.run();
    h.get_by_label("Reset").click();
    h.run();
    near(*h.state(), 50.0, "reset");
    // At its default, the menu has no "Reset".
    h.get_by_label("Gain").click_secondary();
    h.run();
    assert!(h.query_by_label("Type a value").is_some());
    assert!(h.query_by_label("Reset").is_none());
}

#[test]
fn a_knob_without_a_menu_opens_its_entry_on_request() {
    // (value, the host asks for the entry this pass)
    let mut h = Harness::builder()
        .with_size(vec2(200.0, 200.0))
        .build_ui_state(
            |ui, s: &mut (f64, bool)| {
                let out = Knob::new(&mut s.0, "Gain", 0.0..=100.0)
                    .size(Vec2::splat(120.0))
                    .menu(false)
                    .show(ui);
                if std::mem::take(&mut s.1) {
                    egui_pop_core::slider::request_typed_entry(ui.ctx(), out.response.id);
                }
            },
            (50.0, false),
        );
    h.run();
    h.get_by_label("Gain").click_secondary();
    h.run();
    assert!(h.query_by_label("Type a value").is_none(), "no menu");
    h.state_mut().1 = true;
    h.run();
    assert!(h.query_by_label("Gain, typed value").is_some());
}

#[test]
fn a_ctrl_click_resets_only_where_the_profile_binds_it() {
    let mut h = knob(None);
    let c = centre(&h);
    drag(&mut h, &line(c, vec2(0.0, -40.0)), Modifiers::NONE);
    near(*h.state(), 70.0, "moved");
    // No shipped profile binds it.
    assert_eq!(
        egui_pop_core::slider::gestures(&h.ctx).reset_click,
        Modifiers::NONE
    );
    h.get_by_label("Gain").click_modifiers(Modifiers::COMMAND);
    h.run();
    near(*h.state(), 70.0, "a Ctrl-click does nothing by default");
    // A profile binding `Ctrl+PrimaryClick` to `gesture.value.reset`
    // (on Linux the profile's Ctrl is egui's command).
    let mut g = egui_pop_core::slider::gestures(&h.ctx);
    g.reset_click = Modifiers::COMMAND;
    egui_pop_core::slider::set_gestures(&h.ctx, g);
    h.run();
    h.get_by_label("Gain").click_modifiers(Modifiers::COMMAND);
    h.run();
    near(*h.state(), 50.0, "a Ctrl-click resets");
}

#[test]
fn the_reset_click_is_read_from_the_profiles_binding() {
    use egui_pop_core::slider::Gestures;
    use egui_pop_model::ids::OwnerId;
    use egui_pop_model::keymap::{Action, BindOptions, Keymap, Layer};
    use egui_pop_model::pointer::{self, PROFILE_OWNER, PointerMap, gesture};
    let profile = pointer::default_profile();
    let mut k = Keymap::new();
    assert!(
        k.load(
            &PROFILE_OWNER,
            Layer::Default,
            &pointer::profile_doc(profile)
        )
        .is_empty()
    );
    let map = PointerMap::from_keymap(&k, profile);
    assert_eq!(Gestures::from_map(&map).reset_click, Modifiers::NONE);
    k.bind_with(
        &OwnerId::USER,
        Layer::User,
        "Ctrl+PrimaryClick",
        Action::Run(gesture::VALUE_RESET),
        "",
        BindOptions::surface(pointer::SCOPE_VALUE),
    )
    .unwrap();
    let map = PointerMap::from_keymap(&k, profile);
    assert!(Gestures::from_map(&map).reset_click.any());
}
