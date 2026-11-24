//! A window resized by the system is never asked a size back.
//!
//! The backend (eframe) keeps each window's builder and patches it with
//! the one each pass gives: a size, a position or a state that changed
//! is asked of the system again (`ViewportCommand::InnerSize`, …). A
//! builder that follows the window's live size therefore re-asks, one
//! pass late, the size the system just set; while the user drags an edge
//! that pulls the window back, and where the compositor grants another
//! size than the one asked (its own grid, its own constraints) the two
//! answer each other for ever, with no input: the width oscillates
//! during the drag and after it.
//!
//! The rule: a window's size has one owner. The application asks a size
//! when it opens the window or moves it on purpose; a size the system
//! set is followed (kept for the next run), never asked back.

use egui::{ViewportCommand, ViewportId, vec2};
use egui_pop_api::windows::WindowSpec;
use egui_pop_api::{App, Ctx, DefaultPlugins, Plugin, PluginError, workbench};
use egui_pop_model::ids::{PanelId, PluginId};
use egui_pop_model::panels::{Area, PanelSpec};
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

fn run() -> MultiWindow {
    let mut m = MultiWindow::new(|ctx| {
        App::builder()
            .name("Demo")
            .plugins(DefaultPlugins)
            .plugin(Demo)
            .build(ctx)
            .unwrap()
    });
    m.root_pass();
    m
}

/// The commands that ask a window a size or a place.
fn size_asks(sent: &[ViewportCommand]) -> Vec<ViewportCommand> {
    sent.iter()
        .filter(|c| {
            matches!(
                c,
                ViewportCommand::InnerSize(_)
                    | ViewportCommand::MinInnerSize(_)
                    | ViewportCommand::MaxInnerSize(_)
                    | ViewportCommand::OuterPosition(_)
                    | ViewportCommand::Maximized(_)
                    | ViewportCommand::Fullscreen(_)
            )
        })
        .cloned()
        .collect()
}

/// Detaches the log into a window of its own, settled.
fn detach_log(m: &mut MultiWindow) -> ViewportId {
    assert!(m.app().detach_panel(&LOG).unwrap().ok());
    for _ in 0..4 {
        m.root_pass();
    }
    let v = workbench::window_id(&LOG);
    assert!(m.shown_immediate().contains(&v));
    m.take_sent(v);
    v
}

/// Main passes with no input, the detached window's inside each: what
/// the system received for `v` meanwhile.
fn idle(m: &mut MultiWindow, v: ViewportId, passes: usize) -> Vec<ViewportCommand> {
    for _ in 0..passes {
        m.root_pass();
    }
    size_asks(&m.take_sent(v))
}

#[test]
fn a_detached_window_resized_by_the_system_is_not_asked_its_size_back() {
    let mut m = run();
    m.hide_positions();
    let v = detach_log(&mut m);
    m.set_inner_size(v, [500.0, 300.0]);
    assert_eq!(
        idle(&mut m, v, 20),
        vec![],
        "a size the system set is never asked back"
    );
    // The layout still follows it, for the next run.
    let rect = m.app().layout().window_of(&LOG).unwrap().rect;
    assert_eq!([rect[2], rect[3]], [500.0, 300.0]);
}

#[test]
fn a_detached_window_dragged_edge_by_edge_gets_no_size_asked() {
    let mut m = run();
    let v = detach_log(&mut m);
    m.set_outer_position(v, [100.0, 80.0]);
    // A drag: the system sets a new width each pass, the app follows.
    for w in (0..30).map(|i| 400.0 + 7.5 * i as f32) {
        m.set_inner_size(v, [w, 300.0]);
        m.root_pass();
    }
    assert_eq!(
        size_asks(&m.take_sent(v)),
        vec![],
        "a drag is never pulled back"
    );
    assert_eq!(idle(&mut m, v, 20), vec![]);
}

#[test]
fn a_compositor_granting_another_size_than_asked_does_not_loop() {
    let mut m = run();
    m.hide_positions();
    // The compositor obeys a size asked, to its own grid: never the one
    // asked exactly.
    m.grant_sizes(|s| vec2((s.x / 8.0).floor() * 8.0 - 1.0, s.y));
    let v = detach_log(&mut m);
    m.set_inner_size(v, [517.0, 300.0]);
    idle(&mut m, v, 3);
    assert_eq!(
        idle(&mut m, v, 60),
        vec![],
        "settled: no size asked with no input"
    );
}

#[test]
fn a_detached_window_maximized_by_the_system_is_not_asked_it_back() {
    let mut m = run();
    let v = detach_log(&mut m);
    m.set_maximized(v, true);
    m.set_inner_size(v, [1280.0, 800.0]);
    assert_eq!(idle(&mut m, v, 10), vec![]);
    m.set_maximized(v, false);
    m.set_inner_size(v, [600.0, 400.0]);
    assert_eq!(idle(&mut m, v, 10), vec![]);
}

#[test]
fn a_layout_moving_a_detached_window_on_purpose_still_asks_once() {
    let mut m = run();
    let v = detach_log(&mut m);
    m.set_inner_size(v, [500.0, 300.0]);
    idle(&mut m, v, 3);
    // The application gives the window another size (a layout applied).
    let mut layout = m.app().layout();
    layout
        .auxiliary
        .iter_mut()
        .find(|a| a.panel == LOG)
        .unwrap()
        .rect[2] = 700.0;
    m.app().set_layout(layout);
    let asked = idle(&mut m, v, 10);
    assert_eq!(
        asked
            .iter()
            .filter(|c| matches!(c, ViewportCommand::InnerSize(_)))
            .count(),
        1,
        "the size the application chose is asked once: {asked:?}"
    );
}

#[test]
fn an_app_window_resized_by_the_system_is_not_asked_its_size_back() {
    let mut m = run();
    m.hide_positions();
    m.app().open_window(
        WindowSpec::new("a", "A")
            .size([300.0, 200.0])
            .min_size([200.0, 100.0]),
        |ui, _| {
            ui.label("window A");
        },
    );
    m.root_pass();
    let va = m.app().window_viewport(&"a".into()).expect("a is open");
    m.window_pass(va);
    m.root_pass();
    m.take_sent(va);
    for w in [420.0, 431.0, 419.5, 600.0] {
        m.set_inner_size(va, [w, 260.0]);
        m.window_pass(va);
        m.root_pass();
    }
    for _ in 0..20 {
        m.window_pass(va);
        m.root_pass();
    }
    assert_eq!(size_asks(&m.take_sent(va)), vec![]);
}

#[test]
fn the_main_window_resized_by_the_system_is_not_asked_its_size_back() {
    let mut m = run();
    m.hide_positions();
    m.take_sent(ViewportId::ROOT);
    for w in [900.0, 913.0, 897.5, 1200.0] {
        m.set_inner_size(ViewportId::ROOT, [w, 700.0]);
        m.root_pass();
    }
    for _ in 0..20 {
        m.root_pass();
    }
    assert_eq!(size_asks(&m.take_sent(ViewportId::ROOT)), vec![]);
}
