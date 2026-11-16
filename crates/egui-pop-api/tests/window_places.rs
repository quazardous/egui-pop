//! Where windows come back: the main window, application windows and
//! detached panel windows keep their size, maximized and full-screen
//! states everywhere, and their position where the system tells it.
//!
//! Wayland tells no window's position and sets none
//! (`MultiWindow::hide_positions`): the size and the states must still
//! be kept, and a run there must not lose the position an X11 or Windows
//! run kept (`MultiWindow::set_outer_position`). A kept place off every
//! screen comes back onto one.

use std::path::{Path, PathBuf};

use egui::{Rect, ViewportBuilder, ViewportId, pos2, vec2};
use egui_pop_api::windows::WindowSpec;
use egui_pop_api::{App, AppHandle, Ctx, DefaultPlugins, Plugin, PluginError, window, workbench};
use egui_pop_model::ids::{PanelId, PluginId};
use egui_pop_model::panels::{Area, PanelSpec};
use egui_pop_model::values::Value;
use egui_pop_test::windows::MultiWindow;

const LOG: PanelId = PanelId::from_static("demo.log");

struct Demo;

impl Plugin for Demo {
    fn id(&self) -> PluginId {
        PluginId::from_static("demo")
    }

    fn connect(&mut self, ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
        ctx.panel(PanelSpec::new(LOG, "Log", Area::Bottom), |ui, _| {
            ui.label("log body");
        })?;
        Ok(())
    }
}

/// A fresh persistence directory for one test.
fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "egui-pop-window-places-{name}-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

/// An application run keeping its documents in `dir`.
fn run(dir: &Path) -> MultiWindow {
    let d = dir.to_path_buf();
    let mut m = MultiWindow::new(move |ctx| {
        App::builder()
            .name("Demo")
            .persist(&d)
            .plugins(DefaultPlugins)
            .plugin(Demo)
            .build(ctx)
            .unwrap()
    });
    m.root_pass();
    m
}

fn value(app: &AppHandle, path: &str) -> Option<Value> {
    let store = app.store();
    let s = egui_pop_core::lock(&store);
    s.get(path).map(|e| e.value.clone())
}

fn main_rect(app: &AppHandle) -> Vec<f64> {
    match value(app, window::RECT) {
        Some(Value::Floats(r)) => r,
        other => panic!("the main window's rect: {other:?}"),
    }
}

fn flag(app: &AppHandle, path: &str) -> bool {
    value(app, path) == Some(Value::Bool(true))
}

/// Opens the application window `a` and lets the main pass declare it.
fn open_a(m: &mut MultiWindow) -> ViewportId {
    m.app()
        .open_window(WindowSpec::new("a", "A").size([300.0, 200.0]), |ui, _| {
            ui.label("window A");
        });
    m.root_pass();
    m.app().window_viewport(&"a".into()).expect("a is open")
}

/// Closes the application window `a` through the system's close, and
/// writes the kept places.
fn close_a(m: &mut MultiWindow, va: ViewportId) {
    m.request_close(va);
    m.window_pass(va);
    m.root_pass();
    m.app().flush_documents();
}

fn builder(m: &MultiWindow, v: ViewportId) -> ViewportBuilder {
    m.builder(v).expect("the window was given a builder")
}

/// Detaches the log; the backend lists its window from the second pass.
fn detach_log(m: &mut MultiWindow) -> ViewportId {
    assert!(m.app().detach_panel(&LOG).unwrap().ok());
    m.root_pass();
    m.root_pass();
    let v = workbench::window_id(&LOG);
    assert!(m.shown_immediate().contains(&v));
    v
}

fn log_rect(app: &AppHandle) -> [f32; 4] {
    app.layout()
        .window_of(&LOG)
        .expect("the log is in a window")
        .rect
}

/// Its window closed by the system: the log docks back.
fn close_log(m: &mut MultiWindow, v: ViewportId) {
    m.request_close(v);
    m.root_pass();
    m.root_pass();
    assert!(m.app().layout().auxiliary.is_empty());
}

