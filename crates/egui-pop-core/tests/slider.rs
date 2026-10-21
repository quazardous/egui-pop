//! The darktable-style slider and the parameter widget, headless.

use std::sync::{Arc, Mutex};

use egui::{Event, Key, Modifiers, PointerButton, Pos2};
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT as _, Queryable as _};
use egui_pop_core::binding::SharedStore;
use egui_pop_core::icon::Icon;
use egui_pop_core::param::{self, Mode};
use egui_pop_core::slider::{Cause, Change, RangeSlider, Slider, Thumb, Verdict};
use egui_pop_model::ids::OwnerId;
use egui_pop_model::values::{Descriptor, Store, Value};

fn ev(h: &Harness<'_, f64>) -> f64 {
    *h.state()
}

fn slider() -> Harness<'static, f64> {
    let mut h = Harness::builder()
        .with_size(egui::vec2(420.0, 80.0))
        .build_ui_state(
            |ui, v: &mut f64| {
                ui.add(
                    Slider::new(v, "Exposure", -2.0..=2.0)
                        .hard(-10.0..=10.0)
                        .step(0.01)
                        .unit("EV")
                        .default(0.0),
                );
            },
            0.0,
        );
    h.run();
    h
}

fn drag(h: &mut Harness<'_, f64>, from: Pos2, dx: f32, modifiers: Modifiers) {
    let events = [
        Event::PointerMoved(from),
        Event::PointerButton {
            pos: from,
            button: PointerButton::Primary,
            pressed: true,
            modifiers,
        },
    ];
    for e in events {
        h.event(e);
    }
    h.step();
    for i in 1..=4 {
        h.event(Event::PointerMoved(
            from + egui::vec2(dx * i as f32 / 4.0, 0.0),
        ));
        h.step();
    }
    h.event(Event::PointerButton {
        pos: from + egui::vec2(dx, 0.0),
        button: PointerButton::Primary,
        pressed: false,
        modifiers,
    });
    h.run();
}

#[test]
fn a_drag_follows_the_soft_range_and_ctrl_is_finer() {
    let mut h = slider();
    let bar = h.get_by_label("Exposure").rect();
    let node = h.get_by_label("Exposure");
    assert_eq!(node.accesskit_node().role(), egui::accesskit::Role::Slider);
    drag(&mut h, bar.center(), bar.width() / 4.0, Modifiers::NONE);
    let coarse = ev(&h);
    assert!((coarse - 1.0).abs() < 0.05, "a quarter of 4 EV: {coarse}");
    h.event(Event::ModifiersChanged(Modifiers::COMMAND));
    drag(&mut h, bar.center(), bar.width() / 4.0, Modifiers::COMMAND);
    h.event(Event::ModifiersChanged(Modifiers::NONE));
    let fine = ev(&h) - coarse;
    assert!((fine - 0.1).abs() < 0.02, "ten times finer: {fine}");
    // The bar stops at the soft range.
    drag(&mut h, bar.center(), bar.width() * 2.0, Modifiers::NONE);
    assert_eq!(ev(&h), 2.0);
}

#[test]
fn keys_step_enter_types_and_a_double_click_resets() {
    let mut h = slider();
    h.get_by_label("Exposure").click();
    h.run();
    h.key_press(Key::ArrowRight);
    h.run();
    assert!((ev(&h) - 0.01).abs() < 1e-9);
    h.key_press_modifiers(Modifiers::SHIFT, Key::ArrowRight);
    h.run();
    assert!((ev(&h) - 0.11).abs() < 1e-9, "Shift: coarse");
    h.key_press_modifiers(Modifiers::COMMAND, Key::ArrowLeft);
    h.run();
    assert!((ev(&h) - 0.109).abs() < 1e-9, "Ctrl: finer");
    // Typed with its unit, past the soft range, up to the hard one.
    h.key_press(Key::Enter);
    h.run();
    for _ in 0..6 {
        h.key_press(Key::Backspace);
    }
    h.get_by_role(egui::accesskit::Role::TextInput)
        .type_text("12,5 EV");
    h.run();
    h.key_press(Key::Enter);
    h.run();
    assert_eq!(ev(&h), 10.0, "kept to the hard range");
    let pos = h.get_by_label("Exposure").rect().center();
    let events = &mut h.input_mut().events;
    events.push(Event::PointerMoved(pos));
    for _ in 0..2 {
        for pressed in [true, false] {
            events.push(Event::PointerButton {
                pos,
                button: PointerButton::Primary,
                pressed,
                modifiers: Modifiers::NONE,
            });
        }
    }
    h.run();
    assert_eq!(ev(&h), 0.0, "back to the default");
}

