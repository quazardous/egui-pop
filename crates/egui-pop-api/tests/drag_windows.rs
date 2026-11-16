//! A payload dragged from one window to another (ADR 0029): the window
//! a drag began in keeps the pointer while the button is held, so the
//! window it is let go over learns of it from the drag session.

use std::sync::{Arc, Mutex};

use egui::{Pos2, ViewportId};
use egui_pop_api::workbench;
use egui_pop_api::{App, Ctx, DefaultPlugins, Plugin, PluginError};
use egui_pop_model::ids::{PanelId, PluginId};
use egui_pop_model::panels::{Area, PanelSpec};
use egui_pop_test::windows::MultiWindow;

const SOURCE: PanelId = PanelId::from_static("demo.photos");
const BIN: PanelId = PanelId::from_static("demo.bin");

/// Photos dragged by their ids.
#[derive(Clone, Debug, PartialEq)]
struct Photos(Vec<String>);

type Got = Arc<Mutex<Vec<Photos>>>;

/// A panel of photos one drags out; a bin that takes them.
struct Demo(Got);

impl Plugin for Demo {
    fn id(&self) -> PluginId {
        PluginId::from_static("demo")
    }

    fn connect(&mut self, ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
        let got = self.0.clone();
        ctx.panel(PanelSpec::new(SOURCE, "Photos", Area::StartSide), |ui, _| {
            let r = ui.add(egui::Label::new("Two photos").sense(egui::Sense::drag()));
            if r.drag_started() {
                egui::DragAndDrop::set_payload(
                    ui.ctx(),
                    Photos(vec!["a".into(), "b".into()]),
                );
            }
        })?
        .panel(PanelSpec::new(BIN, "Bin", Area::Bottom), move |ui, _| {
            let r = ui.allocate_response(ui.available_size(), egui::Sense::hover());
            if let Some(p) = r.dnd_release_payload::<Photos>() {
                got.lock().unwrap().push((*p).clone());
            }
        })?;
        Ok(())
    }
}

fn multi() -> (MultiWindow, Got) {
    let got = Got::default();
    let g = got.clone();
    let mut m = MultiWindow::new(move |ctx| {
        App::builder()
            .name("Demo")
            .plugins(DefaultPlugins)
            .plugin(Demo(g))
            .build(ctx)
            .unwrap()
    });
    m.root_pass();
    (m, got)
}

/// Detaches the bin into a window of its own.
fn detach_bin(m: &mut MultiWindow) -> ViewportId {
    assert!(m.app().detach_panel(&BIN).unwrap().ok());
    m.root_pass();
    m.root_pass();
    let v = workbench::window_id(&BIN);
    assert!(m.shown_immediate().contains(&v));
    v
}

fn send(m: &mut MultiWindow, e: egui::Event) {
    m.queue(ViewportId::ROOT, e);
    m.root_pass();
}

fn button(at: Pos2, pressed: bool) -> egui::Event {
    egui::Event::PointerButton {
        pos: at,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: egui::Modifiers::NONE,
    }
}

/// Presses the photos in the main window and drags them to `to` (the
/// main window's points), the button held.
fn drag_photos(m: &mut MultiWindow, to: Pos2) {
    let (from, _) = m
        .node(ViewportId::ROOT, "Two photos")
        .expect("the photos in the main window");
    let from = from.center();
    send(m, egui::Event::PointerMoved(from));
    send(m, button(from, true));
    for i in 1..=8 {
        send(
            m,
            egui::Event::PointerMoved(from + (to - from) * (i as f32 / 8.0)),
        );
    }
    m.root_pass();
}

/// Where the system hides window positions (Wayland): photos dragged
/// from the main window and let go over the bin's window are delivered
/// to the bin once, by the first pass of that window that sees the
/// pointer once the drag lets it go.
#[test]
fn a_payload_let_go_over_another_window_is_delivered_there_once_where_positions_are_hidden() {
    let (mut m, got) = multi();
    m.hide_positions();
    let v = detach_bin(&mut m);
    // Past the main window's right edge (640 points wide).
    let to = egui::pos2(700.0, 200.0);
    drag_photos(&mut m, to);
    send(&mut m, button(to, false));
    send(&mut m, egui::Event::PointerGone);
    // The grab ends: the system gives the pointer to the window under it.
    m.queue(v, egui::Event::PointerMoved(egui::pos2(120.0, 200.0)));
    for _ in 0..4 {
        m.root_pass();
    }
    assert_eq!(
        *got.lock().unwrap(),
        vec![Photos(vec!["a".into(), "b".into()])],
        "delivered once, to the bin"
    );
}
