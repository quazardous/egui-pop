//! The protocol side: the globals, the colour manager's announcements,
//! the preferred description, and the image subsurfaces.

use std::collections::HashMap;
use std::fmt;
use std::sync::Arc;

use raw_window_handle::{HasDisplayHandle, HasWindowHandle, RawDisplayHandle, RawWindowHandle};
use wayland_client::globals::{GlobalList, GlobalListContents, registry_queue_init};
use wayland_client::protocol::wl_compositor::WlCompositor;
use wayland_client::protocol::wl_region::WlRegion;
use wayland_client::protocol::wl_registry::WlRegistry;
use wayland_client::protocol::wl_subcompositor::WlSubcompositor;
use wayland_client::protocol::wl_subsurface::WlSubsurface;
use wayland_client::protocol::wl_surface::WlSurface;
use wayland_client::{Connection, Dispatch, EventQueue, Proxy, QueueHandle, delegate_noop};
use wayland_protocols::wp::color_management::v1::client::{
    wp_color_management_surface_feedback_v1::{self, WpColorManagementSurfaceFeedbackV1},
    wp_color_management_surface_v1::WpColorManagementSurfaceV1,
    wp_color_manager_v1::{self, WpColorManagerV1},
    wp_image_description_creator_params_v1::WpImageDescriptionCreatorParamsV1,
    wp_image_description_info_v1::{self, WpImageDescriptionInfoV1},
    wp_image_description_v1::{self, WpImageDescriptionV1},
};
use wayland_protocols::wp::viewporter::client::{
    wp_viewport::WpViewport, wp_viewporter::WpViewporter,
};

use crate::sys::{self, Keep, SurfaceHandle};
use crate::{
    Capabilities, Description, Feature, Luminances, NamedPrimaries, NamedTransfer, Unavailable,
    chromaticities,
};

/// Roundtrips allowed while waiting for the compositor (a description
/// ready, the preferred one's information).
const ROUNDTRIPS: usize = 8;

/// What an image description created here is for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Purpose {
    /// A tag for image surfaces.
    Tag,
    /// The compositor's preferred description for the window.
    Preferred,
}

/// An image description's state.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Status {
    Ready,
    Failed(String),
}

#[derive(Default)]
struct State {
    caps: Capabilities,
    status: HashMap<ObjectKey, Status>,
    /// The preferred description being read, and the information so far.
    reading: Option<(WpImageDescriptionV1, Description)>,
    preferred: Option<Description>,
    /// Bumped each time `preferred` changes.
    revision: u64,
    /// The compositor said the preferred description changed.
    changed: bool,
    preferred_error: Option<String>,
}

type ObjectKey = wayland_client::backend::ObjectId;

