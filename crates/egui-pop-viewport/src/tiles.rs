//! The tile cache: what to ask the source for, what arrived, what is
//! drawn, and what to forget when over budget.
//!
//! Each frame, for each side shown:
//!
//! 1. take what the source delivered (a tile answering a request no
//!    longer wanted is dropped, a changed region marks its tiles stale)
//!    and upload it, within the upload budget when one is set: the
//!    tiles of the visible area first (proxy, then the view's scale,
//!    then other scales), then the others nearest the view's centre;
//!    the rest waits for the next frame, kept when the view moves;
//! 2. want the proxy (the whole image at a power of two at or below the
//!    fit scale) and, once the view has settled, the exact tiles of what
//!    is visible at the view's scale (`min(zoom, 1)`, so below 100 % one
//!    texel lands on one physical pixel);
//! 3. ask for what is wanted and neither cached nor asked for, cancel what
//!    was asked for and is no longer wanted;
//! 4. draw the proxy, then the exact tiles over it; stale tiles stay on
//!    screen until their replacements arrive;
//! 5. evict the least recently drawn tiles past the budget, never one
//!    drawn this frame.
//!
//! Tiles are textures of at most the device's largest texture side, so
//! an image or a window larger than the GPU's limit is drawn whole.

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use egui::{Color32, ColorImage, Context, TextureHandle, TextureOptions};

use crate::display::{self, DisplayTarget, DisplayTransform};
use crate::source::{
    Arrival, Body, CancelToken, Feed, Message, Pixels, Prep, Quality, Readout, RequestId, Scale,
    Side, Stats, TileKey, TileRequest, TileSink, TileSource,
};
use crate::view::{Mapping, fit_zoom};

/// The default side of a tile, in pixels.
pub const TILE_SIZE: u32 = 256;

/// The default texture budget, in bytes.
pub const DEFAULT_BUDGET: usize = 256 << 20;

/// The default time the view stays still before exact tiles are asked
/// for, in seconds.
pub const DEFAULT_SETTLE: f64 = 0.1;

/// The coarsest proxy scale.
pub const MIN_PROXY_SCALE: f64 = 1.0 / 64.0;

static NEXT_TEXTURE: AtomicU64 = AtomicU64::new(1);

/// A tile's texture.
pub(crate) enum Texture {
    /// An egui texture, sRGB 8 bits (display-ready pixels, or scene
    /// pixels transformed on the CPU).
    Egui(TextureHandle),
    /// A texture of the GPU path, by id (scene pixels, transformed in
    /// the shader).
    Gpu(u64),
}

impl std::fmt::Debug for Texture {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Texture::Egui(h) => write!(f, "Egui({:?})", h.id()),
            Texture::Gpu(id) => write!(f, "Gpu({id})"),
        }
    }
}

#[derive(Debug)]
struct Entry {
    texture: Texture,
    size: [u32; 2],
    bytes: usize,
    last_used: u64,
    /// The frame it was uploaded in.
    born: u64,
    stale: bool,
    /// Scene pixels transformed on the CPU: redone when the display
    /// target or transform changes.
    baked: bool,
    /// A refused device tile's fallback pixels: asked for again when the
    /// render device changes.
    fallback: bool,
}

#[derive(Debug)]
struct Asked {
    id: RequestId,
    cancel: CancelToken,
    quality: Quality,
}

/// A tile to draw: its texture and where it lands, in physical pixels
/// (`[x0, y0, x1, y1]`, window coordinates).
#[derive(Debug)]
pub(crate) struct Draw<'a> {
    pub texture: &'a Texture,
    pub size: [u32; 2],
    pub dst_px: [f64; 4],
}

/// A GPU path tile's pixels: scene floats (converted at the paint) or
/// texels prepared by the delivering thread. Only the GPU path reads
/// them: without the `wgpu` feature they are queued and never read.
#[derive(Debug)]
#[cfg_attr(not(feature = "wgpu"), allow(dead_code))]
pub(crate) enum GpuData {
    Scene(Vec<f32>),
    Texels(Vec<u8>),
    /// On the render device: bound, not copied.
    #[cfg(feature = "wgpu")]
    Device(Box<crate::device::DeviceTile>),
}

/// A tile for the GPU path: its texture id, size and pixels.
pub(crate) type Upload = (u64, [u32; 2], GpuData);

/// The GPU path's uploads and releases, waiting for the next paint.
#[derive(Debug, Default)]
pub(crate) struct GpuQueue {
    pub uploads: Vec<Upload>,
    pub frees: Vec<u64>,
    /// What the paints copied and bound so far.
    pub copied: Transfers,
    /// The tile cache is dropped: `frees` holds every texture it had on
    /// the GPU path, for the render state that bound them to release at
    /// its next paint, whichever viewport paints.
    pub closed: bool,
}

impl GpuQueue {
    /// What a paint takes in: the uploads, less those released since they
    /// were queued (a tile replaced within a frame: dropped here, with
    /// what a device tile keeps alive), and the textures to release.
    #[cfg_attr(not(feature = "wgpu"), allow(dead_code))]
    pub(crate) fn drain(&mut self) -> (Vec<Upload>, Vec<u64>) {
        let frees = std::mem::take(&mut self.frees);
        let mut uploads = std::mem::take(&mut self.uploads);
        if !frees.is_empty() {
            let freed: std::collections::HashSet<u64> = frees.iter().copied().collect();
            uploads.retain(|u| !freed.contains(&u.0));
        }
        (uploads, frees)
    }
}

/// Tile pixels copied from the CPU to the GPU, counted where the copy
/// happens: an egui texture loaded (the CPU path), a GPU path texture
/// written at the paint. Device tiles are bound in place and count as
/// `bound`, with no byte. The draws' uniforms (a few hundred bytes a
/// frame) are not tile pixels and are not counted.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Transfers {
    /// Bytes copied.
    pub bytes: u64,
    /// Tiles copied.
    pub tiles: u64,
    /// Device tiles bound, nothing copied.
    pub bound: u64,
}

