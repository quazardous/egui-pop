//! The detached panels' windows on the bus (ADR 0029, slice 4): the
//! workbench owns them. Detaching a panel is its window's `window.open`,
//! docking it back (by command, or moved to an area as a dragged tab is)
//! its `window.reparent` into the main window, closing its window (the
//! system's close, or anyone asking) its `window.close`, which docks its
//! panels back; a vetoed close leaves the registry as it was. Each window
//! is a real viewport of the test kit's backend, so the registry is
//! checked against it after every pass.

use std::path::Path;

use egui::{ViewportCommand, ViewportId};
use egui_pop_api::bus::{Reason, STEP_WINDOW_CLOSE, WORKBENCH_OWNER, WindowKind, WindowRequest};
use egui_pop_api::host::Event;
use egui_pop_api::steps::Before;
use egui_pop_api::workbench::{self, STEP_CLOSE_WINDOW};
use egui_pop_api::{App, AppHandle, Ctx, DefaultPlugins, Plugin, PluginError};
use egui_pop_model::ids::{PanelId, PluginId};
use egui_pop_model::panels::{Area, PanelSpec};
use egui_pop_test::windows::MultiWindow;

const LOG: PanelId = PanelId::from_static("demo.log");
const TREE: PanelId = PanelId::from_static("demo.tree");
const LOG_WINDOW: &str = "panel:demo.log";
const TREE_WINDOW: &str = "panel:demo.tree";

/// Two panels; the window's close vetoed by the workbench's step while the
/// context says `busy`, by `window.close`'s step while it says `hold`.
struct Demo;

impl Plugin for Demo {
    fn id(&self) -> PluginId {
        PluginId::from_static("demo")
    }

    fn connect(&mut self, ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
        ctx.panel(PanelSpec::new(LOG, "Log", Area::Bottom), |ui, _| {
            ui.label("log body");
        })?
        .panel(PanelSpec::new(TREE, "Tree", Area::EndSide), |ui, _| {
            ui.label("tree body");
        })?;
        ctx.before(STEP_CLOSE_WINDOW, |app, _| {
            if app.context().get("busy") == "true" {
                Before::Veto("the log is busy".into())
            } else {
                Before::Go
            }
        });
        ctx.before(STEP_WINDOW_CLOSE, |app, args| {
            if args["kind"] == "panel" && app.context().get("hold") == "true" {
                Before::Veto("held".into())
            } else {
                Before::Go
            }
        });
        Ok(())
    }
}

