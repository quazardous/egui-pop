//! A detached panel's window gets its own image surface: its window is
//! an immediate viewport, painted inside the main window's pass, whose
//! handles the runner pairs from the nested pass and the window's own
//! redraw (`runner::host`, as a host running its own loop calls it); the
//! workbench builds it transparent on Wayland, opaque elsewhere with the
//! reason. The windows are the test kit's (`MultiWindow`), their winit
//! ids and the display made up: nothing is drawn on a real compositor
//! (no colour-managed Wayland session here), the offer is a fake one.

#![cfg(feature = "wayland")]

use std::ptr::NonNull;
use std::sync::Arc;

use egui::ViewportId;
use egui_pop_api::bus::WindowKind;
use egui_pop_api::runner::{self, host};
use egui_pop_api::workbench;
use egui_pop_api::{App, Ctx, DefaultPlugins, Plugin, PluginError};
use egui_pop_core::window_surface::{self, Transparency};
use egui_pop_model::ids::{PanelId, PluginId};
use egui_pop_model::panels::{Area, PanelSpec};
use egui_pop_test::windows::MultiWindow;
use egui_pop_viewport::display::{
    self, ColorSpace, DisplayTarget, Primaries, SurfaceFormat, Transfer,
};
use egui_pop_viewport::image_surface::raw_window_handle::{
    RawDisplayHandle, RawWindowHandle, WaylandDisplayHandle, XlibDisplayHandle,
};
use egui_pop_viewport::image_surface::{self, Offer, Options, Route};
use egui_pop_winit::Display;
use winit::window::WindowId;

const VIEWER: PanelId = PanelId::from_static("demo.viewer");
/// The made-up winit ids (on Wayland, the `wl_surface`).
const MAIN: u64 = 0x1000;
const PANEL: u64 = 0x2000;

struct Demo;

impl Plugin for Demo {
    fn id(&self) -> PluginId {
        PluginId::from_static("demo")
    }

    fn connect(&mut self, ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
        ctx.panel(PanelSpec::new(VIEWER, "Viewer", Area::Bottom), |ui, _| {
            ui.label("image");
        })?;
        Ok(())
    }
}

fn wayland() -> RawDisplayHandle {
    RawDisplayHandle::Wayland(WaylandDisplayHandle::new(NonNull::<u8>::dangling().cast()))
}

fn x11() -> RawDisplayHandle {
    RawDisplayHandle::Xlib(XlibDisplayHandle::new(None, 0))
}

/// The application, its runner's hooks attached as a host's loop
/// attaches them, on `display`; the main window painted once.
fn multi(display: RawDisplayHandle) -> MultiWindow {
    let mut m = MultiWindow::new(|ctx| {
        App::builder()
            .name("Demo")
            .plugins(DefaultPlugins)
            .plugin(Demo)
            .transparent_panel_windows(true)
            .build(ctx)
            .unwrap()
    });
    host::detach();
    host::set_display(display);
    let main = Display::from_raw(display)
        .window(WindowId::from(MAIN))
        .expect("a window on this display");
    host::set_main_window(WindowId::from(MAIN), Arc::new(main));
    host::attach(m.ctx());
    root_redraw(&mut m);
    m
}

/// The main window's redraw, as the loop hands it to eframe.
fn root_redraw(m: &mut MultiWindow) {
    host::redraw(WindowId::from(MAIN), || m.root_pass());
}

/// Detaches the viewer: its window shows in the main window's passes.
fn detach(m: &mut MultiWindow) -> ViewportId {
    assert!(m.app().detach_panel(&VIEWER).unwrap().ok());
    root_redraw(m);
    root_redraw(m);
    let v = workbench::window_id(&VIEWER);
    assert!(m.shown_immediate().contains(&v));
    v
}

fn rec2020() -> DisplayTarget {
    DisplayTarget {
        primaries: Primaries::Bt2020,
        transfer: Transfer::Gamma22,
        ..DisplayTarget::srgb()
    }
}

