//! Custom pages in the settings window: mounted through the mount
//! contract (a component that does not fit is refused, reported), placed
//! in the tree, found by the search through their keywords, opened by a
//! command and seen by agents, their settings drawn and reset by the
//! page.

use egui_kittest::Harness;
use egui_kittest::kittest::Queryable as _;
use egui_pop_api::mounts::SETTINGS_PAGE;
use egui_pop_api::settings_pages::SettingsPage;
use egui_pop_api::settings_window::{self, CMD_SETTINGS_PAGE};
use egui_pop_api::{App, Ctx, DefaultPlugins, Plugin, PluginError};
use egui_pop_model::commands::Origin;
use egui_pop_model::ids::{PluginId, SettingId};
use egui_pop_model::mount::{Display, Mountable};
use egui_pop_model::values::{Change, Descriptor, Set, Value};
use serde_json::json;

const THREADS: SettingId = SettingId::from_static("compute.threads");
const PAGE: &str = "compute.devices";

/// A plugin with a Compute page holding its one setting, and a page
/// under Appearance.
struct Compute;

impl Plugin for Compute {
    fn id(&self) -> PluginId {
        PluginId::from_static("compute")
    }

    fn title(&self) -> String {
        "Compute".into()
    }

    fn connect(&mut self, ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
        ctx.setting(
            THREADS,
            Descriptor::int("Worker threads", 1, 64, 4).description("Threads the pipeline uses."),
        )?;
        ctx.settings_page(
            SettingsPage::new(PAGE, "Devices")
                .keywords(&["OpenCL", "GPU"])
                .description("Which device runs the pipeline.")
                .settings(&[THREADS]),
            |ui, app| {
                ui.label("Devices found: none");
                settings_window::setting_row(ui, app, &THREADS);
            },
        )
        .settings_page(
            SettingsPage::new("compute.swatches", "Swatches").under("Appearance"),
            |ui, _| {
                ui.label("The swatches page");
            },
        );
        Ok(())
    }
}

fn harness() -> Harness<'static, App> {
    let app = App::builder()
        .plugins(DefaultPlugins)
        .plugin(Compute)
        .build(&egui::Context::default())
        .unwrap();
    let mut h = Harness::builder()
        .with_size(egui::vec2(1000.0, 700.0))
        .build_ui_state(|ui, app: &mut App| app.show(ui), app);
    h.run();
    h
}

fn open(h: &mut Harness<'_, App>) {
    h.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::Comma);
    h.run();
    assert!(settings_window::is_open(h.state().handle()));
}

/// Types in the search field (labelled by the window's title), clicked
/// first as a person would.
fn search(h: &mut Harness<'_, App>, text: &str) {
    h.get_by_role_and_label(egui::accesskit::Role::TextInput, "Settings")
        .click();
    h.run();
    h.get_by_role_and_label(egui::accesskit::Role::TextInput, "Settings")
        .type_text(text);
    h.run();
}

fn threads(h: &Harness<'_, App>) -> Value {
    egui_pop_core::lock(&h.state().handle().store())
        .get(&format!("settings/{THREADS}"))
        .unwrap()
        .value
        .clone()
}

#[test]
fn a_page_is_mounted_in_the_settings_page_host() {
    let h = harness();
    let app = h.state().handle();
    assert!(app.host_offers().iter().any(|o| o.id == SETTINGS_PAGE));
    let mounted = app.mounts();
    assert!(
        mounted
            .iter()
            .any(|m| m.component == "settings-page:compute.devices" && m.host == SETTINGS_PAGE),
        "{mounted:?}"
    );
    let pages = app.settings_pages();
    assert_eq!(
        pages.iter().map(|p| p.id.as_str()).collect::<Vec<_>>(),
        [PAGE, "compute.swatches"]
    );
    assert!(app.diagnostics().is_empty(), "{:?}", app.diagnostics());
}

/// A page whose component needs wide gamut, which the settings window
/// cannot show.
struct Mastering;

impl Plugin for Mastering {
    fn id(&self) -> PluginId {
        PluginId::from_static("mastering")
    }

    fn connect(&mut self, ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
        ctx.settings_page(
            SettingsPage::new("mastering.scopes", "Scopes").needs(
                Mountable::new("demo.mastering", "Mastering").display(Display::WideRequired),
            ),
            |ui, _| {
                ui.label("never drawn");
            },
        );
        Ok(())
    }
}

#[test]
fn a_page_that_does_not_fit_is_refused_and_reported() {
    let app = App::builder()
        .plugins(DefaultPlugins)
        .plugin(Mastering)
        .build(&egui::Context::default())
        .unwrap();
    let app = app.handle();
    assert!(app.settings_pages().is_empty());
    assert!(
        app.diagnostics()
            .iter()
            .any(|d| d.code == "mount-refused" && d.subject.contains("demo.mastering"))
    );
}

