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
//! - **Where the pixels are converted**: by default the sink converts
//!   each tile on the thread that delivers it (the display transform on
//!   the CPU path, the half-float texels on the GPU path), so tiles
//!   arrive ready to upload and the interface thread only hands them to
//!   the GPU, within an upload budget per frame. With
//!   [`ViewportState::set_prepare_on_worker`](crate::ViewportState::set_prepare_on_worker)`(false)`,
//!   the interface thread converts each tile when it takes it in.

use std::fmt;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Mutex};

use egui::{Color32, ColorImage};

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
    /// Already on the GPU, rendered by the source on the GPU path's
    /// device ([`TileSink::deliver_texture`]): nothing to copy.
    #[cfg(feature = "wgpu")]
    Texture(egui_wgpu::wgpu::Texture),
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
            Body::Texture(_) => 0,
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
    /// Changes folded into one change of the whole image, and readouts
    /// dropped, while the viewport was not drawn and
    /// [`NOTICES`] were already waiting.
    pub coalesced: AtomicU64,
}

impl Stats {
    /// The counters, by name.
    pub fn snapshot(&self) -> [(&'static str, u64); 8] {
        let g = |a: &AtomicU64| a.load(Ordering::Relaxed);
        [
            ("requested", g(&self.requested)),
            ("delivered", g(&self.delivered)),
            ("dropped", g(&self.dropped)),
            ("cancelled", g(&self.cancelled)),
            ("failed", g(&self.failed)),
            ("evicted", g(&self.evicted)),
            ("changed", g(&self.changed)),
            ("coalesced", g(&self.coalesced)),
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

/// How many changes and readouts wait at most for the viewport to take
/// them in. Tiles and failures answer requests, which the viewport makes
/// only when drawn; changes and readouts come when the source says, so a
/// viewport no longer drawn (a tab behind another) would keep them all.
/// Past this many, a change becomes one change of the whole image, kept
/// as a flag, and a readout is dropped (the next one replaces it).
pub const NOTICES: usize = 256;

/// What waits on the channel beyond the answers to requests.
#[derive(Debug, Default)]
pub(crate) struct Backlog {
    /// Changes and readouts sent and not taken in yet.
    queued: AtomicUsize,
    /// A side changed everywhere, its changes folded past [`NOTICES`]
    /// (after, before).
    whole: [AtomicBool; 2],
    /// Changes folded and readouts dropped since last taken in.
    coalesced: AtomicU64,
}

impl Backlog {
    fn slot(side: Side) -> usize {
        match side {
            Side::After => 0,
            Side::Before => 1,
        }
    }

    /// Whether a notice may go on the channel; counts it if so.
    fn admit(&self) -> bool {
        let admitted = self
            .queued
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |n| {
                (n < NOTICES).then_some(n + 1)
            })
            .is_ok();
        if !admitted {
            self.coalesced.fetch_add(1, Ordering::Relaxed);
        }
        admitted
    }

    /// A notice taken in from the channel.
    pub(crate) fn taken(&self) {
        let _ = self
            .queued
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |n| n.checked_sub(1));
    }

    /// The sides whose changes were folded, cleared, and how many
    /// notices were folded or dropped since the last call.
    pub(crate) fn take_folded(&self) -> (Vec<Side>, u64) {
        let sides = [Side::After, Side::Before]
            .into_iter()
            .filter(|s| self.whole[Self::slot(*s)].swap(false, Ordering::AcqRel))
            .collect();
        (sides, self.coalesced.swap(0, Ordering::Relaxed))
    }

    /// Changes and readouts waiting.
    pub(crate) fn queued(&self) -> usize {
        self.queued.load(Ordering::Acquire)
    }
}

/// Where a source delivers; cheap to clone, usable from any thread.
#[derive(Clone)]
pub struct TileSink {
    side: Side,
    tx: Sender<Message>,
    waker: Arc<Waker>,
    prep: Arc<Mutex<Option<Prep>>>,
    backlog: Arc<Backlog>,
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

    /// Delivers a tile the source rendered on the GPU (feature `wgpu`;
    /// a prototype, measured in docs/threading.md): `texture` holds
    /// scene-referred pixels as [`Pixels::Scene`] does, in the GPU
    /// path's format (`Rgba16Float` when [`GpuInfo::half_float`], else
    /// `Rgba32Float`), at least `request.size`, with `TEXTURE_BINDING`
    /// usage; it was created on the device the GPU path was installed
    /// with, and the work writing it was submitted to that device's
    /// queue before this call. Nothing is copied: the interface thread
    /// only binds it. Without the GPU path, the tile fails.
    ///
    /// [`GpuInfo::half_float`]: crate::gpu::GpuInfo::half_float
    #[cfg(feature = "wgpu")]
    pub fn deliver_texture(&self, request: &TileRequest, texture: egui_wgpu::wgpu::Texture) {
        self.send(Message::Tile(
            self.side,
            Arrival {
                request: request.id,
                key: request.key,
                size: request.size,
                body: Body::Texture(texture),
            },
        ));
    }

    /// The image changed in `region` (`[x0, y0, x1, y1]`, image pixels),
    /// or everywhere (`None`, also for a new size). While [`NOTICES`]
    /// changes and readouts wait (the viewport is not drawn), it is
    /// folded into one change of the whole image.
    pub fn changed(&self, region: Option<[f64; 4]>) {
        if self.backlog.admit() {
            self.send(Message::Changed(self.side, region));
        } else {
            self.backlog.whole[Backlog::slot(self.side)].store(true, Ordering::Release);
            self.waker.wake();
        }
    }

    /// A request could not be served.
    pub fn failed(&self, request: RequestId, error: impl Into<String>) {
        self.send(Message::Failed(self.side, request, error.into()));
    }

    /// Answers [`TileSource::readout`]. Dropped while [`NOTICES`]
    /// changes and readouts wait (the viewport is not drawn).
    pub fn readout(&self, pixel: [i64; 2], values: Vec<(String, f32)>) {
        if !self.backlog.admit() {
            return;
        }
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
    /// The changes and readouts waiting, bounded.
    pub(crate) backlog: Arc<Backlog>,
}

impl Default for Feed {
    fn default() -> Self {
        let (tx, rx) = channel();
        Feed {
            tx,
            rx,
            waker: Arc::default(),
            prep: Arc::default(),
            backlog: Arc::default(),
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
            backlog: self.backlog.clone(),
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
