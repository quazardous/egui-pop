//! A setting changed, by this window or by another client of the host's
//! store, is said once on the bus (`settings.changed`), and its
//! followers (the theme, the pointer profile, the window's frame) do
//! their part in the pass it is heard, with no pass of their own after.

use egui_kittest::Harness;
use egui_pop_api::admin::store::{Changes, MemoryStore};
use egui_pop_api::admin::{ADMIN, AdminPlugin};
use egui_pop_api::{App, DefaultPlugins, SETTING_ACCENT_COLOR};
use egui_pop_core::theme::Tokens;
use egui_pop_core::tokens::ThemeSpec;
use egui_pop_model::admin::ValueScope;
use serde_json::json;

fn harness() -> Harness<'static, App> {
    let app = App::builder()
        .theme(ThemeSpec::new("pop-dark").accent("blue"))
        .plugins(DefaultPlugins)
        .plugin(AdminPlugin)
        .build(&egui::Context::default())
        .unwrap();
    let mut h = Harness::builder()
        .with_size(egui::vec2(800.0, 600.0))
        .build_ui_state(|ui, app: &mut App| app.show(ui), app);
    h.run();
    h
}

fn accent_alias(h: &Harness<'_, App>) -> Option<String> {
    Tokens::of(&h.ctx)
        .resolved()
        .get("accent.default")
        .and_then(|t| t.alias.clone())
}

#[test]
fn an_accent_another_client_changes_is_installed_in_the_pass_it_is_heard() {
    let mut h = harness();
    let store = MemoryStore::new(&[ValueScope::App]);
    let admin = h.state().handle().service(&ADMIN).unwrap();
    admin.attach_store(store.clone());
    h.run();
    assert_ne!(accent_alias(&h).as_deref(), Some("custom.9"));

    // Another client of the store changes the accent.
    let mut changes = Changes::new();
    changes.insert(SETTING_ACCENT_COLOR.to_string(), Some(json!("#c0392b")));
    store.change(ValueScope::App, changes);
    // One pass: the change is heard and the theme installed in it.
    h.step();
    let stored = egui_pop_core::lock(&h.state().handle().store())
        .get(&format!("settings/{SETTING_ACCENT_COLOR}"))
        .and_then(|e| e.value.as_text().map(str::to_string));
    assert_eq!(stored.as_deref(), Some("#c0392b"), "the change is heard");
    assert_eq!(
        accent_alias(&h).as_deref(),
        Some("custom.9"),
        "the theme is installed in the pass the change is heard"
    );
}
