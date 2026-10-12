//! The settings window: search, sections, live form with resets, and the
//! shortcuts editor (capture, conflicts, reset).

use egui::{Key, Modifiers};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable as _;
use egui_pop_api::app::{CMD_QUIT, CMD_UNDO};
use egui_pop_api::settings_window::{self, SHORTCUTS};
use egui_pop_api::{App, DefaultPlugins};

fn harness() -> Harness<'static, App> {
    let app = App::builder()
        .plugins(DefaultPlugins)
        .build(&egui::Context::default())
        .unwrap();
    let mut h = Harness::builder()
        .with_size(egui::vec2(1000.0, 700.0))
        .build_ui_state(|ui, app: &mut App| app.show(ui), app);
    h.run();
    h
}

fn open(h: &mut Harness<'_, App>) {
    h.key_press_modifiers(Modifiers::COMMAND, Key::Comma);
    h.run();
    assert!(settings_window::is_open(h.state().handle()));
}

#[test]
fn search_narrows_the_form_and_a_reset_takes_a_setting_back() {
    let mut h = harness();
    open(&mut h);
    assert!(h.query_all_by_label("Appearance").count() >= 1);
    h.get_by_role(egui::accesskit::Role::TextInput)
        .type_text("focus");
    h.run();
    assert!(h.query_by_label("Keyboard focus").is_some());
    // The shortcuts matching the search are listed too, nothing else.
    assert!(h.query_by_label("Edit: Undo").is_none());
    h.get_by_label("Keyboard focus").click();
    h.run();
    h.get_by_label("hover").click();
    h.run();
    let path = format!("settings/{}", egui_pop_api::SETTING_FOCUS);
    let value = |h: &Harness<'_, App>| {
        egui_pop_core::lock(&h.state().handle().store())
            .get(&path)
            .unwrap()
            .value
            .clone()
    };
    assert_eq!(
        value(&h),
        egui_pop_model::values::Value::Choice("hover".into())
    );
    h.get_by_label("Reset Keyboard focus").click();
    h.run();
    assert_eq!(
        value(&h),
        egui_pop_model::values::Value::Choice("system".into())
    );
    // Escape closes the window.
    h.key_press(Key::Escape);
    h.run();
    assert!(!settings_window::is_open(h.state().handle()));
}

#[test]
fn new_keys_are_captured_conflicts_said_and_a_reset_brings_the_defaults_back() {
    let mut h = harness();
    open(&mut h);
    h.get_by_label(SHORTCUTS).click();
    h.run();
    h.run();
    h.get_by_label("Change the keys of Edit: Undo").click();
    h.run();
    // Captured keys never run a command (Ctrl+Q would quit).
    h.key_press_modifiers(Modifiers::COMMAND, Key::Q);
    h.run();
    h.run();
    let handle = h.state().handle().clone();
    assert_eq!(handle.keys_for(&CMD_UNDO), ["Ctrl+Q"]);
    assert!(handle.history().iter().all(|r| r.id != CMD_QUIT));
    assert!(
        h.query_by_label_contains("Ctrl+Q goes to").is_some()
            || h.query_by_label_contains("Ctrl+Q also bound to").is_some()
    );
    // Escape during a capture cancels it, the window stays.
    h.get_by_label("Change the keys of Edit: Undo").click();
    h.run();
    h.key_press(Key::Escape);
    h.run();
    assert!(settings_window::is_open(&handle));
    assert_eq!(handle.keys_for(&CMD_UNDO), ["Ctrl+Q"]);
    h.get_by_label("Default keys of Edit: Undo").click();
    h.run();
    h.run();
    assert_eq!(handle.keys_for(&CMD_UNDO), ["Ctrl+Z"]);
}
