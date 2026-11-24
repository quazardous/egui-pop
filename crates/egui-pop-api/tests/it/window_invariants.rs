//! The window invariants (ADR 0029, slice 2): each of the five rules
//! broken on purpose is said once, as a `window.invariant` event naming
//! the rule and the window and as an error line, and held again once
//! repaired; a healthy run breaks none.
//!
//! These drive real viewports (`egui_pop_test::windows::MultiWindow`),
//! which fails a test on a broken invariant unless the test allows its
//! rule.

use std::time::Duration;

use crate::support::log_capture::LINES;
use egui::ViewportId;
use egui_pop_api::admin::{APP_WINDOW, AdminPlugin, CMD_OPEN};
use egui_pop_api::bus::bus_contract::ListenOpts;
use egui_pop_api::bus::{
    CHILDREN, FOCUS, PopupOpen, Reason, WindowClose, WindowKind, WindowOpen, WindowRequest,
};
use egui_pop_api::window_invariants::{BACKEND, CHILD_ALIVE, OWNER, PAINTS, POPUP_PARENT};
use egui_pop_api::windows::{WindowId, WindowSpec, WindowsPlugin};
use egui_pop_api::{App, AppHandle, DefaultPlugins};
use egui_pop_test::windows::MultiWindow;
use serde_json::{Value as Json, json};

fn multi() -> MultiWindow {
    crate::support::log_capture::install();
    MultiWindow::new(|ctx| {
        App::builder()
            .plugins(DefaultPlugins)
            .plugin(WindowsPlugin)
            .plugin(AdminPlugin)
            .build(ctx)
            .unwrap()
    })
}

/// The `window.invariant` events said.
fn said(app: &AppHandle) -> Vec<Json> {
    app.events_since(Default::default())
        .into_iter()
        .filter(|e| e.name == "window.invariant")
        .map(|e| e.data)
        .collect()
}

/// The one event of `rule`, after checking it is the only one said.
fn said_once(app: &AppHandle, rule: &str) -> Json {
    let said = said(app);
    assert_eq!(said.len(), 1, "said once: {said:?}");
    assert_eq!(said[0]["rule"], rule, "{}", said[0]);
    said[0].clone()
}

/// Whether an error line names the rule and the window.
fn logged(level: log::Level, rule: &str, window: &str) -> bool {
    LINES.lock().unwrap().iter().any(|(l, m)| {
        *l == level && m.contains(&format!("`{rule}`")) && m.contains(&format!("`{window}`"))
    })
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
    m.window_pass(v);
    v
}

fn close_settings(m: &MultiWindow) {
    assert!(m.app().request_window_close(WindowRequest::new(
        APP_WINDOW,
        WindowKind::App,
        "test",
        Reason::Command,
    )));
}

#[test]
fn a_healthy_run_breaks_no_invariant() {
    let mut m = multi();
    let v = open_settings(&mut m);
    m.app()
        .open_window(WindowSpec::new("text", "Text"), |ui, _| {
            ui.label("something");
        });
    for _ in 0..3 {
        m.root_pass();
        m.window_pass(v);
    }
    close_settings(&m);
    for _ in 0..3 {
        m.root_pass();
    }
    assert_eq!(m.app().broken_invariants(), []);
    assert!(said(m.app()).is_empty());
}

/// Rule 1: a viewport the backend shows that the registry does not hold
/// (a window opened past the registry) is said once its grace pass is
/// over, and holds again once it is gone.
#[test]
fn a_viewport_outside_the_registry_breaks_rule_1() {
    let mut m = multi();
    m.allow_broken(BACKEND);
    let stray = ViewportId::from_hash_of("test.stray");
    m.hold_listed(stray);
    m.root_pass();
    assert!(said(m.app()).is_empty(), "within the backend's grace pass");
    m.root_pass();
    let e = said_once(m.app(), BACKEND);
    let window = format!("viewport {stray:?}");
    assert_eq!(e["window"], window.as_str(), "{e}");
    assert!(logged(log::Level::Error, BACKEND, &window));
    m.root_pass();
    assert_eq!(said(m.app()).len(), 1, "said once only");
    assert_eq!(m.app().invariants_broken_now().len(), 1);
}

