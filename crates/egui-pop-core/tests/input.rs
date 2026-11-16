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

/// Every text an assistive technology reads in the tree: names, values,
/// descriptions, placeholders (text runs carry their text as values).
fn every_text<S>(h: &Harness<'_, S>) -> Vec<String> {
    h.root()
        .children_recursive()
        .flat_map(|n| {
            let a = n.accesskit_node();
            [
                a.label(),
                a.value(),
                a.description().map(|d| d.to_string()),
                a.placeholder().map(|p| p.to_string()),
            ]
            .into_iter()
            .flatten()
            .collect::<Vec<_>>()
        })
        .collect()
}

fn secret_form(read_only: bool) -> Harness<'static, String> {
    let mut h = Harness::builder()
        .with_size(egui::vec2(320.0, 80.0))
        .build_ui_state(
            move |ui, s: &mut String| {
                let state = if read_only {
                    FieldState::new().read_only()
                } else {
                    FieldState::new()
                };
                Input::masked(s, "API token").state(state).show(ui);
            },
            "hunter2-secret".to_string(),
        );
    h.run();
    h
}

#[test]
fn a_masked_input_never_gives_its_value_to_assistive_technologies() {
    for read_only in [false, true] {
        let mut h = secret_form(read_only);
        let field = h.get_by_role_and_label(Role::PasswordInput, "API token");
        assert!(
            !field.value().unwrap_or_default().contains("hunter"),
            "the value is bullets"
        );
        if !read_only {
            h.get_by_label("API token").focus();
            h.run();
            h.key_press(Key::End);
            h.run();
            h.get_by_label("API token").type_text("-more");
            h.run();
            assert_eq!(h.state(), "hunter2-secret-more", "it is typed in");
        }
        for text in every_text(&h) {
            assert!(
                !text.contains("hunter") && !text.contains("secret"),
                "read only: {read_only}, a text in the tree shows the value: {text:?}"
            );
        }
        let fields = egui_pop_core::field::fields(&h.ctx);
        let f = fields.iter().find(|f| f.label == "API token").unwrap();
        assert_eq!(f.kind, "masked", "agents read its kind, not its value");
    }
}

#[derive(Default)]
struct Notes {
    text: String,
    submitted: u32,
    chat: String,
    sent: u32,
    height: f32,
}

fn notes(text: &str) -> Harness<'static, Notes> {
    let mut h = Harness::builder()
        .with_size(egui::vec2(320.0, 400.0))
        .build_ui_state(
            |ui, n: &mut Notes| {
                let out = Input::multiline(&mut n.text, "Notes")
                    .rows(2)
                    .max_rows(4)
                    .width(300.0)
                    .show(ui);
                n.height = out.response.rect.height();
                if out.submitted {
                    n.submitted += 1;
                }
                let out = Input::multiline(&mut n.chat, "Message")
                    .submit_on_enter(true)
                    .width(300.0)
                    .show(ui);
                if out.submitted {
                    n.sent += 1;
                }
            },
            Notes {
                text: text.into(),
                ..Notes::default()
            },
        );
    h.run();
    h
}

#[test]
fn enter_starts_a_new_line_and_ctrl_enter_submits_a_multiline_input() {
    let mut h = notes("");
    let field = h.get_by_role_and_label(Role::MultilineTextInput, "Notes");
    field.focus();
    h.run();
    h.get_by_label("Notes").type_text("one");
    h.run();
    h.key_press(Key::Enter);
    h.run();
    h.get_by_label("Notes").type_text("two");
    h.run();
    assert_eq!(h.state().text, "one\ntwo");
    assert_eq!(h.state().submitted, 0, "Enter is a new line");
    h.key_press_modifiers(egui::Modifiers::COMMAND, Key::Enter);
    h.run();
    assert_eq!(h.state().submitted, 1, "Ctrl+Enter submits");
    assert_eq!(h.state().text, "one\ntwo", "and adds no line");
    assert!(
        h.get_by_label("Notes").is_focused(),
        "the field keeps the focus"
    );
}

#[test]
fn enter_submits_a_multiline_input_set_to_and_shift_enter_starts_a_line() {
    let mut h = notes("");
    h.get_by_label("Message").focus();
    h.run();
    h.get_by_label("Message").type_text("hi");
    h.run();
    h.key_press_modifiers(egui::Modifiers::SHIFT, Key::Enter);
    h.run();
    h.get_by_label("Message").type_text("there");
    h.run();
    assert_eq!(h.state().chat, "hi\nthere");
    assert_eq!(h.state().sent, 0);
    h.key_press(Key::Enter);
    h.run();
    assert_eq!(h.state().sent, 1, "Enter submits");
    assert_eq!(h.state().chat, "hi\nthere", "and adds no line");
}

#[test]
fn a_multiline_input_grows_with_its_text_up_to_its_most_rows() {
    let height = |text: &str| notes(text).state().height;
    let two = height("");
    assert_eq!(two, height("a\nb"), "short text: its rows");
    let three = height("a\nb\nc");
    let four = height("a\nb\nc\nd");
    assert!(three > two && four > three, "{two} {three} {four}");
    assert_eq!(four, height("a\nb\nc\nd\ne\nf"), "then it scrolls");
    let mut h = notes("");
    h.get_by_label("Notes").focus();
    h.run();
    h.key_press(Key::Tab);
    h.run();
    assert!(
        h.get_by_label("Message").is_focused(),
        "Tab moves the focus, it types no tab"
    );
    assert_eq!(h.state().text, "");
}
