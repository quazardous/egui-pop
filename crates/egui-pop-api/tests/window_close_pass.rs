//! The close of an application window is one request (ADR 0029): closed
//! by any way, its owner's state is closed by the time `window.closed` is
//! said, and a pass of its window that the backend runs before the main
//! window's (which lets the system window go) still draws something:
//! the window's last page, never an empty frame (a black window without
//! its title bar on GNOME under Wayland).
//!
//! The Settings app in its own window is the case an application saw:
//! its window went at once, the app's state followed at the main
//! window's next pass, up to two seconds later, and the passes in between
//! painted nothing.
//!
//! These drive real viewports (`egui_pop_test::windows::MultiWindow`):
//! kittest embeds windows in the main one.

use egui::{Key, Modifiers, ViewportId};
use egui_pop_api::admin::{self, APP_WINDOW, AdminPlugin, CMD_OPEN};
use egui_pop_api::windows::{WindowId, WindowsPlugin};
use egui_pop_api::{App, AppHandle, DefaultPlugins};
use egui_pop_test::windows::MultiWindow;
use serde_json::json;

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

/// How many times the Settings window was said closed.
fn closed_said(app: &AppHandle) -> usize {
    app.events_since(Default::default())
        .into_iter()
        .filter(|e| e.name == "window.closed" && e.data["window"] == APP_WINDOW)
        .count()
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

/// A way to close the Settings window, with the pass it is seen in.
type Close = fn(&mut MultiWindow, ViewportId);

fn ways() -> [(&'static str, Close); 5] {
    [
        ("its title bar's close", |m, v| {
            m.request_close(v);
            m.window_pass(v);
        }),
        ("the page's Close", |m, v| {
            let (rect, _) = m.node(v, "Close").expect("the page's Close button");
            m.click(v, rect.center());
        }),
        ("Escape", |m, v| {
            m.press(v, Key::Escape, Modifiers::NONE);
            m.window_pass(v);
        }),
        ("the window.close command", |m, _| {
            m.app().run(
                egui_pop_api::windows::CMD_CLOSE,
                json!({ "window": APP_WINDOW }),
            );
            m.root_pass();
        }),
        ("the application closing it", |m, _| {
            m.app().close_window(&WindowId::new(APP_WINDOW));
        }),
    ]
}

#[test]
fn the_settings_window_closes_in_one_step_whatever_closes_it() {
    let mut m = multi();
    for (how, close) in ways() {
        let v = open_settings(&mut m);
        assert!(admin::is_open(m.app()), "{how}: open");
        let said = closed_said(m.app());
        close(&mut m, v);
        // The window said closed, and the app with it: no pass between.
        assert_eq!(
            closed_said(m.app()),
            said + 1,
            "{how}: `window.closed` said once"
        );
        assert!(
            !admin::is_open(m.app()),
            "{how}: the Settings app is closed in the pass its window closed"
        );
        // The backend runs its window once more before the main window's
        // pass lets it go: that pass draws, never an empty frame.
        if m.declared().contains(&v) {
            m.window_pass(v);
            assert!(
                m.painted(v),
                "{how}: a pass of the closed window painted nothing (a black window)"
            );
        }
        for pass in 0..3 {
            m.root_pass();
            assert!(
                m.declared().is_empty(),
                "{how}: declared again in main pass {pass}"
            );
        }
        assert!(!admin::is_open(m.app()), "{how}: still closed");
    }
}
