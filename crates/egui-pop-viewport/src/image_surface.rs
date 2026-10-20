//! The image's own surface (feature `wayland`): on a Wayland compositor
//! with colour management, the viewport draws its image into a surface
//! of its own, below the window, that the compositor is told the
//! encoding of; egui's interface stays sRGB on the window's surface
//! (`docs/wayland-colour.md`).
//!
//! - [`install`] (once, with eframe's creation context) joins the
//!   compositor through the window's handles and records what it offers
//!   ([`Offer`]): its colour manager's announcements, the formats and
//!   colour spaces a subsurface can be presented in.
//! - Each frame, the main window's viewports [`negotiate`] what the
//!   application wants ([`display::want`]) against that offer:
//!   - [`Route::Tagged`]: a subsurface egui-pop tags itself through
//!     `wp_color_management_v1` (BT.2020 with a 2.2 power, for one;
//!     KWin, sway, Weston);
//!   - [`Route::Wsi`]: a subsurface presented in a colour space the
//!     driver's window system integration declares (Display P3 where the
//!     compositor announces the sRGB transfer: GNOME, Hyprland);
//!   - [`Route::Window`]: the window's surface, sRGB, and why
//!     ([`display::Resolution::reason`]).
//! - With an image surface, the viewport renders its tiles there,
//!   presents it just before egui presents the window (both apply in the
//!   same compositor frame), and punches a transparent hole in the
//!   interface where the image is: overlays, menus and tooltips drawn
//!   after stay above it. The window must be transparent
//!   (`ViewportBuilder::with_transparent(true)`), or the hole would show
//!   black.
//!
//! The `EGUI_POP_IMAGE_SURFACE` variable turns it `off`, or keeps the
//! driver's colour spaces only (`no-tag`). The debug bridge's `viewport`
//! collector reports it ([`report`]), with the compositor's preferred
//! image description for the window.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};

use egui::{Color32, Context, ViewportId};
use egui_pop_core::state::Slot;
use egui_pop_wayland::{Capabilities, Feature, NamedPrimaries, NamedTransfer};
use egui_wgpu::wgpu;
use serde_json::{Value as Json, json};

use crate::display::{
    self, ColorSpace, DisplayTarget, HDR_REFERENCE_WHITE, Primaries, Stage, SurfaceFormat,
    TargetSource, Transfer,
};
use crate::gpu::{Resources, TilePaint};

/// The raw window handles [`install`] takes (eframe's creation context
/// implements both traits).
pub use egui_pop_wayland::raw_window_handle;

/// The environment variable: `off` (no image surface), `no-tag` (the
/// driver's colour spaces only), anything else or unset (both).
pub const VARIABLE: &str = "EGUI_POP_IMAGE_SURFACE";

/// What the platform offers an image surface.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Offer {
    /// Why there is no image surface at all (not Wayland, an opaque
    /// window, no subsurfaces, turned off), if there is none.
    pub unavailable: Option<String>,
    /// The colour manager's announcements (to a version 1 client),
    /// `None` without one.
    pub colour: Option<Capabilities>,
    /// The formats and colour spaces an image surface can be presented
    /// in, as its driver offers them.
    pub wsi: Vec<(SurfaceFormat, ColorSpace)>,
    /// Whether egui-pop may tag the image surface itself.
    pub may_tag: bool,
}

impl Offer {
    /// No image surface, and why.
    pub fn unavailable(why: impl Into<String>) -> Offer {
        Offer {
            unavailable: Some(why.into()),
            ..Offer::default()
        }
    }
}

/// Where the image is drawn.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Route {
    /// On the window's surface.
    Window,
    /// On an image surface egui-pop tagged through the protocol.
    Tagged(DisplayTarget),
    /// On an image surface in a colour space the driver declares.
    Wsi(DisplayTarget),
}

impl Route {
    /// What the image surface expects, when there is one.
    pub fn target(&self) -> Option<DisplayTarget> {
        match self {
            Route::Window => None,
            Route::Tagged(t) | Route::Wsi(t) => Some(*t),
        }
    }

    pub fn name(&self) -> &'static str {
        match self {
            Route::Window => "window",
            Route::Tagged(_) => "tagged",
            Route::Wsi(_) => "wsi",
        }
    }
}

/// Where the image goes for a wanted target, and why not better.
#[derive(Clone, Debug, PartialEq)]
pub struct Negotiation {
    pub route: Route,
    /// Why the image stays on the window's surface, or gets less than
    /// wanted.
    pub why: Option<String>,
}

/// The protocol's name for primaries (custom ones have none).
pub fn named_primaries(primaries: Primaries) -> Option<NamedPrimaries> {
    match primaries {
        Primaries::Srgb => Some(NamedPrimaries::Srgb),
        Primaries::DisplayP3 => Some(NamedPrimaries::DisplayP3),
        Primaries::Bt2020 => Some(NamedPrimaries::Bt2020),
        Primaries::Custom { .. } => None,
    }
}

/// The protocol's name for a transfer (extended sRGB has none).
pub fn named_transfer(transfer: Transfer) -> Option<NamedTransfer> {
    match transfer {
        Transfer::SrgbPiecewise => Some(NamedTransfer::Srgb),
        Transfer::Gamma22 => Some(NamedTransfer::Gamma22),
        Transfer::ExtLinear => Some(NamedTransfer::ExtLinear),
        Transfer::Pq => Some(NamedTransfer::St2084Pq),
        Transfer::Hlg => Some(NamedTransfer::Hlg),
        Transfer::ExtSrgb => None,
    }
}