#[test]
fn a_page_appears_in_the_tree_draws_its_body_and_takes_its_settings() {
    let mut h = harness();
    open(&mut h);
    // A page of its own, and one under Appearance.
    assert!(h.query_by_label("Devices").is_some());
    assert!(h.query_by_label("Swatches").is_some());
    // Its setting left the generated sections: no Compute section.
    assert!(h.query_by_label("Compute").is_none());
    h.get_by_label("Devices").click();
    h.run();
    h.run();
    assert!(h.query_by_label("Devices found: none").is_some());
    assert!(h.query_by_label("Reset Worker threads").is_some());
    h.get_by_label("Swatches").click();
    h.run();
    h.run();
    assert!(h.query_by_label("The swatches page").is_some());
    assert!(h.query_by_label("Devices found: none").is_none());
}

#[test]
fn the_search_finds_a_page_by_keyword_and_opens_it() {
    let mut h = harness();
    open(&mut h);
    search(&mut h, "opencl");
    h.run();
    // Found by a keyword alone; the body is drawn once opened.
    assert!(h.query_by_label("Devices found: none").is_none());
    // Other settings match too (the search is fuzzy): the hit may be
    // below the fold.
    h.get_by_label("Open the Devices page").scroll_to_me();
    h.run();
    h.get_by_label("Open the Devices page").click();
    h.run();
    h.run();
    assert!(h.query_by_label("Devices found: none").is_some());
    let w = h
        .state()
        .handle()
        .service(&settings_window::SETTINGS_WINDOW)
        .unwrap();
    assert_eq!(w.shown(), (Some(PAGE.to_string()), String::new()));
}

#[test]
fn the_search_finds_a_page_by_its_settings() {
    let mut h = harness();
    open(&mut h);
    search(&mut h, "worker threads");
    h.run();
    assert!(h.query_by_label("Open the Devices page").is_some());
    assert!(h.query_by_label("Reset Worker threads").is_some());
}

#[test]
fn a_command_opens_a_page_and_agents_see_the_pages() {
    let mut h = harness();
    let out = h.state_mut().run(
        &CMD_SETTINGS_PAGE,
        json!({ "page": PAGE }),
        Origin::Agent("test".into()),
    );
    assert_eq!(out.now(), Some(Ok(json!({ "open": true, "page": PAGE }))));
    h.run();
    h.run();
    assert!(settings_window::is_open(h.state().handle()));
    assert!(h.query_by_label("Devices found: none").is_some());
    let out = h.state_mut().run(
        &CMD_SETTINGS_PAGE,
        json!({ "page": "nowhere" }),
        Origin::Agent("test".into()),
    );
    assert!(matches!(out.now(), Some(Err(_))));
    let snapshot = h.state().handle().debug_snapshot();
    let window = &snapshot["settings_window"];
    assert_eq!(window["open"], json!(true));
    assert_eq!(window["section"], json!(PAGE));
    assert_eq!(window["pages"][0]["id"], json!(PAGE));
    assert_eq!(window["pages"][0]["keywords"], json!(["OpenCL", "GPU"]));
    assert_eq!(window["pages"][0]["settings"], json!([THREADS.as_str()]));
    let tree = window["tree"].as_array().unwrap();
    let at = |key: &str| tree.iter().position(|n| n["key"] == json!(key)).unwrap();
    assert_eq!(tree[at("compute.swatches")]["depth"], json!(1));
    assert!(at("Appearance") < at("compute.swatches"));
    assert!(at(PAGE) < at(settings_window::SHORTCUTS));
}

#[test]
fn the_page_reset_takes_its_settings_back_in_one_step() {
    let mut h = harness();
    let handle = h.state().handle().clone();
    let _ = egui_pop_core::lock(&handle.store()).set(Set::new(
        &format!("settings/{THREADS}"),
        Change::To(Value::Int(12)),
        Origin::User,
    ));
    assert_eq!(threads(&h), Value::Int(12));
    assert!(handle.settings_page(PAGE).unwrap().is_changed(&handle));
    open(&mut h);
    h.get_by_label("Devices").click();
    h.run();
    h.run();
    // The settings window writes into its own history, apart from the
    // application's.
    let settings = egui_pop_model::history::SETTINGS;
    let steps = egui_pop_core::lock(&handle.store())
        .undo_steps_in(&settings)
        .len();
    h.get_by_label("Reset Devices").click();
    h.run();
    assert_eq!(threads(&h), Value::Int(4));
    assert!(!handle.settings_page(PAGE).unwrap().is_changed(&handle));
    assert_eq!(
        egui_pop_core::lock(&handle.store())
            .undo_steps_in(&settings)
            .len(),
        steps + 1
    );
}