struct Params {
    store: SharedStore,
    local: Value,
}

#[test]
fn a_parameter_widget_binds_in_three_modes_and_a_drag_is_one_undo_step() {
    let store: SharedStore = Arc::new(Mutex::new(Store::new()));
    {
        let mut s = store.lock().unwrap();
        s.declare(
            &OwnerId::FRAMEWORK,
            "p/gain",
            Descriptor::float("Gain", 0.0, 2.0, 1.0),
        )
        .unwrap();
        s.declare(
            &OwnerId::FRAMEWORK,
            "p/mode",
            Descriptor::choice("Mode", &["fast", "fine"], "fast"),
        )
        .unwrap();
        s.declare(
            &OwnerId::FRAMEWORK,
            "p/tint",
            Descriptor::floats("Tint", 3, 0.0, 1.0, &[1.0, 0.5, 0.0]),
        )
        .unwrap();
    }
    let mut h = Harness::builder()
        .with_size(egui::vec2(420.0, 300.0))
        .build_ui_state(
            |ui, p: &mut Params| {
                let gain = Descriptor::float("Gain", 0.0, 2.0, 1.0);
                param::param(ui, &gain, Mode::Bound(&p.store, "p/gain"));
                let watch = Descriptor::float("Gain, watched", 0.0, 2.0, 1.0);
                param::param(ui, &watch, Mode::Watch(&p.store, "p/gain"));
                let local = Descriptor::int("Local", 0, 10, 5);
                param::param(ui, &local, Mode::Plain(&mut p.local));
                let mode = Descriptor::choice("Mode", &["fast", "fine"], "fast");
                param::param(ui, &mode, Mode::Bound(&p.store, "p/mode"));
                let tint = Descriptor::floats("Tint", 3, 0.0, 1.0, &[1.0, 0.5, 0.0]);
                assert!(param::is_colour(&tint));
                param::param(ui, &tint, Mode::Bound(&p.store, "p/tint"));
            },
            Params {
                store: store.clone(),
                local: Value::Int(5),
            },
        );
    h.run();
    assert!(
        h.get_by_label("Gain, watched")
            .accesskit_node()
            .is_disabled()
    );
    let bar = h.get_by_label("Gain").rect();
    let from = bar.center();
    h.event(Event::PointerMoved(from));
    h.event(Event::PointerButton {
        pos: from,
        button: PointerButton::Primary,
        pressed: true,
        modifiers: Modifiers::NONE,
    });
    h.step();
    for i in 1..=5 {
        h.event(Event::PointerMoved(
            from + egui::vec2(bar.width() / 10.0 * i as f32, 0.0),
        ));
        h.step();
    }
    h.event(Event::PointerButton {
        pos: from + egui::vec2(bar.width() / 2.0, 0.0),
        button: PointerButton::Primary,
        pressed: false,
        modifiers: Modifiers::NONE,
    });
    h.run();
    let gain = store
        .lock()
        .unwrap()
        .get("p/gain")
        .unwrap()
        .value
        .as_f64()
        .unwrap();
    assert!((gain - 2.0).abs() < 0.05, "{gain}");
    assert_eq!(
        store.lock().unwrap().undo_steps().len(),
        1,
        "one drag, one step"
    );
    // The plain one writes the caller's value.
    h.get_by_label("Local").click();
    h.run();
    h.key_press(Key::ArrowRight);
    h.run();
    assert_eq!(h.state().local, Value::Int(6));
    // The choice is written as a choice.
    h.get_by_label("Mode").click();
    h.run();
    h.get_by_label("fine").click();
    h.run();
    assert_eq!(
        store.lock().unwrap().get("p/mode").unwrap().value,
        Value::Choice("fine".into())
    );
}

struct Panel {
    values: Vec<f64>,
}

fn wheel(h: &mut Harness<'_, Panel>, at: Pos2, modifiers: Modifiers) {
    h.event(Event::PointerMoved(at));
    h.event(Event::ModifiersChanged(modifiers));
    h.event(Event::MouseWheel {
        unit: egui::MouseWheelUnit::Line,
        phase: egui::TouchPhase::Move,
        delta: egui::vec2(0.0, -1.0),
        modifiers,
    });
    h.run();
    h.event(Event::ModifiersChanged(Modifiers::NONE));
    h.run();
}