/// Whether a target carries anything sRGB does not.
fn wider_than_srgb(t: &DisplayTarget) -> bool {
    t.primaries.rank() > 0 || t.transfer.is_hdr()
}

/// The format to tag for a transfer, among those offered with the sRGB
/// colour space (the driver then leaves the surface's colour alone).
fn tag_format(transfer: Transfer, formats: &[SurfaceFormat]) -> Option<SurfaceFormat> {
    use SurfaceFormat as F;
    let order: &[SurfaceFormat] = match transfer {
        Transfer::ExtLinear | Transfer::ExtSrgb => &[F::Rgba16Float],
        Transfer::Pq | Transfer::Hlg => &[F::Rgb10a2Unorm, F::Rgba16Float],
        Transfer::SrgbPiecewise | Transfer::Gamma22 => {
            &[F::Rgb10a2Unorm, F::Rgba8Unorm, F::Rgba16Float]
        }
    };
    order.iter().copied().find(|f| formats.contains(f))
}

/// The encoding to tag the image surface with: the wanted primaries (or
/// BT.2020, which holds them) with the wanted transfer (or a 2.2 power,
/// or the sRGB curve), the first the compositor takes on an offered
/// format.
fn tag(
    wanted: &DisplayTarget,
    caps: &Capabilities,
    offer: &Offer,
) -> Result<DisplayTarget, String> {
    if !offer.may_tag {
        return Err(format!("tagging is turned off ({VARIABLE}=no-tag)"));
    }
    // upstream: Mesa holds the colour object of sRGB swapchains and wgpu
    // has no pass-through colour space (docs/upstream.md, Wayland).
    if caps.wsi_claims_srgb_surfaces() {
        return Err(
            "the compositor announces sRGB with the sRGB transfer, so the driver's \
                    window system integration may hold the image surface's colour itself \
                    (Mesa does): egui-pop does not tag it"
                .into(),
        );
    }
    if !caps.has_feature(Feature::Parametric) {
        return Err("the compositor takes no parametric image description".into());
    }
    let formats: Vec<SurfaceFormat> = offer
        .wsi
        .iter()
        .filter(|(_, space)| *space == ColorSpace::Srgb)
        .map(|(format, _)| *format)
        .collect();
    let mut primaries = vec![wanted.primaries];
    if wanted.primaries != Primaries::Bt2020 {
        primaries.push(Primaries::Bt2020);
    }
    let mut transfers = vec![wanted.transfer];
    for t in [Transfer::Gamma22, Transfer::SrgbPiecewise] {
        if !transfers.contains(&t) {
            transfers.push(t);
        }
    }
    for &transfer in &transfers {
        for &p in &primaries {
            let (Some(np), Some(nt)) = (named_primaries(p), named_transfer(transfer)) else {
                continue;
            };
            if !caps.can_tag(np, nt) {
                continue;
            }
            let Some(format) = tag_format(transfer, &formats) else {
                continue;
            };
            let white = match transfer {
                Transfer::Pq | Transfer::Hlg => HDR_REFERENCE_WHITE,
                _ => 80.0,
            };
            let target = DisplayTarget {
                primaries: p,
                transfer,
                sdr_white_nits: white,
                headroom: if transfer.is_hdr() {
                    wanted.headroom.clamp(1.0, transfer.max_headroom(white))
                } else {
                    1.0
                },
                surface_format: format,
                source: TargetSource::Tagged,
            };
            if wider_than_srgb(&target) && target.is_supported() {
                return Ok(target);
            }
        }
    }
    if formats.is_empty() {
        return Err("the image surface's driver offers no format to tag".into());
    }
    let [_, prims, tfs, _] = caps.names();
    Err(format!(
        "the compositor takes no encoding wider than sRGB for {} (it announces the primaries {} and the transfers {})",
        wanted.describe(),
        prims.join(", "),
        tfs.join(", ")
    ))
}

/// The driver's colour space for the image surface: an SDR one for an
/// SDR wish, then one carrying the wish, then the widest gamut, then the
/// fewest bits.
fn wsi(wanted: &DisplayTarget, pairs: &[(SurfaceFormat, ColorSpace)]) -> Option<DisplayTarget> {
    let mut best: Option<(u32, DisplayTarget)> = None;
    for &(format, space) in pairs {
        if space == ColorSpace::Srgb
            || matches!(format, SurfaceFormat::Rgba8UnormSrgb | SurfaceFormat::Other)
        {
            continue;
        }
        let target = DisplayTarget::for_color_space(
            space,
            format,
            wanted.headroom.max(1.0),
            TargetSource::Wsi,
        );
        if !target.is_supported() || !wider_than_srgb(&target) {
            continue;
        }
        let same_kind =
            wanted.transfer.is_hdr() || !matches!(target.transfer, Transfer::Pq | Transfer::Hlg);
        let carries = target.missing(wanted).is_none();
        let gamut = if target.transfer.is_extended() {
            3
        } else {
            u32::from(target.primaries.rank())
        };
        let score = u32::from(same_kind) * 2000
            + u32::from(carries) * 1000
            + gamut * 50
            + (32 - u32::from(format.bits().min(32)) / 2);
        if best.is_none_or(|(s, _)| score > s) {
            best = Some((score, target));
        }
    }
    best.map(|(_, t)| t)
}