/// Rule 2: a shown window whose passes paint nothing past the guard's
/// passes and delay.
#[test]
fn a_window_painting_nothing_breaks_rule_2() {
    let mut m = multi();
    m.allow_broken(PAINTS);
    m.app().set_empty_window_delay(Some(Duration::ZERO));
    m.app()
        .open_window(WindowSpec::new("blank", "Blank"), |_ui, _win| {});
    m.root_pass();
    let v = m.app().window_viewport(&WindowId::new("blank")).unwrap();
    for _ in 0..4 {
        m.window_pass(v);
    }
    let e = said_once(m.app(), PAINTS);
    assert_eq!(e["window"], "blank", "{e}");
    assert!(logged(log::Level::Error, PAINTS, "blank"));
    // Closed, it holds again.
    m.app().close_window(&WindowId::new("blank"));
    assert!(m.app().invariants_broken_now().is_empty());
    assert!(logged(log::Level::Info, PAINTS, "blank"));
}

/// Rule 3: a window without an owner (here a path reporting to the
/// registry straight on the bus) is found by the main window's pass.
#[test]
fn a_window_without_an_owner_breaks_rule_3() {
    let mut m = multi();
    m.allow_broken(OWNER);
    m.app().bus().alter::<WindowOpen>(
        WindowRequest::new("ghost", WindowKind::Native, "", Reason::Command),
        Vec::new(),
    );
    m.root_pass();
    let e = said_once(m.app(), OWNER);
    assert_eq!(e["window"], "ghost", "{e}");
    assert!(logged(log::Level::Error, OWNER, "ghost"));
    // Owned now: it holds again.
    m.app().bus().alter::<WindowOpen>(
        WindowRequest::new("ghost", WindowKind::Native, "test", Reason::Command),
        Vec::new(),
    );
    m.root_pass();
    assert!(m.app().invariants_broken_now().is_empty());
    assert!(logged(log::Level::Info, OWNER, "ghost"));
}

/// Rule 4: a popup whose parent window closed under it, said by the
/// close request itself, before any pass.
#[test]
fn a_popup_whose_parent_closed_breaks_rule_4() {
    let mut m = multi();
    m.allow_broken(POPUP_PARENT);
    open_settings(&mut m);
    // Opened while its parent closes, once its children were closed.
    let bus = m.app().bus();
    let opener = bus.clone();
    bus.listen::<WindowClose>(
        "test.late",
        ListenOpts::new()
            .owner("test")
            .after(CHILDREN)
            .before(FOCUS),
        move |req, _| {
            if req.window == APP_WINDOW {
                opener.alter::<PopupOpen>(
                    WindowRequest::new("test.menu", WindowKind::Popup, "test", Reason::User)
                        .parent(APP_WINDOW),
                    Vec::new(),
                );
            }
            Ok(())
        },
    )
    .unwrap();
    m.root_pass();
    assert!(said(m.app()).is_empty(), "its parent is open");
    close_settings(&m);
    let e = said_once(m.app(), POPUP_PARENT);
    assert_eq!(e["window"], "test.menu", "{e}");
    assert_eq!(e["owner"], "test", "{e}");
    assert!(logged(log::Level::Error, POPUP_PARENT, "test.menu"));
}

/// Rule 5: a child window left alive by its parent's close.
#[test]
fn a_child_left_alive_breaks_rule_5() {
    let mut m = multi();
    m.allow_broken(CHILD_ALIVE);
    open_settings(&mut m);
    // Opened while its parent closes, once its children were closed.
    let bus = m.app().bus();
    let opener = bus.clone();
    bus.listen::<WindowClose>(
        "test.late",
        ListenOpts::new()
            .owner("test")
            .after(CHILDREN)
            .before(FOCUS),
        move |req, _| {
            if req.window == APP_WINDOW {
                opener.alter::<WindowOpen>(
                    WindowRequest::new(
                        "test.web",
                        WindowKind::Native,
                        "test.viewer",
                        Reason::Command,
                    )
                    .parent(APP_WINDOW),
                    Vec::new(),
                );
            }
            Ok(())
        },
    )
    .unwrap();
    m.root_pass();
    assert!(said(m.app()).is_empty(), "its parent is open");
    close_settings(&m);
    let e = said_once(m.app(), CHILD_ALIVE);
    assert_eq!(e["window"], "test.web", "{e}");
    assert!(logged(log::Level::Error, CHILD_ALIVE, "test.web"));
    // Said once, whatever the passes after.
    m.root_pass();
    m.root_pass();
    assert_eq!(said(m.app()).len(), 1);
}

/// The test kit fails a test on a broken invariant it did not allow.
#[test]
#[should_panic(expected = "window invariants broken")]
fn the_test_kit_fails_on_a_broken_invariant() {
    let mut m = multi();
    m.app().bus().alter::<WindowOpen>(
        WindowRequest::new("orphan", WindowKind::Native, "", Reason::Command),
        Vec::new(),
    );
    m.root_pass();
}
