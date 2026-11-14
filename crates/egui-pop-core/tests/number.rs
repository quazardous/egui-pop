//! The number input: a typed value commits on Enter, a value out of its
//! range or not a whole number is refused with its reason, the arrows
//! and the arrow keys step it within the range; a wide int setting is
//! drawn as one by `param`.

use egui::Key;
use egui::accesskit::Role;
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable as _;
use egui_pop_core::number::NumberInput;
use egui_pop_core::param::{self, Mode};
use egui_pop_model::values::{Descriptor, Value};

fn days() -> Harness<'static, f64> {
    let mut h = Harness::builder()
        .with_size(egui::vec2(520.0, 120.0))
        .build_ui_state(
            |ui, v: &mut f64| {
                ui.horizontal(|ui| {
                    let l = ui.label("Missing files kept");
                    NumberInput::new(v, "Missing files kept", 0.0..=3650.0)
                        .integer()
                        .unit("days")
                        .show(ui)
                        .labelled_by(l.id);
                });
            },
            30.0,
        );
    h.run();
    h
}

fn type_in(h: &mut Harness<'_, f64>, text: &str) {
    let field = h.get_by_role_and_label(Role::TextInput, "Missing files kept");
    field.focus();
    h.run();
    h.key_press_modifiers(egui::Modifiers::COMMAND, Key::A);
    h.get_by_role_and_label(Role::TextInput, "Missing files kept")
        .type_text(text);
    h.run();
    h.key_press(Key::Enter);
    h.run();
    h.run();
}

#[test]
fn a_typed_value_commits_and_a_wrong_one_is_refused_with_its_reason() {
    let mut h = days();
    type_in(&mut h, "90");
    assert_eq!(*h.state(), 90.0);

    type_in(&mut h, "5000");
    assert_eq!(*h.state(), 90.0, "out of range: refused");
    assert!(h.query_by_label_contains("Between 0").is_some());

    type_in(&mut h, "1.5");
    assert_eq!(*h.state(), 90.0);
    assert!(h.query_by_label("A whole number is expected").is_some());

    type_in(&mut h, "365");
    assert_eq!(*h.state(), 365.0);
    assert!(h.query_by_label("A whole number is expected").is_none());
}

#[test]
fn the_arrows_and_the_keys_step_within_the_range() {
    let mut h = days();
    h.get_by_label("Increase Missing files kept").click();
    h.run();
    assert_eq!(*h.state(), 31.0);
    h.get_by_label("Decrease Missing files kept").click();
    h.run();
    h.get_by_label("Decrease Missing files kept").click();
    h.run();
    assert_eq!(*h.state(), 29.0);

    h.get_by_role_and_label(Role::TextInput, "Missing files kept")
        .focus();
    h.run();
    h.key_press_modifiers(egui::Modifiers::SHIFT, Key::ArrowUp);
    h.run();
    assert_eq!(*h.state(), 39.0);
    *h.state_mut() = 3650.0;
    h.run();
    h.key_press(Key::ArrowUp);
    h.run();
    assert_eq!(*h.state(), 3650.0, "clamped to the range");
}

#[test]
fn param_types_a_wide_range_and_slides_a_short_one() {
    let mut h = Harness::builder()
        .with_size(egui::vec2(520.0, 160.0))
        .build_ui_state(
            |ui, v: &mut (Value, Value)| {
                let files =
                    Descriptor::int("Files seen kept", 1000, 100_000_000, 1_000_000).unit("files");
                param::param(ui, &files, Mode::Plain(&mut v.0));
                let calls = Descriptor::int("Model calls at once", 1, 64, 4);
                param::param(ui, &calls, Mode::Plain(&mut v.1));
            },
            (Value::Int(1_000_000), Value::Int(4)),
        );
    h.run();
    h.get_by_role_and_label(Role::TextInput, "Files seen kept");
    h.get_by_role_and_label(Role::Slider, "Model calls at once");
    h.get_by_label("Increase Files seen kept").click();
    h.run();
    assert_eq!(h.state().0, Value::Int(1_000_001));
}