/// Where the image goes for `wanted`, given what the platform offers:
/// the window's surface for sRGB or without an image surface; else a
/// surface tagged by egui-pop when it may and the compositor takes the
/// encoding; else one in a colour space the driver declares; else the
/// window's, with why each step failed.
pub fn negotiate(wanted: &DisplayTarget, offer: &Offer) -> Negotiation {
    let window = |why: Option<String>| Negotiation {
        route: Route::Window,
        why,
    };
    if !wider_than_srgb(wanted) {
        return window(None);
    }
    if let Some(why) = &offer.unavailable {
        return window(Some(why.clone()));
    }
    let mut why = Vec::new();
    match &offer.colour {
        None => why.push("the compositor has no colour management (wp_color_manager_v1)".into()),
        Some(caps) => match tag(wanted, caps, offer) {
            Ok(target) => {
                return Negotiation {
                    route: Route::Tagged(target),
                    why: None,
                };
            }
            Err(e) => why.push(e),
        },
    }
    match wsi(wanted, &offer.wsi) {
        Some(target) => Negotiation {
            route: Route::Wsi(target),
            why: target.missing(wanted).map(|_| why.join("; ")),
        },
        None => {
            why.push("the image surface's driver offers no colour space wider than sRGB".into());
            window(Some(why.join("; ")))
        }
    }
}

/// Where an image surface sits for an image rectangle.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Layout {
    /// `[x, y, width, height]` in logical pixels of the window's surface:
    /// whole pixels covering the rectangle (the overflow hides under the
    /// interface).
    pub logical: [i32; 4],
    /// The buffer's size in physical pixels.
    pub buffer: [u32; 2],
    /// The buffer's top left in window pixels.
    pub origin: [f32; 2],
}

/// The image surface's place for a rectangle `[x0, y0, x1, y1]` in
/// window pixels, at `scale` physical pixels per logical one; `None` for
/// an empty one.
pub fn layout(rect_px: [f32; 4], scale: f32) -> Option<Layout> {
    let scale = if scale > 0.0 { scale } else { 1.0 };
    let x0 = (rect_px[0] / scale).floor() as i32;
    let y0 = (rect_px[1] / scale).floor() as i32;
    let x1 = (rect_px[2] / scale).ceil() as i32;
    let y1 = (rect_px[3] / scale).ceil() as i32;
    let (w, h) = (x1 - x0, y1 - y0);
    if w <= 0 || h <= 0 {
        return None;
    }
    Some(Layout {
        logical: [x0, y0, w, h],
        buffer: [
            ((w as f32 * scale).round() as u32).max(1),
            ((h as f32 * scale).round() as u32).max(1),
        ],
        origin: [x0 as f32 * scale, y0 as f32 * scale],
    })
}

/// An sRGB colour encoded for a target (the image surface's background).
pub fn clear_colour(colour: Color32, target: &DisplayTarget) -> [f64; 4] {
    let linear = egui::Rgba::from(colour);
    let e = Stage::convert(Primaries::Srgb, target).encode([linear.r(), linear.g(), linear.b()]);
    [f64::from(e[0]), f64::from(e[1]), f64::from(e[2]), 1.0]
}

/// How [`install`] sets up image surfaces.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Options {
    /// The main window was built transparent
    /// (`ViewportBuilder::with_transparent(true)`): required, the image
    /// surface shows through a hole in it.
    pub transparent_window: bool,
}

#[derive(Default)]
struct Shared {
    offer: Offer,
    negotiated: Option<(DisplayTarget, Negotiation)>,
    /// Why the image surface failed (it then stays on the window until
    /// the wish changes).
    failure: Option<String>,
    #[cfg(target_os = "linux")]
    manager: Option<wl::Manager>,
}

/// The image surfaces of a context, shared with the paint callbacks.
#[derive(Clone)]
pub(crate) struct ImageSurfaces(Arc<Mutex<Shared>>);

impl std::fmt::Debug for ImageSurfaces {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ImageSurfaces").finish_non_exhaustive()
    }
}

