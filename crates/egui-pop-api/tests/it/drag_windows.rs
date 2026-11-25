//! A payload dragged from one window to another (ADR 0029): the window
//! a drag began in keeps the pointer while the button is held, so the
//! window it is let go over learns of it from the drag session
//! (`bus::drag`). The drop is delivered once, to the target under the
//! pointer, and said on the bus: `drag.started`, then `drag.dropped` or
//! `drag.refused` with the reason.

use std::sync::{Arc, Mutex};

use egui::{Pos2, ViewportId};
use egui_pop_api::bus::drag::{self, Seen};
use egui_pop_api::host::Event;
use egui_pop_api::workbench;
use egui_pop_api::{App, AppHandle, Ctx, DefaultPlugins, Plugin, PluginError};
use egui_pop_model::ids::{PanelId, PluginId};
use egui_pop_model::panels::{Area, PanelSpec};
use egui_pop_test::windows::MultiWindow;

const SOURCE: PanelId = PanelId::from_static("demo.photos");
const BIN: PanelId = PanelId::from_static("demo.bin");

/// Photos dragged by their ids.
#[derive(Clone, Debug, PartialEq)]
struct Photos(Vec<String>);

type Got = Arc<Mutex<Vec<Photos>>>;

/// A panel of photos one drags out; a bin that takes them, unless the
/// context says `locked`.
struct Demo(Got);

impl Plugin for Demo {
    fn id(&self) -> PluginId {
        PluginId::from_static("demo")
    }

