//! The decorated input, the field states and Tab order, headless.

use egui::Key;
use egui::accesskit::Role;
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT as _, Queryable as _};
use egui_pop_core::input::Input;
use egui_pop_core::tab::{self, ScopeKind, TabStop};
use egui_pop_model::field::FieldState;

#[derive(Default)]
struct Form {
    a: String,
    b: String,
    c: String,
    d: String,
    n: f64,
}

fn form() -> Harness<'static, Form> {
    let mut h = Harness::builder()
        .with_size(egui::vec2(520.0, 240.0))
        .build_ui_state(
            |ui, f: &mut Form| {
                tab::scope(ui, "form", ScopeKind::Panel, |ui| {
                    Input::text(&mut f.a, "First").width(200.0).show(ui);
                    Input::text(&mut f.b, "Off")
                        .state(FieldState::new().disabled("Not now"))
                        .width(200.0)
                        .show(ui);
                    Input::text(&mut f.c, "Locked")
                        .state(FieldState::new().read_only())
                        .width(200.0)
                        .show(ui);
                    Input::text(&mut f.d, "Early")
                        .tab(TabStop::new().index(1))
                        .width(200.0)
                        .show(ui);
                    Input::number(&mut f.n, "Amount").width(200.0).show(ui);
                });
            },
            Form {
                n: 2.0,
                ..Form::default()
            },
        );
    h.run();
    h
}

fn focused(h: &Harness<'_, Form>) -> String {
    h.get_all_by_role(Role::TextInput)
        .find(|n| n.is_focused())
        .and_then(|n| n.accesskit_node().label())
        .unwrap_or_default()
}

#[test]
fn tab_puts_indices_first_skips_disabled_and_keeps_read_only() {
    let mut h = form();
    h.get_by_label("Early").focus();
    h.run();
    let mut seen = vec![focused(&h)];
    for _ in 0..3 {
        h.key_press(Key::Tab);
        h.run();
        seen.push(focused(&h));
    }
    assert_eq!(seen, ["Early", "First", "Locked", "Amount"]);
    h.key_press_modifiers(egui::Modifiers::SHIFT, Key::Tab);
    h.run();
    assert_eq!(focused(&h), "Locked");
    assert_eq!(
        tab::order(&h.ctx, "form").len(),
        4,
        "the disabled field has no place"
    );
}

#[test]
fn a_read_only_input_keeps_its_value() {
    let mut h = form();
    h.state_mut().c = "fixed".into();
    h.run();
    h.get_by_label("Locked").focus();
    h.run();
    h.get_by_label("Locked").type_text("more");
    h.run();
    assert_eq!(h.state().c, "fixed");
}

#[test]
fn a_number_input_commits_an_expression_when_it_is_left() {
    let mut h = form();
    h.get_by_label("Amount").focus();
    h.run();
    h.key_press(Key::End);
    h.run();
    h.get_by_label("Amount").type_text("*3");
    h.run();
    assert_eq!(h.state().n, 2.0, "nothing until the field is left");
    h.key_press(Key::Enter);
    h.run();
    assert_eq!(h.state().n, 6.0);
}

#[test]
fn every_part_of_an_input_is_named() {
    let mut h = Harness::builder()
        .with_size(egui::vec2(520.0, 80.0))
        .build_ui_state(
            |ui, s: &mut String| {
                Input::search(s, "Find")
                    .choices(["one", "two"])
                    .button(
                        egui_pop_core::input::Attached::text("Go", "Search now").menu(["Again"]),
                    )
                    .show(ui);
            },
            "query".to_string(),
        );
    h.run();
    for label in [
        "Find",
        "Clear Find",
        "Find, choices",
        "Search now",
        "Search now, more",
    ] {
        assert!(h.query_by_label(label).is_some(), "{label}");
    }
    let chevron = h.get_by_label("Find, choices");
    assert_eq!(
        chevron.accesskit_node().has_popup(),
        Some(egui::accesskit::HasPopup::Listbox)
    );
}

#[test]
fn escape_leaves_a_number_input_with_nothing() {
    let mut h = form();
    egui_pop_core::focus::install(&h.ctx);
    h.run();
    h.get_by_label("Amount").focus();
    h.run();
    h.key_press(Key::End);
    h.run();
    h.get_by_label("Amount").type_text("*3");
    h.run();
    h.key_press(Key::Escape);
    h.run();
    assert!(!h.get_by_label("Amount").is_focused());
    assert_eq!(h.state().n, 2.0, "what was typed is dropped");
    assert_eq!(h.get_by_label("Amount").value().as_deref(), Some("2"));
}

/// Focuses the text field labelled `label` through AccessKit and types
/// `text` in the same frame, as a screen reader or an agent may.
fn focus_and_type<S>(h: &mut Harness<'_, S>, label: &str, text: &str) {
    let (target_node, target_tree) = h
        .get_by_role_and_label(Role::TextInput, label)
        .accesskit_node()
        .locate();
    let events = &mut h.input_mut().events;
    events.push(egui::Event::AccessKitActionRequest(
        egui::accesskit::ActionRequest {
            action: egui::accesskit::Action::Focus,
            target_node,
            target_tree,
            data: None,
        },
    ));
    events.push(egui::Event::Text(text.to_owned()));
    h.step();
    h.run();
}

#[test]
fn text_typed_with_an_accesskit_focus_reaches_the_input() {
    let mut h = form();
    egui_pop_core::focus::install(&h.ctx);
    h.run();
    focus_and_type(&mut h, "First", "typed");
    assert_eq!(focused(&h), "First");
    assert_eq!(h.state().a, "typed");
}

#[test]
fn text_typed_with_an_accesskit_focus_reaches_a_plain_text_edit() {
    let mut h = Harness::builder()
        .with_size(egui::vec2(320.0, 80.0))
        .build_ui_state(
            |ui, s: &mut String| {
                egui_pop_core::focus::install(ui.ctx());
                let label = ui.label("Name");
                ui.text_edit_singleline(s).labelled_by(label.id);
            },
            String::new(),
        );
    h.run();
    focus_and_type(&mut h, "Name", "Ada");
    assert_eq!(h.state(), "Ada");
}
