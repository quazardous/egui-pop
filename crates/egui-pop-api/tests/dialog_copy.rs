//! A dialog's Copy button: it copies the title, the message and the
//! details (hidden, or shown in a collapsed monospace area holding the
//! same lines), and says so with a short note.

use egui::accesskit::Role;
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable as _;
use egui_pop_api::dialogs::{self, Confirm, DetailsMode, Info};
use egui_pop_api::{App, DefaultPlugins};
use egui_pop_model::ids::CommandId;

const GO: CommandId = CommandId::from_static("doc.go");

fn harness() -> (Harness<'static, App>, dialogs::Dialogs) {
    let app = App::builder()
        .plugins(DefaultPlugins)
        .build(&egui::Context::default())
        .unwrap();
    let d = app.handle().service(&dialogs::DIALOGS).unwrap();
    let h = Harness::builder()
        .with_size(egui::vec2(800.0, 600.0))
        .build_ui_state(|ui, app: &mut App| app.show(ui), app);
    (h, (*d).clone())
}

/// The text the last pass put on the clipboard, if any.
fn copied(h: &Harness<'_, App>) -> Option<String> {
    h.output()
        .platform_output
        .commands
        .iter()
        .find_map(|c| match c {
            egui::OutputCommand::CopyText(text) => Some(text.clone()),
            _ => None,
        })
}

/// Clicks the dialog's Copy button; returns what it copied.
fn copy(h: &mut Harness<'static, App>) -> Option<String> {
    h.get_by_role_and_label(Role::Button, "Copy").click();
    h.step();
    let text = copied(h);
    h.run();
    text
}

/// The text of every text area on screen.
fn areas(h: &Harness<'_, App>) -> Vec<String> {
    h.query_all_by_role(Role::MultilineTextInput)
        .chain(h.query_all_by_role(Role::TextInput))
        .filter_map(|n| n.value())
        .collect()
}

const LINES: [(&str, &str); 2] = [("version", "1.2.0"), ("job", "42")];

#[test]
fn the_copy_button_copies_the_text_and_the_hidden_details() {
    let (mut h, d) = harness();
    d.open(
        Confirm::new("Export the page?", "Export", GO)
            .detail("A PNG goes next to the document.")
            .details(LINES, DetailsMode::Auto),
    );
    h.run();
    egui_pop_test::assert_all_named(&h);
    assert_eq!(
        copy(&mut h).as_deref(),
        Some(
            "Export the page?\n\nA PNG goes next to the document.\n\n---\nversion: 1.2.0\njob: 42"
        )
    );
    // Said, and still open.
    assert!(h.query_by_label("Copied.").is_some());
    assert!(h.query_by_label("Export the page?").is_some());
    // Hidden details are nowhere on screen.
    assert!(h.query_by_label("Details").is_none());
    assert!(h.query_by_label_contains("1.2.0").is_none());
    assert!(areas(&h).is_empty(), "{:?}", areas(&h));
}

#[test]
fn shown_details_hold_exactly_the_copied_lines() {
    let (mut h, d) = harness();
    d.open(Info::error("Export failed", "The disk is full.").details(LINES, DetailsMode::Auto));
    h.run();
    // An error shows them, collapsed.
    assert!(areas(&h).is_empty());
    h.get_by_label("Details").click();
    h.run();
    let shown = areas(&h);
    assert_eq!(shown, ["version: 1.2.0\njob: 42"]);
    let text = copy(&mut h).unwrap();
    let (_, lines) = text.split_once("\n\n---\n").unwrap();
    assert_eq!(lines, shown[0]);
    egui_pop_test::assert_all_named(&h);
}

#[test]
fn a_message_shows_its_details_only_when_asked() {
    let (mut h, d) = harness();
    d.open(Info::new("Saved", "The document was saved.").details(LINES, DetailsMode::Auto));
    h.run();
    assert!(h.query_by_label("Details").is_none());
    d.answer(
        dialogs::Answer::Cancel,
        egui_pop_model::commands::Origin::User,
    )
    .unwrap();
    h.run();
    d.open(Info::new("Saved", "The document was saved.").details(LINES, DetailsMode::Shown));
    h.run();
    assert!(h.query_by_label("Details").is_some());
    let (mut h, d) = harness();
    d.open(Info::error("Failed", "No room.").details(LINES, DetailsMode::Hidden));
    h.run();
    assert!(h.query_by_label("Details").is_none());
}

#[test]
fn without_details_only_the_text_is_copied() {
    let (mut h, d) = harness();
    d.open(Info::new("Saved", "The document was saved."));
    h.run();
    assert_eq!(
        copy(&mut h).as_deref(),
        Some("Saved\n\nThe document was saved.")
    );
}

#[test]
fn enter_on_the_copy_button_copies_and_keeps_the_dialog() {
    let (mut h, d) = harness();
    d.open(Info::new("Saved", "The document was saved."));
    h.run();
    // The focus starts on Close; Tab reaches Copy.
    assert!(h.get_by_role_and_label(Role::Button, "Close").is_focused());
    h.key_press(egui::Key::Tab);
    h.run();
    assert!(h.get_by_role_and_label(Role::Button, "Copy").is_focused());
    h.key_press(egui::Key::Enter);
    h.run();
    assert!(h.query_by_label("Copied.").is_some());
    assert!(h.query_by_label("Saved").is_some());
    assert!(!d.is_empty());
}

#[test]
fn a_dialog_may_go_without_the_copy_button() {
    let (mut h, d) = harness();
    d.open(Confirm::new("Export the page?", "Export", GO).copyable(false));
    h.run();
    assert!(h.query_by_role_and_label(Role::Button, "Copy").is_none());
}
