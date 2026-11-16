//! The drop frames of a detached panel's window, as pictures: a tab
//! dragged from the main window over a detached window paints the drop
//! zone over that window's body; where the system hides window positions
//! (Wayland), each detached window outlines itself once the tab leaves
//! the main window. Each window is a real viewport of the test kit's
//! `MultiWindow`, rendered on its own.

use egui::{Pos2, ViewportId};
use egui_kittest::SnapshotOptions;
use egui_pop::api::workbench;
use egui_pop::api::{App, Ctx, DefaultPlugins, Plugin, PluginError};
use egui_pop::model::ids::{PanelId, PluginId};
use egui_pop::model::panels::{Area, PanelSpec};
use egui_pop_test::snapshot_skip_reason;
use egui_pop_test::windows::MultiWindow;

const LOG: PanelId = PanelId::from_static("demo.log");
const TREE: PanelId = PanelId::from_static("demo.tree");

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
        Ok(())
    }
}

/// The application with the log detached in a window of its own, or
/// `None` where snapshots are not compared.
fn detached() -> Option<(MultiWindow, ViewportId)> {
    if let Some(reason) = snapshot_skip_reason() {
        eprintln!("snapshot skipped: {reason}");
        return None;
    }
    let mut m = MultiWindow::new(|ctx| {
        App::builder()
            .name("Demo")
            .plugins(DefaultPlugins)
            .plugin(Demo)
            .build(ctx)
            .unwrap()
    });
    m.root_pass();
    assert!(m.app().detach_panel(&LOG).unwrap().ok());
    m.root_pass();
    m.root_pass();
    Some((m, workbench::window_id(&LOG)))
}

/// Drags the tree's tab from the main window to `to` (its points), the
/// button held.
fn drag_tree(m: &mut MultiWindow, to: Pos2) {
    let (from, _) = m.node(ViewportId::ROOT, "Tree").expect("the tree's tab");
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
}

fn shot(m: &MultiWindow, v: ViewportId, name: &str) {
    let image = m.render(v).expect("the window renders");
    if let Err(e) =
        egui_kittest::try_image_snapshot_options(&image, name, &SnapshotOptions::default())
    {
        panic!("{e}");
    }
}

#[test]
fn the_detached_window_paints_its_drop_frame() {
    let Some((mut m, v)) = detached() else {
        return;
    };
    m.set_outer_position(ViewportId::ROOT, [0.0, 0.0]);
    m.set_outer_position(v, [700.0, 0.0]);
    m.root_pass();
    drag_tree(&mut m, egui::pos2(900.0, 200.0));
    assert!(m.app().window_drop().is_some());
    shot(&m, v, "window-drop-frame-pop-dark");
}

#[test]
fn the_detached_window_outlines_itself_where_positions_are_hidden() {
    let Some((mut m, v)) = detached() else {
        return;
    };
    m.hide_positions();
    m.root_pass();
    drag_tree(&mut m, egui::pos2(700.0, 200.0));
    assert_eq!(m.app().window_outlines(), [LOG]);
    shot(&m, v, "window-drop-outline-pop-dark");
}