fn screen() -> Rect {
    Rect::from_min_size(pos2(0.0, 0.0), vec2(1280.0, 800.0))
}

// The main window.

#[test]
fn the_main_window_keeps_its_size_and_maximized_where_positions_are_hidden() {
    let dir = scratch("main-hidden");
    let mut m = run(&dir);
    m.hide_positions();
    m.set_inner_size(ViewportId::ROOT, [900.0, 700.0]);
    m.root_pass();
    let r = main_rect(m.app());
    assert_eq!(r[2..], [900.0, 700.0], "the size is kept");
    assert!(!flag(m.app(), window::PLACED), "no position is kept");
    // Maximized: the state is kept, the normal size stays.
    m.set_maximized(ViewportId::ROOT, true);
    m.set_inner_size(ViewportId::ROOT, [1920.0, 1080.0]);
    m.root_pass();
    assert!(flag(m.app(), window::MAXIMIZED));
    assert_eq!(main_rect(m.app())[2..], [900.0, 700.0]);
    // Full screen too.
    m.set_maximized(ViewportId::ROOT, false);
    m.set_fullscreen(ViewportId::ROOT, true);
    m.root_pass();
    assert!(flag(m.app(), window::FULLSCREEN));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_main_window_keeps_its_position_where_the_system_tells_it() {
    let dir = scratch("main-placed");
    let mut m = run(&dir);
    m.set_outer_position(ViewportId::ROOT, [300.0, 200.0]);
    m.root_pass();
    assert_eq!(main_rect(m.app()), [300.0, 200.0, 640.0, 480.0]);
    assert!(flag(m.app(), window::PLACED));
    // A later pass where the position is not told (Wayland) resizes: the
    // position kept stays, for the next run where it is told.
    m.hide_positions();
    m.set_inner_size(ViewportId::ROOT, [900.0, 700.0]);
    m.root_pass();
    assert_eq!(main_rect(m.app()), [300.0, 200.0, 900.0, 700.0]);
    assert!(flag(m.app(), window::PLACED));
    let _ = std::fs::remove_dir_all(&dir);
}

// Application windows.

#[test]
fn an_app_window_keeps_its_size_and_maximized_where_positions_are_hidden() {
    let dir = scratch("app-hidden");
    {
        let mut m = run(&dir);
        m.hide_positions();
        let va = open_a(&mut m);
        m.set_inner_size(va, [420.0, 260.0]);
        m.window_pass(va);
        m.set_maximized(va, true);
        m.set_inner_size(va, [1600.0, 900.0]);
        m.window_pass(va);
        close_a(&mut m, va);
    }
    let mut m = run(&dir);
    m.hide_positions();
    let va = open_a(&mut m);
    let b = builder(&m, va);
    assert_eq!(b.inner_size, Some(vec2(420.0, 260.0)), "its normal size");
    assert_eq!(b.maximized, Some(true));
    assert_eq!(b.position, None, "no position was kept");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn an_app_window_keeps_full_screen() {
    let dir = scratch("app-fullscreen");
    {
        let mut m = run(&dir);
        m.hide_positions();
        let va = open_a(&mut m);
        m.set_inner_size(va, [420.0, 260.0]);
        m.window_pass(va);
        m.set_fullscreen(va, true);
        m.set_inner_size(va, [1920.0, 1080.0]);
        m.window_pass(va);
        close_a(&mut m, va);
    }
    let mut m = run(&dir);
    let va = open_a(&mut m);
    let b = builder(&m, va);
    assert_eq!(b.fullscreen, Some(true));
    assert_eq!(b.inner_size, Some(vec2(420.0, 260.0)));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn an_app_window_comes_back_where_it_was_moved() {
    let dir = scratch("app-placed");
    {
        let mut m = run(&dir);
        let va = open_a(&mut m);
        m.set_outer_position(va, [300.0, 120.0]);
        m.window_pass(va);
        close_a(&mut m, va);
    }
    // A run where positions are hidden (Wayland) keeps it for the next.
    {
        let mut m = run(&dir);
        m.hide_positions();
        let va = open_a(&mut m);
        m.set_inner_size(va, [500.0, 300.0]);
        m.window_pass(va);
        close_a(&mut m, va);
    }
    let mut m = run(&dir);
    let va = open_a(&mut m);
    let b = builder(&m, va);
    assert_eq!(b.position, Some(pos2(300.0, 120.0)));
    assert_eq!(b.inner_size, Some(vec2(500.0, 300.0)));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn an_app_window_kept_off_every_screen_comes_back_onto_one() {
    let dir = scratch("app-off");
    {
        let mut m = run(&dir);
        let va = open_a(&mut m);
        m.set_outer_position(va, [5000.0, 4000.0]);
        m.window_pass(va);
        close_a(&mut m, va);
    }
    let mut m = run(&dir);
    m.set_screens(&[screen()]);
    let va = open_a(&mut m);
    let p = builder(&m, va).position.expect("a position");
    assert!(screen().contains(p), "{p:?} is on the screen");
    let _ = std::fs::remove_dir_all(&dir);
}

// Detached panel windows.

#[test]
fn a_detached_window_keeps_its_size_and_maximized_where_positions_are_hidden() {
    let dir = scratch("detached-hidden");
    let mut m = run(&dir);
    m.hide_positions();
    let v = detach_log(&mut m);
    let first = log_rect(m.app());
    m.set_inner_size(v, [500.0, 300.0]);
    m.root_pass();
    let r = log_rect(m.app());
    assert_eq!(r[2..], [500.0, 300.0], "the size is kept");
    assert_eq!(r[..2], first[..2], "the position given at first stays");
    m.set_maximized(v, true);
    m.set_inner_size(v, [1600.0, 900.0]);
    m.root_pass();
    m.root_pass();
    assert_eq!(log_rect(m.app())[2..], [500.0, 300.0], "its normal size");
    assert_eq!(builder(&m, v).maximized, Some(true));
    // Closed and detached again: as it was.
    close_log(&mut m, v);
    let v = detach_log(&mut m);
    let b = builder(&m, v);
    assert_eq!(b.inner_size, Some(vec2(500.0, 300.0)));
    assert_eq!(b.maximized, Some(true));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_detached_window_comes_back_where_it_was_moved() {
    let dir = scratch("detached-placed");
    let mut m = run(&dir);
    let v = detach_log(&mut m);
    m.set_outer_position(v, [700.0, 50.0]);
    m.root_pass();
    assert_eq!(log_rect(m.app())[..2], [700.0, 50.0]);
    close_log(&mut m, v);
    let v = detach_log(&mut m);
    assert_eq!(builder(&m, v).position, Some(pos2(700.0, 50.0)));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_detached_window_kept_off_every_screen_comes_back_onto_one() {
    let dir = scratch("detached-off");
    let mut m = run(&dir);
    detach_log(&mut m);
    let mut layout = m.app().layout();
    layout.auxiliary[0].rect = [5000.0, 4000.0, 400.0, 300.0];
    m.app().set_layout(layout);
    m.set_screens(&[screen()]);
    m.root_pass();
    m.root_pass();
    let v = workbench::window_id(&LOG);
    let p = builder(&m, v).position.expect("a position");
    assert!(screen().contains(p), "{p:?} is on the screen");
    let _ = std::fs::remove_dir_all(&dir);
}

// Saying it.

#[test]
fn the_view_menu_says_the_system_places_windows_only_where_positions_are_hidden() {
    let dir = scratch("note");
    let mut m = run(&dir);
    m.root_pass();
    let note = |m: &MultiWindow| {
        let app = m.app();
        app.list_items(window::LIST_PLACEMENT, &app.when_context())
    };
    assert!(note(&m).is_empty(), "positions are told: nothing to say");
    m.hide_positions();
    m.root_pass();
    let items = note(&m);
    assert_eq!(items.len(), 1);
    assert!(items[0].label.contains("Wayland"), "{:?}", items[0].label);
    assert!(!items[0].enabled, "a note, not a command");
    let _ = std::fs::remove_dir_all(&dir);
}
