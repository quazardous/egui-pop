//! The image's own surface (feature `wayland`): on a Wayland compositor
//! with colour management, the viewport draws its image into a surface
//! of its own, below the window, that the compositor is told the
//! encoding of; egui's interface stays sRGB on the window's surface
//! (`docs/wayland-colour.md`).
//!
//! <div class="warning">Experimental: may change in any release
//! (docs/stability.md).</div>
//!
//! - [`install`] (once, with eframe's creation context) joins the
//!   compositor through the main window's handles and records what it
//!   offers ([`Offer`]): its colour manager's announcements, the formats
//!   and colour spaces a subsurface can be presented in.
//! - Every other window gets an image surface of its own, under its own
//!   surface and negotiated for what that window wants: set up with its
//!   handles ([`install_window`]), or the first frame it draws the image
//!   once [`set_window_source`] says where windows' handles are read;
//!   freed when the window closes ([`close_window`], or at the next frame
//!   once egui no longer knows the window). egui's interface keeps
//!   egui-wgpu's one surface format in every window.
//! - Each frame, a window's viewports [`negotiate`] what the application
//!   wants for that window ([`display::want`]) against its offer
//!   ([`negotiate_window`]):
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
//! image description for each window; the window registry
//! (`windows.list`) shows each window's (`egui_pop_core::window_surface`).

use std::collections::HashMap;
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
    /// The window was built transparent
    /// (`ViewportBuilder::with_transparent(true)`): required, the image
    /// surface shows through a hole in it.
    pub transparent_window: bool,
}

/// A window's raw handles, as [`set_window_source`] gives them.
pub trait WindowHandles:
    raw_window_handle::HasWindowHandle + raw_window_handle::HasDisplayHandle
{
}

impl<T: raw_window_handle::HasWindowHandle + raw_window_handle::HasDisplayHandle + ?Sized>
    WindowHandles for T
{
}

/// Where the handles of a window opened after [`install`] are read.
type Source = Arc<dyn Fn(ViewportId) -> Option<Box<dyn WindowHandles>> + Send + Sync>;

/// One window's image surface: what its platform offers, what was
/// negotiated there, and the surfaces made under it.
#[derive(Default)]
struct WindowSurface {
    offer: Offer,
    negotiated: Option<(DisplayTarget, Negotiation)>,
    /// Why the image surface failed (it then stays on the window until
    /// the wish changes).
    failure: Option<String>,
    #[cfg(target_os = "linux")]
    manager: Option<wl::Manager>,
}

impl WindowSurface {
    fn new(offer: Offer) -> WindowSurface {
        WindowSurface {
            offer,
            ..WindowSurface::default()
        }
    }
}