impl Dispatch<WlRegistry, GlobalListContents> for State {
    fn event(
        _: &mut Self,
        _: &WlRegistry,
        _: <WlRegistry as Proxy>::Event,
        _: &GlobalListContents,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

delegate_noop!(State: WlCompositor);
delegate_noop!(State: WlSubcompositor);
delegate_noop!(State: WlSubsurface);
delegate_noop!(State: WlRegion);
delegate_noop!(State: ignore WlSurface);
delegate_noop!(State: WpViewporter);
delegate_noop!(State: WpViewport);
delegate_noop!(State: WpColorManagementSurfaceV1);
delegate_noop!(State: WpImageDescriptionCreatorParamsV1);

impl Dispatch<WpColorManagerV1, ()> for State {
    fn event(
        state: &mut Self,
        _: &WpColorManagerV1,
        event: wp_color_manager_v1::Event,
        (): &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        let caps = &mut state.caps;
        match event {
            wp_color_manager_v1::Event::SupportedIntent { render_intent } => {
                caps.intents.push(u32::from(render_intent));
            }
            wp_color_manager_v1::Event::SupportedFeature { feature } => {
                caps.features.push(Feature::from_code(u32::from(feature)));
            }
            wp_color_manager_v1::Event::SupportedTfNamed { tf } => {
                caps.transfers.push(NamedTransfer::from_code(u32::from(tf)));
            }
            wp_color_manager_v1::Event::SupportedPrimariesNamed { primaries } => {
                caps.primaries
                    .push(NamedPrimaries::from_code(u32::from(primaries)));
            }
            _ => {}
        }
    }
}

impl Dispatch<WpColorManagementSurfaceFeedbackV1, ()> for State {
    fn event(
        state: &mut Self,
        _: &WpColorManagementSurfaceFeedbackV1,
        event: wp_color_management_surface_feedback_v1::Event,
        (): &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let wp_color_management_surface_feedback_v1::Event::PreferredChanged { .. } = event {
            state.changed = true;
        }
    }
}

impl Dispatch<WpImageDescriptionV1, Purpose> for State {
    fn event(
        state: &mut Self,
        description: &WpImageDescriptionV1,
        event: wp_image_description_v1::Event,
        purpose: &Purpose,
        _: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        let status = match event {
            wp_image_description_v1::Event::Ready { .. } => Status::Ready,
            wp_image_description_v1::Event::Failed { cause, msg } => {
                Status::Failed(format!("{msg} (cause {})", u32::from(cause)))
            }
            _ => return,
        };
        if *purpose == Purpose::Preferred {
            match &status {
                Status::Ready => {
                    let _info = description.get_information(qh, ());
                }
                Status::Failed(e) => {
                    state.preferred_error = Some(e.clone());
                    if let Some((d, _)) = state.reading.take() {
                        d.destroy();
                    }
                }
            }
        }
        state.status.insert(description.id(), status);
    }
}

impl Dispatch<WpImageDescriptionInfoV1, ()> for State {
    fn event(
        state: &mut Self,
        _: &WpImageDescriptionInfoV1,
        event: wp_image_description_info_v1::Event,
        (): &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        use wp_image_description_info_v1::Event as E;
        if let E::Done = event {
            if let Some((description, info)) = state.reading.take() {
                description.destroy();
                state.preferred = Some(info);
                state.preferred_error = None;
                state.revision += 1;
            }
            return;
        }
        let Some((_, d)) = state.reading.as_mut() else {
            return;
        };
        match event {
            E::IccFile { .. } => d.icc = true,
            E::Primaries {
                r_x,
                r_y,
                g_x,
                g_y,
                b_x,
                b_y,
                w_x,
                w_y,
            } => d.primaries = Some(chromaticities([r_x, r_y, g_x, g_y, b_x, b_y, w_x, w_y])),
            E::PrimariesNamed { primaries } => {
                d.primaries_named = Some(NamedPrimaries::from_code(u32::from(primaries)));
            }
            E::TfPower { eexp } => d.tf_power = Some(f64::from(eexp) / 10_000.0),
            E::TfNamed { tf } => d.transfer = Some(NamedTransfer::from_code(u32::from(tf))),
            E::Luminances {
                min_lum,
                max_lum,
                reference_lum,
            } => {
                d.luminances = Some(Luminances {
                    min: f64::from(min_lum) / 10_000.0,
                    max: f64::from(max_lum),
                    reference: f64::from(reference_lum),
                });
            }
            E::TargetPrimaries {
                r_x,
                r_y,
                g_x,
                g_y,
                b_x,
                b_y,
                w_x,
                w_y,
            } => {
                d.target_primaries = Some(chromaticities([r_x, r_y, g_x, g_y, b_x, b_y, w_x, w_y]));
            }
            E::TargetLuminance { min_lum, max_lum } => {
                d.target_luminance = Some([f64::from(min_lum) / 10_000.0, f64::from(max_lum)]);
            }
            E::TargetMaxCll { max_cll } => d.target_max_cll = Some(max_cll),
            E::TargetMaxFall { max_fall } => d.target_max_fall = Some(max_fall),
            _ => {}
        }
    }
}

/// The compositor, as seen from a window: its globals, its colour
/// manager's announcements, the preferred description for the window.
pub struct Display {
    conn: Connection,
    queue: EventQueue<State>,
    qh: QueueHandle<State>,
    state: State,
    _globals: GlobalList,
    parent: WlSurface,
    compositor: Option<WlCompositor>,
    subcompositor: Option<WlSubcompositor>,
    viewporter: Option<WpViewporter>,
    manager: Option<WpColorManagerV1>,
    feedback: Option<WpColorManagementSurfaceFeedbackV1>,
    descriptions: HashMap<(u32, u32), WpImageDescriptionV1>,
}

impl fmt::Debug for Display {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Display")
            .field("capabilities", &self.capabilities())
            .field("subcompositor", &self.subcompositor.is_some())
            .field("viewporter", &self.viewporter.is_some())
            .field("preferred", &self.state.preferred)
            .finish_non_exhaustive()
    }
}

impl Display {
    /// Joins the connection of a Wayland window (eframe's creation
    /// context or frame gives its handles), binds the compositor's
    /// globals on a queue of its own, and reads the colour manager's
    /// announcements and the preferred description for the window.
    pub fn attach(
        window: &(impl HasWindowHandle + HasDisplayHandle),
    ) -> Result<Display, Unavailable> {
        let display = window
            .display_handle()
            .map_err(|e| Unavailable::Handle(e.to_string()))?
            .as_raw();
        let surface = window
            .window_handle()
            .map_err(|e| Unavailable::Handle(e.to_string()))?
            .as_raw();
        let (RawDisplayHandle::Wayland(display), RawWindowHandle::Wayland(surface)) =
            (display, surface)
        else {
            return Err(Unavailable::NotWayland);
        };
        let conn = sys::connection(display.display);
        let parent = sys::foreign_surface(&conn, surface.surface).map_err(Unavailable::Handle)?;
        let (globals, queue) = registry_queue_init::<State>(&conn)
            .map_err(|e| Unavailable::Connection(e.to_string()))?;
        let qh = queue.handle();
        let compositor = globals.bind::<WlCompositor, _, _>(&qh, 1..=4, ()).ok();
        let subcompositor = globals.bind::<WlSubcompositor, _, _>(&qh, 1..=1, ()).ok();
        let viewporter = globals.bind::<WpViewporter, _, _>(&qh, 1..=1, ()).ok();
        // Version 1: what a Vulkan driver binding version 1 sees too.
        let manager = globals.bind::<WpColorManagerV1, _, _>(&qh, 1..=1, ()).ok();
        let version = globals.contents().with_list(|list| {
            list.iter()
                .find(|g| g.interface == WpColorManagerV1::interface().name)
                .map_or(0, |g| g.version)
        });
        let mut display = Display {
            conn,
            queue,
            qh,
            state: State::default(),
            _globals: globals,
            parent,
            compositor,
            subcompositor,
            viewporter,
            manager,
            feedback: None,
            descriptions: HashMap::new(),
        };
        display.state.caps.version = version;
        display.roundtrip()?;
        display.feedback = display
            .manager
            .as_ref()
            .map(|m| m.get_surface_feedback(&display.parent, &display.qh, ()));
        if display.feedback.is_some() {
            display.request_preferred();
            // Ready, then the information.
            for _ in 0..ROUNDTRIPS {
                if display.state.reading.is_none() {
                    break;
                }
                display.roundtrip()?;
            }
        }
        Ok(display)
    }