#[test]
fn the_wheel_scrolls_past_sliders_and_steps_a_focused_one_or_with_alt() {
    let mut h = Harness::builder()
        .with_size(egui::vec2(420.0, 160.0))
        .build_ui_state(
            |ui, p: &mut Panel| {
                egui::ScrollArea::vertical().show(ui, |ui| {
                    for (i, v) in p.values.iter_mut().enumerate() {
                        ui.add(Slider::new(v, &format!("Value {i}"), 0.0..=1.0).step(0.1));
                    }
                });
            },
            Panel {
                values: vec![0.5; 12],
            },
        );
    h.run();
    let at = h.get_by_label("Value 1").rect().center();
    wheel(&mut h, at, Modifiers::NONE);
    assert!(
        h.state().values.iter().all(|v| *v == 0.5),
        "scrolling changes nothing"
    );
    let moved = h.get_by_label("Value 1").rect().center();
    assert!(moved.y < at.y, "the panel scrolled");
    // With Alt over a slider, the wheel steps it, and the panel does
    // not drift with what is left of the notch's scroll.
    let at = h.get_by_label("Value 3").rect().center();
    wheel(&mut h, at, Modifiers::ALT);
    assert!(
        (h.state().values[3] - 0.4).abs() < 1e-9,
        "{:?}",
        h.state().values
    );
    assert_eq!(
        h.get_by_label("Value 3").rect().center(),
        at,
        "the panel stayed"
    );
    // A focused slider steps under the wheel.
    h.get_by_label("Value 4").click();
    h.run();
    let at = h.get_by_label("Value 4").rect().center();
    wheel(&mut h, at, Modifiers::NONE);
    assert!(
        (h.state().values[4] - 0.4).abs() < 1e-9,
        "{:?}",
        h.state().values
    );
}

#[test]
fn escape_during_a_drag_puts_the_value_back_with_no_undo_step() {
    let store: SharedStore = Arc::new(Mutex::new(Store::new()));
    store
        .lock()
        .unwrap()
        .declare(
            &OwnerId::FRAMEWORK,
            "p/gain",
            Descriptor::float("Gain", 0.0, 2.0, 1.0),
        )
        .unwrap();
    let mut h = Harness::builder()
        .with_size(egui::vec2(420.0, 80.0))
        .build_ui_state(
            |ui, s: &mut SharedStore| {
                let d = Descriptor::float("Gain", 0.0, 2.0, 1.0);
                param::param(ui, &d, Mode::Bound(s, "p/gain"));
            },
            store.clone(),
        );
    h.run();
    let bar = h.get_by_label("Gain").rect();
    let from = bar.center();
    h.event(Event::PointerMoved(from));
    h.event(Event::PointerButton {
        pos: from,
        button: PointerButton::Primary,
        pressed: true,
        modifiers: Modifiers::NONE,
    });
    h.step();
    for i in 1..=3 {
        h.event(Event::PointerMoved(
            from + egui::vec2(bar.width() / 10.0 * i as f32, 0.0),
        ));
        h.step();
    }
    let mid = store
        .lock()
        .unwrap()
        .get("p/gain")
        .unwrap()
        .value
        .as_f64()
        .unwrap();
    assert!(mid > 1.2, "{mid}");
    h.key_press(Key::Escape);
    h.run();
    h.event(Event::PointerButton {
        pos: from,
        button: PointerButton::Primary,
        pressed: false,
        modifiers: Modifiers::NONE,
    });
    h.run();
    let s = store.lock().unwrap();
    assert_eq!(s.get("p/gain").unwrap().value, Value::Float(1.0));
    assert!(s.undo_steps().is_empty(), "no trace");
}

fn type_value(h: &mut Harness<'_, f64>, text: &str) {
    h.key_press(Key::Enter);
    h.run();
    for _ in 0..12 {
        h.key_press(Key::Backspace);
    }
    h.get_by_role(egui::accesskit::Role::TextInput)
        .type_text(text);
    h.run();
    h.key_press(Key::Enter);
    h.run();
}

