//! The Settings app in the gallery: the gallery's settings as a generated
//! page with a library and a workspace value (their origin tags and
//! modified bars), a manifest's section, and the search across every
//! scope. Snapshots: `admin-*` (`tests/snapshots/`).

use egui_kittest::Harness;
use egui_kittest::kittest::Queryable as _;
use egui_pop::theme::Density;
use egui_pop_gallery::{Gallery, Options};
use egui_pop_test::SharedGpu as _;

fn harness() -> Harness<'static, Gallery> {
    egui_pop::api::files::refuse_system_chooser();
    let options = Options {
        theme: "pop-dark".into(),
        density: Density::Comfortable,
        page: "settings".into(),
        ..Options::default()
    };
    Harness::builder()
        .with_size(egui::vec2(1100.0, 720.0))
        .with_max_steps(8)
        .shared_wgpu()
        .build_eframe(move |cc| {
            egui_pop::core::motion::freeze(&cc.egui_ctx, 0.0);
            Gallery::from_creation_context(cc, options).expect("the gallery builds")
        })
}

#[test]
fn the_settings_app_shows_generated_pages_value_scopes_and_search() {
    let mut results = egui_kittest::SnapshotResults::new();
    let mut h = harness();
    h.run();
    h.get_by_label("Open the Settings app").click();
    h.run();
    h.run();
    let app = h.state().app.handle();
    assert!(egui_pop::api::admin::is_open(app));
    egui_pop_test::assert_all_named(&h);
    // The library's size and the workspace's tone, each said.
    h.get_by_role_and_label(egui::accesskit::Role::Group, "Size, modified, library");
    h.get_by_role_and_label(egui::accesskit::Role::Group, "Tone, modified, workspace");
    results.add(h.try_snapshot("admin-pop-dark"));
    // The section a manifest declares.
    h.get_by_label("Greeting").click();
    h.run();
    h.run();
    h.get_by_label("5 letters");
    egui_pop_test::assert_all_named(&h);
    results.add(h.try_snapshot("admin-manifest-pop-dark"));
    // The search covers every scope; the hits are editable in place.
    let hint = "Search every setting";
    h.get_by_role_and_label(egui::accesskit::Role::SearchInput, hint)
        .click();
    h.run();
    h.get_by_role_and_label(egui::accesskit::Role::SearchInput, hint)
        .type_text("size");
    h.run();
    h.run();
    egui_pop_test::assert_all_named(&h);
    results.add(h.try_snapshot("admin-search-pop-dark"));
}
