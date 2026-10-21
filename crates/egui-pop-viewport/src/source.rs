//! Where the pixels come from: a [`TileSource`] the application
//! implements, asynchronous and live.
//!
//! - The viewport **asks** for tiles ([`TileSource::request`]): square
//!   tiles of [`TileRequest::size`] pixels at a scale of the image, first
//!   a *proxy* of the whole image at a coarse scale, then the *exact*
//!   tiles of what is visible at the view's own scale. A call never
//!   blocks: the source works elsewhere (a thread, an engine, a server).
//! - The source **delivers** through a [`TileSink`] (from
//!   [`ViewportState::sink`](crate::ViewportState::sink)), from any thread;
//!   the sink wakes the interface.
//! - **Cancellation**: a request the view no longer needs (the user
//!   panned on) has its [`CancelToken`] set; the source may check it and
//!   drop the work. A late tile is dropped by the viewport anyway.
//! - **Live content**: when the image changes (a parameter moved), the
//!   source says which region with [`TileSink::changed`]; the tiles there
//!   stay on screen until their replacements arrive.
//! - **Pixels** come either display-ready ([`Pixels::Display`], the
//!   source applied its own display transform on the CPU) or
//!   scene-referred ([`Pixels::Scene`], linear floats the viewport passes
//!   through the display transform, on the GPU when it can).
//! - **Device tiles** (feature `wgpu`): a source rendering on the GPU
//!   hands over a texture or a storage buffer that lives on the render
//!   device (`TileSink::deliver_device`, see the `device` module);
//!   nothing is copied, and the display transform runs in the shader as
//!   for scene pixels.
//! - **Where the pixels are converted**: by default the sink converts
//!   each tile on the thread that delivers it (the display transform on
//!   the CPU path, the half-float texels on the GPU path), so tiles
//!   arrive ready to upload and the interface thread only hands them to
//!   the GPU, within an upload budget per frame. With
//!   [`ViewportState::set_prepare_on_worker`](crate::ViewportState::set_prepare_on_worker)`(false)`,
//!   the interface thread converts each tile when it takes it in.

use std::fmt;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Mutex};

use egui::{Color32, ColorImage};

#[cfg(feature = "wgpu")]
use crate::device::{DeviceTile, RenderDevice};
use crate::display::{self, DisplayTarget, DisplayTransform};

/// A tile's quality.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Quality {
    /// Part of the coarse picture of the whole image, shown first.
    Proxy,
    /// What is visible, at the view's own scale.
    Exact,
}

impl Quality {
    pub fn name(self) -> &'static str {
        match self {
            Quality::Proxy => "proxy",
            Quality::Exact => "exact",
        }
    }
}

/// A scale of the image (scaled pixels per image pixel, at most 1),
/// hashable.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Scale(u64);

impl Scale {
    pub fn new(s: f64) -> Scale {
        Scale(s.to_bits())
    }

    pub fn get(self) -> f64 {
        f64::from_bits(self.0)
    }
}

impl fmt::Debug for Scale {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Scale({})", self.get())
    }
}

/// Which image a tile belongs to: the one edited, or the "before" of a
/// comparison.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Side {
    After,
    Before,
}

/// A tile: column `x` and row `y` of the grid of `size`-pixel squares
/// covering the image scaled by `scale`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TileKey {
    pub scale: Scale,
    pub x: i64,
    pub y: i64,
}

/// A request's id, unique per viewport state.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct RequestId(pub u64);

/// Set when the viewport no longer needs a request.
#[derive(Clone, Debug, Default)]
pub struct CancelToken(Arc<AtomicBool>);

impl CancelToken {
    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::Relaxed)
    }

    pub(crate) fn cancel(&self) {
        self.0.store(true, Ordering::Relaxed);
    }
}

/// One tile wanted.
#[derive(Clone, Debug)]
pub struct TileRequest {
    pub id: RequestId,
    pub key: TileKey,
    pub quality: Quality,
    /// The scaled image's pixels the tile covers: `[x, y]` (top left)
    /// and `[w, h]`, clipped to the scaled image.
    pub origin: [i64; 2],
    pub size: [u32; 2],
    pub cancel: CancelToken,
}

impl TileRequest {
    /// The scale (scaled pixels per image pixel).
    pub fn scale(&self) -> f64 {
        self.key.scale.get()
    }

    /// Whether the viewport gave up on it.
    pub fn is_cancelled(&self) -> bool {
        self.cancel.is_cancelled()
    }