/// A compositor whose driver offers a wide colour space.
fn wide_offer() -> Offer {
    Offer {
        wsi: vec![
            (SurfaceFormat::Rgba16Float, ColorSpace::Srgb),
            (SurfaceFormat::Rgba16Float, ColorSpace::ExtendedSrgbLinear),
        ],
        ..Offer::default()
    }
}

fn record(m: &MultiWindow, v: ViewportId) -> egui_pop_api::bus::WindowRecord {
    m.app()
        .windows_list()
        .into_iter()
        .find(|w| w.kind == WindowKind::Panel && w.viewport_id() == Some(v))
        .expect("the viewer's window listed")
}

#[test]
fn a_detached_panel_s_window_has_its_handles_and_its_own_image_surface() {
    let mut m = multi(wayland());
    assert_eq!(runner::ids_verified(), Some(true));
    let v = detach(&mut m);
    // Built transparent on Wayland, and says so.
    assert_eq!(m.builder(v).and_then(|b| b.transparent), Some(true));
    assert_eq!(
        window_surface::transparency(m.ctx(), v),
        Some(Transparency::Transparent)
    );
    // Painted inside the main window's pass, its window not known yet.
    assert!(runner::window(v).is_none());
    assert!(!record(&m, v).handles);
    image_surface::set_window_source(m.ctx(), Options::default(), runner::window);
    display::want(m.ctx(), v, rec2020());
    let why = image_surface::negotiate_window(m.ctx(), v).why.unwrap();
    assert!(why.contains("not known yet"), "{why}");

    // Its window's redraw runs no pass (eframe repaints the parent): the
    // window is the viewport's.
    host::redraw(WindowId::from(PANEL), || ());
    let window = runner::window(v).expect("the viewer's window");
    match window.raw_window() {
        Some(RawWindowHandle::Wayland(h)) => assert_eq!(h.surface.as_ptr() as u64, PANEL),
        other => panic!("{other:?}"),
    }
    assert!(runner::windows().contains(&v));
    let r = record(&m, v);
    assert!(r.handles);
    assert_eq!(r.transparency, Some(Transparency::Transparent));
    // Handles known: what is missing now is the GPU (no `install` here),
    // not the window.
    let why = image_surface::negotiate_window(m.ctx(), v).why.unwrap();
    assert!(why.contains("not installed"), "{why}");

    // Its own surface, on the fake compositor's offer: wide, the main
    // window's untouched.
    image_surface::install_offer(m.ctx(), v, wide_offer());
    let n = image_surface::negotiate_window(m.ctx(), v);
    assert!(matches!(n.route, Route::Wsi(_)), "{n:?}");
    assert_eq!(image_surface::windows(m.ctx()), vec![v]);
    assert_eq!(record(&m, v).surface.unwrap()["route"], "wsi");

    // The window destroyed: its handles go.
    host::window_event(WindowId::from(PANEL), &winit::event::WindowEvent::Destroyed);
    assert!(runner::window(v).is_none());
    host::detach();
}

#[test]
fn a_detached_panel_s_window_on_x11_is_opaque_and_says_why() {
    let mut m = multi(x11());
    let v = detach(&mut m);
    assert_ne!(m.builder(v).and_then(|b| b.transparent), Some(true));
    let t = window_surface::transparency(m.ctx(), v).expect("said");
    let why = t.why_opaque().expect("opaque").to_string();
    assert!(why.contains("X11") && why.contains("Wayland"), "{why}");
    assert_eq!(record(&m, v).transparency, Some(t));
    // Even with its handles, no image surface: the window's reason.
    host::redraw(WindowId::from(PANEL), || ());
    assert!(record(&m, v).handles);
    image_surface::set_window_source(m.ctx(), Options::default(), runner::window);
    display::want(m.ctx(), v, rec2020());
    let n = image_surface::negotiate_window(m.ctx(), v);
    assert_eq!(n.route, Route::Window);
    let said = n.why.unwrap();
    assert!(said.contains("opaque") && said.contains("X11"), "{said}");
    assert!(image_surface::windows(m.ctx()).is_empty());
    host::detach();
}