impl ImageSurfaces {
    fn lock(&self) -> MutexGuard<'_, Shared> {
        self.0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

const SURFACES: Slot<ImageSurfaces> = Slot::opaque("viewport.image_surface");

/// Sets up image surfaces for the main window: `rs` is the render state
/// the GPU path was installed on ([`crate::gpu::install`]), `window` the
/// main window's handles (eframe's creation context). Returns what the
/// platform offers; viewports in the main window use it from the next
/// frame.
// upstream: winit binds no colour management and egui-wgpu takes no
// colour space, hence a surface of the image's own (docs/upstream.md).
pub fn install(
    ctx: &Context,
    rs: &egui_wgpu::RenderState,
    window: &(impl raw_window_handle::HasWindowHandle + raw_window_handle::HasDisplayHandle),
    options: Options,
) -> Offer {
    let mode = std::env::var(VARIABLE).unwrap_or_default();
    let mut shared = Shared::default();
    if mode == "off" {
        shared.offer = Offer::unavailable(format!("turned off ({VARIABLE}=off)"));
    } else if !options.transparent_window {
        shared.offer = Offer::unavailable(
            "the window is opaque: the image surface sits below it and needs a transparent \
             window (ViewportBuilder::with_transparent)",
        );
    } else {
        #[cfg(target_os = "linux")]
        match wl::Manager::new(rs, window) {
            Ok((manager, mut offer)) => {
                offer.may_tag = mode != "no-tag";
                shared.offer = offer;
                shared.manager = Some(manager);
            }
            Err(e) => shared.offer = Offer::unavailable(e),
        }
        #[cfg(not(target_os = "linux"))]
        {
            let _ = (rs, window);
            shared.offer = Offer::unavailable("not a Wayland window (X11 or another system)");
        }
    }
    let offer = shared.offer.clone();
    SURFACES.set(ctx, ImageSurfaces(Arc::new(Mutex::new(shared))));
    ctx.request_repaint();
    offer
}

/// What [`install`] found, if it ran.
pub fn offer(ctx: &Context) -> Option<Offer> {
    let surfaces = SURFACES.get(ctx)?;
    Some(surfaces.lock().offer.clone())
}

/// Where the image of the window being drawn goes, for what it wants
/// (on the GPU path): negotiates when the wish changes, records the
/// image surface's target and why ([`display::set_image_surface`]), and
/// keeps the compositor's events read. The image surfaces when it has
/// one.
pub(crate) fn frame(ctx: &Context) -> Option<(ImageSurfaces, Route)> {
    let surfaces = SURFACES.get(ctx)?;
    let viewport = ctx.viewport_id();
    let wanted = display::wanted(ctx);
    if viewport != ViewportId::ROOT {
        let why = wider_than_srgb(&wanted)
            .then(|| "the image gets a surface of its own in the main window only".to_string());
        display::set_image_surface(ctx, viewport, display::ImageSurface { target: None, why });
        return None;
    }
    let negotiation = {
        let mut s = surfaces.lock();
        #[cfg(target_os = "linux")]
        if let Some(Err(e)) = s.manager.as_mut().map(wl::Manager::poll) {
            s.failure = Some(format!("the compositor's events: {e}"));
        }
        if s.negotiated.as_ref().is_none_or(|(w, _)| *w != wanted) {
            s.failure = None;
            let n = negotiate(&wanted, &s.offer);
            s.negotiated = Some((wanted, n));
        }
        let mut n = s
            .negotiated
            .as_ref()
            .map(|(_, n)| n.clone())
            .unwrap_or(Negotiation {
                route: Route::Window,
                why: None,
            });
        if let Some(f) = &s.failure
            && n.route != Route::Window
        {
            n = Negotiation {
                route: Route::Window,
                why: Some(format!("the image surface failed: {f}")),
            };
        }
        #[cfg(target_os = "linux")]
        if let Some(m) = s.manager.as_mut() {
            if n.route == Route::Window {
                m.clear();
            } else {
                m.collect(ctx.cumulative_pass_nr());
            }
        }
        n
    };
    display::set_image_surface(
        ctx,
        viewport,
        display::ImageSurface {
            target: negotiation.route.target(),
            why: negotiation.why,
        },
    );
    (negotiation.route != Route::Window).then_some((surfaces, negotiation.route))
}

/// One viewport's image for its image surface, and the hole for it in
/// the window.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
pub(crate) struct ImagePaint {
    pub ctx: Context,
    pub surfaces: ImageSurfaces,
    pub route: Route,
    /// The viewport's key (its tile slot).
    pub key: u64,
    pub pass: u64,
    /// The visible image rectangle, in window pixels.
    pub rect_px: [f32; 4],
    /// Physical pixels per logical pixel of the window's surface.
    pub scale: f32,
    pub background: [f64; 4],
    /// Each side's tiles and its clip, in window pixels.
    pub sides: Vec<(TilePaint, [f32; 4])>,
    /// The image surface shows this frame's image (else no hole).
    pub shown: AtomicBool,
}

impl egui_wgpu::CallbackTrait for ImagePaint {
    fn prepare(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        _screen: &egui_wgpu::ScreenDescriptor,
        _encoder: &mut wgpu::CommandEncoder,
        resources: &mut egui_wgpu::CallbackResources,
    ) -> Vec<wgpu::CommandBuffer> {
        let Some(r) = resources.get_mut::<Resources>() else {
            return Vec::new();
        };
        for (side, _) in &self.sides {
            side.sync_textures(device, queue, r);
        }
        #[cfg(target_os = "linux")]
        {
            let mut s = self.surfaces.lock();
            let result = match s.manager.as_mut() {
                Some(m) => m.render(self, device, queue, r),
                None => Ok(false),
            };
            match result {
                Ok(shown) => self.shown.store(shown, Ordering::Relaxed),
                Err(e) => {
                    s.failure = Some(e);
                    self.ctx.request_repaint();
                }
            }
        }
        Vec::new()
    }