impl Transfers {
    fn plus(self, o: Transfers) -> Transfers {
        Transfers {
            bytes: self.bytes + o.bytes,
            tiles: self.tiles + o.tiles,
            bound: self.bound + o.bound,
        }
    }

    fn minus(self, o: Transfers) -> Transfers {
        Transfers {
            bytes: self.bytes.saturating_sub(o.bytes),
            tiles: self.tiles.saturating_sub(o.tiles),
            bound: self.bound.saturating_sub(o.bound),
        }
    }

    /// Nothing copied.
    pub fn is_zero_copy(&self) -> bool {
        self.bytes == 0 && self.tiles == 0
    }
}

/// How much a frame uploads at most: tiles and bytes (whichever is
/// reached first; at least one tile a frame). What is left waits for the
/// next frame, never dropped. [`UploadBudget::DEFAULT`] (8 MiB a frame)
/// unless the viewport is told otherwise.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UploadBudget {
    pub tiles: usize,
    pub bytes: usize,
}

impl UploadBudget {
    /// The default: 8 MiB a frame, sixteen 256-pixel tiles of the GPU
    /// path's half floats or thirty-two of 8-bit RGBA. It bounds the
    /// copy to the GPU at the render when a fast source (a cache, a GPU
    /// pipeline) delivers a whole view at once; a source on worker
    /// threads rarely reaches it.
    pub const DEFAULT: UploadBudget = UploadBudget {
        tiles: usize::MAX,
        bytes: 8 << 20,
    };

    /// No limit: every tile arrived is uploaded in the frame that takes
    /// it in (the behaviour before the budget existed).
    pub const UNLIMITED: UploadBudget = UploadBudget {
        tiles: usize::MAX,
        bytes: usize::MAX,
    };

    /// At most `n` tiles a frame.
    pub fn tiles(n: usize) -> UploadBudget {
        UploadBudget {
            tiles: n.max(1),
            bytes: usize::MAX,
        }
    }

    /// At most `n` bytes a frame (and at least one tile).
    pub fn bytes(n: usize) -> UploadBudget {
        UploadBudget {
            tiles: usize::MAX,
            bytes: n,
        }
    }

    /// Whether it limits anything.
    pub fn is_limited(&self) -> bool {
        *self != UploadBudget::UNLIMITED
    }
}

impl Default for UploadBudget {
    fn default() -> Self {
        UploadBudget::DEFAULT
    }
}

/// A tile taken in and not uploaded yet.
#[derive(Debug)]
struct Pending {
    size: [u32; 2],
    body: Body,
    /// Arrival order, to keep it among equals.
    seq: u64,
    /// A refused device tile's fallback pixels.
    fallback: bool,
}

/// What the uploads of the last frame did.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct UploadReport {
    /// Tiles uploaded.
    pub tiles: usize,
    /// Bytes they copy.
    pub bytes: usize,
    /// Tiles left for the next frames.
    pub waiting: usize,
    /// Exact tiles of the view uploaded in that frame and drawn.
    pub fresh_exact_drawn: usize,
}

/// What the device allows, from the GPU path's install (or egui's
/// limits on the CPU path).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Device {
    /// The largest texture side.
    pub max_texture_side: u32,
    /// Scene tiles go to the GPU path (installed).
    pub gpu: bool,
}

/// The device of the windows of `ctx`: the GPU path's when installed,
/// else egui's texture limit and the CPU path.
pub(crate) fn device(ctx: &Context) -> Device {
    #[cfg(feature = "wgpu")]
    if let Some(d) = crate::gpu::installed(ctx) {
        return d;
    }
    Device {
        max_texture_side: ctx.input(|i| i.max_texture_side) as u32,
        gpu: false,
    }
}

/// The tile cache of one viewport state.
#[derive(Debug)]
pub(crate) struct Tiles {
    pub feed: Feed,
    entries: HashMap<(Side, TileKey), Entry>,
    asked: HashMap<(Side, TileKey), Asked>,
    next_id: u64,
    frame: u64,
    used: usize,
    pub budget: usize,
    /// The tile side in use: the one wanted, within the device's limit.
    pub tile_size: u32,
    /// The tile side wanted.
    pub wanted_size: u32,
    pub settle: f64,
    pub stats: Arc<Stats>,
    sizes: HashMap<Side, [u32; 2]>,
    view_key: Option<[u64; 3]>,
    view_since: f64,
    /// Wanted exact tiles not on screen yet (or stale), last frame.
    missing: usize,
    pub readout: Option<Readout>,
    /// The last error a source reported.
    pub last_error: Option<String>,
    pub gpu: Arc<std::sync::Mutex<GpuQueue>>,
    /// Tiles taken in, waiting for their upload.
    pending: HashMap<(Side, TileKey), Pending>,
    next_seq: u64,
    pub upload_budget: UploadBudget,
    /// Tiles converted by the delivering thread.
    pub prepare_on_worker: bool,
    /// What the delivering threads convert with, and its stamp.
    prep_key: Option<PrepKey>,
    prep_stamp: u64,
    pub report: UploadReport,
    /// What the interface thread copied (egui textures).
    cpu_copied: Transfers,
    /// Everything copied, as of the start of this frame.
    last_total: Transfers,
    /// What the last whole frame copied: from the start of the frame
    /// before to the start of this one, its paint included.
    pub frame_transfers: Transfers,
    /// The render device seen at the last frame's start.
    #[cfg(feature = "wgpu")]
    render_seen: Option<egui_wgpu::wgpu::Device>,
}

impl Drop for Tiles {
    /// What the GPU path holds for this cache is released by the next
    /// paint on the render state that bound it (with what device tiles
    /// keep alive): the queue, closed, keeps the textures to release,
    /// and that render state keeps the queue. Tiles waiting for their
    /// paint are dropped now.
    fn drop(&mut self) {
        let ids: Vec<u64> = self
            .entries
            .values()
            .filter_map(|e| match e.texture {
                Texture::Gpu(id) => Some(id),
                Texture::Egui(_) => None,
            })
            .collect();
        let mut q = self
            .gpu
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        q.uploads.clear();
        q.frees.extend(ids);
        q.closed = true;
    }
}