#[derive(Default)]
struct Shared {
    /// The render state [`install`] was given, for the windows after.
    render: Option<egui_wgpu::RenderState>,
    /// The image surface of each window, the main window's from
    /// [`install`], another's once it draws the image.
    windows: HashMap<ViewportId, WindowSurface>,
    /// Where another window's handles are read, and how it was built.
    source: Option<(Source, Options)>,
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

/// What the platform offers a window's image surface: joins the
/// compositor through the window's own handles, with a manager of its
/// own (one parent surface per window).
fn offer_for(
    rs: &egui_wgpu::RenderState,
    window: &(impl raw_window_handle::HasWindowHandle + raw_window_handle::HasDisplayHandle),
    options: Options,
) -> WindowSurface {
    let mode = std::env::var(VARIABLE).unwrap_or_default();
    if mode == "off" {
        return WindowSurface::new(Offer::unavailable(format!("turned off ({VARIABLE}=off)")));
    }
    if !options.transparent_window {
        return WindowSurface::new(Offer::unavailable(
            "the window is opaque: the image surface sits below it and needs a transparent \
             window (ViewportBuilder::with_transparent)",
        ));
    }
    #[cfg(target_os = "linux")]
    match wl::Manager::new(rs, window) {
        Ok((manager, mut offer)) => {
            offer.may_tag = mode != "no-tag";
            WindowSurface {
                manager: Some(manager),
                ..WindowSurface::new(offer)
            }
        }
        Err(e) => WindowSurface::new(Offer::unavailable(e)),
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = (rs, window);
        WindowSurface::new(Offer::unavailable(
            "not a Wayland window (X11 or another system)",
        ))
    }
}

/// The image surfaces of `ctx`, made empty the first time.
fn surfaces(ctx: &Context) -> ImageSurfaces {
    SURFACES.get_or_insert_with(ctx, || {
        ImageSurfaces(Arc::new(Mutex::new(Shared::default())))
    })
}

/// Sets up image surfaces for the main window: `rs` is the render state
/// the GPU path was installed on ([`crate::gpu::install`]), `window` the
/// main window's handles (eframe's creation context). Returns what the
/// platform offers; viewports in the main window use it from the next
/// frame. Other windows get theirs with [`install_window`], or as they
/// draw the image once [`set_window_source`] says where their handles
/// are.
// upstream: winit binds no colour management and egui-wgpu takes no
// colour space, hence a surface of the image's own (docs/upstream.md).
pub fn install(
    ctx: &Context,
    rs: &egui_wgpu::RenderState,
    window: &(impl raw_window_handle::HasWindowHandle + raw_window_handle::HasDisplayHandle),
    options: Options,
) -> Offer {
    let surfaces = surfaces(ctx);
    let window = offer_for(rs, window, options);
    let offer = window.offer.clone();
    {
        let mut s = surfaces.lock();
        s.render = Some(rs.clone());
        s.windows.insert(ViewportId::ROOT, window);
    }
    ctx.request_repaint();
    offer
}

/// Sets up the image surface of another window (`viewport`), with its
/// own handles, after [`install`] (whose render state it shares): its
/// own parent surface, negotiated for what that window wants. Returns
/// what the platform offers it; it is freed when the window closes
/// ([`close_window`]). Without [`install`] first, none and why.
pub fn install_window(
    ctx: &Context,
    viewport: ViewportId,
    window: &(impl raw_window_handle::HasWindowHandle + raw_window_handle::HasDisplayHandle),
    options: Options,
) -> Offer {
    let surfaces = surfaces(ctx);
    let mut s = surfaces.lock();
    let w = match s.render.clone() {
        Some(rs) => offer_for(&rs, window, options),
        None => WindowSurface::new(Offer::unavailable(
            "the image surfaces are not installed (image_surface::install, with the main window)",
        )),
    };
    let offer = w.offer.clone();
    s.windows.insert(viewport, w);
    drop(s);
    ctx.request_repaint_of(viewport);
    offer
}

/// Says where the handles of a window opened after [`install`] are read
/// (`egui_pop::api::runner::window` on egui-pop's runner), and how such
/// windows are built (`options`): a window drawing the image gets its own
/// image surface the first frame its handles are known, freed when it
/// closes. A window whose builder said how it was built
/// (`egui_pop_core::window_surface::set_transparency`) is taken as said:
/// transparent, it gets a surface whatever `options` say; opaque, none,
/// with the builder's reason.
pub fn set_window_source<W>(
    ctx: &Context,
    options: Options,
    source: impl Fn(ViewportId) -> Option<W> + Send + Sync + 'static,
) where
    W: raw_window_handle::HasWindowHandle + raw_window_handle::HasDisplayHandle + 'static,
{
    let source: Source =
        Arc::new(move |v| source(v).map(|w| Box::new(w) as Box<dyn WindowHandles>));
    surfaces(ctx).lock().source = Some((source, options));
}

/// Records what a window's platform offers without joining it: no
/// surface is made, only the negotiation and its state (a test's fake
/// compositor, or a host presenting the image surface itself). The
/// viewport's widget then sends the image to that surface: use it where
/// something presents it.
pub fn install_offer(ctx: &Context, viewport: ViewportId, offer: Offer) {
    surfaces(ctx)
        .lock()
        .windows
        .insert(viewport, WindowSurface::new(offer));
    ctx.request_repaint_of(viewport);
}

/// Frees a window's image surface (the window closed): its subsurfaces
/// and connection go, its state is forgotten. Windows egui no longer
/// knows are freed this way at the next frame.
pub fn close_window(ctx: &Context, viewport: ViewportId) {
    if let Some(surfaces) = SURFACES.get(ctx) {
        let gone = surfaces.lock().windows.remove(&viewport);
        drop(gone);
    }
    display::set_image_surface(ctx, viewport, display::ImageSurface::default());
    egui_pop_core::window_surface::set(ctx, viewport, None);
}

/// What [`install`] found for the main window, if it ran.
pub fn offer(ctx: &Context) -> Option<Offer> {
    offer_of(ctx, ViewportId::ROOT)
}

/// What the platform offers a window's image surface, once set up.
pub fn offer_of(ctx: &Context, viewport: ViewportId) -> Option<Offer> {
    let surfaces = SURFACES.get(ctx)?;
    let s = surfaces.lock();
    s.windows.get(&viewport).map(|w| w.offer.clone())
}

/// The windows with an image surface set up, the main window first.
pub fn windows(ctx: &Context) -> Vec<ViewportId> {
    let Some(surfaces) = SURFACES.get(ctx) else {
        return Vec::new();
    };
    let s = surfaces.lock();
    let mut out: Vec<ViewportId> = s.windows.keys().copied().collect();
    out.sort_by_key(|v| (*v != ViewportId::ROOT, format!("{v:?}")));
    out
}

/// A window's image surface as `windows.list` shows it.
fn state(wanted: &DisplayTarget, n: &Negotiation) -> Json {
    let target = n.route.target();
    json!({
        "kind": "image",
        "route": n.route.name(),
        "wanted": wanted.describe(),
        "format": target.map(|t| format!("{:?}", t.surface_format)),
        "colour_space": target.map(|t| t.describe()),
        "why": n.why,
    })
}

/// Where the image of `viewport` goes for what it wants
/// ([`display::want`]): negotiates against that window's offer when the
/// wish changes, records the image surface's target and why
/// ([`display::set_image_surface`], and the window's entry in
/// `windows.list`). The viewport calls it each frame it draws the image
/// on the GPU path; call it yourself only with an offer from
/// [`install_offer`] (tests).
pub fn negotiate_window(ctx: &Context, viewport: ViewportId) -> Negotiation {
    let window = |why: Option<String>| Negotiation {
        route: Route::Window,
        why,
    };
    let Some(surfaces) = SURFACES.get(ctx) else {
        return window(None);
    };
    let wanted = display::wanted_of(ctx, viewport);
    // Windows egui no longer knows closed: their surfaces go.
    let open: Vec<ViewportId> = ctx.input(|i| i.raw.viewports.keys().copied().collect());
    let closed: Vec<ViewportId> = if open.contains(&ViewportId::ROOT) {
        let s = surfaces.lock();
        s.windows
            .keys()
            .filter(|v| **v != ViewportId::ROOT && !open.contains(v))
            .copied()
            .collect()
    } else {
        Vec::new()
    };
    for v in closed {
        close_window(ctx, v);
    }
    // How the window was built, when its builder said it: an opaque
    // window gets no surface, whatever the source's options say.
    let built = egui_pop_core::window_surface::transparency(ctx, viewport);
    let opaque = built
        .as_ref()
        .and_then(|t| t.why_opaque())
        .map(|why| format!("the window is opaque ({why}): the image surface sits below it"));
    let negotiation = {
        let mut s = surfaces.lock();
        let mut no_render = false;
        if !s.windows.contains_key(&viewport) && wider_than_srgb(&wanted) && opaque.is_none() {
            let handles = s.source.as_ref().map(|(source, o)| (source(viewport), *o));
            if let Some((Some(handles), mut options)) = handles {
                if built.as_ref().is_some_and(|t| t.is_transparent()) {
                    options.transparent_window = true;
                }
                match s.render.clone() {
                    Some(rs) => {
                        let w = offer_for(&rs, &handles, options);
                        s.windows.insert(viewport, w);
                    }
                    None => no_render = true,
                }
            }
        }
        match s.windows.get_mut(&viewport) {
            None => window(wider_than_srgb(&wanted).then(|| {
                if let Some(why) = &opaque {
                    why.clone()
                } else if no_render {
                    "the image surfaces are not installed (image_surface::install, with the \
                     main window)"
                        .to_string()
                } else if s.source.is_some() {
                    "the window's handles are not known yet".to_string()
                } else {
                    "no image surface for this window (image_surface::install_window, or \
                     set_window_source)"
                        .to_string()
                }
            })),
            Some(w) => {
                #[cfg(target_os = "linux")]
                if let Some(Err(e)) = w.manager.as_mut().map(wl::Manager::poll) {
                    w.failure = Some(format!("the compositor's events: {e}"));
                }
                if w.negotiated.as_ref().is_none_or(|(old, _)| *old != wanted) {
                    w.failure = None;
                    let n = negotiate(&wanted, &w.offer);
                    w.negotiated = Some((wanted, n));
                }
                let mut n = w
                    .negotiated
                    .as_ref()
                    .map(|(_, n)| n.clone())
                    .unwrap_or(window(None));
                if let Some(f) = &w.failure
                    && n.route != Route::Window
                {
                    n = window(Some(format!("the image surface failed: {f}")));
                }
                #[cfg(target_os = "linux")]
                if let Some(m) = w.manager.as_mut() {
                    if n.route == Route::Window {
                        m.clear();
                    } else {
                        m.collect(ctx.cumulative_pass_nr());
                    }
                }
                n
            }
        }
    };
    display::set_image_surface(
        ctx,
        viewport,
        display::ImageSurface {
            target: negotiation.route.target(),
            why: negotiation.why.clone(),
        },
    );
    egui_pop_core::window_surface::set(ctx, viewport, Some(state(&wanted, &negotiation)));
    negotiation
}

/// Where the image of the window `viewport` goes, on the GPU path
/// ([`negotiate_window`]); the image surfaces when it has one.
pub(crate) fn frame(ctx: &Context, viewport: ViewportId) -> Option<(ImageSurfaces, Route)> {
    let surfaces = SURFACES.get(ctx)?;
    let n = negotiate_window(ctx, viewport);
    (n.route != Route::Window).then_some((surfaces, n.route))
}

/// One viewport's image for its image surface, and the hole for it in
/// the window.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
pub(crate) struct ImagePaint {
    pub ctx: Context,
    pub surfaces: ImageSurfaces,
    /// The window it is drawn in (whose image surface takes it).
    pub viewport: ViewportId,
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
            let Some(w) = s.windows.get_mut(&self.viewport) else {
                return Vec::new();
            };
            let result = match w.manager.as_mut() {
                Some(m) => m.render(self, device, queue, r),
                None => Ok(false),
            };
            match result {
                Ok(shown) => self.shown.store(shown, Ordering::Relaxed),
                Err(e) => {
                    w.failure = Some(e);
                    self.ctx.request_repaint_of(self.viewport);
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

/// One window's image surface, for the debug bridge.
fn report_window(viewport: ViewportId, w: &WindowSurface) -> Json {
    let colour = w.offer.colour.as_ref().map(|c| {
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
    let negotiated = w.negotiated.as_ref().map(|(wanted, n)| {
        json!({
            "wanted": wanted.describe(),
            "route": n.route.name(),
            "target": n.route.target().map(|t| format!("{} ({:?})", t.describe(), t.surface_format)),
            "why": n.why,
        })
    });
    #[cfg(target_os = "linux")]
    let manager = w.manager.as_ref().map(wl::Manager::report);
    #[cfg(not(target_os = "linux"))]
    let manager: Option<Json> = None;
    json!({
        "window": format!("{viewport:?}"),
        "unavailable": w.offer.unavailable,
        "colour_management": colour,
        "wsi": w.offer.wsi.iter().map(|(f, c)| format!("{f:?} {c:?}")).collect::<Vec<_>>(),
        "may_tag": w.offer.may_tag,
        "negotiated": negotiated,
        "failure": w.failure,
        "surfaces": manager,
    })
}

/// What the image surfaces of `ctx` are doing, for the debug bridge: the
/// main window's at the top level (as before), every window's in
/// `windows`.
pub fn report(ctx: &Context) -> Json {
    let Some(surfaces) = SURFACES.get(ctx) else {
        return json!({ "installed": false });
    };
    let order = windows(ctx);
    let s = surfaces.lock();
    let mut out = s
        .windows
        .get(&ViewportId::ROOT)
        .map(|w| report_window(ViewportId::ROOT, w))
        .unwrap_or_else(|| json!({}));
    out["installed"] = json!(s.windows.contains_key(&ViewportId::ROOT));
    out["windows"] = Json::Array(
        order
            .iter()
            .filter_map(|v| s.windows.get(v).map(|w| report_window(*v, w)))
            .collect(),
    );
    out["window_source"] = json!(s.source.is_some());
    out
}

#[cfg(target_os = "linux")]
mod wl {
    //! The Wayland image surfaces of one window: one subsurface and wgpu
    //! surface per viewport drawn in it, under that window's surface.

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
            r.ensure_image_pipelines(device, format);
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
                    side.draw(&mut pass, r, &r.image_pipelines[&format]);
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

    /// One pass of `ctx` with `raw`, its output dropped.
    fn pass(ctx: &Context, raw: egui::RawInput) {
        ctx.run_ui(raw, |_| {}).textures_delta.clear();
    }

    /// A context whose egui knows the main window and `others`.
    fn context_with(others: &[ViewportId]) -> Context {
        let ctx = Context::default();
        let mut raw = egui::RawInput::default();
        raw.viewports
            .insert(ViewportId::ROOT, egui::ViewportInfo::default());
        for v in others {
            raw.viewports.insert(*v, egui::ViewportInfo::default());
        }
        pass(&ctx, raw);
        ctx
    }

    #[test]
    fn each_window_gets_its_own_surface_negotiated_for_what_it_wants() {
        let wide = ViewportId::from_hash_of("wide viewer");
        let plain = ViewportId::from_hash_of("plain viewer");
        let ctx = context_with(&[wide, plain]);
        // The main window on GNOME's offer, the two viewers on KWin's.
        install_offer(&ctx, ViewportId::ROOT, gnome());
        install_offer(&ctx, wide, kwin());
        install_offer(&ctx, plain, kwin());
        display::want(&ctx, wide, rec2020());
        display::want(&ctx, plain, DisplayTarget::srgb());
        display::want(&ctx, ViewportId::ROOT, rec2020());

        let n = negotiate_window(&ctx, wide);
        let Route::Tagged(t) = n.route else {
            panic!("{n:?}")
        };
        assert_eq!(t.surface_format, SurfaceFormat::Rgb10a2Unorm);
        assert_eq!(negotiate_window(&ctx, plain).route, Route::Window);
        let Route::Wsi(main) = negotiate_window(&ctx, ViewportId::ROOT).route else {
            panic!()
        };
        assert_eq!(main.color_space(), Some(ColorSpace::DisplayP3));

        // Each window's image surface, apart.
        let of = |v| display::image_surface_of(&ctx, v).and_then(|i| i.target);
        assert_eq!(of(wide), Some(t));
        assert_eq!(of(plain), None);
        assert_eq!(of(ViewportId::ROOT), Some(main));
        assert_eq!(display::target_of(&ctx, wide), t);
        // What windows.list shows of each.
        let state = egui_pop_core::window_surface::get(&ctx, wide).unwrap();
        assert_eq!(state["route"], "tagged");
        assert_eq!(state["format"], "Rgb10a2Unorm");
        assert!(
            state["colour_space"].as_str().unwrap().contains("2020"),
            "{state}"
        );
        let state = egui_pop_core::window_surface::get(&ctx, plain).unwrap();
        assert_eq!(state["route"], "window");
        assert_eq!(state["format"], Json::Null);
        let r = report(&ctx);
        assert_eq!(r["windows"].as_array().unwrap().len(), 3, "{r}");
        assert_eq!(r["windows"][0]["window"], format!("{:?}", ViewportId::ROOT));
    }

    #[test]
    fn closing_a_window_frees_its_surface() {
        let viewer = ViewportId::from_hash_of("viewer");
        let ctx = context_with(&[viewer]);
        install_offer(&ctx, ViewportId::ROOT, kwin());
        install_offer(&ctx, viewer, kwin());
        display::want(&ctx, viewer, rec2020());
        display::want(&ctx, ViewportId::ROOT, rec2020());
        assert!(matches!(
            negotiate_window(&ctx, viewer).route,
            Route::Tagged(_)
        ));
        close_window(&ctx, viewer);
        assert_eq!(offer_of(&ctx, viewer), None);
        assert_eq!(windows(&ctx), vec![ViewportId::ROOT]);
        assert_eq!(egui_pop_core::window_surface::get(&ctx, viewer), None);
        assert_eq!(
            display::image_surface_of(&ctx, viewer).and_then(|i| i.target),
            None
        );

        // A window egui no longer knows is freed at the next frame.
        install_offer(&ctx, viewer, kwin());
        let _ = negotiate_window(&ctx, viewer);
        let mut raw = egui::RawInput::default();
        raw.viewports
            .insert(ViewportId::ROOT, egui::ViewportInfo::default());
        pass(&ctx, raw);
        let _ = negotiate_window(&ctx, ViewportId::ROOT);
        assert_eq!(windows(&ctx), vec![ViewportId::ROOT]);
        assert_eq!(egui_pop_core::window_surface::get(&ctx, viewer), None);
        // The main window's stays.
        assert!(matches!(
            negotiate_window(&ctx, ViewportId::ROOT).route,
            Route::Tagged(_)
        ));
    }

    #[test]
    fn a_window_without_a_surface_says_why() {
        let viewer = ViewportId::from_hash_of("viewer");
        let ctx = context_with(&[viewer]);
        install_offer(&ctx, ViewportId::ROOT, kwin());
        display::want(&ctx, viewer, rec2020());
        let n = negotiate_window(&ctx, viewer);
        assert_eq!(n.route, Route::Window);
        assert!(n.why.unwrap().contains("install_window"));
        // Told where handles are, but none yet for this window.
        set_window_source(&ctx, Options::default(), |_| None::<NoHandles>);
        let why = negotiate_window(&ctx, viewer).why.unwrap();
        assert!(why.contains("not known yet"), "{why}");
        // Handles refused: the window's offer says why, kept.
        set_window_source(&ctx, Options::default(), |_| Some(NoHandles));
        // No render state: install never ran with a GPU.
        assert_eq!(offer_of(&ctx, viewer), None);
    }

    #[test]
    fn a_window_built_opaque_says_why_its_builder_gave() {
        use egui_pop_core::window_surface::{Transparency, set_transparency};
        let viewer = ViewportId::from_hash_of("viewer");
        let ctx = context_with(&[viewer]);
        install_offer(&ctx, ViewportId::ROOT, kwin());
        display::want(&ctx, viewer, rec2020());
        set_window_source(&ctx, Options::default(), |_| Some(NoHandles));
        set_transparency(
            &ctx,
            viewer,
            Some(Transparency::Opaque("X11: no subsurface".into())),
        );
        let n = negotiate_window(&ctx, viewer);
        assert_eq!(n.route, Route::Window);
        let why = n.why.unwrap();
        assert!(why.contains("opaque") && why.contains("X11"), "{why}");
        assert_eq!(offer_of(&ctx, viewer), None);
        // Transparent, handles known, but no GPU installed: said so, not
        // "handles not known".
        set_transparency(&ctx, viewer, Some(Transparency::Transparent));
        let why = negotiate_window(&ctx, viewer).why.unwrap();
        assert!(why.contains("not installed"), "{why}");
    }

    struct NoHandles;

    impl raw_window_handle::HasWindowHandle for NoHandles {
        fn window_handle(
            &self,
        ) -> Result<raw_window_handle::WindowHandle<'_>, raw_window_handle::HandleError> {
            Err(raw_window_handle::HandleError::Unavailable)
        }
    }

    impl raw_window_handle::HasDisplayHandle for NoHandles {
        fn display_handle(
            &self,
        ) -> Result<raw_window_handle::DisplayHandle<'_>, raw_window_handle::HandleError> {
            Err(raw_window_handle::HandleError::Unavailable)
        }
    }
}