    fn paint(
        &self,
        info: egui::PaintCallbackInfo,
        pass: &mut wgpu::RenderPass<'static>,
        resources: &egui_wgpu::CallbackResources,
    ) {
        if !self.shown.load(Ordering::Relaxed) {
            return;
        }
        if let Some(r) = resources.get::<Resources>() {
            r.draw_hole(pass, info.screen_size_px);
        }
    }
}

/// What the image surfaces of `ctx` are doing, for the debug bridge.
pub fn report(ctx: &Context) -> Json {
    let Some(surfaces) = SURFACES.get(ctx) else {
        return json!({ "installed": false });
    };
    let s = surfaces.lock();
    let colour = s.offer.colour.as_ref().map(|c| {
        let [features, primaries, transfers, intents] = c.names();
        json!({
            "version": c.version,
            "features": features,
            "primaries": primaries,
            "transfers": transfers,
            "intents": intents,
            "wsi_claims_srgb_surfaces": c.wsi_claims_srgb_surfaces(),
        })
    });
    let negotiated = s.negotiated.as_ref().map(|(wanted, n)| {
        json!({
            "wanted": wanted.describe(),
            "route": n.route.name(),
            "target": n.route.target().map(|t| format!("{} ({:?})", t.describe(), t.surface_format)),
            "why": n.why,
        })
    });
    #[cfg(target_os = "linux")]
    let manager = s.manager.as_ref().map(wl::Manager::report);
    #[cfg(not(target_os = "linux"))]
    let manager: Option<Json> = None;
    json!({
        "installed": true,
        "unavailable": s.offer.unavailable,
        "colour_management": colour,
        "wsi": s.offer.wsi.iter().map(|(f, c)| format!("{f:?} {c:?}")).collect::<Vec<_>>(),
        "may_tag": s.offer.may_tag,
        "negotiated": negotiated,
        "failure": s.failure,
        "surfaces": manager,
    })
}

#[cfg(target_os = "linux")]
mod wl {
    //! The Wayland image surfaces: one subsurface and wgpu surface per
    //! viewport of the main window.

    use std::collections::HashMap;

    use egui_pop_wayland::{Description, Display, Subsurface};
    use egui_wgpu::wgpu;
    use serde_json::{Value as Json, json};

    use super::{ImagePaint, Route, layout, named_primaries, named_transfer, raw_window_handle};
    use crate::display::{ColorSpace, SurfaceFormat};
    use crate::gpu::{self, Resources};

    struct Image {
        // Dropped first: the swapchain goes before its surface.
        surface: wgpu::Surface<'static>,
        sub: Subsurface,
        config: Option<wgpu::SurfaceConfiguration>,
        route: Route,
        used: u64,
        presented: bool,
    }

    pub(super) struct Manager {
        // Dropped first: the image surfaces go before the connection.
        images: HashMap<u64, Image>,
        display: Display,
        instance: wgpu::Instance,
        formats: Vec<(wgpu::TextureFormat, SurfaceFormat, ColorSpace)>,
        present_mode: wgpu::PresentMode,
        alpha_mode: wgpu::CompositeAlphaMode,
        max_side: u32,
    }

    impl Manager {
        /// Joins the compositor and probes a subsurface's capabilities.
        pub(super) fn new(
            rs: &egui_wgpu::RenderState,
            window: &(impl raw_window_handle::HasWindowHandle + raw_window_handle::HasDisplayHandle),
        ) -> Result<(Manager, super::Offer), String> {
            let mut display = Display::attach(window).map_err(|e| e.to_string())?;
            if !display.has_subcompositor() {
                return Err("the compositor offers no subsurfaces".into());
            }
            if !display.has_viewporter() {
                return Err("the compositor has no wp_viewporter to size a subsurface".into());
            }
            let sub = display.subsurface()?;
            let surface = rs
                .instance
                .create_surface(sub.handle())
                .map_err(|e| format!("no wgpu surface on a subsurface: {e}"))?;
            if !rs.adapter.is_surface_supported(&surface) {
                return Err("the GPU cannot present to a subsurface".into());
            }
            let caps = surface.get_capabilities(&rs.adapter);
            drop(surface);
            drop(sub);
            let mut formats = Vec::new();
            for &format in &caps.formats {
                let sf = gpu::surface_format(format);
                if matches!(sf, SurfaceFormat::Other | SurfaceFormat::Rgba8UnormSrgb) {
                    continue;
                }
                let spaces = caps.color_spaces(format);
                for space in ColorSpace::ALL {
                    if spaces.contains(gpu::color_space_flag(space))
                        && !formats.iter().any(|(_, f, c)| *f == sf && *c == space)
                    {
                        formats.push((format, sf, space));
                    }
                }
            }
            let present_mode = [wgpu::PresentMode::Mailbox, wgpu::PresentMode::Immediate]
                .into_iter()
                .find(|m| caps.present_modes.contains(m))
                .unwrap_or(wgpu::PresentMode::Fifo);
            let alpha_mode = if caps.alpha_modes.contains(&wgpu::CompositeAlphaMode::Opaque) {
                wgpu::CompositeAlphaMode::Opaque
            } else {
                wgpu::CompositeAlphaMode::Auto
            };
            let offer = super::Offer {
                unavailable: None,
                colour: display.capabilities().cloned(),
                wsi: formats.iter().map(|(_, f, c)| (*f, *c)).collect(),
                may_tag: true,
            };
            Ok((
                Manager {
                    images: HashMap::new(),
                    display,
                    instance: rs.instance.clone(),
                    formats,
                    present_mode,
                    alpha_mode,
                    max_side: rs.device.limits().max_texture_dimension_2d,
                },
                offer,
            ))
        }