/// A waiting tile's upload order: its class, its distance to the view's
/// centre, its arrival.
type Rank = (u8, f64, u64);

/// What a preparation depends on.
#[derive(Clone, Copy, Debug, PartialEq)]
struct PrepKey {
    transform: usize,
    revision: u64,
    target: DisplayTarget,
    gpu: Option<bool>,
}

impl Default for Tiles {
    fn default() -> Self {
        Tiles {
            feed: Feed::default(),
            entries: HashMap::new(),
            asked: HashMap::new(),
            next_id: 1,
            frame: 0,
            used: 0,
            budget: DEFAULT_BUDGET,
            tile_size: TILE_SIZE,
            wanted_size: TILE_SIZE,
            settle: DEFAULT_SETTLE,
            stats: Arc::default(),
            sizes: HashMap::new(),
            view_key: None,
            view_since: 0.0,
            missing: 0,
            readout: None,
            last_error: None,
            gpu: Arc::default(),
            pending: HashMap::new(),
            next_seq: 0,
            upload_budget: UploadBudget::DEFAULT,
            prepare_on_worker: true,
            prep_key: None,
            prep_stamp: 0,
            report: UploadReport::default(),
            cpu_copied: Transfers::default(),
            last_total: Transfers::default(),
            frame_transfers: Transfers::default(),
            #[cfg(feature = "wgpu")]
            render_seen: None,
        }
    }
}

/// The proxy scale for an image in a view: the power of two at or below
/// the fit scale, at most 1.
pub fn proxy_scale(m: &Mapping) -> f64 {
    let size = egui::vec2(
        (m.rect_px[2] - m.rect_px[0]) as f32,
        (m.rect_px[3] - m.rect_px[1]) as f32,
    );
    let fit = fit_zoom(m.image, size).min(1.0);
    2f64.powf(fit.log2().floor()).clamp(MIN_PROXY_SCALE, 1.0)
}

/// The exact scale of a view: one texel per physical pixel below 100 %,
/// the image's own pixels above.
pub fn exact_scale(m: &Mapping) -> f64 {
    m.zoom.min(1.0)
}

/// The scaled image's size at `scale`.
pub fn scaled_size(image: [f64; 2], scale: f64) -> [i64; 2] {
    [
        ((image[0] * scale).ceil() as i64).max(1),
        ((image[1] * scale).ceil() as i64).max(1),
    ]
}

fn rects_meet(a: [f64; 4], b: [f64; 4]) -> bool {
    a[0] < b[2] && b[0] < a[2] && a[1] < b[3] && b[1] < a[3]
}

impl Tiles {
    pub fn sink(&self, side: Side) -> TileSink {
        self.feed.sink(side)
    }

    /// No request outstanding, nothing waiting for its upload, and every
    /// wanted exact tile on screen.
    pub fn complete(&self) -> bool {
        self.asked.is_empty() && self.pending.is_empty() && self.missing == 0
    }

    /// Requests still waiting for an answer.
    pub fn outstanding(&self) -> usize {
        self.asked.len()
    }

    /// Everything copied so far (the GPU path's paints up to the last
    /// one).
    pub fn transfers(&self) -> Transfers {
        let gpu = self
            .gpu
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .copied;
        self.cpu_copied.plus(gpu)
    }

    /// Bytes of textures held.
    pub fn used(&self) -> usize {
        self.used
    }

    /// Tiles held.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// The image rectangle a tile covers, in image pixels.
    fn tile_rect(&self, key: &TileKey) -> [f64; 4] {
        let s = key.scale.get();
        let t = f64::from(self.tile_size);
        [
            key.x as f64 * t / s,
            key.y as f64 * t / s,
            (key.x + 1) as f64 * t / s,
            (key.y + 1) as f64 * t / s,
        ]
    }

    /// Tells the delivering threads what to convert with (or that the
    /// interface thread converts), when it changed.
    fn update_prep(
        &mut self,
        ctx: &Context,
        device: Device,
        target: &DisplayTarget,
        transform: &Arc<dyn DisplayTransform>,
    ) {
        if !self.prepare_on_worker {
            if self.prep_key.take().is_some() {
                self.feed.set_prep(None);
            }
            return;
        }
        #[cfg(feature = "wgpu")]
        let gpu = device.gpu.then(|| crate::gpu::half_float(ctx));
        #[cfg(not(feature = "wgpu"))]
        let gpu = {
            let _ = (ctx, device);
            None
        };
        let key = PrepKey {
            transform: Arc::as_ptr(transform).cast::<()>() as usize,
            revision: transform.revision(),
            target: *target,
            gpu,
        };
        if self.prep_key == Some(key) {
            return;
        }
        self.prep_key = Some(key);
        self.prep_stamp += 1;
        self.feed.set_prep(Some(Prep {
            stamp: self.prep_stamp,
            transform: transform.clone(),
            target: *target,
            gpu,
        }));
    }

