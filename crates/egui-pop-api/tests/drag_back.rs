//! A tab dragged from a detached panel's window back into another window
//! of the application (#5251): the drag session (`bus::drag`) carries it,
//! and the main window's areas are its drop targets, with the same rules
//! as a drag inside the main window. The detached window's tab bar also
//! has a Dock back button.

use egui::{Pos2, ViewportId};
use egui_pop_api::host::Event;
use egui_pop_api::workbench;
use egui_pop_api::{App, AppHandle, Ctx, DefaultPlugins, Plugin, PluginError};
use egui_pop_model::ids::{PanelId, PluginId};
use egui_pop_model::panels::{Area, PanelSpec};
use egui_pop_test::windows::MultiWindow;

const VIEW: PanelId = PanelId::from_static("demo.view");
const TOOL: PanelId = PanelId::from_static("demo.tool");
const PARAMS: PanelId = PanelId::from_static("demo.params");

/// A view in the centre; a tool that goes anywhere; parameters that stay
/// out of the centre.
struct Demo;

impl Plugin for Demo {
    fn id(&self) -> PluginId {
        PluginId::from_static("demo")
    }

    fn connect(&mut self, ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
        ctx.panel(PanelSpec::new(VIEW, "View", Area::Center), |ui, _| {
            ui.label("The view");
        })?
        .panel(PanelSpec::new(TOOL, "Tool", Area::StartSide), |ui, _| {
            ui.label("The tool");
        })?
        .panel(
            PanelSpec::new(PARAMS, "Params", Area::EndSide)
                .only_in(&[Area::StartSide, Area::EndSide]),
            |ui, _| {
                ui.label("The parameters");
            },
        )?;
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

/// Detaches a panel into a window of its own, placed right of the main
/// window (640 points wide) on the screen.
fn detach(m: &mut MultiWindow, panel: &PanelId) -> ViewportId {
    assert!(m.app().detach_panel(panel).unwrap().ok());
    m.root_pass();
    m.root_pass();
    let v = workbench::window_id(panel);
    assert!(m.shown_immediate().contains(&v));
    m.set_outer_position(ViewportId::ROOT, [0.0, 0.0]);
    m.set_outer_position(v, [700.0, 0.0]);
    m.root_pass();
    m.root_pass();
    v
}

fn events(app: &AppHandle, name: &str) -> Vec<Event> {
    app.events_since(Default::default())
        .into_iter()
        .filter(|e| e.name == name)
        .collect()
}

fn button(at: Pos2, pressed: bool) -> egui::Event {
    egui::Event::PointerButton {
        pos: at,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: egui::Modifiers::NONE,
    }
}

fn send(m: &mut MultiWindow, v: ViewportId, e: egui::Event) {
    m.queue(v, e);
    m.root_pass();
}

/// Drags the tab titled `title` in window `v` to `to` (`v`'s points, past
/// its edges: the window keeps the pointer while the button is held), and
/// lets it go there.
fn drag_tab_out(m: &mut MultiWindow, v: ViewportId, title: &str, to: Pos2) {
    let (tab, _) = m
        .node(v, title)
        .unwrap_or_else(|| panic!("the tab `{title}` in its window: {:?}", m.names(v)));
    let from = tab.center();
    send(m, v, egui::Event::PointerMoved(from));
    send(m, v, button(from, true));
    for i in 1..=10 {
        send(
            m,
            v,
            egui::Event::PointerMoved(from + (to - from) * (i as f32 / 10.0)),
        );
    }
    m.root_pass();
    send(m, v, button(to, false));
    send(m, v, egui::Event::PointerGone);
    for _ in 0..4 {
        m.root_pass();
    }
}

/// Dragged out of its window and let go over the main window's centre
/// (the screen's coordinates place it there), the tab docks in the centre
/// and its window closes.
#[test]
fn a_tab_dragged_from_a_detached_window_docks_in_the_main_window() {
    let mut m = multi();
    let v = detach(&mut m, &TOOL);
    // (320, 300) on the screen: the main window's centre, 380 points left
    // of the tool's window.
    drag_tab_out(&mut m, v, "Tool", egui::pos2(-380.0, 300.0));
    let layout = m.app().layout();
    assert!(layout.window_of(&TOOL).is_none(), "its window is gone");
    assert_eq!(layout.area_of(&TOOL), Some(Area::Center));
    let started = events(m.app(), "drag.started");
    assert_eq!(started.len(), 1);
    assert_eq!(started[0].data["kind"], "tab");
    assert_eq!(started[0].data["window"], "panel:demo.tool");
    let dropped = events(m.app(), "drag.dropped");
    assert_eq!(dropped.len(), 1);
    assert_eq!(dropped[0].data["window"], "main");
    assert_eq!(dropped[0].data["target"], "workbench.area.center");
}

/// Let go over the centre, a panel that stays out of it is refused there:
/// the reason is said, and the tab stays in its window.
#[test]
fn a_panel_refused_by_the_area_stays_in_its_window_with_the_reason() {
    let mut m = multi();
    let v = detach(&mut m, &PARAMS);
    drag_tab_out(&mut m, v, "Params", egui::pos2(-380.0, 300.0));
    let layout = m.app().layout();
    assert_eq!(
        layout.window_of(&PARAMS).map(|w| w.panel.clone()),
        Some(PARAMS)
    );
    let refused = events(m.app(), "drag.refused");
    assert_eq!(refused.len(), 1, "{refused:?}");
    assert_eq!(refused[0].data["target"], "workbench.area.center");
    let reason = refused[0].data["reason"].as_str().unwrap_or_default();
    assert!(reason.contains("Params stays out of"), "{reason}");
    let said = events(m.app(), "workbench.refused");
    assert_eq!(said.len(), 1, "the refusal is shown");
    assert!(events(m.app(), "drag.dropped").is_empty());
}

/// Let go over a side area of the main window, the tab docks there.
#[test]
fn a_tab_dragged_back_docks_in_the_area_under_the_pointer() {
    let mut m = multi();
    let v = detach(&mut m, &PARAMS);
    // The main window's left edge: the start side area (closed or open).
    drag_tab_out(&mut m, v, "Params", egui::pos2(-695.0, 300.0));
    let layout = m.app().layout();
    assert!(layout.window_of(&PARAMS).is_none());
    assert_eq!(layout.area_of(&PARAMS), Some(Area::StartSide));
}

/// Let go outside every window: nothing changes, the tab stays.
#[test]
fn a_tab_let_go_outside_every_window_stays() {
    let mut m = multi();
    let v = detach(&mut m, &TOOL);
    drag_tab_out(&mut m, v, "Tool", egui::pos2(300.0, 900.0));
    let layout = m.app().layout();
    assert_eq!(layout.window_of(&TOOL).map(|w| w.panel.clone()), Some(TOOL));
}

/// The detached window's tab bar has a Dock back button: the panel goes
/// back to its declared area.
#[test]
fn the_dock_back_button_returns_the_panel_to_its_area() {
    let mut m = multi();
    let v = detach(&mut m, &PARAMS);
    let (rect, _) = m
        .node(v, "Dock back")
        .unwrap_or_else(|| panic!("a Dock back button: {:?}", m.names(v)));
    let at = rect.center();
    send(&mut m, v, egui::Event::PointerMoved(at));
    send(&mut m, v, button(at, true));
    send(&mut m, v, button(at, false));
    m.root_pass();
    let layout = m.app().layout();
    assert!(layout.window_of(&PARAMS).is_none());
    assert_eq!(layout.area_of(&PARAMS), Some(Area::EndSide));
}