        pub(super) fn poll(&mut self) -> Result<(), String> {
            self.display.poll()
        }

        /// Drops every image surface (the image went back to the window).
        pub(super) fn clear(&mut self) {
            self.images.clear();
        }

        /// Drops the image surfaces of viewports no longer drawn.
        pub(super) fn collect(&mut self, pass: u64) {
            self.images.retain(|_, i| i.used + 2 >= pass);
        }

        /// Renders a viewport's image into its surface and presents it;
        /// whether the surface shows an image.
        pub(super) fn render(
            &mut self,
            p: &ImagePaint,
            device: &wgpu::Device,
            queue: &wgpu::Queue,
            r: &mut Resources,
        ) -> Result<bool, String> {
            let Some(target) = p.route.target() else {
                return Ok(false);
            };
            let Some(place) = layout(p.rect_px, p.scale) else {
                return Ok(false);
            };
            if place.buffer.iter().any(|s| *s > self.max_side) {
                return Err(format!(
                    "the image ({}x{} pixels) is larger than the GPU's surfaces",
                    place.buffer[0], place.buffer[1]
                ));
            }
            let space = match p.route {
                Route::Wsi(t) => t.color_space().ok_or("no colour space for the target")?,
                _ => ColorSpace::Srgb,
            };
            let format = self
                .formats
                .iter()
                .find(|(_, f, c)| *f == target.surface_format && *c == space)
                .map(|(w, _, _)| *w)
                .ok_or("the image surface's format is not offered")?;
            if self.images.get(&p.key).is_some_and(|i| i.route != p.route) {
                self.images.remove(&p.key);
            }
            if !self.images.contains_key(&p.key) {
                let mut sub = self.display.subsurface()?;
                if let Route::Tagged(t) = p.route {
                    let (Some(np), Some(nt)) =
                        (named_primaries(t.primaries), named_transfer(t.transfer))
                    else {
                        return Err("the target has no protocol name".into());
                    };
                    sub.tag(&mut self.display, np, nt)?;
                }
                let surface = self
                    .instance
                    .create_surface(sub.handle())
                    .map_err(|e| format!("no wgpu surface on the subsurface: {e}"))?;
                self.images.insert(
                    p.key,
                    Image {
                        surface,
                        sub,
                        config: None,
                        route: p.route,
                        used: p.pass,
                        presented: false,
                    },
                );
            }
            let Some(image) = self.images.get_mut(&p.key) else {
                return Ok(false);
            };
            image.used = p.pass;
            image.sub.place(place.logical);
            let config = wgpu::SurfaceConfiguration {
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                format,
                color_space: gpu::to_wgpu(space),
                width: place.buffer[0],
                height: place.buffer[1],
                present_mode: self.present_mode,
                desired_maximum_frame_latency: 2,
                alpha_mode: self.alpha_mode,
                view_formats: Vec::new(),
            };
            if image.config.as_ref() != Some(&config) {
                image.surface.configure(device, &config);
                image.config = Some(config);
            }
            let frame = match image.surface.get_current_texture() {
                wgpu::CurrentSurfaceTexture::Success(t)
                | wgpu::CurrentSurfaceTexture::Suboptimal(t) => t,
                // The last image stays.
                wgpu::CurrentSurfaceTexture::Timeout | wgpu::CurrentSurfaceTexture::Occluded => {
                    return Ok(image.presented);
                }
                wgpu::CurrentSurfaceTexture::Outdated
                | wgpu::CurrentSurfaceTexture::Lost
                | wgpu::CurrentSurfaceTexture::Validation => {
                    image.config = None;
                    return Ok(false);
                }
            };
            let [bw, bh] = place.buffer;
            let size = [bw as f32, bh as f32];
            let mut drawn = Vec::new();
            for (side, clip) in &p.sides {
                if side.write_uniforms(device, queue, r, size, place.origin, false) {
                    drawn.push((side, *clip));
                }
            }
            let view = frame
                .texture
                .create_view(&wgpu::TextureViewDescriptor::default());
            let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("egui-pop-viewport.image"),
            });
            {
                let [cr, cg, cb, ca] = p.background;
                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("egui-pop-viewport.image"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &view,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color {
                                r: cr,
                                g: cg,
                                b: cb,
                                a: ca,
                            }),
                            store: wgpu::StoreOp::Store,
                        },
                        depth_slice: None,
                    })],
                    ..Default::default()
                });
                pass.set_viewport(0.0, 0.0, size[0], size[1], 0.0, 1.0);
                for (side, clip) in drawn {
                    let x0 = (clip[0] - place.origin[0]).floor().clamp(0.0, size[0]) as u32;
                    let y0 = (clip[1] - place.origin[1]).floor().clamp(0.0, size[1]) as u32;
                    let x1 = (clip[2] - place.origin[0]).ceil().clamp(0.0, size[0]) as u32;
                    let y1 = (clip[3] - place.origin[1]).ceil().clamp(0.0, size[1]) as u32;
                    if x1 <= x0 || y1 <= y0 {
                        continue;
                    }
                    pass.set_scissor_rect(x0, y0, x1 - x0, y1 - y0);
                    pass.set_pipeline(r.image_pipeline(device, format));
                    side.draw(&mut pass, r);
                }
            }
            queue.submit([encoder.finish()]);
            // The position, size and tag go with the present's commit.
            self.display.flush()?;
            queue.present(frame);
            image.presented = true;
            Ok(true)
        }

        pub(super) fn report(&self) -> Json {
            json!({
                "present_mode": format!("{:?}", self.present_mode),
                "images": self.images.values().map(|i| json!({
                    "route": i.route.name(),
                    "placed": i.sub.placed(),
                    "tagged": i.sub.tagged().map(|(p, t)| format!("{} {}", p.name(), t.name())),
                    "config": i.config.as_ref().map(|c| format!("{:?} {:?} {}x{}", c.format, c.color_space, c.width, c.height)),
                })).collect::<Vec<_>>(),
                "preferred": self.display.preferred().map(description),
                "preferred_error": self.display.preferred_error(),
            })
        }
    }

    /// A description as JSON.
    fn description(d: &Description) -> Json {
        json!({
            "primaries_named": d.primaries_named.map(|p| p.name()),
            "primaries": d.primaries,
            "transfer": d.transfer.map(|t| t.name()),
            "tf_power": d.tf_power,
            "luminances": d.luminances.map(|l| json!({ "min": l.min, "max": l.max, "reference": l.reference })),
            "target_primaries": d.target_primaries,
            "target_luminance": d.target_luminance,
            "target_max_cll": d.target_max_cll,
            "target_max_fall": d.target_max_fall,
            "headroom": d.headroom(),
            "icc": d.icc,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui_pop_wayland::{Feature, NamedPrimaries as P, NamedTransfer as T};

    fn caps(primaries: &[P], transfers: &[T], features: &[Feature]) -> Capabilities {
        Capabilities {
            version: 1,
            features: features.to_vec(),
            primaries: primaries.to_vec(),
            transfers: transfers.to_vec(),
            intents: vec![0],
        }
    }

    /// A Mesa driver's offer for a subsurface on a compositor announcing
    /// these names (its colour spaces, on 8 and 10 bits and half floats).
    fn offer(c: Capabilities, wsi: &[ColorSpace]) -> Offer {
        let mut pairs = Vec::new();
        for f in [
            SurfaceFormat::Rgba8Unorm,
            SurfaceFormat::Rgb10a2Unorm,
            SurfaceFormat::Rgba16Float,
        ] {
            pairs.push((f, ColorSpace::Srgb));
            for &s in wsi {
                pairs.push((f, s));
            }
        }
        Offer {
            unavailable: None,
            colour: Some(c),
            wsi: pairs,
            may_tag: true,
        }
    }

    fn kwin() -> Offer {
        offer(
            caps(
                &[P::Srgb, P::Bt2020, P::DisplayP3],
                &[T::Gamma22, T::St2084Pq, T::ExtLinear, T::Hlg],
                &[Feature::Parametric, Feature::ExtendedTargetVolume],
            ),
            &[
                ColorSpace::ExtendedSrgbLinear,
                ColorSpace::Bt2100Pq,
                ColorSpace::Bt2100Hlg,
            ],
        )
    }

    fn gnome() -> Offer {
        offer(
            caps(
                &[P::Srgb, P::Bt2020, P::DisplayP3],
                &[T::Gamma22, T::St2084Pq, T::ExtLinear, T::Srgb],
                &[Feature::Parametric],
            ),
            &[ColorSpace::DisplayP3, ColorSpace::Bt2100Pq],
        )
    }

    fn rec2020() -> DisplayTarget {
        DisplayTarget {
            primaries: Primaries::Bt2020,
            transfer: Transfer::Gamma22,
            ..DisplayTarget::srgb()
        }
    }

    #[test]
    fn srgb_stays_on_the_window() {
        let n = negotiate(&DisplayTarget::srgb(), &kwin());
        assert_eq!(n.route, Route::Window);
        assert_eq!(n.why, None);
    }

    #[test]
    fn rec2020_is_tagged_where_the_driver_leaves_the_surface_alone() {
        let n = negotiate(&rec2020(), &kwin());
        let Route::Tagged(t) = n.route else {
            panic!("{n:?}");
        };
        assert_eq!(
            (t.primaries, t.transfer),
            (Primaries::Bt2020, Transfer::Gamma22)
        );
        assert_eq!(t.surface_format, SurfaceFormat::Rgb10a2Unorm);
        assert_eq!(t.source, TargetSource::Tagged);
        assert!(t.is_supported());
        assert!(display::resolve(&rec2020(), &t, true).faithful());
        assert_eq!(n.why, None);
    }

    #[test]
    fn display_p3_is_tagged_with_a_power_where_the_srgb_curve_is_not_announced() {
        let n = negotiate(&DisplayTarget::display_p3(), &kwin());
        let Route::Tagged(t) = n.route else {
            panic!("{n:?}");
        };
        assert_eq!(
            (t.primaries, t.transfer),
            (Primaries::DisplayP3, Transfer::Gamma22)
        );
    }

    #[test]
    fn hdr_is_tagged_on_ten_bits() {
        let n = negotiate(&DisplayTarget::hdr_pq(4.0), &kwin());
        let Route::Tagged(t) = n.route else {
            panic!("{n:?}");
        };
        assert_eq!(t.transfer, Transfer::Pq);
        assert_eq!(t.surface_format, SurfaceFormat::Rgb10a2Unorm);
        assert_eq!(t.headroom, 4.0);
    }

    #[test]
    fn the_driver_declares_where_it_holds_the_surface() {
        // GNOME announces sRGB with the sRGB transfer: Mesa holds the
        // colour of every surface it presents to; Display P3 through it.
        let n = negotiate(&rec2020(), &gnome());
        let Route::Wsi(t) = n.route else {
            panic!("{n:?}");
        };
        assert_eq!(t.color_space(), Some(ColorSpace::DisplayP3));
        assert_eq!(t.source, TargetSource::Wsi);
        let why = n.why.unwrap();
        assert!(why.contains("Mesa"), "{why}");
        // What is lost is said by the resolution.
        let r = display::resolve(&rec2020(), &t, true);
        assert!(r.reason().unwrap().contains("BT.2020"), "{r:?}");
    }

    #[test]
    fn tagging_can_be_turned_off() {
        let mut o = kwin();
        o.may_tag = false;
        let n = negotiate(&rec2020(), &o);
        let Route::Wsi(t) = n.route else {
            panic!("{n:?}");
        };
        // scRGB carries BT.2020 (negatives) and is SDR-compatible.
        assert_eq!(t.color_space(), Some(ColorSpace::ExtendedSrgbLinear));
        assert_eq!(n.why, None);
    }

    #[test]
    fn without_colour_management_the_image_stays_srgb_and_says_why() {
        let mut o = offer(Capabilities::default(), &[]);
        o.colour = None;
        let n = negotiate(&rec2020(), &o);
        assert_eq!(n.route, Route::Window);
        let why = n.why.unwrap();
        assert!(why.contains("no colour management"), "{why}");
        assert!(why.contains("no colour space wider than sRGB"), "{why}");
        // Not Wayland, an opaque window: the reason as given.
        let n = negotiate(&rec2020(), &Offer::unavailable("not a Wayland window"));
        assert_eq!(n.route, Route::Window);
        assert_eq!(n.why.as_deref(), Some("not a Wayland window"));
    }

    #[test]
    fn a_compositor_without_the_encoding_falls_to_the_driver_then_the_window() {
        // Parametric, but only sRGB primaries.
        let narrow = offer(caps(&[P::Srgb], &[T::Gamma22], &[Feature::Parametric]), &[]);
        let n = negotiate(&rec2020(), &narrow);
        assert_eq!(n.route, Route::Window);
        let why = n.why.unwrap();
        assert!(why.contains("announces the primaries srgb"), "{why}");
        // No parametric feature (ICC only).
        let icc = offer(caps(&[P::Bt2020], &[T::Gamma22], &[]), &[]);
        let why = negotiate(&rec2020(), &icc).why.unwrap();
        assert!(why.contains("parametric"), "{why}");
    }

    #[test]
    fn custom_primaries_are_tagged_bt2020() {
        let custom = DisplayTarget {
            primaries: Primaries::Custom {
                r: [0.69, 0.30],
                g: [0.20, 0.72],
                b: [0.15, 0.05],
                w: display::D65,
            },
            ..rec2020()
        };
        let Route::Tagged(t) = negotiate(&custom, &kwin()).route else {
            panic!()
        };
        assert_eq!(t.primaries, Primaries::Bt2020);
    }

    #[test]
    fn the_layout_covers_the_rectangle_in_whole_logical_pixels() {
        // Scale 1.25: 400 by 300 window pixels are 320 by 240 logical.
        let l = layout([10.0, 20.0, 410.0, 320.0], 1.25).unwrap();
        assert_eq!(l.logical, [8, 16, 320, 240]);
        assert_eq!(l.buffer, [400, 300]);
        assert_eq!(l.origin, [10.0, 20.0]);
        // Off the grid: the surface covers it, and starts before.
        let l = layout([11.0, 21.0, 100.5, 50.0], 2.0).unwrap();
        assert_eq!(l.logical, [5, 10, 46, 15]);
        assert_eq!(l.buffer, [92, 30]);
        assert_eq!(l.origin, [10.0, 20.0]);
        assert_eq!(layout([5.0, 5.0, 5.0, 9.0], 1.0), None);
    }

    #[test]
    fn the_background_is_encoded_for_the_target() {
        let grey = Color32::from_gray(128);
        let srgb = clear_colour(grey, &DisplayTarget::srgb());
        assert!((srgb[0] - 128.0 / 255.0).abs() < 2e-3, "{srgb:?}");
        // Grey stays grey in BT.2020; a 2.2 power is darker than the
        // sRGB curve's toe but close at mid grey.
        let t = rec2020().with_source(TargetSource::Tagged);
        let c = clear_colour(grey, &t);
        assert!(
            (c[0] - c[1]).abs() < 1e-4 && (c[1] - c[2]).abs() < 1e-4,
            "{c:?}"
        );
        assert!((c[0] - srgb[0]).abs() < 0.02, "{c:?}");
        assert_eq!(c[3], 1.0);
    }
}