    /// Starts a frame: takes in what the sources sent and uploads it,
    /// within the upload budget, the view's tiles first (`view`: the
    /// mapping of this frame).
    pub fn receive(
        &mut self,
        ctx: &Context,
        device: Device,
        target: &DisplayTarget,
        transform: &Arc<dyn DisplayTransform>,
        view: &Mapping,
    ) {
        // Every tile that arrived since the last frame, uploaded now (or
        // within the budget).
        profiling::scope!("pop.tiles.receive");
        self.frame += 1;
        self.missing = 0;
        self.report = UploadReport::default();
        let total = self.transfers();
        self.frame_transfers = total.minus(self.last_total);
        self.last_total = total;
        self.feed.set_context(ctx);
        #[cfg(feature = "wgpu")]
        self.note_render(ctx);
        let effective = self.wanted_size.min(device.max_texture_side).max(16);
        if effective != self.tile_size {
            self.tile_size = effective;
            self.clear_all();
        }
        self.update_prep(ctx, device, target, transform);
        while let Ok(m) = self.feed.rx.try_recv() {
            if matches!(m, Message::Changed(..) | Message::Readout(_)) {
                self.feed.backlog.taken();
            }
            match m {
                Message::Tile(side, tile) => self.take(side, tile),
                Message::Failed(side, id, error) => {
                    self.last_error = Some(error);
                    let k = self
                        .asked
                        .iter()
                        .find(|(k, a)| k.0 == side && a.id == id)
                        .map(|(k, _)| *k);
                    if let Some(k) = k {
                        self.asked.remove(&k);
                        Stats::add(&self.stats.failed, 1);
                    }
                }
                Message::Changed(side, region) => {
                    Stats::add(&self.stats.changed, 1);
                    self.invalidate(side, region);
                }
                #[cfg(feature = "wgpu")]
                Message::Refused(error) => {
                    Stats::add(&self.stats.refused, 1);
                    self.last_error = Some(error);
                }
                Message::Readout(r) => self.readout = Some(r),
            }
        }
        // Changes folded while the viewport was not drawn: the whole
        // image changed.
        let (whole, coalesced) = self.feed.backlog.take_folded();
        Stats::add(&self.stats.coalesced, coalesced);
        for side in whole {
            Stats::add(&self.stats.changed, 1);
            self.invalidate(side, None);
        }
        self.upload(ctx, device, target, transform.as_ref(), view);
    }

    /// A new render device (the GPU path installed, or installed again):
    /// the fallback pixels of device tiles refused before are asked for
    /// again, as device tiles, and shown until they arrive.
    #[cfg(feature = "wgpu")]
    fn note_render(&mut self, ctx: &Context) {
        let now = crate::gpu::render_device(ctx).map(|r| r.device);
        if now == self.render_seen {
            return;
        }
        self.render_seen = now;
        let before = self.pending.len();
        self.pending.retain(|_, p| !p.fallback);
        Stats::add(&self.stats.dropped, (before - self.pending.len()) as u64);
        for e in self.entries.values_mut() {
            if e.fallback {
                e.stale = true;
            }
        }
    }

    /// A tile delivered: kept for its upload when still wanted.
    fn take(&mut self, side: Side, tile: Arrival) {
        let k = (side, tile.key);
        if self.asked.get(&k).is_none_or(|a| a.id != tile.request) {
            Stats::add(&self.stats.dropped, 1);
            return;
        }
        self.asked.remove(&k);
        self.next_seq += 1;
        self.pending.insert(
            k,
            Pending {
                size: tile.size,
                body: tile.body,
                seq: self.next_seq,
                fallback: tile.fallback,
            },
        );
    }

    /// The upload order of a waiting tile: the visible area first (the
    /// proxy, then the view's scale, then other scales), then the rest;
    /// nearest the view's centre first within each.
    fn priority(&self, key: &TileKey, view: &Mapping) -> (u8, f64) {
        let r = self.tile_rect(key);
        let centre = [(r[0] + r[2]) / 2.0, (r[1] + r[3]) / 2.0];
        let Some(visible) = view.visible() else {
            return (3, 0.0);
        };
        let c = [
            (visible[0] + visible[2]) / 2.0,
            (visible[1] + visible[3]) / 2.0,
        ];
        let d = (centre[0] - c[0]).powi(2) + (centre[1] - c[1]).powi(2);
        let scale = key.scale.get();
        let class = if !rects_meet(r, visible) {
            3
        } else if scale == proxy_scale(view) {
            0
        } else if scale == exact_scale(view) {
            1
        } else {
            2
        };
        (class, d)
    }

    /// Uploads what waits, within the budget; asks for another frame
    /// when some is left.
    fn upload(
        &mut self,
        ctx: &Context,
        device: Device,
        target: &DisplayTarget,
        transform: &dyn DisplayTransform,
        view: &Mapping,
    ) {
        if self.pending.is_empty() {
            return;
        }
        let budget = self.upload_budget;
        let mut order: Vec<(Rank, (Side, TileKey))> = self
            .pending
            .iter()
            .map(|(k, p)| {
                let (class, d) = if budget.is_limited() {
                    self.priority(&k.1, view)
                } else {
                    (0, 0.0)
                };
                ((class, d, p.seq), *k)
            })
            .collect();
        order.sort_by(|a, b| {
            a.0.0
                .cmp(&b.0.0)
                .then(a.0.1.total_cmp(&b.0.1))
                .then(a.0.2.cmp(&b.0.2))
        });
        let (mut tiles, mut bytes) = (0usize, 0usize);
        for (_, k) in order {
            let Some(p) = self.pending.get(&k) else {
                continue;
            };
            let cost = p.body.upload_bytes(p.size, device.gpu);
            if tiles > 0 && (tiles >= budget.tiles || bytes.saturating_add(cost) > budget.bytes) {
                break;
            }
            let Some(p) = self.pending.remove(&k) else {
                continue;
            };
            tiles += 1;
            bytes += cost;
            self.arrived(ctx, device, target, transform, k, p);
        }
        self.report.tiles = tiles;
        self.report.bytes = bytes;
        self.report.waiting = self.pending.len();
        if !self.pending.is_empty() {
            ctx.request_repaint();
        }
    }

