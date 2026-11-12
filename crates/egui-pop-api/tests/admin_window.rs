//! The Settings app in its window mode (`admin.window`): an application
//! window of its own (`settings`), opened and closed with the app; its
//! page drawn in the window's own pass; its close button, Escape and the
//! page's Close close the app and only its window; "Pop in" takes it back
//! over the main window.
//!
//! kittest embeds windows in the main one, where the app keeps its overlay:
//! these tests drive real viewports (`egui_pop_test::windows::MultiWindow`).

use egui::{Key, Modifiers, ViewportId};
use egui_pop_api::admin::{self, APP_WINDOW, AdminPlugin, CMD_CLOSE, CMD_OPEN, CMD_POP_OUT};
use egui_pop_api::windows::{WindowId, WindowsPlugin};
use egui_pop_api::{App, Ctx, DefaultPlugins, Plugin, PluginError};
use egui_pop_model::ids::{PluginId, SettingId};
use egui_pop_model::values::Descriptor;
use egui_pop_test::windows::MultiWindow;
use serde_json::json;

const THREADS: SettingId = SettingId::from_static("compute.threads");

struct Compute;

impl Plugin for Compute {
    fn id(&self) -> PluginId {
        PluginId::from_static("compute")
    }

    fn title(&self) -> String {
        "Compute".into()
    }

    fn requires(&self) -> Vec<PluginId> {
        vec![PluginId::from_static("egui_pop.admin")]
    }

    fn connect(&mut self, ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
        ctx.setting(
            THREADS,
            Descriptor::int("Worker threads", 1, 64, 4).description("Threads the pipeline uses."),
        )?;
        Ok(())
    }
}

fn multi() -> MultiWindow {
    MultiWindow::new(|ctx| {
        App::builder()
            .plugins(DefaultPlugins)
            .plugin(WindowsPlugin)
            .plugin(AdminPlugin)
            .plugin(Compute)
            .build(ctx)
            .unwrap()
    })
}

/// Opens the app in its window mode: the window's viewport.
fn open_in_window(m: &mut MultiWindow) -> ViewportId {
    m.app().run(CMD_OPEN, json!({ "mode": "window" }));
    m.root_pass();
    m.root_pass();
    let v = m
        .app()
        .window_viewport(&WindowId::new(APP_WINDOW))
        .expect("the settings window is open");
    m.focus(v);
    m.window_pass(v);
    v
}

#[test]
fn the_window_mode_opens_an_application_window_drawing_the_page() {
    let mut m = multi();
    let v = open_in_window(&mut m);
    assert!(admin::is_open(m.app()));
    assert_eq!(m.declared(), [v].into_iter().collect());
    let listed: Vec<(String, String)> = m
        .app()
        .windows()
        .into_iter()
        .map(|w| (w.id.to_string(), w.title))
        .collect();
    assert_eq!(listed[1], ("settings".to_owned(), "Settings".to_owned()));
    // The page is the window's, not the main window's.
    let names = m.names(v);
    assert!(
        names.iter().any(|n| n == "Search every setting"),
        "the page in its window: {names:?}"
    );
    m.root_pass();
    assert!(
        !m.names(ViewportId::ROOT)
            .iter()
            .any(|n| n == "Search every setting"),
        "nothing of it over the main window"
    );
    // Opened again: the same window, brought forward.
    m.app().run(CMD_OPEN, json!({}));
    m.root_pass();
    assert_eq!(m.declared().len(), 1);
}

#[test]
fn its_close_button_closes_the_app_and_only_its_window() {
    let mut m = multi();
    let v = open_in_window(&mut m);
    m.request_close(v);
    m.window_pass(v);
    m.root_pass();
    assert!(!admin::is_open(m.app()));
    assert!(m.declared().is_empty(), "its window is gone");
    let ids: Vec<String> = m.app().windows().iter().map(|w| w.id.to_string()).collect();
    assert_eq!(ids, ["main"]);
    assert!(!m.app().quitting());
    // Opened again: a window again.
    open_in_window(&mut m);
    assert!(admin::is_open(m.app()));
}

#[test]
fn escape_and_admin_close_close_its_window() {
    let mut m = multi();
    let v = open_in_window(&mut m);
    m.press(v, Key::Escape, Modifiers::NONE);
    m.window_pass(v);
    m.root_pass();
    assert!(!admin::is_open(m.app()));
    assert!(m.declared().is_empty());
    open_in_window(&mut m);
    m.app().run(CMD_CLOSE, json!({}));
    m.root_pass();
    m.root_pass();
    assert!(!admin::is_open(m.app()));
    assert!(m.declared().is_empty());
}

#[test]
fn pop_in_takes_it_back_over_the_main_window() {
    let mut m = multi();
    open_in_window(&mut m);
    m.app().run(CMD_POP_OUT, json!({}));
    m.root_pass();
    m.root_pass();
    assert!(admin::is_open(m.app()), "still open, over the main window");
    assert!(!admin::in_window(m.app()));
    assert!(m.declared().is_empty(), "its window closed");
    m.root_pass();
    assert!(
        m.names(ViewportId::ROOT)
            .iter()
            .any(|n| n == "Search every setting"),
        "the page over the main window"
    );
}
