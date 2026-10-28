//! About: Help > About *application* opens a dialog with what the host
//! said about the application, its version info is copied through the
//! clipboard (or straight to the system's without its plugin), agents
//! read the same as JSON, and the title follows the language.

use std::sync::Arc;

use egui_kittest::Harness;
use egui_kittest::kittest::Queryable as _;
use egui_pop_api::about::{
    self, ABOUT, AboutPlugin, AppInfo, CMD_ABOUT, CMD_ABOUT_READ, EGUI_VERSION,
};
use egui_pop_api::app::MENU_HELP;
use egui_pop_api::clipboard::{self, ClipboardPlugin};
use egui_pop_api::dialogs::{DIALOGS, Dialog, DialogsPlugin};
use egui_pop_api::{App, AppBuilder};
use egui_pop_core::strings::{self, MapStrings};
use egui_pop_model::clipboard::Slot;
use egui_pop_model::commands::Origin;
use egui_pop_model::menus::Item;
use serde_json::Value as Json;

fn info() -> AppInfo {
    AppInfo::new("", "1.2.0")
        .description("A photo editor.")
        .licence("GPL-3.0-or-later")
        .author("The Darkroom contributors")
        .repository("https://example.org/darkroom")
        .docs("https://example.org/darkroom/docs")
        .renderer("test")
}

fn builder() -> AppBuilder {
    App::builder()
        .name("Darkroom")
        .plugin(DialogsPlugin)
        .plugin(AboutPlugin::new(info()))
}

fn harness(app: App) -> Harness<'static, App> {
    Harness::builder()
        .with_size(egui::vec2(900.0, 640.0))
        .build_ui_state(|ui, app: &mut App| app.show(ui), app)
}

#[test]
fn the_help_menu_says_about_the_application_by_its_name() {
    let app = builder().build(&egui::Context::default()).unwrap();
    let h = app.handle();
    assert!(h.diagnostics().is_empty(), "{:?}", h.diagnostics());
    let items = h.menu(&MENU_HELP);
    assert!(
        items
            .iter()
            .any(|i| matches!(&i.item, Item::Command { id, .. } if *id == CMD_ABOUT)),
        "{items:?}"
    );
    let spec = h.command(&CMD_ABOUT).unwrap();
    assert_eq!(h.command_menu_title(&spec), "About Darkroom");
    // The name comes from the application when the info has none.
    assert_eq!(h.service(&ABOUT).unwrap().info().name, "Darkroom");
}

#[test]
fn the_title_is_translated_with_the_name_filled() {
    let ctx = egui::Context::default();
    let app = builder().build(&ctx).unwrap();
    strings::install(
        &ctx,
        Arc::new(MapStrings::new("fr").with("command-about.title", "À propos de { $application }")),
    );
    let spec = app.handle().command(&CMD_ABOUT).unwrap();
    assert_eq!(app.handle().command_title(&spec), "À propos de Darkroom");
}

#[test]
fn about_opens_a_dialog_with_the_info_and_its_links() {
    let mut app = builder().build(&egui::Context::default()).unwrap();
    let out = app.run(&CMD_ABOUT, Json::Null, Origin::User);
    assert_eq!(out.now(), Some(Ok(Json::Null)));
    let mut h = harness(app);
    h.run();
    let d = h.state().handle().service(&DIALOGS).unwrap();
    let Some((_, Dialog::Custom(c))) = d.current() else {
        panic!("no custom dialog");
    };
    assert_eq!(c.title, "About Darkroom");
    assert!(h.query_by_label("Darkroom").is_some());
    assert!(h.query_by_label("Version 1.2.0").is_some());
    assert!(h.query_by_label("A photo editor.").is_some());
    assert!(h.query_by_label("GPL-3.0-or-later").is_some());
    assert!(h.query_by_label("Source code").is_some());
    assert!(h.query_by_label("Documentation").is_some());
    // No website nor issues given: no link.
    assert!(h.query_by_label("Website").is_none());
    assert!(h.query_by_label("Report a problem").is_none());
    assert_eq!(d.describe()["body"]["version"], "1.2.0");
}

#[test]
fn agents_read_the_same_as_json() {
    let mut app = builder().build(&egui::Context::default()).unwrap();
    let spec = app.handle().command(&CMD_ABOUT_READ).unwrap();
    assert!(!spec.palette);
    let out = app.run(&CMD_ABOUT_READ, Json::Null, Origin::User);
    let v = out.now().unwrap().unwrap();
    assert_eq!(v["name"], "Darkroom");
    assert_eq!(v["version"], "1.2.0");
    assert_eq!(v["licences"][0], "GPL-3.0-or-later");
    assert_eq!(v["links"]["repository"], "https://example.org/darkroom");
    assert_eq!(v["links"]["website"], Json::Null);
    assert_eq!(v["egui"], EGUI_VERSION);
    assert_eq!(v["renderer"], "test");
    let text = v["version_info"].as_str().unwrap();
    assert!(text.starts_with("Darkroom 1.2.0\negui-pop "), "{text}");
    assert!(text.contains("\nRenderer: test\n"), "{text}");
}

#[test]
fn the_version_info_is_copied_through_the_clipboard() {
    let app = builder()
        .plugin(ClipboardPlugin)
        .build(&egui::Context::default())
        .unwrap();
    about::copy_version_info(app.handle()).unwrap();
    let item = clipboard::with_owner(app.handle(), |c| c.get(&Slot::User))
        .flatten()
        .expect("copied into the user's slot");
    assert!(
        item.payload
            .as_str()
            .unwrap()
            .starts_with("Darkroom 1.2.0\n")
    );
    // Without the clipboard plugin, straight to the system's.
    let app = builder().build(&egui::Context::default()).unwrap();
    assert!(about::copy_version_info(app.handle()).is_ok());
}

#[test]
fn egui_s_version_follows_the_workspace() {
    let manifest = include_str!("../../../Cargo.toml");
    let line = manifest
        .lines()
        .find(|l| l.starts_with("egui = "))
        .expect("the workspace requires egui");
    assert!(
        line.contains(&format!("version = \"{EGUI_VERSION}\"")),
        "about::EGUI_VERSION ({EGUI_VERSION}) is not the workspace's egui: {line}"
    );
}