    fn roundtrip(&mut self) -> Result<(), Unavailable> {
        self.queue
            .roundtrip(&mut self.state)
            .map(|_| ())
            .map_err(|e| Unavailable::Connection(e.to_string()))
    }

    fn request_preferred(&mut self) {
        let Some(feedback) = &self.feedback else {
            return;
        };
        if self.state.reading.is_some() {
            return;
        }
        let description = feedback.get_preferred(&self.qh, Purpose::Preferred);
        self.state.reading = Some((description, Description::default()));
        self.state.changed = false;
        let _ = self.conn.flush();
    }

    /// What the colour manager announces, `None` without one.
    pub fn capabilities(&self) -> Option<&Capabilities> {
        self.manager.as_ref().map(|_| &self.state.caps)
    }

    /// Whether subsurfaces can be made (`wl_subcompositor`).
    pub fn has_subcompositor(&self) -> bool {
        self.compositor.is_some() && self.subcompositor.is_some()
    }

    /// Whether subsurfaces can be scaled to any size (`wp_viewporter`).
    pub fn has_viewporter(&self) -> bool {
        self.viewporter.is_some()
    }

    /// The compositor's preferred description for the window, once read.
    pub fn preferred(&self) -> Option<&Description> {
        self.state.preferred.as_ref()
    }