fn multi() -> MultiWindow {
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

fn events(app: &AppHandle, name: &str) -> Vec<Event> {
    app.events_since(Default::default())
        .into_iter()
        .filter(|e| e.name == name)
        .collect()
}

fn panels_listed(app: &AppHandle) -> Vec<(String, String)> {
    app.windows_list()
        .into_iter()
        .filter(|w| w.kind == WindowKind::Panel)
        .map(|w| (w.id, w.owner))
        .collect()
}

fn did(data: &serde_json::Value) -> Vec<String> {
    data.as_array()
        .into_iter()
        .flatten()
        .filter_map(|d| d.as_str().map(String::from))
        .collect()
}

/// Detaches the log and lets the backend show its window: two main
/// passes, the second listing it.
fn detach_log(m: &mut MultiWindow) -> ViewportId {
    assert!(m.app().detach_panel(&LOG).unwrap().ok());
    m.root_pass();
    m.root_pass();
    let v = workbench::window_id(&LOG);
    assert!(m.shown_immediate().contains(&v));
    v
}

/// The window is gone from the registry and from the backend, the log
/// docked in its area.
fn assert_docked(m: &mut MultiWindow, v: ViewportId) {
    assert!(panels_listed(m.app()).is_empty());
    assert!(m.app().layout().auxiliary.is_empty());
    assert_eq!(m.app().layout().area_of(&LOG), Some(Area::Bottom));
    m.root_pass();
    m.root_pass();
    assert!(!m.shown_immediate().contains(&v));
}

#[test]
fn detaching_a_panel_is_its_window_open_owned_by_the_workbench() {
    let mut m = multi();
    let v = detach_log(&mut m);
    assert_eq!(
        panels_listed(m.app()),
        [(LOG_WINDOW.to_string(), WORKBENCH_OWNER.to_string())]
    );
    let opened = events(m.app(), "window.opened");
    assert_eq!(opened.len(), 1);
    assert_eq!(opened[0].data["window"], LOG_WINDOW);
    assert_eq!(opened[0].data["owner"], WORKBENCH_OWNER);
    let did = did(&opened[0].data["did"]);
    assert_eq!(
        did[..2],
        [
            "workbench.window: `demo.log` detached".to_string(),
            "window.registry: holds it".to_string(),
        ],
        "{did:?}"
    );
    assert!(did[2].starts_with("window.invariants: "), "{did:?}");
    // A real viewport, painted in a pass of its own.
    assert!(m.painted(v));
    assert!(m.names(v).iter().any(|n| n == "log body"));
}

#[test]
fn docking_back_by_command_is_its_window_reparent() {
    let mut m = multi();
    let v = detach_log(&mut m);
    assert!(m.app().attach_panel(&LOG).unwrap().ok());
    assert_eq!(events(m.app(), "window.reparent").len(), 1);
    let moved = events(m.app(), "window.moved");
    assert_eq!(moved.len(), 1);
    assert_eq!(moved[0].data["window"], LOG_WINDOW);
    assert_eq!(moved[0].data["to"], "main");
    assert_docked(&mut m, v);
}

#[test]
fn docking_back_by_moving_it_to_an_area_is_its_window_reparent() {
    let mut m = multi();
    let v = detach_log(&mut m);
    // What a tab dragged into an area runs.
    assert!(m.app().move_panel(&LOG, Area::Bottom).unwrap().ok());
    let moved = events(m.app(), "window.moved");
    assert_eq!(moved.len(), 1);
    assert_eq!(moved[0].data["window"], LOG_WINDOW);
    assert_docked(&mut m, v);
}

#[test]
fn closing_its_window_is_its_window_close_and_docks_it_back() {
    let mut m = multi();
    let v = detach_log(&mut m);
    // The system's close (its title bar).
    m.request_close(v);
    m.root_pass();
    assert!(!m.commands(v).contains(&ViewportCommand::CancelClose));
    let closed = events(m.app(), "window.closed");
    assert_eq!(closed.len(), 1);
    assert_eq!(closed[0].data["window"], LOG_WINDOW);
    assert_eq!(closed[0].data["owner"], WORKBENCH_OWNER);
    assert_docked(&mut m, v);
    // Anyone asking `window.close` of it: the workbench docks it back.
    let v = detach_log(&mut m);
    assert!(m.app().request_window_close(WindowRequest::new(
        LOG_WINDOW,
        WindowKind::Panel,
        "test",
        Reason::Command,
    )));
    assert_docked(&mut m, v);
}

#[test]
fn a_vetoed_panel_close_leaves_the_registry_unchanged() {
    let mut m = multi();
    let v = detach_log(&mut m);
    let before = m.app().windows_list();
    // Vetoed by the workbench's step, then by `window.close`'s.
    for key in ["busy", "hold"] {
        m.app().set_context(key, "true");
        m.request_close(v);
        m.root_pass();
        assert!(
            m.commands(v).contains(&ViewportCommand::CancelClose),
            "{key}"
        );
        assert_eq!(m.app().windows_list(), before, "{key}");
        assert_eq!(m.app().layout().auxiliary.len(), 1, "{key}");
        m.root_pass();
        assert!(m.shown_immediate().contains(&v), "{key}");
        m.app().set_context(key, "false");
    }
    assert!(events(m.app(), "window.closed").is_empty());
    let refused = events(m.app(), "window.refused");
    assert_eq!(refused.len(), 1);
    assert_eq!(refused[0].data["window"], LOG_WINDOW);
    assert_eq!(refused[0].data["vetoed_by"], "demo");
}

#[test]
fn a_panel_joining_another_window_closes_its_own() {
    let mut m = multi();
    detach_log(&mut m);
    assert!(m.app().detach_panel(&TREE).unwrap().ok());
    m.root_pass();
    m.root_pass();
    assert_eq!(panels_listed(m.app()).len(), 2);
    assert!(m.app().move_to_window(&TREE, &LOG).unwrap().ok());
    assert_eq!(
        panels_listed(m.app()),
        [(LOG_WINDOW.to_string(), WORKBENCH_OWNER.to_string())]
    );
    let closed = events(m.app(), "window.closed");
    assert_eq!(closed.len(), 1);
    assert_eq!(closed[0].data["window"], TREE_WINDOW);
    m.root_pass();
    m.root_pass();
    assert!(!m.shown_immediate().contains(&workbench::window_id(&TREE)));
}

/// The workbench's detach and dock code makes no raw viewport call: its
/// windows are shown and kept by the bus's panels module.
#[test]
fn the_workbench_has_no_direct_window_path() {
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/workbench.rs");
    let text = std::fs::read_to_string(src).unwrap();
    for raw in [
        "show_viewport",
        "send_viewport_cmd",
        "ViewportCommand",
        "report_window",
    ] {
        assert!(
            !text.contains(raw),
            "workbench.rs uses `{raw}`: ask the bus (`window.open`, `window.reparent`, `window.close`)"
        );
    }
}

/// A layout put in again (what an application does when it re-applies a
/// workspace) is not a close: a detached window it still holds stays
/// open, the same viewport, with no request asked; a layout without it
/// docks the panel back, as its window's `window.reparent`.
#[test]
fn a_layout_put_in_again_keeps_the_detached_window() {
    let mut m = multi();
    let v = detach_log(&mut m);
    let asked = |m: &MultiWindow| {
        ["window.opened", "window.closed", "window.moved"]
            .iter()
            .map(|n| events(m.app(), n).len())
            .sum::<usize>()
    };
    let before = asked(&m);
    let layout = m.app().layout();
    m.app().set_layout(layout.clone());
    assert_eq!(asked(&m), before, "nothing asked: the window stays");
    assert_eq!(
        panels_listed(m.app()),
        [(LOG_WINDOW.to_string(), WORKBENCH_OWNER.to_string())]
    );
    m.root_pass();
    m.root_pass();
    assert!(m.shown_immediate().contains(&v));
    // Without the window: the log docks back, its window reparented.
    let mut docked = layout;
    docked.place(&LOG, Area::Bottom);
    m.app().set_layout(docked);
    assert_eq!(events(m.app(), "window.moved").len(), 1);
    assert_docked(&mut m, v);
}

/// Where the system hides window positions (Wayland), no window sees a
/// tab dragged from the main one while the drag holds the pointer: let go
/// outside the main window, the tab is taken by the detached window that
/// sees the pointer once the drag lets it go, and joins its tabs.
#[test]
fn a_tab_let_go_over_a_detached_window_joins_it_where_positions_are_hidden() {
    let mut m = multi();
    m.hide_positions();
    let v = detach_log(&mut m);
    let (from, _) = m
        .node(ViewportId::ROOT, "Tree")
        .expect("the tree's tab in the main window");
    let from = from.center();
    // Past the main window's right edge (640 points wide): the drag's
    // grab keeps the main window's pointer going there.
    let to = egui::pos2(700.0, from.y);
    let send = |m: &mut MultiWindow, e: egui::Event| {
        m.queue(ViewportId::ROOT, e);
        m.root_pass();
    };
    send(&mut m, egui::Event::PointerMoved(from));
    send(
        &mut m,
        egui::Event::PointerButton {
            pos: from,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: egui::Modifiers::NONE,
        },
    );
    for i in 1..=8 {
        send(
            &mut m,
            egui::Event::PointerMoved(from + (to - from) * (i as f32 / 8.0)),
        );
    }
    assert!(
        m.app().window_drop().is_none(),
        "no window can tell the pointer is over it"
    );
    send(
        &mut m,
        egui::Event::PointerButton {
            pos: to,
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: egui::Modifiers::NONE,
        },
    );
    send(&mut m, egui::Event::PointerGone);
    // The grab ends: the system gives the pointer to the window under it.
    m.queue(v, egui::Event::PointerMoved(egui::pos2(120.0, 200.0)));
    m.root_pass();
    m.root_pass();
    let layout = m.app().layout();
    let window = layout.window_of(&TREE).expect("the tree joined the window");
    assert_eq!(window.panel, LOG);
    assert_eq!(layout.area_of(&TREE), None);
    assert_eq!(layout.auxiliary.len(), 1);
}

/// The rectangles a window's last pass painted over most of its body (a
/// drop zone over the window, the body's own background): those a drag
/// adds are its drop frames.
fn covering(m: &MultiWindow, v: ViewportId) -> usize {
    fn count(shape: &egui::Shape) -> usize {
        match shape {
            egui::Shape::Vec(v) => v.iter().map(count).sum(),
            egui::Shape::Rect(r) => {
                let big = r.rect.width() >= 500.0 && r.rect.height() >= 300.0;
                let seen = r.fill.a() > 0 || (r.stroke.width > 0.0 && r.stroke.color.a() > 0);
                usize::from(big && seen)
            }
            _ => 0,
        }
    }
    m.shapes(v).iter().map(|c| count(&c.shape)).sum()
}

/// Presses the tree's tab in the main window and drags it to `to` (the
/// main window's points), the button held; one more pass shows the
/// windows what the drag reached.
fn drag_tree(m: &mut MultiWindow, to: egui::Pos2) -> egui::Pos2 {
    let (from, _) = m
        .node(ViewportId::ROOT, "Tree")
        .expect("the tree's tab in the main window");
    let from = from.center();
    let send = |m: &mut MultiWindow, e: egui::Event| {
        m.queue(ViewportId::ROOT, e);
        m.root_pass();
    };
    send(m, egui::Event::PointerMoved(from));
    send(
        m,
        egui::Event::PointerButton {
            pos: from,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: egui::Modifiers::NONE,
        },
    );
    for i in 1..=8 {
        send(
            m,
            egui::Event::PointerMoved(from + (to - from) * (i as f32 / 8.0)),
        );
    }
    m.root_pass();
    from
}

/// Lets the dragged tab go at `at` (the main window's points).
fn release_tree(m: &mut MultiWindow, at: egui::Pos2) {
    m.queue(
        ViewportId::ROOT,
        egui::Event::PointerButton {
            pos: at,
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: egui::Modifiers::NONE,
        },
    );
    m.root_pass();
}

/// A tab dragged from the main window over a detached window, where the
/// system tells where windows are: the detached window paints its drop
/// frame over its body, in its own frame, and a drop there joins it.
#[test]
fn a_tab_dragged_over_a_detached_window_paints_its_drop_frame_there() {
    let mut m = multi();
    let v = detach_log(&mut m);
    // The main window at the screen's origin, the log's window right of
    // it (the main one is 640 points wide).
    m.set_outer_position(ViewportId::ROOT, [0.0, 0.0]);
    m.set_outer_position(v, [700.0, 0.0]);
    m.root_pass();
    m.root_pass();
    let rest = covering(&m, v);
    // Over the log's window, at (200, 200) of its own points.
    let to = egui::pos2(900.0, 200.0);
    drag_tree(&mut m, to);
    let over = m
        .app()
        .window_drop()
        .expect("the drag is seen over the window");
    assert_eq!(over.window, LOG);
    assert!(
        covering(&m, v) > rest,
        "the detached window paints its drop frame over its body ({} at rest, {} now)",
        rest,
        covering(&m, v)
    );
    release_tree(&mut m, to);
    m.root_pass();
    let layout = m.app().layout();
    assert_eq!(layout.window_of(&TREE).map(|w| w.panel.clone()), Some(LOG));
}

/// Where the system hides window positions (Wayland), no window can tell
/// the pointer is over it while the drag holds it: once the pointer
/// leaves the main window, each detached window that would take the tab
/// outlines itself, the best hint there is; back in the main window, the
/// outline goes.
#[test]
fn a_tab_dragged_out_of_the_main_window_outlines_the_windows_where_positions_are_hidden() {
    let mut m = multi();
    m.hide_positions();
    let v = detach_log(&mut m);
    m.root_pass();
    let rest = covering(&m, v);
    let to = egui::pos2(700.0, 200.0);
    let from = drag_tree(&mut m, to);
    assert!(
        m.app().window_drop().is_none(),
        "no window can tell it is under the pointer"
    );
    assert_eq!(m.app().window_outlines(), [LOG]);
    assert!(
        covering(&m, v) > rest,
        "the detached window outlines itself ({} at rest, {} now)",
        rest,
        covering(&m, v)
    );
    // Back over the main window: no outline.
    m.queue(ViewportId::ROOT, egui::Event::PointerMoved(from));
    m.root_pass();
    m.root_pass();
    assert_eq!(
        covering(&m, v),
        rest,
        "the outline goes with the pointer back"
    );
    assert!(m.app().window_outlines().is_empty());
}

/// A detached panel's window is drawn by the main window's pass (egui's
/// immediate viewport). With the main window covered by it (GNOME under
/// Wayland sends a window it does not show no frame callback), what
/// happens in the panel's window still runs with no outside event: the
/// loop paints the main window once its frame is owed too long
/// (`main_wake`). Closing it docks it back; detaching it again and
/// docking it by command too.
#[test]
fn a_detached_panel_works_with_the_main_window_covered() {
    let mut m = multi();
    let v = detach_log(&mut m);
    m.run_requested(20);
    m.withhold_frames(ViewportId::ROOT, true);
    // Its title bar's close: the system's event to its window.
    m.request_close(v);
    let ran = m.run_requested(10);
    assert_eq!(
        events(m.app(), "window.closed").len(),
        1,
        "closed with no outside event ({ran:?})"
    );
    m.run_requested(10);
    assert!(!m.shown_immediate().contains(&v), "its window is gone");
    assert_eq!(m.app().layout().area_of(&LOG), Some(Area::Bottom));
    // eframe's loop alone: the close waits for the main window shown.
    m.withhold_frames(ViewportId::ROOT, false);
    let v = detach_log(&mut m);
    m.run_requested(20);
    m.withhold_frames(ViewportId::ROOT, true);
    m.host_wake(false);
    m.request_close(v);
    m.run_requested(10);
    assert_eq!(events(m.app(), "window.closed").len(), 1, "not closed yet");
    m.host_wake(true);
    m.run_requested(10);
    assert_eq!(events(m.app(), "window.closed").len(), 2);
    // Docked back by command from its window's menu, the main window
    // covered.
    m.withhold_frames(ViewportId::ROOT, false);
    let v = detach_log(&mut m);
    m.run_requested(20);
    m.withhold_frames(ViewportId::ROOT, true);
    assert!(m.app().attach_panel(&LOG).unwrap().ok());
    m.run_requested(10);
    assert!(!m.shown_immediate().contains(&v), "docked back");
}