#[test]
fn a_typed_expression_uses_the_current_value() {
    let mut h = slider();
    h.get_by_label("Exposure").click();
    h.run();
    h.key_press(Key::ArrowUp);
    h.run();
    type_value(&mut h, "x * 100 + 0,5");
    assert!((ev(&h) - 1.5).abs() < 1e-9, "{}", ev(&h));
    type_value(&mut h, "(1 + 2) / 4 EV");
    assert!((ev(&h) - 0.75).abs() < 1e-9, "{}", ev(&h));
    // Not a value: nothing changes.
    type_value(&mut h, "EV");
    assert!((ev(&h) - 0.75).abs() < 1e-9);
}

#[derive(Default)]
struct Two {
    values: [f64; 2],
    ids: Vec<egui::Id>,
}

fn focus_report(h: &Harness<'_, Two>) -> String {
    let ids = &h.state().ids;
    format!(
        "focused {:?}; first {:?}, second {:?}, first's entry {:?}",
        h.ctx.memory(|m| m.focused()),
        ids[0],
        ids[1],
        egui_pop_core::slider::typing_id(ids[0]).with("edit")
    )
}

#[test]
fn leaving_the_typed_entry_commits_it_and_the_keyboard_goes_where_it_was_sent() {
    let mut h = Harness::builder()
        .with_size(egui::vec2(420.0, 120.0))
        .build_ui_state(
            |ui, p: &mut Two| {
                let [a, b] = &mut p.values;
                let first = ui.add(Slider::new(a, "First", 0.0..=1.0).step(0.1));
                let second = ui.add(Slider::new(b, "Second", 0.0..=1.0).step(0.1));
                p.ids = vec![first.id, second.id];
            },
            Two::default(),
        );
    h.run();
    h.get_by_label("First").click();
    h.run();
    h.key_press(Key::Enter);
    h.run();
    assert_eq!(
        h.ctx.memory(|m| m.focused()),
        Some(egui_pop_core::slider::typing_id(h.state().ids[0]).with("edit")),
        "the entry takes the keyboard when it opens: {}",
        focus_report(&h)
    );
    for _ in 0..6 {
        h.key_press(Key::Backspace);
    }
    h.get_by_role(egui::accesskit::Role::TextInput)
        .type_text("0.3");
    h.run();
    // A click on another widget: the entry commits and closes, and does
    // not take the keyboard back.
    h.get_by_label("Second").click();
    h.run();
    assert_eq!(h.state().values[0], 0.3, "committed");
    assert!(
        h.query_by_role(egui::accesskit::Role::TextInput).is_none(),
        "closed"
    );
    assert_eq!(
        h.ctx.memory(|m| m.focused()),
        Some(h.state().ids[1]),
        "the keyboard went to the other slider: {}",
        focus_report(&h)
    );
    h.key_press(Key::ArrowRight);
    h.run();
    assert!(
        (h.state().values[1] - 0.1).abs() < 1e-9,
        "{:?}",
        h.state().values
    );
    assert_eq!(h.state().values[0], 0.3);
    // Escape still leaves the entry with nothing.
    h.key_press(Key::Enter);
    h.run();
    h.get_by_role(egui::accesskit::Role::TextInput)
        .type_text("9");
    h.run();
    h.key_press(Key::Escape);
    h.run();
    assert!((h.state().values[1] - 0.1).abs() < 1e-9);
    assert!(h.query_by_role(egui::accesskit::Role::TextInput).is_none());
}

#[test]
fn the_bar_tells_assistive_technologies_its_range_step_and_text() {
    let mut h = slider();
    *h.state_mut() = -0.001;
    h.run();
    let node = h.get_by_label("Exposure");
    let a = node.accesskit_node();
    assert_eq!(a.min_numeric_value(), Some(-2.0));
    assert_eq!(a.max_numeric_value(), Some(2.0));
    assert_eq!(a.numeric_value_step(), Some(0.01));
    assert_eq!(a.value().as_deref(), Some("0.00 EV"), "never -0.00");
    // Increment and decrement as an assistive technology asks.
    let (target_node, target_tree) = a.locate();
    for action in [
        egui::accesskit::Action::Increment,
        egui::accesskit::Action::Increment,
        egui::accesskit::Action::Decrement,
    ] {
        h.event(Event::AccessKitActionRequest(
            egui::accesskit::ActionRequest {
                action,
                target_node,
                target_tree,
                data: None,
            },
        ));
        h.run();
    }
    assert!((ev(&h) - 0.009).abs() < 1e-9, "{}", ev(&h));
    // Page Up: a coarse step.
    h.get_by_label("Exposure").click();
    h.run();
    h.key_press(Key::PageUp);
    h.run();
    assert!((ev(&h) - 0.109).abs() < 1e-9, "{}", ev(&h));
}