    /// Why the preferred description could not be read, if it could not.
    pub fn preferred_error(&self) -> Option<&str> {
        self.state.preferred_error.as_deref()
    }

    /// Bumped each time the preferred description changes.
    pub fn revision(&self) -> u64 {
        self.state.revision
    }

    /// Handles what arrived for this queue (winit reads the socket) and
    /// asks again for the preferred description when the compositor says
    /// it changed (the window moved to another screen). Never blocks.
    pub fn poll(&mut self) -> Result<(), String> {
        self.queue
            .dispatch_pending(&mut self.state)
            .map_err(|e| e.to_string())?;
        if self.state.changed {
            self.request_preferred();
        }
        self.conn.flush().map_err(|e| e.to_string())
    }

    /// A ready description for named primaries and transfer (made once,
    /// waiting for the compositor).
    fn description(
        &mut self,
        primaries: NamedPrimaries,
        transfer: NamedTransfer,
    ) -> Result<WpImageDescriptionV1, String> {
        let key = (primaries.code(), transfer.code());
        if let Some(d) = self.descriptions.get(&key) {
            return Ok(d.clone());
        }
        let caps = self
            .capabilities()
            .ok_or("the compositor has no colour manager")?;
        if !caps.can_tag(primaries, transfer) {
            return Err(format!(
                "the compositor does not take {} primaries with the {} transfer",
                primaries.name(),
                transfer.name()
            ));
        }
        let manager = self.manager.as_ref().ok_or("no colour manager")?;
        let named_primaries = wp_color_manager_v1::Primaries::try_from(primaries.code())
            .map_err(|_| format!("unknown primaries {}", primaries.code()))?;
        let named_transfer = wp_color_manager_v1::TransferFunction::try_from(transfer.code())
            .map_err(|_| format!("unknown transfer {}", transfer.code()))?;
        let creator = manager.create_parametric_creator(&self.qh, ());
        creator.set_primaries_named(named_primaries);
        creator.set_tf_named(named_transfer);
        let description = creator.create(&self.qh, Purpose::Tag);
        for _ in 0..ROUNDTRIPS {
            match self.state.status.get(&description.id()) {
                Some(Status::Ready) => {
                    self.descriptions.insert(key, description.clone());
                    return Ok(description);
                }
                Some(Status::Failed(e)) => {
                    let e = e.clone();
                    description.destroy();
                    return Err(format!("the compositor refused the description: {e}"));
                }
                None => self.roundtrip().map_err(|e| e.to_string())?,
            }
        }
        description.destroy();
        Err("the compositor did not answer for the description".into())
    }

    /// A surface for an image, below the window's: synchronised with it
    /// (its state applies with the window's commit), taking no input,
    /// sized through `wp_viewporter`.
    pub fn subsurface(&mut self) -> Result<Subsurface, String> {
        let compositor = self.compositor.as_ref().ok_or("no wl_compositor")?;
        let subcompositor = self.subcompositor.as_ref().ok_or("no wl_subcompositor")?;
        let viewporter = self.viewporter.as_ref().ok_or("no wp_viewporter")?;
        let surface = compositor.create_surface(&self.qh, ());
        let subsurface = subcompositor.get_subsurface(&surface, &self.parent, &self.qh, ());
        subsurface.place_below(&self.parent);
        subsurface.set_sync();
        let region = compositor.create_region(&self.qh, ());
        surface.set_input_region(Some(&region));
        region.destroy();
        let viewport = viewporter.get_viewport(&surface, &self.qh, ());
        self.conn.flush().map_err(|e| e.to_string())?;
        Ok(Subsurface {
            keep: Arc::new(Keep {
                surface,
                conn: self.conn.clone(),
            }),
            subsurface,
            viewport,
            colour: None,
            placed: None,
            tag: None,
        })
    }

