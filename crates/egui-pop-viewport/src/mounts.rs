//! What this crate's components need, and what its overlays offer, in
//! the mount contract's terms (ADR 0013): `docs/hosts.md` and
//! `AppHandle::where_can` are made from these.

use egui_pop_core::model::mount::{ClaimKey, Display, HostOffer, HostSize, Keyboard, Mountable};

/// The image viewport.
pub const VIEWPORT: &str = "viewport";
/// The navigator (a minimap) and zoom control.
pub const NAVIGATOR: &str = "navigator";
/// The node graph editor.
pub const NODE_GRAPH: &str = "node-graph";
/// The curve editor.
pub const CURVES: &str = "curves";
/// The colour wheels.
pub const WHEELS: &str = "wheels";
/// The histogram.
pub const HISTOGRAM: &str = "histogram";
/// A thumbnail.
pub const THUMBNAIL: &str = "thumbnail";
/// The viewport's overlays, as a host.
pub const OVERLAY: &str = "viewport.overlay";

/// The image viewport: tiles from workers, wide gamut wanted.
pub fn viewport() -> Mountable {
    Mountable::new(VIEWPORT, "Image viewport")
        .crate_name("egui-pop-viewport")
        .keyboard(Keyboard::Hover)
        .claims(&[ClaimKey::Space, ClaimKey::Escape])
        .drag()
        .wheel()
        .cursor_images()
        .workers()
        .keep_under_modal()
        .display(Display::WideWanted)
        .fills()
        .role("image")
}

/// The navigator: a minimap sharing the viewport's state.
pub fn navigator() -> Mountable {
    Mountable::new(NAVIGATOR, "Navigator")
        .crate_name("egui-pop-viewport")
        .drag()
        .wheel()
        .min(120.0, 80.0)
        .max(320.0, 214.0)
        .aspect(1.5)
        .role("image")
}

/// The node graph editor: wiring is a session.
pub fn node_graph() -> Mountable {
    Mountable::new(NODE_GRAPH, "Node graph")
        .crate_name("egui-pop-viewport")
        .claims(&[ClaimKey::Escape, ClaimKey::Space])
        .keymap()
        .drag()
        .wheel()
        .fills()
        .session()
        .role("group")
}

/// The curve editor.
pub fn curves() -> Mountable {
    Mountable::new(CURVES, "Curves")
        .crate_name("egui-pop-viewport")
        .claims(&[ClaimKey::Escape])
        .drag()
        .min(120.0, 120.0)
        .max(320.0, 320.0)
        .aspect(1.0)
        .role("image")
}

/// The colour wheels.
pub fn wheels() -> Mountable {
    Mountable::new(WHEELS, "Wheels")
        .crate_name("egui-pop-viewport")
        .claims(&[ClaimKey::Escape])
        .drag()
        .min(48.0, 48.0)
        .preferred(160.0, 160.0)
        .aspect(1.0)
        .role("slider")
}

/// The histogram: read-only unless draggable.
pub fn histogram() -> Mountable {
    Mountable::new(HISTOGRAM, "Histogram")
        .crate_name("egui-pop-viewport")
        .claims(&[ClaimKey::Escape])
        .drag()
        .min(120.0, 40.0)
        .max(600.0, 200.0)
        .role("image")
        .preview()
}

/// A thumbnail: a texture the application gives, clicked.
pub fn thumbnail() -> Mountable {
    Mountable::new(THUMBNAIL, "Thumbnail")
        .crate_name("egui-pop-viewport")
        .keyboard(Keyboard::None)
        .min(16.0, 16.0)
        .preferred(96.0, 72.0)
        .role("image")
        .preview()
}

/// This crate's components.
pub fn components() -> Vec<Mountable> {
    vec![
        viewport(),
        navigator(),
        node_graph(),
        curves(),
        wheels(),
        histogram(),
        thumbnail(),
    ]
}

/// The viewport's overlays: shapes and handles in image coordinates,
/// data, not a `Ui`.
pub fn overlay_host() -> HostOffer {
    HostOffer::new(OVERLAY, "V", "Viewport overlay")
        .source("egui_pop_viewport::overlay")
        .size(HostSize::NoUi)
        .focus(egui_pop_core::model::mount::FocusPolicy::None)
        .window(egui_pop_core::model::mount::Window::FollowsPanel)
        .keymap(false)
        .gives_app(false)
        .scope(false)
        .fault_boundary(false)
}
