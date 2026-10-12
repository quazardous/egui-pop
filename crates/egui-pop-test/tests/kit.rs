//! The test kit on itself and on a small application.

use egui_kittest::kittest::Queryable as _;
use egui_pop_core::theme::ThemeSpec;
use egui_pop_test::{a11y_tree, assert_all_named, double_click, themed, unnamed_interactive};

#[test]
fn unnamed_interactive_nodes_are_found() {
    let mut h = themed(&ThemeSpec::default(), egui::vec2(300.0, 120.0), |ui| {
        let _ = ui.button("Named");
        // A button with an empty label says nothing to a screen reader.
        let _ = ui.button("");
        ui.label("Text is not interactive");
    });
    h.run();
    let unnamed = unnamed_interactive(&h);
    assert_eq!(unnamed.len(), 1, "{unnamed:?}");
    assert!(
        unnamed[0].starts_with("Button") || unnamed[0].contains("Button"),
        "{unnamed:?}"
    );
    let tree = a11y_tree(&h);
    assert!(tree.contains("Button \"Named\""), "{tree}");
    assert!(
        tree.contains("Label value=\"Text is not interactive\""),
        "{tree}"
    );
    assert!(!tree.contains("TextRun"), "{tree}");
}

#[test]
fn a_double_click_registers() {
    let mut count = 0;
    {
        let mut h = themed(
            &ThemeSpec::new("pop-light"),
            egui::vec2(200.0, 80.0),
            |ui| {
                if ui
                    .add(egui::Label::new("Open").sense(egui::Sense::click()))
                    .double_clicked()
                {
                    count += 1;
                }
            },
        );
        h.run();
        double_click(&mut h, "Open");
        h.run();
        assert_all_named(&h);
        assert!(h.query_by_label("Open").is_some());
    }
    assert_eq!(count, 1);
}

#[test]
fn an_app_harness_runs_an_application_on_its_context() {
    let mut h = egui_pop_test::app(
        egui_pop_api::App::builder().theme(ThemeSpec::new("pop-high-contrast")),
        egui::vec2(800.0, 500.0),
    );
    h.run();
    let tokens = egui_pop_core::theme::Tokens::of(&h.ctx);
    assert_eq!(tokens.resolved().spec().base, "pop-high-contrast");
    assert!(h.query_by_label("File").is_some());
    let snapshot = h.state().handle().debug_snapshot();
    assert_eq!(snapshot["theme"]["spec"]["base"], "pop-high-contrast");
    let _ = egui_pop_model::ids::OwnerId::FRAMEWORK;
}

#[test]
fn the_shell_and_its_palette_name_every_interactive_node() {
    let mut h = egui_pop_test::app(
        egui_pop_api::App::builder().plugins(egui_pop_api::DefaultPlugins),
        egui::vec2(800.0, 500.0),
    );
    h.run();
    assert_all_named(&h);
    h.key_press_modifiers(
        egui::Modifiers::COMMAND | egui::Modifiers::SHIFT,
        egui::Key::P,
    );
    h.run();
    assert!(egui_pop_api::palette::is_open(h.state().handle()));
    assert_all_named(&h);
}