/// A number provider writing a comma, as a French interface does.
struct Comma;

impl egui_pop_core::strings::Strings for Comma {
    fn get(&self, _key: &str) -> Option<String> {
        None
    }

    fn language(&self) -> &str {
        "fr"
    }

    fn number(&self, value: f64, decimals: usize) -> Option<String> {
        Some(format!("{value:.decimals$}").replace('.', ","))
    }
}

#[test]
fn the_value_is_written_in_the_interfaces_language() {
    let mut h = slider();
    egui_pop_core::strings::install(&h.ctx, Arc::new(Comma));
    *h.state_mut() = 1.25;
    h.run();
    let a = h.get_by_label("Exposure").accesskit_node().value();
    assert_eq!(a.as_deref(), Some("1,25 EV"));
    // The typed entry starts from the same text, and reads it back.
    h.get_by_label("Exposure").click();
    h.run();
    h.key_press(Key::Enter);
    h.run();
    h.key_press(Key::Enter);
    h.run();
    assert_eq!(ev(&h), 1.25);
}

#[derive(Default)]
struct Hooked {
    value: f64,
    on: bool,
    log: Vec<Change>,
    picks: usize,
}

fn hooked() -> Harness<'static, Hooked> {
    let mut h = Harness::builder()
        .with_size(egui::vec2(420.0, 80.0))
        .build_ui_state(
            |ui, s: &mut Hooked| {
                let log = &mut s.log;
                let out = Slider::new(&mut s.value, "Gain", 0.0..=1.0)
                    .step(0.01)
                    .quad_toggle(Icon::Visible, "Gain on", &mut s.on)
                    .on_change(|c| {
                        log.push(*c);
                        match (c.cause, c.done) {
                            // A drag is refused at its end.
                            (Cause::Drag, true) => Verdict::Veto,
                            // Typed values snap to tenths.
                            (Cause::Typed, _) => Verdict::Redirect((c.to * 10.0).round() / 10.0),
                            _ => Verdict::Go,
                        }
                    })
                    .show(ui);
                if out.quad.as_ref().is_some_and(|q| q.clicked()) {
                    s.picks += 1;
                }
            },
            Hooked {
                value: 0.5,
                ..Hooked::default()
            },
        );
    h.run();
    h
}

#[test]
fn a_hook_sees_each_change_and_vetoes_or_redirects_it() {
    let mut h = hooked();
    let bar = h.get_by_label("Gain").rect();
    let from = bar.center();
    h.event(Event::PointerMoved(from));
    h.event(Event::PointerButton {
        pos: from,
        button: PointerButton::Primary,
        pressed: true,
        modifiers: Modifiers::NONE,
    });
    h.step();
    for i in 1..=4 {
        h.event(Event::PointerMoved(
            from + egui::vec2(bar.width() / 16.0 * i as f32, 0.0),
        ));
        h.step();
    }
    assert!(
        h.state().value > 0.7,
        "the drag follows: {}",
        h.state().value
    );
    h.event(Event::PointerButton {
        pos: from + egui::vec2(bar.width() / 4.0, 0.0),
        button: PointerButton::Primary,
        pressed: false,
        modifiers: Modifiers::NONE,
    });
    h.run();
    assert_eq!(h.state().value, 0.5, "vetoed at the release");
    let log = &h.state().log;
    assert!(log.iter().any(|c| !c.done && c.cause == Cause::Drag));
    let end = log.iter().find(|c| c.done).unwrap();
    assert_eq!((end.thumb, end.from), (Thumb::Value, 0.5));
    // A typed value is redirected.
    h.get_by_label("Gain").click();
    h.run();
    h.key_press(Key::Enter);
    h.run();
    for _ in 0..8 {
        h.key_press(Key::Backspace);
    }
    h.get_by_role(egui::accesskit::Role::TextInput)
        .type_text("0.33");
    h.run();
    h.key_press(Key::Enter);
    h.run();
    assert!((h.state().value - 0.3).abs() < 1e-9, "{}", h.state().value);
}