    fn arrived(
        &mut self,
        ctx: &Context,
        device: Device,
        target: &DisplayTarget,
        transform: &dyn DisplayTransform,
        k: (Side, TileKey),
        tile: Pending,
    ) {
        let n = tile.size[0] as usize * tile.size[1] as usize;
        let ok = match &tile.body {
            Body::Raw(Pixels::Display(v)) => v.len() == n * 4,
            Body::Raw(Pixels::Scene(v)) => v.len() == n * 4,
            Body::Image { image, .. } => image.pixels.len() == n,
            Body::Texels { .. } => true,
            // Checked below, against the render device of this frame.
            #[cfg(feature = "wgpu")]
            Body::Device(_) => true,
        };
        if !ok || n == 0 {
            Stats::add(&self.stats.failed, 1);
            return;
        }
        let id = NEXT_TEXTURE.fetch_add(1, Ordering::Relaxed);
        let current = self.prep_stamp;
        let load = |image: ColorImage| {
            ctx.load_texture(
                format!("egui-pop-viewport-tile-{id}"),
                image,
                TextureOptions::NEAREST,
            )
        };
        let (texture, bytes, baked, stale) = match tile.body {
            Body::Raw(Pixels::Scene(data)) if device.gpu => {
                self.gpu
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .uploads
                    .push((id, tile.size, GpuData::Scene(data)));
                (Texture::Gpu(id), n * 8, false, false)
            }
            Body::Texels { data, stamp } => {
                if stamp != current || !device.gpu {
                    // Prepared for a device since replaced: asked for
                    // again.
                    Stats::add(&self.stats.dropped, 1);
                    return;
                }
                self.gpu
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .uploads
                    .push((id, tile.size, GpuData::Texels(data)));
                (Texture::Gpu(id), n * 8, false, false)
            }
            #[cfg(feature = "wgpu")]
            Body::Device(dt) => {
                // The device may have changed since the source checked:
                // refused, the tile is dropped (what it keeps with it,
                // its fallback not called: never on the interface
                // thread) and asked for again, the source checking it
                // against the device of now.
                let render = crate::gpu::render_device(ctx);
                if let Err(e) = dt.check(render.as_ref(), tile.size) {
                    Stats::add(&self.stats.refused, 1);
                    self.last_error = Some(format!("device tile refused: {e}"));
                    // Asked for since it was neither held fresh nor asked
                    // for: the next request pass asks again (an older
                    // tile there, stale, stays on screen until then).
                    drop(dt);
                    ctx.request_repaint();
                    return;
                }
                Stats::add(&self.stats.device, 1);
                let bytes = dt.footprint(tile.size);
                self.gpu
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .uploads
                    .push((id, tile.size, GpuData::Device(dt)));
                (Texture::Gpu(id), bytes, false, false)
            }
            Body::Image {
                image,
                baked,
                stamp,
            } => {
                profiling::scope!("pop.tiles.upload");
                // Transformed with a transform since replaced: shown
                // until its replacement arrives.
                let stale = baked && stamp != current;
                self.cpu_copied.bytes += (n * 4) as u64;
                self.cpu_copied.tiles += 1;
                (Texture::Egui(load(image)), n * 4, baked, stale)
            }
            Body::Raw(pixels) => {
                profiling::scope!("pop.tiles.upload");
                let baked = matches!(pixels, Pixels::Scene(_));
                let image = match pixels {
                    Pixels::Display(bytes) => ColorImage::from_rgba_unmultiplied(
                        [tile.size[0] as usize, tile.size[1] as usize],
                        &bytes,
                    ),
                    Pixels::Scene(data) => {
                        let mut out = vec![Color32::BLACK; n];
                        display::apply_cpu(transform, target, &data, &mut out);
                        ColorImage::new([tile.size[0] as usize, tile.size[1] as usize], out)
                    }
                };
                self.cpu_copied.bytes += (n * 4) as u64;
                self.cpu_copied.tiles += 1;
                (Texture::Egui(load(image)), n * 4, baked, false)
            }
        };
        Stats::add(&self.stats.delivered, 1);
        if let Some(old) = self.entries.remove(&k) {
            self.release(old);
        }
        self.used += bytes;
        self.entries.insert(
            k,
            Entry {
                texture,
                size: tile.size,
                bytes,
                last_used: self.frame,
                born: self.frame,
                stale,
                baked,
                fallback: tile.fallback,
            },
        );
    }

    /// Marks the tiles transformed on the CPU stale (a new display
    /// target or transform): they are asked for again, and stay on
    /// screen until then.
    pub fn rebake(&mut self) {
        for e in self.entries.values_mut() {
            if e.baked {
                e.stale = true;
            }
        }
    }

    fn release(&mut self, e: Entry) {
        self.used = self.used.saturating_sub(e.bytes);
        if let Texture::Gpu(id) = e.texture {
            self.gpu
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .frees
                .push(id);
        }
    }

    fn invalidate(&mut self, side: Side, region: Option<[f64; 4]>) {
        let hit = |r: [f64; 4]| region.is_none_or(|reg| rects_meet(r, reg));
        // Waiting tiles there show the image as it was: dropped, asked
        // for again.
        let waiting: Vec<_> = self
            .pending
            .keys()
            .filter(|k| k.0 == side)
            .copied()
            .collect();
        for k in waiting {
            if hit(self.tile_rect(&k.1)) {
                self.pending.remove(&k);
                Stats::add(&self.stats.dropped, 1);
            }
        }
        let keys: Vec<_> = self
            .entries
            .keys()
            .filter(|k| k.0 == side)
            .copied()
            .collect();
        for k in keys {
            if hit(self.tile_rect(&k.1))
                && let Some(e) = self.entries.get_mut(&k)
            {
                e.stale = true;
            }
        }
        let asked: Vec<_> = self.asked.keys().filter(|k| k.0 == side).copied().collect();
        for k in asked {
            if hit(self.tile_rect(&k.1))
                && let Some(a) = self.asked.remove(&k)
            {
                a.cancel.cancel();
                Stats::add(&self.stats.cancelled, 1);
            }
        }
    }

    fn clear_side(&mut self, side: Side) {
        self.pending.retain(|k, _| k.0 != side);
        let keys: Vec<_> = self
            .entries
            .keys()
            .filter(|k| k.0 == side)
            .copied()
            .collect();
        for k in keys {
            if let Some(e) = self.entries.remove(&k) {
                self.release(e);
            }
        }
        let asked: Vec<_> = self.asked.keys().filter(|k| k.0 == side).copied().collect();
        for k in asked {
            if let Some(a) = self.asked.remove(&k) {
                a.cancel.cancel();
                Stats::add(&self.stats.cancelled, 1);
            }
        }
    }

    fn clear_all(&mut self) {
        self.clear_side(Side::After);
        self.clear_side(Side::Before);
    }

