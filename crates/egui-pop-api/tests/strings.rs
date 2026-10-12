//! The interface in another language: what plugins declare (command
//! titles, categories, panel titles, submenu labels, settings, plugin
//! titles) and the interface's own text, shown through the installed
//! strings provider, while the data (palette entries, stored values)
//! stays English.

use std::sync::Arc;

use egui::{Key, Modifiers};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable as _;
use egui_pop_api::devtools::{DevtoolsPlugin, PANEL_DEVTOOLS, SETTING_DEVTOOLS};
use egui_pop_api::settings_window::{self, CMD_SETTINGS};
use egui_pop_api::{App, DefaultPlugins, SETTING_FOCUS, palette, strings as txt};
use egui_pop_core::strings::{self, MapStrings};
use egui_pop_model::commands::Origin;
use egui_pop_model::values::{Change, Set, Value};

fn french() -> MapStrings {
    MapStrings::new("fr")
        // A command's title and its category.
        .with("command-app-settings.title", "Paramètres")
        .with("category-file", "Fichier")
        // A submenu of the menu bar.
        .with("menu-app-menu-file.label", "Fichier…")
        // A panel's title.
        .with("panel-egui-pop-devtools.title", "Outils")
        // A setting, one of its values, its section (the framework's).
        .with("setting-appearance-focus.label", "Focus clavier")
        .with("setting-appearance-focus-choice-hover", "survol")
        .with("plugin-framework.title", "Apparence")
        // The interface's own text.
        .with(txt::PALETTE_TITLE.key, "Palette de commandes")
        .with(txt::DEVTOOLS_TAB_JOURNAL.key, "Historique")
        .with(txt::SETTINGS_RESET_SETTING.key, "Rétablir { $setting }")
}

fn harness() -> Harness<'static, App> {
    let app = App::builder()
        .plugins(DefaultPlugins)
        .plugin(DevtoolsPlugin::default())
        .build(&egui::Context::default())
        .unwrap();
    let mut h = Harness::builder()
        .with_size(egui::vec2(1000.0, 700.0))
        .build_ui_state(|ui, app: &mut App| app.show(ui), app);
    strings::install(&h.ctx, Arc::new(french()));
    h.run();
    h
}

#[test]
fn the_helpers_translate_by_derived_keys() {
    let h = harness();
    let app = h.state().handle();
    let spec = app.command(&CMD_SETTINGS).unwrap();
    assert_eq!(spec.title, "Settings");
    assert_eq!(app.command_title(&spec), "Paramètres");
    assert_eq!(strings::category(&h.ctx, &spec.category), "Fichier");
    assert_eq!(
        strings::tr(&h.ctx, &txt::DEVTOOLS_TAB_STEPS),
        "Steps",
        "a key without a translation shows its English"
    );
}

#[test]
fn menus_panels_and_devtools_show_the_translations() {
    let mut h = harness();
    assert!(h.query_by_label("Fichier…").is_some());
    assert!(h.query_by_label("File").is_none());
    egui_pop_core::lock(&h.state().handle().store())
        .set(Set::new(
            &format!("settings/{SETTING_DEVTOOLS}"),
            Change::To(Value::Bool(true)),
            Origin::User,
        ))
        .unwrap();
    h.run();
    h.state().handle().focus_panel(&PANEL_DEVTOOLS);
    h.run();
    h.run();
    // The tab names its panel; the devtools' own tabs are translated.
    assert!(h.query_by_label("Outils").is_some());
    assert!(h.query_by_label("Historique").is_some());
    assert!(h.query_by_label("Journal").is_none());
}

#[test]
fn the_palette_shows_and_matches_the_translated_label_on_english_data() {
    let mut h = harness();
    h.key_press_modifiers(Modifiers::COMMAND | Modifiers::SHIFT, Key::P);
    h.run();
    assert!(palette::is_open(h.state().handle()));
    assert!(h.query_by_label("Palette de commandes").is_some());
    h.get_by_role(egui::accesskit::Role::TextInput)
        .type_text("fichier param");
    h.run();
    // The line shows its keys after the label.
    assert!(h.query_by_label_contains("Fichier: Paramètres").is_some());
    assert!(h.query_by_label_contains("File: Settings").is_none());
    let entry = h
        .state()
        .handle()
        .palette()
        .into_iter()
        .find(|e| e.command == CMD_SETTINGS)
        .unwrap();
    assert_eq!(
        (entry.category.as_str(), entry.title.as_str()),
        ("File", "Settings")
    );
}

#[test]
fn the_settings_window_shows_translated_settings_and_stores_raw_values() {
    let mut h = harness();
    h.key_press_modifiers(Modifiers::COMMAND, Key::Comma);
    h.run();
    assert!(settings_window::is_open(h.state().handle()));
    assert!(h.query_all_by_label("Apparence").count() >= 1);
    assert!(h.query_by_label("Appearance").is_none());
    h.get_by_label("Focus clavier").click();
    h.run();
    h.get_by_label("survol").click();
    h.run();
    let path = format!("settings/{SETTING_FOCUS}");
    let value = egui_pop_core::lock(&h.state().handle().store())
        .get(&path)
        .unwrap()
        .value
        .clone();
    assert_eq!(value, Value::Choice("hover".into()));
    assert!(h.query_by_label("Rétablir Focus clavier").is_some());
}