    /// The image point sampled by the centre of scaled pixel `(i, j)` of
    /// this tile (`i`, `j` from 0): how a source samples consistently.
    pub fn image_point(&self, i: u32, j: u32) -> [f64; 2] {
        let s = self.scale();
        [
            (self.origin[0] as f64 + f64::from(i) + 0.5) / s,
            (self.origin[1] as f64 + f64::from(j) + 0.5) / s,
        ]
    }
}

/// A tile's pixels, row by row, `size[0] × size[1]`.
#[derive(Clone, PartialEq)]
pub enum Pixels {
    /// RGBA, 8 bits, sRGB-encoded, not premultiplied: the source applied
    /// its display transform itself (the CPU path).
    Display(Vec<u8>),
    /// RGBA, linear floats in the source's working space: the viewport
    /// applies the display transform (the GPU path when installed).
    Scene(Vec<f32>),
}

impl fmt::Debug for Pixels {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Pixels::Display(v) => write!(f, "Display({} bytes)", v.len()),
            Pixels::Scene(v) => write!(f, "Scene({} floats)", v.len()),
        }
    }
}

impl Pixels {
    /// Bytes held on the CPU.
    pub fn bytes(&self) -> usize {
        match self {
            Pixels::Display(v) => v.len(),
            Pixels::Scene(v) => v.len() * 4,
        }
    }
}

/// A tile delivered.
#[derive(Clone, Debug)]
pub struct Tile {
    /// The request it answers.
    pub request: RequestId,
    pub key: TileKey,
    pub size: [u32; 2],
    pub pixels: Pixels,
}

impl Tile {
    /// A tile answering `request`.
    pub fn answer(request: &TileRequest, pixels: Pixels) -> Tile {
        Tile {
            request: request.id,
            key: request.key,
            size: request.size,
            pixels,
        }
    }
}

/// The values under a pixel, from the source.
#[derive(Clone, Debug, PartialEq)]
pub struct Readout {
    pub side: Side,
    pub pixel: [i64; 2],
    /// Named values (`R`, `G`, `B`, `L`...).
    pub values: Vec<(String, f32)>,
}

pub(crate) enum Message {
    Tile(Side, Arrival),
    Changed(Side, Option<[f64; 4]>),
    Failed(Side, RequestId, String),
    /// A device tile refused, and why.
    #[cfg(feature = "wgpu")]
    Refused(String),
    Readout(Readout),
}

/// What the delivering thread needs to convert a tile: the display
/// transform and target of the last frame, and the GPU path's texel
/// format. `stamp` changes with any of them.
#[derive(Clone, Debug)]
pub(crate) struct Prep {
    pub stamp: u64,
    pub transform: Arc<dyn DisplayTransform>,
    pub target: DisplayTarget,
    /// The GPU path is installed; `true` for half-float texels.
    pub gpu: Option<bool>,
}

/// A tile's pixels as they reach the interface thread.
#[derive(Debug)]
pub(crate) enum Body {
    /// As the source made them: converted on the interface thread.
    Raw(Pixels),
    /// Ready for an egui texture; `baked` when made from scene pixels
    /// through the transform of `stamp`.
    Image {
        image: ColorImage,
        baked: bool,
        stamp: u64,
    },
    /// Ready for a GPU path texture: texels in the format of `stamp`.
    Texels { data: Vec<u8>, stamp: u64 },
    /// Already on the render device ([`TileSink::deliver_device`]):
    /// bound, nothing copied.
    #[cfg(feature = "wgpu")]
    Device(Box<DeviceTile>),
}

impl Body {
    /// Bytes the upload copies.
    pub(crate) fn upload_bytes(&self, size: [u32; 2], gpu: bool) -> usize {
        let n = size[0] as usize * size[1] as usize;
        match self {
            Body::Raw(Pixels::Scene(_)) if gpu => n * 8,
            Body::Raw(_) | Body::Image { .. } => n * 4,
            Body::Texels { data, .. } => data.len(),
            #[cfg(feature = "wgpu")]
            Body::Device(_) => 0,
        }
    }
}

/// A tile delivered, as the viewport takes it in.
#[derive(Debug)]
pub(crate) struct Arrival {
    pub request: RequestId,
    pub key: TileKey,
    pub size: [u32; 2],
    pub body: Body,
}

/// The GPU path's texels for scene pixels: half floats, else 32-bit
/// floats, little endian.
pub(crate) fn texels(data: &[f32], half_float: bool) -> Vec<u8> {
    #[cfg(feature = "wgpu")]
    if half_float {
        return data
            .iter()
            .flat_map(|v| half::f16::from_f32(*v).to_bits().to_le_bytes())
            .collect();
    }
    let _ = half_float;
    data.iter().flat_map(|v| v.to_le_bytes()).collect()
}