    /// Notes the view; exact tiles wait until it has been still for
    /// `settle` seconds.
    pub fn note_view(&mut self, m: &Mapping, now: f64) -> bool {
        let key = [
            m.zoom.to_bits(),
            m.origin[0].to_bits(),
            m.origin[1].to_bits(),
        ];
        if self.view_key != Some(key) {
            self.view_key = Some(key);
            self.view_since = now;
        }
        now - self.view_since >= self.settle
    }

    /// When the view will have settled, if it has not.
    pub fn settles_in(&self, now: f64) -> Option<f64> {
        let left = self.settle - (now - self.view_since);
        (left > 0.0).then_some(left)
    }

    /// The tiles of one level covering `region` (image pixels).
    fn keys(&self, image: [f64; 2], scale: f64, region: [f64; 4]) -> Vec<TileKey> {
        let t = f64::from(self.tile_size);
        let size = scaled_size(image, scale);
        let tiles = [
            (size[0] as f64 / t).ceil() as i64,
            (size[1] as f64 / t).ceil() as i64,
        ];
        let x0 = ((region[0] * scale / t).floor() as i64).clamp(0, tiles[0]);
        let y0 = ((region[1] * scale / t).floor() as i64).clamp(0, tiles[1]);
        let x1 = ((region[2] * scale / t).ceil() as i64).clamp(0, tiles[0]);
        let y1 = ((region[3] * scale / t).ceil() as i64).clamp(0, tiles[1]);
        let mut keys = Vec::new();
        for y in y0..y1 {
            for x in x0..x1 {
                keys.push(TileKey {
                    scale: Scale::new(scale),
                    x,
                    y,
                });
            }
        }
        // Nearest the centre first.
        let c = [(x0 + x1) as f64 / 2.0, (y0 + y1) as f64 / 2.0];
        keys.sort_by(|a, b| {
            let d =
                |k: &TileKey| (k.x as f64 + 0.5 - c[0]).powi(2) + (k.y as f64 + 0.5 - c[1]).powi(2);
            d(a).total_cmp(&d(b))
        });
        keys
    }

    /// Asks a source for what one side needs, and returns what to draw,
    /// proxy first.
    pub fn frame(
        &mut self,
        side: Side,
        source: &dyn TileSource,
        m: &Mapping,
        settled: bool,
    ) -> Vec<Draw<'_>> {
        profiling::scope!("pop.tiles.frame");
        let Some(size) = source.size() else {
            self.clear_side(side);
            return Vec::new();
        };
        if self.sizes.get(&side) != Some(&size) {
            self.clear_side(side);
            self.sizes.insert(side, size);
        }
        let image = [f64::from(size[0]), f64::from(size[1])];
        // Nothing of the image in view: no exact tile wanted (the
        // proxy is still asked for, to be ready).
        let visible = m.visible().unwrap_or([0.0, 0.0, 0.0, 0.0]);
        let p = proxy_scale(m);
        let s = exact_scale(m);
        let whole = [0.0, 0.0, image[0], image[1]];
        let mut wanted: Vec<(TileKey, Quality)> = Vec::new();
        let exact_keys = if settled || s == p {
            self.keys(image, s, visible)
        } else {
            Vec::new()
        };
        if s != p {
            wanted.extend(
                self.keys(image, p, whole)
                    .into_iter()
                    .map(|k| (k, Quality::Proxy)),
            );
        }
        wanted.extend(exact_keys.iter().map(|k| (*k, Quality::Exact)));

        // Cancel exact requests no longer wanted (proxies stay useful).
        let keep: std::collections::HashSet<TileKey> = wanted.iter().map(|w| w.0).collect();
        let gone: Vec<_> = self
            .asked
            .iter()
            .filter(|(k, a)| k.0 == side && a.quality == Quality::Exact && !keep.contains(&k.1))
            .map(|(k, _)| *k)
            .collect();
        for k in gone {
            if let Some(a) = self.asked.remove(&k) {
                a.cancel.cancel();
                Stats::add(&self.stats.cancelled, 1);
            }
        }

        // Ask for what is neither held (fresh) nor asked for.
        let t = i64::from(self.tile_size);
        let scaled = |scale: f64| scaled_size(image, scale);
        let mut batch = Vec::new();
        for (key, quality) in &wanted {
            let k = (side, *key);
            let fresh = self.entries.get(&k).is_some_and(|e| !e.stale);
            if fresh || self.asked.contains_key(&k) || self.pending.contains_key(&k) {
                continue;
            }
            let full = scaled(key.scale.get());
            let origin = [key.x * t, key.y * t];
            let w = (full[0] - origin[0]).min(t).max(0) as u32;
            let h = (full[1] - origin[1]).min(t).max(0) as u32;
            if w == 0 || h == 0 {
                continue;
            }
            let id = RequestId(self.next_id);
            self.next_id += 1;
            let cancel = CancelToken::default();
            self.asked.insert(
                k,
                Asked {
                    id,
                    cancel: cancel.clone(),
                    quality: *quality,
                },
            );
            batch.push(TileRequest {
                id,
                key: *key,
                quality: *quality,
                origin,
                size: [w, h],
                cancel,
            });
        }
        if !batch.is_empty() {
            Stats::add(&self.stats.requested, batch.len() as u64);
            source.request(batch);
        }
        self.missing += exact_keys
            .iter()
            .filter(|k| self.entries.get(&(side, **k)).is_none_or(|e| e.stale))
            .count()
            + usize::from(!settled && s != p);