#[test]
fn the_quad_button_toggles_and_is_named() {
    let mut h = hooked();
    let quad = h.get_by_label("Gain on");
    assert_eq!(
        quad.accesskit_node().role(),
        egui::accesskit::Role::CheckBox
    );
    quad.click();
    h.run();
    assert!(h.state().on);
    assert_eq!(h.state().picks, 1);
    assert_eq!(h.state().value, 0.5, "the bar did not move");
}

#[test]
fn a_logarithmic_drag_moves_by_ratios() {
    let mut h = Harness::builder()
        .with_size(egui::vec2(420.0, 80.0))
        .build_ui_state(
            |ui, v: &mut f64| {
                ui.add(Slider::new(v, "ISO", 100.0..=6400.0).logarithmic());
            },
            100.0,
        );
    h.run();
    let bar = h.get_by_label("ISO").rect();
    drag(&mut h, bar.center(), bar.width() / 2.0, Modifiers::NONE);
    assert!(
        (ev(&h) - 800.0).abs() < 25.0,
        "half the bar: x8, {}",
        ev(&h)
    );
}

struct Range {
    low: f64,
    high: f64,
}

fn range() -> Harness<'static, Range> {
    let mut h = Harness::builder()
        .with_size(egui::vec2(420.0, 80.0))
        .build_ui_state(
            |ui, r: &mut Range| {
                ui.add(
                    RangeSlider::new(&mut r.low, &mut r.high, "Focal", 0.0..=100.0)
                        .step(1.0)
                        .unit("mm")
                        .default(10.0, 90.0),
                );
            },
            Range {
                low: 20.0,
                high: 80.0,
            },
        );
    h.run();
    h
}

#[test]
fn a_range_has_two_named_thumbs_that_never_cross() {
    let mut h = range();
    let low = h.get_by_label("Focal, minimum");
    assert_eq!(low.accesskit_node().role(), egui::accesskit::Role::Slider);
    assert_eq!(low.accesskit_node().max_numeric_value(), Some(80.0));
    let high = h.get_by_label("Focal, maximum");
    assert_eq!(high.accesskit_node().min_numeric_value(), Some(20.0));
    // Keys on the high thumb, down past the low one: it stops there.
    high.click();
    h.run();
    for _ in 0..7 {
        h.key_press_modifiers(Modifiers::SHIFT, Key::ArrowLeft);
        h.run();
    }
    assert_eq!((h.state().low, h.state().high), (20.0, 20.0));
    h.key_press(Key::End);
    h.run();
    assert_eq!(h.state().high, 100.0);
    // A drag from the left of the bar moves the low thumb.
    let bar_left = h.get_by_label("Focal, minimum").rect();
    let from = bar_left.left_center() + egui::vec2(10.0, 0.0);
    h.event(Event::PointerMoved(from));
    h.event(Event::PointerButton {
        pos: from,
        button: PointerButton::Primary,
        pressed: true,
        modifiers: Modifiers::NONE,
    });
    h.step();
    let width = bar_left.width() + h.get_by_label("Focal, maximum").rect().width();
    for i in 1..=4 {
        h.event(Event::PointerMoved(
            from + egui::vec2(width * 0.1 * i as f32, 0.0),
        ));
        h.step();
    }
    h.event(Event::PointerButton {
        pos: from + egui::vec2(width * 0.4, 0.0),
        button: PointerButton::Primary,
        pressed: false,
        modifiers: Modifiers::NONE,
    });
    h.run();
    assert!(
        (h.state().low - 60.0).abs() < 1.5,
        "40% of the bar: {}",
        h.state().low
    );
    assert_eq!(h.state().high, 100.0);
}

#[test]
fn a_range_resets_both_thumbs_and_its_wheel_follows_the_rules() {
    let mut h = range();
    // Alt and the wheel over the low thumb's half.
    let at = h.get_by_label("Focal, minimum").rect().center();
    h.event(Event::PointerMoved(at));
    h.event(Event::ModifiersChanged(Modifiers::ALT));
    h.event(Event::MouseWheel {
        unit: egui::MouseWheelUnit::Line,
        phase: egui::TouchPhase::Move,
        delta: egui::vec2(0.0, 1.0),
        modifiers: Modifiers::ALT,
    });
    h.run();
    h.event(Event::ModifiersChanged(Modifiers::NONE));
    h.run();
    assert_eq!((h.state().low, h.state().high), (21.0, 80.0));
    let pos = h.get_by_label("Focal, maximum").rect().center();
    let events = &mut h.input_mut().events;
    events.push(Event::PointerMoved(pos));
    for _ in 0..2 {
        for pressed in [true, false] {
            events.push(Event::PointerButton {
                pos,
                button: PointerButton::Primary,
                pressed,
                modifiers: Modifiers::NONE,
            });
        }
    }
    h.run();
    assert_eq!((h.state().low, h.state().high), (10.0, 90.0));
}