    /// Sends what was asked so far (before a present, so the compositor
    /// gets it with the commit).
    pub fn flush(&self) -> Result<(), String> {
        self.conn.flush().map_err(|e| e.to_string())
    }
}

impl Drop for Display {
    fn drop(&mut self) {
        for (_, d) in self.descriptions.drain() {
            d.destroy();
        }
        if let Some((d, _)) = self.state.reading.take() {
            d.destroy();
        }
        if let Some(f) = self.feedback.take() {
            f.destroy();
        }
        if let Some(m) = self.manager.take() {
            m.destroy();
        }
        if let Some(v) = self.viewporter.take() {
            v.destroy();
        }
        if let Some(s) = self.subcompositor.take() {
            s.destroy();
        }
        let _ = self.conn.flush();
    }
}

/// An image's surface under a window's (see [`Display::subsurface`]).
pub struct Subsurface {
    keep: Arc<Keep>,
    subsurface: WlSubsurface,
    viewport: WpViewport,
    colour: Option<WpColorManagementSurfaceV1>,
    placed: Option<[i32; 4]>,
    tag: Option<(NamedPrimaries, NamedTransfer)>,
}

impl fmt::Debug for Subsurface {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Subsurface")
            .field("surface", &self.keep.surface.id())
            .field("placed", &self.placed)
            .field("tag", &self.tag)
            .finish_non_exhaustive()
    }
}

impl Subsurface {
    /// The handles to create a wgpu surface on; the `wl_surface` lives
    /// as long as they do.
    pub fn handle(&self) -> SurfaceHandle {
        SurfaceHandle {
            keep: self.keep.clone(),
        }
    }

    /// Places it at `[x, y, width, height]`, in logical pixels of the
    /// window's surface (its buffer is scaled to that size). Applied with
    /// the subsurface's next commit and the window's.
    pub fn place(&mut self, rect: [i32; 4]) {
        if self.placed == Some(rect) {
            return;
        }
        let [x, y, w, h] = rect;
        self.subsurface.set_position(x, y);
        if self.placed.is_none_or(|p| p[2] != w || p[3] != h) {
            self.viewport.set_destination(w.max(1), h.max(1));
        }
        self.placed = Some(rect);
    }

    /// Where it was placed last.
    pub fn placed(&self) -> Option<[i32; 4]> {
        self.placed
    }

    /// Declares its encoding to the compositor, with the perceptual
    /// intent (the one every compositor takes); applied with its next
    /// commit. The surface must not be presented to by a driver that
    /// manages its colour itself
    /// ([`Capabilities::wsi_claims_srgb_surfaces`]).
    pub fn tag(
        &mut self,
        display: &mut Display,
        primaries: NamedPrimaries,
        transfer: NamedTransfer,
    ) -> Result<(), String> {
        if self.tag == Some((primaries, transfer)) {
            return Ok(());
        }
        let description = display.description(primaries, transfer)?;
        let manager = display.manager.as_ref().ok_or("no colour manager")?;
        let colour = self
            .colour
            .get_or_insert_with(|| manager.get_surface(&self.keep.surface, &display.qh, ()));
        colour.set_image_description(&description, wp_color_manager_v1::RenderIntent::Perceptual);
        self.tag = Some((primaries, transfer));
        display.flush()
    }

    /// The encoding it was tagged with.
    pub fn tagged(&self) -> Option<(NamedPrimaries, NamedTransfer)> {
        self.tag
    }
}

impl Drop for Subsurface {
    fn drop(&mut self) {
        if let Some(c) = self.colour.take() {
            c.destroy();
        }
        self.viewport.destroy();
        self.subsurface.destroy();
        let _ = self.keep.conn.flush();
    }
}