        // Draw: the proxy under, the exact tiles over.
        let frame = self.frame;
        let mut draws_keys: Vec<(Side, TileKey)> = Vec::new();
        if s != p {
            for key in self.keys(image, p, visible) {
                if self.entries.contains_key(&(side, key)) {
                    draws_keys.push((side, key));
                }
            }
        }
        for key in self.keys(image, s, visible) {
            if self.entries.contains_key(&(side, key)) {
                draws_keys.push((side, key));
            }
        }
        let exact = Scale::new(s);
        for k in &draws_keys {
            if let Some(e) = self.entries.get_mut(k) {
                e.last_used = frame;
                if e.born == frame && k.1.scale == exact {
                    self.report.fresh_exact_drawn += 1;
                }
            }
        }
        let tile = f64::from(self.tile_size);
        draws_keys
            .into_iter()
            .filter_map(|k| {
                let e = self.entries.get(&k)?;
                let scale = k.1.scale.get();
                let x0 = k.1.x as f64 * tile / scale;
                let y0 = k.1.y as f64 * tile / scale;
                let a = m.image_to_px([x0, y0]);
                let b = m.image_to_px([
                    x0 + f64::from(e.size[0]) / scale,
                    y0 + f64::from(e.size[1]) / scale,
                ]);
                Some(Draw {
                    texture: &e.texture,
                    size: e.size,
                    dst_px: [a[0], a[1], b[0], b[1]],
                })
            })
            .collect()
    }

    /// Evicts the least recently drawn tiles past the budget; never one
    /// drawn this frame.
    pub fn evict(&mut self) {
        if self.used <= self.budget {
            return;
        }
        let mut old: Vec<_> = self
            .entries
            .iter()
            .filter(|(_, e)| e.last_used < self.frame)
            .map(|(k, e)| (e.last_used, *k))
            .collect();
        old.sort();
        for (_, k) in old {
            if self.used <= self.budget {
                break;
            }
            if let Some(e) = self.entries.remove(&k) {
                self.release(e);
                Stats::add(&self.stats.evicted, 1);
            }
        }
    }
}

#[cfg(all(test, feature = "wgpu"))]
mod device_tests {
    //! Device tiles through the cache, on wgpu's stub device (no GPU):
    //! what a tile keeps alive, where its fallback runs, and the render
    //! device read when it is needed.

    use std::sync::atomic::AtomicUsize;
    use std::sync::{Arc, Mutex};
    use std::thread::ThreadId;

    use egui::{Rect, pos2, vec2};
    use egui_wgpu::wgpu;

    use super::*;
    use crate::device::DeviceTile;
    use crate::display::LinearSrgb;
    use crate::view::View;

    fn stub_device() -> wgpu::Device {
        wgpu::Device::noop(&wgpu::DeviceDescriptor::default()).0
    }