impl Prep {
    /// Converts a tile's pixels on the calling thread; the pixels back
    /// when their size is wrong (the interface thread reports it).
    fn convert(&self, size: [u32; 2], pixels: Pixels) -> Body {
        // On the delivering thread: what the interface thread did when
        // taking the tile in.
        profiling::scope!("pop.tiles.prepare");
        let n = size[0] as usize * size[1] as usize;
        let ok = match &pixels {
            Pixels::Display(v) => v.len() == n * 4,
            Pixels::Scene(v) => v.len() == n * 4,
        };
        if !ok || n == 0 {
            return Body::Raw(pixels);
        }
        let dims = [size[0] as usize, size[1] as usize];
        match (pixels, self.gpu) {
            (Pixels::Scene(data), Some(half)) => Body::Texels {
                data: texels(&data, half),
                stamp: self.stamp,
            },
            (Pixels::Scene(data), None) => {
                let mut out = vec![Color32::BLACK; n];
                display::apply_cpu(self.transform.as_ref(), &self.target, &data, &mut out);
                Body::Image {
                    image: ColorImage::new(dims, out),
                    baked: true,
                    stamp: self.stamp,
                }
            }
            (Pixels::Display(bytes), _) => Body::Image {
                image: ColorImage::from_rgba_unmultiplied(dims, &bytes),
                baked: false,
                stamp: self.stamp,
            },
        }
    }
}

/// What a source does with the sink, counted; [`Stats::snapshot`] for
/// the debug bridge and the tests.
#[derive(Debug, Default)]
pub struct Stats {
    pub requested: AtomicU64,
    pub delivered: AtomicU64,
    /// Tiles that arrived for a request no longer wanted.
    pub dropped: AtomicU64,
    pub cancelled: AtomicU64,
    pub failed: AtomicU64,
    pub evicted: AtomicU64,
    pub changed: AtomicU64,
    /// Device tiles taken in (bound on the render device, not copied).
    pub device: AtomicU64,
    /// Device tiles refused (another device, no GPU path, a resource
    /// that does not fit), fallback or not.
    pub refused: AtomicU64,
}

impl Stats {
    /// The counters, by name.
    pub fn snapshot(&self) -> [(&'static str, u64); 9] {
        let g = |a: &AtomicU64| a.load(Ordering::Relaxed);
        [
            ("requested", g(&self.requested)),
            ("delivered", g(&self.delivered)),
            ("dropped", g(&self.dropped)),
            ("cancelled", g(&self.cancelled)),
            ("failed", g(&self.failed)),
            ("evicted", g(&self.evicted)),
            ("changed", g(&self.changed)),
            ("device", g(&self.device)),
            ("refused", g(&self.refused)),
        ]
    }

    pub(crate) fn add(a: &AtomicU64, n: u64) {
        a.fetch_add(n, Ordering::Relaxed);
    }
}

#[derive(Default)]
struct Waker(Mutex<Option<egui::Context>>);

impl Waker {
    fn wake(&self) {
        if let Some(ctx) = self
            .0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .as_ref()
        {
            ctx.request_repaint();
        }
    }
}

/// Where a source delivers; cheap to clone, usable from any thread.
#[derive(Clone)]
pub struct TileSink {
    side: Side,
    tx: Sender<Message>,
    waker: Arc<Waker>,
    prep: Arc<Mutex<Option<Prep>>>,
    #[cfg(feature = "wgpu")]
    render: Arc<Mutex<Option<RenderDevice>>>,
}

impl fmt::Debug for TileSink {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TileSink")
            .field("side", &self.side)
            .finish()
    }
}

impl TileSink {
    fn send(&self, m: Message) {
        // The viewport is gone: nothing to do.
        if self.tx.send(m).is_ok() {
            self.waker.wake();
        }
    }

    /// Delivers a tile. When the viewport prepares on the worker side,
    /// the pixels are converted here, on the calling thread.
    pub fn deliver(&self, tile: Tile) {
        let prep = self
            .prep
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone();
        let body = match prep {
            Some(p) => p.convert(tile.size, tile.pixels),
            None => Body::Raw(tile.pixels),
        };
        self.send(Message::Tile(
            self.side,
            Arrival {
                request: tile.request,
                key: tile.key,
                size: tile.size,
                body,
            },
        ));
    }

