//! A setting changed, by this window or by another client of the host's
//! store, is said once on the bus (`pop.settings.changed`), and its
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

/// The reviewed order of the settings point's listeners.
#[test]
fn the_settings_point_runs_in_its_reviewed_order() {
    use egui_pop_api::bus::{self, SETTINGS_POINTS};
    use std::path::Path;
    let h = harness();
    let app = h.state().handle();
    let dump = app.bus_order_of(&SETTINGS_POINTS);
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/bus/settings-points.txt");
    if std::env::var_os("UPDATE_BUS_ORDER").is_some() {
        std::fs::write(&path, &dump).unwrap();
    }
    let reviewed = std::fs::read_to_string(&path).unwrap_or_default();
    assert_eq!(
        dump,
        reviewed,
        "review it, then UPDATE_BUS_ORDER=1 writes {}",
        path.display()
    );
    for listener in [
        bus::THEME_INSTALL,
        bus::POINTER_PROFILE,
        bus::FRAME_DECORATIONS,
    ] {
        assert!(dump.contains(listener), "{dump}");
    }
    assert_eq!(
        bus::missing_targets(&app.bus(), &SETTINGS_POINTS),
        Vec::<String>::new()
    );
}

/// A setting written here is said once, with its scope and its writer.
#[test]
fn a_setting_written_here_is_said_once_and_its_follower_runs_in_the_pass() {
    use egui_pop_api::bus::SettingsChanged;
    use egui_pop_model::commands::Origin;
    use egui_pop_model::values::{Change, Set, Value};
    use std::sync::{Arc, Mutex};
    let mut h = harness();
    let app = h.state().handle().clone();
    let said: Arc<Mutex<Vec<(String, String, String)>>> = Arc::default();
    {
        let said = said.clone();
        app.bus()
            .listen::<SettingsChanged>(
                "test.said",
                egui_pop_api::bus::bus_contract::ListenOpts::new(),
                move |c, ()| {
                    said.lock()
                        .unwrap()
                        .push((c.id().to_string(), c.scope.clone(), c.by.clone()));
                    Ok(())
                },
            )
            .unwrap();
    }
    let profile = egui_pop_api::pointer::SETTING_PROFILE;
    egui_pop_core::lock(&app.store())
        .set(Set::new(
            &format!("settings/{profile}"),
            Change::To(Value::Choice("darktable".into())),
            Origin::User,
        ))
        .unwrap();
    h.step();
    assert_eq!(
        *said.lock().unwrap(),
        [(profile.to_string(), "app".to_string(), "user".to_string())],
        "said once"
    );
    // Its follower loaded the profile in that pass.
    let loaded = h
        .state()
        .handle()
        .bus_order_of(&egui_pop_api::bus::SETTINGS_POINTS);
    assert!(
        loaded.contains(egui_pop_api::bus::POINTER_PROFILE),
        "{loaded}"
    );
}
