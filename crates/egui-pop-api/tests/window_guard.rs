//! Application windows closed by every way stop being declared for good
//! (egui's backend then destroys their system window), a pass of a
//! window that is closed asks for its close and the main window's pass,
//! and a window whose content paints nothing gets an error and the
//! fallback page (the paint guard, per window).
//!
//! These drive real viewports (`egui_pop_test::windows::MultiWindow`):
//! kittest embeds windows in the main one.

use std::time::Duration;

use egui::{Key, Modifiers, ViewportCommand, ViewportId};
use egui_pop_api::admin::{self, APP_WINDOW, AdminPlugin, CMD_CLOSE, CMD_OPEN};
use egui_pop_api::windows::{WindowId, WindowSpec, WindowsPlugin};
use egui_pop_api::{App, AppHandle, DefaultPlugins};
use egui_pop_test::windows::MultiWindow;
use serde_json::{Value as Json, json};

fn multi() -> MultiWindow {
    MultiWindow::new(|ctx| {
        App::builder()
            .plugins(DefaultPlugins)
            .plugin(WindowsPlugin)
            .plugin(AdminPlugin)
            .build(ctx)
            .unwrap()
    })
}

fn events(app: &AppHandle, name: &str) -> Vec<Json> {
    app.events_since(Default::default())
        .into_iter()
        .filter(|e| e.name == name)
        .map(|e| e.data)
        .collect()
}

fn ids(app: &AppHandle) -> Vec<String> {
    app.windows().iter().map(|w| w.id.to_string()).collect()
}

/// The Settings app opened in its own window: its viewport, drawn once.
fn open_settings(m: &mut MultiWindow) -> ViewportId {
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

/// After a close, the window is no longer declared, in the next main
/// pass and in every one after: never declared again (re-created).
fn gone_for_good(m: &mut MultiWindow, how: &str) {
    for pass in 0..4 {
        m.root_pass();
        assert!(
            m.declared().is_empty(),
            "{how}: declared again in main pass {pass}"
        );
    }
    assert!(!admin::is_open(m.app()), "{how}: the app is closed");
    assert_eq!(ids(m.app()), ["main"], "{how}");
}

/// A way to close the Settings window.
type Close = fn(&mut MultiWindow, ViewportId);

#[test]
fn the_settings_window_closed_by_every_way_is_gone_for_good() {
    let mut m = multi();
    let ways: [(&str, Close); 4] = [
        ("close button", |m, v| {
            m.request_close(v);
            m.window_pass(v);
        }),
        ("escape", |m, v| {
            m.press(v, Key::Escape, Modifiers::NONE);
            m.window_pass(v);
        }),
        ("admin.close", |m, _| {
            m.app().run(CMD_CLOSE, json!({}));
        }),
        ("window.close", |m, _| {
            m.app().run(
                egui_pop_api::windows::CMD_CLOSE,
                json!({ "window": APP_WINDOW }),
            );
        }),
    ];
    for (how, close) in ways {
        let v = open_settings(&mut m);
        close(&mut m, v);
        gone_for_good(&mut m, how);
    }
}

#[test]
fn a_pass_of_a_closed_window_asks_for_its_close_and_the_main_pass() {
    let mut m = multi();
    let v = open_settings(&mut m);
    // Closed from elsewhere; the system runs one more pass of its window
    // before the main window's.
    m.app().close_window(&WindowId::new(APP_WINDOW));
    m.window_pass(v);
    assert!(
        m.commands(v).contains(&ViewportCommand::Close),
        "the leftover pass asks its window's close: {:?}",
        m.commands(v)
    );
    gone_for_good(&mut m, "close_window");
}

#[test]
fn a_window_painting_nothing_gets_an_error_and_the_fallback_page() {
    let mut m = multi();
    m.app().set_empty_window_delay(Some(Duration::ZERO));
    m.app()
        .open_window(WindowSpec::new("blank", "Blank"), |_ui, _win| {});
    m.root_pass();
    let v = m
        .app()
        .window_viewport(&WindowId::new("blank"))
        .expect("open");
    for _ in 0..3 {
        m.window_pass(v);
    }
    let said = events(m.app(), "window.painted_nothing");
    assert_eq!(said.len(), 1, "said once: {said:?}");
    assert_eq!(said[0]["window"]["id"], "blank", "{}", said[0]);
    m.window_pass(v);
    assert_eq!(
        events(m.app(), "window.painted_nothing").len(),
        1,
        "said once only"
    );
    let names = m.names(v);
    assert!(
        names.iter().any(|n| n == "Nothing to show here"),
        "the fallback page: {names:?}"
    );
    // Its button closes the window.
    let (rect, _) = m.node(v, "Close").expect("the page's Close button");
    m.click(v, rect.center());
    gone_after_close(&mut m);
}

fn gone_after_close(m: &mut MultiWindow) {
    for _ in 0..2 {
        m.root_pass();
        assert!(m.declared().is_empty(), "closed from the fallback page");
    }
    assert_eq!(ids(m.app()), ["main"]);
}

#[test]
fn a_window_with_content_is_left_alone() {
    let mut m = multi();
    m.app().set_empty_window_delay(Some(Duration::ZERO));
    m.app()
        .open_window(WindowSpec::new("text", "Text"), |ui, _win| {
            ui.label("something");
        });
    m.root_pass();
    let v = m.app().window_viewport(&WindowId::new("text")).unwrap();
    for _ in 0..4 {
        m.window_pass(v);
    }
    assert!(events(m.app(), "window.painted_nothing").is_empty());
    assert!(!m.names(v).iter().any(|n| n == "Nothing to show here"));
}

#[test]
fn a_closed_window_its_backend_keeps_is_said_and_hidden() {
    let mut m = multi();
    m.app().set_empty_window_delay(Some(Duration::ZERO));
    let v = open_settings(&mut m);
    m.hold_listed(v);
    m.app().close_window(&WindowId::new(APP_WINDOW));
    // The main pass right after the close still sees it: not said.
    m.root_pass();
    assert!(events(m.app(), "window.outlived").is_empty());
    assert!(m.declared().is_empty());
    m.root_pass();
    let said = events(m.app(), "window.outlived");
    assert_eq!(said.len(), 1, "{said:?}");
    assert_eq!(said[0]["window"], APP_WINDOW);
    assert!(
        m.declared().contains(&v),
        "declared hidden for one pass, for the backend to hide it"
    );
    m.root_pass();
    assert!(m.declared().is_empty(), "then dropped");
    assert_eq!(events(m.app(), "window.outlived").len(), 1, "said once");
}