    fn connect(&mut self, ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
        let got = self.0.clone();
        ctx.panel(
            PanelSpec::new(SOURCE, "Photos", Area::StartSide),
            |ui, _| {
                let r = ui.add(egui::Label::new("Two photos").sense(egui::Sense::drag()));
                if r.drag_started() {
                    drag::start(ui.ctx(), "photos", Photos(vec!["a".into(), "b".into()]));
                }
            },
        )?
        .panel(PanelSpec::new(BIN, "Bin", Area::Bottom), move |ui, app| {
            let r = ui.allocate_response(ui.available_size(), egui::Sense::hover());
            let locked = app.context().get("locked") == "true";
            let seen = drag::target::<Photos>(ui.ctx(), "demo.bin", r.rect, |_| {
                locked.then(|| "the bin is locked".to_string())
            });
            if let Seen::Dropped(p) = seen {
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

fn events(app: &AppHandle, name: &str) -> Vec<Event> {
    app.events_since(Default::default())
        .into_iter()
        .filter(|e| e.name == name)
        .collect()
}

fn photos() -> Vec<Photos> {
    vec![Photos(vec!["a".into(), "b".into()])]
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

/// Lets the photos go at `to` (the main window's points), out of the
/// main window: the grab ends, and the system gives the pointer to the
/// bin's window (`v`), at `there` of its points.
fn let_go_over(m: &mut MultiWindow, to: Pos2, v: ViewportId, there: Pos2) {
    send(m, button(to, false));
    send(m, egui::Event::PointerGone);
    m.queue(v, egui::Event::PointerMoved(there));
    for _ in 0..4 {
        m.root_pass();
    }
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
    let started = events(m.app(), "drag.started");
    assert_eq!(started.len(), 1);
    assert_eq!(started[0].data["kind"], "photos");
    assert_eq!(started[0].data["window"], "main");
    let_go_over(&mut m, to, v, egui::pos2(120.0, 200.0));
    assert_eq!(*got.lock().unwrap(), photos(), "delivered once, to the bin");
    let dropped = events(m.app(), "drag.dropped");
    assert_eq!(dropped.len(), 1);
    assert_eq!(dropped[0].data["kind"], "photos");
    assert_eq!(dropped[0].data["source"], "main");
    assert_eq!(dropped[0].data["window"], "panel:demo.bin");
    assert_eq!(dropped[0].data["target"], "demo.bin");
    assert!(events(m.app(), "drag.refused").is_empty());
    // More passes of the bin's window over it: still once.
    m.queue(v, egui::Event::PointerMoved(egui::pos2(130.0, 210.0)));
    m.root_pass();
    m.root_pass();
    assert_eq!(got.lock().unwrap().len(), 1);
}

/// Let go over a target that refuses it: not delivered, and
/// `drag.refused` names the target and its reason.
#[test]
fn a_refusing_target_is_said_refused_with_its_reason() {
    let (mut m, got) = multi();
    m.hide_positions();
    m.app().set_context("locked", "true");
    let v = detach_bin(&mut m);
    let to = egui::pos2(700.0, 200.0);
    drag_photos(&mut m, to);
    let_go_over(&mut m, to, v, egui::pos2(120.0, 200.0));
    assert!(got.lock().unwrap().is_empty());
    let refused = events(m.app(), "drag.refused");
    assert_eq!(refused.len(), 1);
    assert_eq!(refused[0].data["window"], "panel:demo.bin");
    assert_eq!(refused[0].data["target"], "demo.bin");
    assert_eq!(refused[0].data["reason"], "the bin is locked");
    assert!(events(m.app(), "drag.dropped").is_empty());
}

/// Where the system tells where windows are: the bin's window sees the
/// photos over it while the drag holds the pointer, and the release
/// delivers them there, in the pass it happens.
#[test]
fn a_payload_is_delivered_to_the_window_under_the_pointer_where_positions_are_known() {
    let (mut m, got) = multi();
    let v = detach_bin(&mut m);
    // The main window at the screen's origin, the bin's window right of
    // it (the main one is 640 points wide).
    m.set_outer_position(ViewportId::ROOT, [0.0, 0.0]);
    m.set_outer_position(v, [700.0, 0.0]);
    m.root_pass();
    m.root_pass();
    // Over the bin's window, at (200, 200) of its own points.
    let to = egui::pos2(900.0, 200.0);
    drag_photos(&mut m, to);
    send(&mut m, button(to, false));
    m.root_pass();
    assert_eq!(*got.lock().unwrap(), photos());
    let dropped = events(m.app(), "drag.dropped");
    assert_eq!(dropped.len(), 1);
    assert_eq!(dropped[0].data["window"], "panel:demo.bin");
}

/// Let go outside every window, with no window seeing the pointer: not
/// delivered, said refused.
#[test]
fn a_payload_let_go_nowhere_is_said_refused() {
    let (mut m, got) = multi();
    m.hide_positions();
    detach_bin(&mut m);
    let to = egui::pos2(700.0, 200.0);
    drag_photos(&mut m, to);
    send(&mut m, button(to, false));
    send(&mut m, egui::Event::PointerGone);
    for _ in 0..20 {
        m.root_pass();
    }
    assert!(got.lock().unwrap().is_empty());
    let refused = events(m.app(), "drag.refused");
    assert_eq!(refused.len(), 1);
    assert_eq!(refused[0].data["reason"], "let go outside every window");
    assert!(refused[0].data["target"].is_null());
}

/// A tab goes through the same session: dragged out of the main window
/// and let go over the bin's window, it joins the window's tabs, said as
/// a drag of kind `tab` dropped on that window.
#[test]
fn a_tab_is_a_drag_of_the_same_session() {
    let (mut m, _) = multi();
    m.hide_positions();
    let v = detach_bin(&mut m);
    let (from, _) = m
        .node(ViewportId::ROOT, "Photos")
        .expect("the photos' tab in the main window");
    let from = from.center();
    let to = egui::pos2(700.0, from.y);
    send(&mut m, egui::Event::PointerMoved(from));
    send(&mut m, button(from, true));
    for i in 1..=8 {
        send(
            &mut m,
            egui::Event::PointerMoved(from + (to - from) * (i as f32 / 8.0)),
        );
    }
    let_go_over(&mut m, to, v, egui::pos2(120.0, 200.0));
    let layout = m.app().layout();
    assert_eq!(
        layout.window_of(&SOURCE).map(|w| w.panel.clone()),
        Some(BIN)
    );
    let started = events(m.app(), "drag.started");
    assert_eq!(started.len(), 1);
    assert_eq!(started[0].data["kind"], "tab");
    let dropped = events(m.app(), "drag.dropped");
    assert_eq!(dropped.len(), 1);
    assert_eq!(dropped[0].data["kind"], "tab");
    assert_eq!(dropped[0].data["target"], "panel:demo.bin");
}

/// The reviewed order of the drag points' listeners.
#[test]
fn the_drag_points_run_in_their_reviewed_order() {
    let (m, _) = multi();
    let dump = m.app().bus_order_of(&egui_pop_api::bus::DRAG_POINTS);
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/bus/drag-points.txt");
    if std::env::var_os("UPDATE_BUS_ORDER").is_some() {
        std::fs::write(&path, &dump).unwrap();
    }
    let reviewed = std::fs::read_to_string(&path).unwrap_or_default();
    assert_eq!(
        dump,
        reviewed,
        "review it, then UPDATE_BUS_ORDER=1 writes {}",
        path.display()
    );
}