/// A host that reads the keys before its widgets are drawn: egui-pop's
/// key dispatcher (with or without the focus plugin), or a greedy one
/// that takes Escape whatever owns the keyboard.
struct Host {
    keymap: egui_pop_model::keymap::Keymap,
    dispatcher: egui_pop_core::keys::KeyDispatcher,
    ran: Vec<String>,
    value: f64,
    plugin: bool,
    greedy: bool,
}

fn host(plugin: bool, greedy: bool) -> Harness<'static, Host> {
    use egui_pop_model::keymap::{Action, Keymap, Layer};
    let mut keymap = Keymap::new();
    for (keys, cmd, when) in [
        ("Escape", "app.cancel", ""),
        ("Enter", "app.ok", "textInputFocus"),
    ] {
        keymap
            .bind(
                &OwnerId::FRAMEWORK,
                Layer::Default,
                keys,
                Action::Run(cmd.parse().unwrap()),
                when,
            )
            .unwrap();
    }
    let mut h = Harness::builder()
        .with_size(egui::vec2(420.0, 80.0))
        .build_ui_state(
            |ui, s: &mut Host| {
                use egui_pop_core::focus;
                if s.plugin {
                    focus::install(ui.ctx());
                }
                if s.greedy {
                    if ui.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Escape)) {
                        s.ran.push("greedy.escape".into());
                    }
                } else {
                    let base = egui_pop_model::when::WhenContext::new();
                    let cx = focus::when_context(ui.ctx(), &base);
                    for d in s.dispatcher.process(ui.ctx(), &s.keymap, &cx) {
                        s.ran.push(d.command.to_string());
                    }
                }
                ui.add(
                    Slider::new(&mut s.value, "Exposure", -2.0..=2.0)
                        .hard(-10.0..=10.0)
                        .step(0.01),
                );
            },
            Host {
                keymap,
                dispatcher: egui_pop_core::keys::KeyDispatcher::new(),
                ran: Vec::new(),
                value: 0.0,
                plugin,
                greedy,
            },
        );
    h.run();
    h
}

/// Focuses the slider, opens its entry and types `text` in it.
fn open_and_type(h: &mut Harness<'_, Host>, text: &str) {
    h.get_by_label("Exposure").click();
    h.run();
    h.key_press(Key::Enter);
    h.run();
    for _ in 0..12 {
        h.key_press(Key::Backspace);
    }
    h.get_by_role(egui::accesskit::Role::TextInput)
        .type_text(text);
    h.run();
}

#[test]
fn the_typed_entry_owns_escape_and_enter_in_a_host_that_dispatches_keys_first() {
    for plugin in [false, true] {
        let mut h = host(plugin, false);
        open_and_type(&mut h, "1,5");
        assert!(
            egui_pop_core::focus::text_input_owns_keyboard(&h.ctx),
            "the open entry owns the keyboard (plugin {plugin})"
        );
        h.key_press(Key::Escape);
        h.run();
        assert_eq!(h.state().value, 0.0, "Escape cancels (plugin {plugin})");
        assert!(
            h.query_by_role(egui::accesskit::Role::TextInput).is_none(),
            "and closes the entry (plugin {plugin})"
        );
        assert!(
            h.state().ran.is_empty(),
            "no binding saw Escape (plugin {plugin}): {:?}",
            h.state().ran
        );
        open_and_type(&mut h, "1,5");
        h.key_press(Key::Enter);
        h.run();
        assert_eq!(h.state().value, 1.5, "Enter commits (plugin {plugin})");
        assert!(
            h.state().ran.is_empty(),
            "no binding saw Enter (plugin {plugin}): {:?}",
            h.state().ran
        );
        // Closed, the keys are the bindings' again.
        h.key_press(Key::Escape);
        h.run();
        assert_eq!(h.state().ran, ["app.cancel"], "plugin {plugin}");
    }
    // A host that takes Escape before the widgets, whatever owns the
    // keyboard: the entry still cancels rather than commits.
    let mut h = host(false, true);
    open_and_type(&mut h, "1,5");
    h.key_press(Key::Escape);
    h.run();
    assert_eq!(h.state().ran, ["greedy.escape"]);
    assert_eq!(h.state().value, 0.0, "cancelled, not committed");
    assert!(h.query_by_role(egui::accesskit::Role::TextInput).is_none());
}

