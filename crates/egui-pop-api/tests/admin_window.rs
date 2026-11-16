//! The Settings app in its window mode (`admin.window`): an application
//! window of its own (`settings`), opened and closed with the app; its
//! page drawn in the window's own pass; its close button, Escape and the
//! page's Close close the app and only its window; "Pop in" takes it back
//! over the main window.
//!
//! kittest embeds windows in the main one, where the app keeps its overlay:
//! these tests drive real viewports (`egui_pop_test::windows::MultiWindow`).
//!
//! On the bus (ADR 0029, slice 3): every way the window opens is one
//! `window.open` and every way it closes one `window.close`, asked in the
//! call that changed the app, their listeners acting in the same reviewed
//! order whoever asked; the registry and the backend agree after each.

use std::path::Path;

use egui::{Key, Modifiers, ViewportId};
use egui_pop_api::admin::{self, APP_WINDOW, AdminPlugin, CMD_CLOSE, CMD_OPEN, CMD_POP_OUT};
use egui_pop_api::host::Event;
use egui_pop_api::windows::{self, WindowId, WindowsPlugin};
use egui_pop_api::{App, AppHandle, Ctx, DefaultPlugins, Plugin, PluginError};
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

/// A peer of the main window: the main menu is drawn in it too
/// (`WindowSpec::shell_bars`).
#[test]
fn the_window_draws_the_main_menu() {
    let mut m = multi();
    let v = open_in_window(&mut m);
    m.window_pass(v);
    let names = m.names(v);
    for name in ["File", "Window"] {
        assert!(names.iter().any(|n| n == name), "{name} in {names:?}");
    }
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

/// What `window.open`'s listeners did, opening the Settings app's window:
/// the same whatever opened it.
const OPENED: [&str; 3] = [
    "admin.window: the Settings app is open in its window",
    "window.registry: holds it",
    "window.invariants: held",
];

/// What `window.close`'s listeners did, closing it while it was the
/// window worked in: the same whatever closed it.
const CLOSED: [&str; 6] = [
    "window.children: its dialogs cancelled",
    "admin.window: the Settings app closed",
    "window.focus: the window worked in before is active",
    "window.registry: closed",
    "window.place: its place kept",
    "window.invariants: held",
];

fn events(app: &AppHandle, name: &str) -> Vec<Event> {
    app.events_since(Default::default())
        .into_iter()
        .filter(|e| e.name == name)
        .collect()
}

/// What the listeners of the last `name` request did (`window.close`'s
/// step event), and how many there were.
fn did(app: &AppHandle, name: &str) -> (Vec<String>, usize) {
    let all = events(app, name);
    let did = all.last().map_or_else(Vec::new, |e| {
        e.data["result"]["did"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|d| d.as_str().map(String::from))
            .collect()
    });
    (did, all.len())
}

/// What the listeners of the last `window.opened` did.
fn opened(app: &AppHandle) -> (Vec<String>, usize) {
    let all = events(app, "window.opened");
    let did = all.last().map_or_else(Vec::new, |e| {
        e.data["did"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|d| d.as_str().map(String::from))
            .collect()
    });
    (did, all.len())
}

fn held(app: &AppHandle) -> bool {
    app.windows_list().iter().any(|w| w.id == APP_WINDOW)
}

/// Runs a command as the person would: before the main window's next
/// pass, in it.
fn run(m: &mut MultiWindow, command: egui_pop_model::ids::CommandId, args: serde_json::Value) {
    m.app().run(command, args);
    m.root_pass();
}

/// Closed in the call that asked (the window's own pass, or the main
/// window's pass running the command): the app closed, the registry let
/// the window go, said once with `reason`; the main window's pass no
/// longer declares it.
fn closed_in_the_call(m: &mut MultiWindow, closes: usize, reason: &str) {
    let app = m.app().clone();
    assert!(!admin::is_open(&app), "the app closed in the same call");
    assert!(!held(&app), "the registry let its window go");
    let (did, n) = did(&app, "window.close");
    assert_eq!(n, closes, "one `window.close`");
    assert_eq!(did, CLOSED, "its listeners, in their order");
    let closed = events(&app, "window.closed");
    assert_eq!(closed.last().unwrap().data["reason"], reason);
    m.root_pass();
    assert!(m.declared().is_empty(), "the backend agrees: no window");
}

/// Opened in the call that opened the app (the main window's pass
/// running the command): the registry holds it with the app as owner,
/// and that pass declares it.
fn opened_in_the_call(m: &mut MultiWindow, opens: usize) {
    let app = m.app().clone();
    assert!(admin::is_open(&app));
    let owner = app
        .windows_list()
        .into_iter()
        .find(|w| w.id == APP_WINDOW)
        .map(|w| w.owner);
    assert_eq!(
        owner.as_deref(),
        Some(admin::OWNER),
        "the registry holds it"
    );
    assert_eq!(opened(&app), (OPENED.map(String::from).to_vec(), opens));
    let v = app.window_viewport(&WindowId::new(APP_WINDOW)).unwrap();
    assert_eq!(
        m.declared(),
        [v].into_iter().collect(),
        "the backend agrees"
    );
}

#[test]
fn every_way_of_opening_is_one_window_open_in_the_same_call() {
    let mut m = multi();
    // `admin.open` in the window mode.
    run(&mut m, CMD_OPEN, json!({ "mode": "window" }));
    opened_in_the_call(&mut m, 1);
    // Opened again: brought to the front, not opened twice.
    run(&mut m, CMD_OPEN, json!({}));
    assert_eq!(opened(m.app()).1, 1);
    assert_eq!(m.app().windows_list().len(), 2);
    run(&mut m, CMD_CLOSE, json!({}));
    // `app.settings` (Ctrl+,), the mode kept.
    run(
        &mut m,
        egui_pop_api::settings_window::CMD_SETTINGS,
        json!({}),
    );
    opened_in_the_call(&mut m, 2);
    run(&mut m, CMD_CLOSE, json!({}));
    // Over the main window, then "Pop out".
    run(&mut m, CMD_OPEN, json!({ "mode": "overlay" }));
    assert!(!held(m.app()));
    run(&mut m, CMD_POP_OUT, json!({}));
    opened_in_the_call(&mut m, 3);
}

#[test]
fn closed_by_the_system_or_its_title_bar() {
    let mut m = multi();
    let v = open_in_window(&mut m);
    m.request_close(v);
    m.window_pass(v);
    closed_in_the_call(&mut m, 1, "user");
}

#[test]
fn closed_by_the_page_close() {
    let mut m = multi();
    let v = open_in_window(&mut m);
    let (at, _) = m.node(v, "Close").expect("the page's Close");
    m.click(v, at.center());
    closed_in_the_call(&mut m, 1, "user");
}

#[test]
fn closed_by_escape() {
    let mut m = multi();
    let v = open_in_window(&mut m);
    m.press(v, Key::Escape, Modifiers::NONE);
    m.window_pass(v);
    closed_in_the_call(&mut m, 1, "user");
}

#[test]
fn closed_by_the_window_close_command() {
    let mut m = multi();
    open_in_window(&mut m);
    run(&mut m, windows::CMD_CLOSE, json!({ "window": APP_WINDOW }));
    closed_in_the_call(&mut m, 1, "command");
}

#[test]
fn closed_by_admin_close_and_ctrl_comma() {
    let mut m = multi();
    open_in_window(&mut m);
    run(&mut m, CMD_CLOSE, json!({}));
    closed_in_the_call(&mut m, 1, "command");
    open_in_window(&mut m);
    run(
        &mut m,
        egui_pop_api::settings_window::CMD_SETTINGS,
        json!({}),
    );
    closed_in_the_call(&mut m, 2, "command");
}

/// "Pop in": its window closes, the app stays open, over the main window.
#[test]
fn pop_in_is_a_close_the_app_survives() {
    let mut m = multi();
    open_in_window(&mut m);
    run(&mut m, CMD_POP_OUT, json!({}));
    assert!(admin::is_open(m.app()));
    assert!(!held(m.app()));
    let mut moved = CLOSED.map(String::from).to_vec();
    moved[1] = "admin.window: the Settings app moves over the main window".into();
    assert_eq!(did(m.app(), "window.close"), (moved, 1));
    m.root_pass();
    assert!(m.declared().is_empty());
}

/// Its plugin disconnected: its window goes (`owner-gone`), before
/// `plugin.disconnected` is said; its own listener went with it.
#[test]
fn its_plugin_disconnected_closes_its_window() {
    let mut m = multi();
    open_in_window(&mut m);
    m.app()
        .set_plugin_enabled(PluginId::from_static("egui_pop.admin"), false);
    m.root_pass();
    assert!(!held(m.app()));
    let mut gone = CLOSED.map(String::from).to_vec();
    gone.remove(1);
    assert_eq!(did(m.app(), "window.close"), (gone, 1));
    let closed = events(m.app(), "window.closed");
    assert_eq!(closed.last().unwrap().data["reason"], "owner-gone");
    let all = m.app().events_since(Default::default());
    let closed = all.iter().position(|e| e.name == "window.closed");
    let gone = all
        .iter()
        .position(|e| e.name == "plugin.disconnected" && e.data["plugin"] == "egui_pop.admin");
    assert!(
        closed.unwrap() < gone.unwrap(),
        "closed before it is said gone"
    );
    m.root_pass();
    assert!(m.declared().is_empty());
}

/// No path of the Settings app opens, closes or follows its viewport but
/// its requests on the bus.
#[test]
fn the_settings_app_has_no_direct_window_path() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/admin");
    for entry in std::fs::read_dir(&dir).unwrap() {
        let path = entry.unwrap().path();
        let text = std::fs::read_to_string(&path).unwrap();
        for raw in [
            "open_window(",
            "close_window(",
            "send_viewport_cmd",
            "show_viewport",
            "follow_window",
            "window_shown",
        ] {
            assert!(
                !text.contains(raw),
                "{} calls `{raw}`: ask the bus",
                path.display()
            );
        }
    }
}