    /// Two stub devices of one instance (stub devices of two instances
    /// may compare equal).
    fn two_stub_devices() -> (wgpu::Device, wgpu::Device) {
        fn ready<T>(f: impl std::future::Future<Output = T>) -> T {
            let mut f = std::pin::pin!(f);
            let mut cx = std::task::Context::from_waker(std::task::Waker::noop());
            match f.as_mut().poll(&mut cx) {
                std::task::Poll::Ready(v) => v,
                std::task::Poll::Pending => panic!("the stub backend answers at once"),
            }
        }
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: wgpu::Backends::NOOP,
            backend_options: wgpu::BackendOptions {
                noop: wgpu::NoopBackendOptions::enabled(),
                ..Default::default()
            },
            ..wgpu::InstanceDescriptor::new_without_display_handle()
        });
        let adapter = ready(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))
            .expect("the stub adapter");
        let device = || {
            ready(adapter.request_device(&wgpu::DeviceDescriptor::default()))
                .expect("a stub device")
                .0
        };
        let (a, b) = (device(), device());
        assert!(a != b);
        (a, b)
    }

    /// Counts the values alive: one more made, one less dropped.
    struct Token(Arc<AtomicUsize>);

    impl Token {
        fn new(live: &Arc<AtomicUsize>) -> Token {
            live.fetch_add(1, Ordering::SeqCst);
            Token(live.clone())
        }
    }

    impl Drop for Token {
        fn drop(&mut self) {
            self.0.fetch_sub(1, Ordering::SeqCst);
        }
    }

    /// A 64-pixel image (one tile), delivered from a worker thread as a
    /// texture on `device`, keeping a token, with a fallback that notes
    /// the thread it runs on.
    struct Source {
        sink: TileSink,
        device: wgpu::Device,
        live: Arc<AtomicUsize>,
        requests: Arc<AtomicUsize>,
        fallbacks: Arc<Mutex<Vec<ThreadId>>>,
    }

    impl Source {
        fn new(sink: TileSink, device: wgpu::Device) -> Source {
            Source {
                sink,
                device,
                live: Arc::default(),
                requests: Arc::default(),
                fallbacks: Arc::default(),
            }
        }

        fn live(&self) -> usize {
            self.live.load(Ordering::SeqCst)
        }

        fn fallback_threads(&self) -> Vec<ThreadId> {
            self.fallbacks.lock().unwrap().clone()
        }
    }

    impl TileSource for Source {
        fn size(&self) -> Option<[u32; 2]> {
            Some([64, 64])
        }

        fn request(&self, requests: Vec<TileRequest>) {
            self.requests.fetch_add(requests.len(), Ordering::SeqCst);
            std::thread::scope(|s| {
                s.spawn(|| {
                    for r in requests {
                        let texture = self.device.create_texture(&wgpu::TextureDescriptor {
                            label: None,
                            size: wgpu::Extent3d {
                                width: r.size[0],
                                height: r.size[1],
                                depth_or_array_layers: 1,
                            },
                            mip_level_count: 1,
                            sample_count: 1,
                            dimension: wgpu::TextureDimension::D2,
                            format: wgpu::TextureFormat::Rgba16Float,
                            usage: wgpu::TextureUsages::TEXTURE_BINDING,
                            view_formats: &[],
                        });
                        let fallbacks = self.fallbacks.clone();
                        let n = (r.size[0] * r.size[1] * 4) as usize;
                        let tile = DeviceTile::texture(&self.device, texture)
                            .keep(Token::new(&self.live))
                            .fallback(move || {
                                fallbacks.lock().unwrap().push(std::thread::current().id());
                                Some(Pixels::Scene(vec![0.5; n]))
                            });
                        self.sink.deliver_device(&r, tile);
                    }
                });
            });
        }
    }

    struct Bench {
        ctx: Context,
        tiles: Tiles,
        mapping: Mapping,
        transform: Arc<dyn DisplayTransform>,
    }

    impl Bench {
        fn new() -> Bench {
            let rect = Rect::from_min_size(pos2(0.0, 0.0), vec2(100.0, 100.0));
            Bench {
                ctx: Context::default(),
                tiles: Tiles::default(),
                mapping: Mapping::new(&View::actual([32.0, 32.0]), [64.0, 64.0], rect, 1.0),
                transform: Arc::new(LinearSrgb),
            }
        }

        /// One frame: take in what arrived, then ask for what is wanted.
        fn step(&mut self, source: &Source) {
            let gpu = crate::gpu::render_device(&self.ctx).is_some();
            let device = Device {
                max_texture_side: 8192,
                gpu,
            };
            self.tiles.receive(
                &self.ctx,
                device,
                &DisplayTarget::default(),
                &self.transform,
                &self.mapping,
            );
            let _ = self
                .tiles
                .frame(Side::After, source, &self.mapping, true)
                .len();
        }

        /// What the next paint does with the queue: binds the uploads
        /// (held, returned) and releases the rest.
        fn paint(&self) -> Vec<Upload> {
            self.tiles.gpu.lock().unwrap().drain().0
        }

        fn stat(&self, name: &str) -> u64 {
            self.tiles
                .stats
                .snapshot()
                .iter()
                .find(|(k, _)| *k == name)
                .map_or(0, |(_, v)| *v)
        }
    }

    #[test]
    fn what_a_device_tile_keeps_lives_as_long_as_the_tile() {
        let mut b = Bench::new();
        let device = stub_device();
        crate::gpu::set_render_device(&b.ctx, &device);
        let source = Source::new(b.tiles.feed.sink(Side::After), device);
        b.step(&source);
        assert_eq!(source.live(), 1, "delivered, waiting to be taken in");
        b.step(&source);
        assert_eq!(b.stat("device"), 1);
        let bound = b.paint();
        assert_eq!(bound.len(), 1);
        assert_eq!(source.live(), 1, "held with the binding");
        // The image changed: the tile is asked for again, the new one
        // replaces it in the frame that takes it in.
        source.sink.changed(None);
        b.step(&source);
        assert_eq!(source.live(), 2, "the shown tile and its replacement");
        b.step(&source);
        assert_eq!(b.stat("device"), 2);
        let (uploads, frees) = b.tiles.gpu.lock().unwrap().drain();
        assert_eq!(frees, vec![bound[0].0], "the replaced tile is released");
        // The paint drops the released binding with what it keeps.
        drop(bound);
        assert_eq!(source.live(), 1);
        // Replaced twice before a paint: the upload released before it
        // was bound is dropped by the paint, with what it keeps.
        source.sink.changed(None);
        b.step(&source);
        b.step(&source);
        source.sink.changed(None);
        b.step(&source);
        b.step(&source);
        assert_eq!(source.live(), 3, "one bound, two queued");
        let next = b.paint();
        assert_eq!(next.len(), 1);
        assert_eq!(source.live(), 2);
        drop(uploads);
        assert_eq!(source.live(), 1);
        // The viewport's state dropped: the tiles waiting for a paint are
        // dropped at once, the bound ones released by the next paint.
        source.sink.changed(None);
        b.step(&source);
        b.step(&source);
        assert_eq!(source.live(), 2, "the bound tile and one queued");
        drop(b);
        assert_eq!(source.live(), 1, "the queued one dropped");
        drop(next);
        assert_eq!(source.live(), 0);
    }

    #[test]
    fn a_tile_refused_at_take_in_is_asked_again_and_falls_back_off_the_interface_thread() {
        let mut b = Bench::new();
        let (first, second) = two_stub_devices();
        crate::gpu::set_render_device(&b.ctx, &first);
        let source = Source::new(b.tiles.feed.sink(Side::After), first);
        // Delivered and checked against the first device.
        b.step(&source);
        assert_eq!(source.requests.load(Ordering::SeqCst), 1);
        assert_eq!(b.stat("refused"), 0);
        // The render device changes before the interface takes it in.
        crate::gpu::set_render_device(&b.ctx, &second);
        b.step(&source);
        assert_eq!(b.stat("refused"), 1, "at take-in");
        assert_eq!(
            source.requests.load(Ordering::SeqCst),
            2,
            "asked for again in the same frame"
        );
        let main = std::thread::current().id();
        let threads = source.fallback_threads();
        assert_eq!(threads.len(), 1, "the fallback ran once");
        assert!(threads.iter().all(|t| *t != main), "never on this thread");
        assert_eq!(source.live(), 0, "both refused tiles dropped");
        b.step(&source);
        assert_eq!(b.stat("refused"), 2, "then on the worker");
        assert_eq!(b.tiles.entries.len(), 1, "the fallback pixels shown");
        assert!(
            b.tiles
                .last_error
                .as_deref()
                .is_some_and(|e| e.contains("another wgpu device"))
        );
    }

    #[test]
    fn the_gpu_path_installed_after_the_first_frame_replaces_the_fallback() {
        let mut b = Bench::new();
        let device = stub_device();
        let source = Source::new(b.tiles.feed.sink(Side::After), device.clone());
        // No GPU path: refused on the worker, the fallback shown.
        b.step(&source);
        assert!(source.sink.render_device().is_none());
        b.step(&source);
        assert_eq!(b.tiles.entries.len(), 1);
        assert_eq!(b.stat("device"), 0);
        assert_eq!(source.fallback_threads().len(), 1);
        // Installed between frames: the sink reads it at once.
        crate::gpu::set_render_device(&b.ctx, &device);
        assert!(
            source
                .sink
                .render_device()
                .is_some_and(|r| r.device == device)
        );
        // The next frame asks for the fallback's tile again, as a device
        // tile, shown in the frame after.
        b.step(&source);
        assert_eq!(source.requests.load(Ordering::SeqCst), 2);
        b.step(&source);
        assert_eq!(b.stat("device"), 1);
        assert_eq!(source.fallback_threads().len(), 1);
        assert!(b.tiles.entries.values().all(|e| !e.fallback && !e.stale));
    }
}