struct Redrawn {
    value: f64,
    /// The panel skips its slider for a pass, as when another writer's
    /// change rebuilds it.
    skip: bool,
    id: Option<egui::Id>,
}

#[test]
fn a_panel_redrawn_while_typing_keeps_the_entry_open_and_a_real_blur_commits() {
    let mut h = Harness::builder()
        .with_size(egui::vec2(420.0, 120.0))
        .build_ui_state(
            |ui, s: &mut Redrawn| {
                if std::mem::take(&mut s.skip) {
                    ui.label("Rebuilding");
                    return;
                }
                let r = ui.add(Slider::new(&mut s.value, "Gain", 0.0..=1.0).step(0.01));
                s.id = Some(r.id);
                let _ = ui.button("Other");
            },
            Redrawn {
                value: 0.0,
                skip: false,
                id: None,
            },
        );
    h.run();
    h.get_by_label("Gain").click();
    h.run();
    h.key_press(Key::Enter);
    h.run();
    for _ in 0..6 {
        h.key_press(Key::Backspace);
    }
    h.get_by_role(egui::accesskit::Role::TextInput)
        .type_text("0.4");
    h.run();
    // One pass without the slider: egui drops the focus of a field not
    // drawn. The entry takes it back instead of committing.
    h.state_mut().skip = true;
    h.step();
    h.run();
    let edit = egui_pop_core::slider::typing_id(h.state().id.unwrap()).with("edit");
    assert_eq!(h.state().value, 0.0, "not committed by the redraw");
    assert_eq!(
        h.ctx.memory(|m| m.focused()),
        Some(edit),
        "the entry has the keyboard again"
    );
    // Typing goes on where it was.
    h.get_by_role(egui::accesskit::Role::TextInput)
        .type_text("5");
    h.run();
    // Tab: a real blur, which commits; the keyboard moves on.
    h.key_press(Key::Tab);
    h.run();
    assert!(
        (h.state().value - 0.45).abs() < 1e-9,
        "committed on the blur: {}",
        h.state().value
    );
    assert!(h.query_by_role(egui::accesskit::Role::TextInput).is_none());
    assert_ne!(h.ctx.memory(|m| m.focused()), Some(edit));
}

#[test]
fn under_darktables_profile_the_wheel_steps_the_slider_under_the_pointer() {
    let mut h = Harness::builder()
        .with_size(egui::vec2(420.0, 160.0))
        .build_ui_state(
            |ui, p: &mut Panel| {
                egui::ScrollArea::vertical().show(ui, |ui| {
                    for (i, v) in p.values.iter_mut().enumerate() {
                        ui.add(Slider::new(v, &format!("Value {i}"), 0.0..=1.0).step(0.1));
                    }
                });
            },
            Panel {
                values: vec![0.5; 12],
            },
        );
    let darktable = egui_pop_model::pointer::profile("darktable").unwrap();
    egui_pop_core::pointer::set_map(
        &h.ctx,
        egui_pop_model::pointer::PointerMap::from_profile(darktable),
    );
    h.run();
    let g = egui_pop_core::slider::gestures(&h.ctx);
    assert!(g.wheel_steps);
    // A plain wheel steps the slider under the pointer, focused or not;
    // the panel stays.
    let at = h.get_by_label("Value 1").rect().center();
    wheel(&mut h, at, Modifiers::NONE);
    assert!(
        (h.state().values[1] - 0.4).abs() < 1e-9,
        "{:?}",
        h.state().values
    );
    assert_eq!(h.get_by_label("Value 1").rect().center(), at);
    // Ctrl+Alt leaves the wheel to the panel.
    wheel(&mut h, at, Modifiers::COMMAND | Modifiers::ALT);
    assert!((h.state().values[1] - 0.4).abs() < 1e-9);
    // Ctrl is ten times finer.
    let at = h.get_by_label("Value 2").rect().center();
    wheel(&mut h, at, Modifiers::COMMAND);
    assert!(
        (h.state().values[2] - 0.49).abs() < 1e-9,
        "{:?}",
        h.state().values
    );
}
