//! The darktable-style slider and the parameter widget, headless.

use std::sync::{Arc, Mutex};

use egui::{Event, Key, Modifiers, PointerButton, Pos2};
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT as _, Queryable as _};
use egui_pop_core::binding::SharedStore;
use egui_pop_core::param::{self, Mode};
use egui_pop_core::slider::Slider;
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
fn a_drag_follows_the_soft_range_and_shift_is_finer() {
    let mut h = slider();
    let bar = h.get_by_label("Exposure").rect();
    let node = h.get_by_label("Exposure");
    assert_eq!(node.accesskit_node().role(), egui::accesskit::Role::Slider);
    drag(&mut h, bar.center(), bar.width() / 4.0, Modifiers::NONE);
    let coarse = ev(&h);
    assert!((coarse - 1.0).abs() < 0.05, "a quarter of 4 EV: {coarse}");
    h.event(Event::ModifiersChanged(Modifiers::SHIFT));
    drag(&mut h, bar.center(), bar.width() / 4.0, Modifiers::SHIFT);
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
    assert!((ev(&h) - 0.11).abs() < 1e-9);
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