    /// Delivers a tile that lives on the render device (feature `wgpu`,
    /// see [`crate::device`]): bound by the GPU path, nothing copied.
    /// The work writing it must have been submitted to the render
    /// device's queue before this call. Checked here, on the calling
    /// thread, against the render device of the last frame (and again
    /// when the interface takes it in): a tile the viewport cannot draw
    /// is refused, the error kept
    /// ([`ViewportState::last_error`](crate::ViewportState::last_error)),
    /// and its fallback pixels, when it has some, delivered as
    /// [`TileSink::deliver`] does; without, the request fails.
    #[cfg(feature = "wgpu")]
    pub fn deliver_device(&self, request: &TileRequest, tile: DeviceTile) {
        let render = self.render_device();
        match tile.check(render.as_ref(), request.size) {
            Ok(()) => self.send(Message::Tile(
                self.side,
                Arrival {
                    request: request.id,
                    key: request.key,
                    size: request.size,
                    body: Body::Device(Box::new(tile)),
                },
            )),
            Err(e) => {
                let error = format!("device tile refused: {e}");
                match tile.fallback.and_then(|f| f()) {
                    Some(pixels) => {
                        self.send(Message::Refused(error));
                        self.deliver(Tile::answer(request, pixels));
                    }
                    None => {
                        self.send(Message::Refused(error.clone()));
                        self.failed(request.id, error);
                    }
                }
            }
        }
    }

    /// The render device device tiles must be created on, as of the last
    /// frame drawn (feature `wgpu`); `None` before it, or without a GPU
    /// path.
    #[cfg(feature = "wgpu")]
    pub fn render_device(&self) -> Option<RenderDevice> {
        self.render
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }

    /// The image changed in `region` (`[x0, y0, x1, y1]`, image pixels),
    /// or everywhere (`None`, also for a new size).
    pub fn changed(&self, region: Option<[f64; 4]>) {
        self.send(Message::Changed(self.side, region));
    }

    /// A request could not be served.
    pub fn failed(&self, request: RequestId, error: impl Into<String>) {
        self.send(Message::Failed(self.side, request, error.into()));
    }

    /// Answers [`TileSource::readout`].
    pub fn readout(&self, pixel: [i64; 2], values: Vec<(String, f32)>) {
        self.send(Message::Readout(Readout {
            side: self.side,
            pixel,
            values,
        }));
    }
}

/// The receiving end, kept by the viewport state.
pub(crate) struct Feed {
    tx: Sender<Message>,
    pub(crate) rx: Receiver<Message>,
    waker: Arc<Waker>,
    /// Set when tiles are converted by the delivering thread.
    pub(crate) prep: Arc<Mutex<Option<Prep>>>,
    /// The render device of the last frame, for device tiles.
    #[cfg(feature = "wgpu")]
    pub(crate) render: Arc<Mutex<Option<RenderDevice>>>,
}

impl Default for Feed {
    fn default() -> Self {
        let (tx, rx) = channel();
        Feed {
            tx,
            rx,
            waker: Arc::default(),
            prep: Arc::default(),
            #[cfg(feature = "wgpu")]
            render: Arc::default(),
        }
    }
}

impl fmt::Debug for Feed {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Feed")
    }
}

impl Feed {
    pub(crate) fn sink(&self, side: Side) -> TileSink {
        TileSink {
            side,
            tx: self.tx.clone(),
            waker: self.waker.clone(),
            prep: self.prep.clone(),
            #[cfg(feature = "wgpu")]
            render: self.render.clone(),
        }
    }

    /// What the delivering threads convert with, or `None` to leave
    /// the conversion to the interface thread.
    pub(crate) fn set_prep(&self, prep: Option<Prep>) {
        *self
            .prep
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = prep;
    }

    /// The render device device tiles are checked against.
    #[cfg(feature = "wgpu")]
    pub(crate) fn set_render(&self, render: Option<RenderDevice>) {
        let mut r = self
            .render
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let same = match (&*r, &render) {
            (Some(a), Some(b)) => a.device == b.device,
            (None, None) => true,
            _ => false,
        };
        if !same {
            *r = render;
        }
    }

    pub(crate) fn set_context(&self, ctx: &egui::Context) {
        let mut w = self
            .waker
            .0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if w.is_none() {
            *w = Some(ctx.clone());
        }
    }
}

/// The application's pixels. Every method returns at once; work happens
/// elsewhere and comes back through the [`TileSink`].
pub trait TileSource {
    /// The image's size in pixels, `None` while unknown. Cheap: read on
    /// every frame.
    fn size(&self) -> Option<[u32; 2]>;

    /// New tiles wanted, most useful first. Requests the viewport stops
    /// wanting get their [`CancelToken`] set.
    fn request(&self, requests: Vec<TileRequest>);

    /// The pointer is over `pixel`: answer with
    /// [`TileSink::readout`] when the values are known (default: no
    /// readout).
    fn readout(&self, pixel: [i64; 2]) {
        let _ = pixel;
    }
}
