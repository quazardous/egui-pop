//! The viewport widget: draws a view of an image and turns gestures into
//! changes of the [`View`], reported as [`Signal`]s.
//!
//! The application owns the view (per window or per document) and the
//! [`ViewportState`] (what the widget keeps between frames, its hooks
//! included). Every change the widget makes goes through the
//! `before_view` hook and is reported as a signal with its phase, so an
//! application that keeps the view in a store records a pan as one
//! gesture, and a cancelled pan as nothing.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use std::sync::Arc;

use egui::{
    Color32, Id, Key, Pos2, Rect, Response, Role, Sense, Stroke, StrokeKind, Ui, Vec2, WidgetInfo,
    pos2,
};
use egui_pop_core::kit::{Kit, KitCx, Part, State};

use crate::cursor::{self, Cursor, CursorQuery, Shown, Tool};
use crate::debug;
use crate::display::{self, DisplayTarget, DisplayTransform, LinearSrgb, Stage};
use crate::gestures::{self, DoubleClick, Gestures, WheelAction};
use crate::hooks::{Cause, HandleMove, Hooks, TRACE_KEPT, TraceEntry, Verdict, ViewChange};
use crate::loupe::{self, Loupe, LoupeFrame};
use crate::overlay::{self, Handle, HandleKind, HandleState, Overlay};
use crate::source::{Readout, Side, Stats, TileSink, TileSource};
use crate::tiles::{self, Texture, Tiles, UploadBudget, UploadReport};
use crate::tokens::ViewportTokens;
use crate::view::{self, Mapping, View, Zoom};

/// Where a change stands in its gesture.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    /// A gesture starts (a drag); the value is the one before it.
    Begin,
    /// A gesture goes on.
    Update,
    /// A gesture ends; the value is the final one.
    End,
    /// A gesture is cancelled; the value is the one before it, restored.
    Cancel,
    /// A change in one step (a wheel notch, a key, a double click, a
    /// command).
    Once,
}

impl Phase {
    pub fn name(self) -> &'static str {
        match self {
            Phase::Begin => "begin",
            Phase::Update => "update",
            Phase::End => "end",
            Phase::Cancel => "cancel",
            Phase::Once => "once",
        }
    }
}

/// What the viewport tells the application.
#[derive(Clone, Debug, PartialEq)]
pub enum Signal {
    /// The view changed.
    View {
        view: View,
        phase: Phase,
        cause: Cause,
    },
    /// The window's display target changed (another monitor, another
    /// surface format); the viewport follows it.
    Display { target: DisplayTarget },
    /// A handle moved: `from` is where it was when the gesture began,
    /// `at` where it is now (image pixels). On `Cancel`, put the shape
    /// back as it was at `Begin`.
    Handle {
        overlay: String,
        handle: u32,
        kind: HandleKind,
        phase: Phase,
        from: [f64; 2],
        at: [f64; 2],
    },
    /// A double click on a handle: reset what it adjusts.
    HandleReset {
        overlay: String,
        handle: u32,
        kind: HandleKind,
    },
    /// A click on a handle (pressed and released without moving): select
    /// what it belongs to, toggle a point's corner.
    HandleClick {
        overlay: String,
        handle: u32,
        kind: HandleKind,
        modifiers: egui::Modifiers,
    },
    /// A click on an overlay's outline away from its handles: `segment`
    /// as `overlay::hit_edge` numbers it, `at` the nearest point of the
    /// outline (image pixels), to add a point there.
    EdgeClick {
        overlay: String,
        segment: u32,
        at: [f64; 2],
        modifiers: egui::Modifiers,
    },
    /// The wheel over a handle (points, positive up): adjust what the
    /// handle adjusts, never the zoom.
    HandleWheel {
        overlay: String,
        handle: u32,
        kind: HandleKind,
        delta: f32,
    },
    /// The comparison changed: the split moved, the "before" shown or
    /// hidden.
    Compare { compare: Compare, phase: Phase },
    /// The source's values under the pointer arrived.
    Readout(Readout),
    /// The loupe was turned on or off with its key.
    Loupe { on: bool },
    /// Tiles moved this frame: asked for, arrived, given up. `complete`
    /// when every tile wanted is on screen.
    Tiles {
        requested: u64,
        delivered: u64,
        cancelled: u64,
        complete: bool,
    },
}

/// Before and after: what the viewport shows of the "before" image.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum Compare {
    /// The image only.
    #[default]
    Off,
    /// "Before" left of the split (a fraction of the view's width),
    /// "after" right of it; the split is dragged with its handle.
    Split(f32),
    /// "Before" only (a toggle, or held with the gestures' `before`
    /// key).
    Before,
}

/// A handle being dragged.
#[derive(Clone, Debug)]
struct HandleDrag {
    overlay: String,
    handle: Handle,
    /// The pointer at the start, in image pixels.
    press: [f64; 2],
    at: [f64; 2],
}

/// What the widget keeps between frames. One per viewport; the
/// application owns it.
#[derive(Debug, Default)]
pub struct ViewportState {
    transform: Transform,
    transform_revision: u64,
    target: Option<DisplayTarget>,
    resolution: Option<display::Resolution>,
    tiles: Tiles,
    /// This state's GPU uniforms (0 until first drawn).
    slot: u64,
    last_counts: [u64; 3],
    drag: Option<HandleDrag>,
    split_drag: Option<f32>,
    before_held: bool,
    asked_readout: Option<(Side, [i64; 2])>,
    signalled_readout: Option<(Side, [i64; 2])>,
    last_complete: bool,
    pan: Option<Pan>,
    /// The wheel landed on 100 % and is held there until it stops.
    detent: bool,
    last_mapping: Option<Mapping>,
    hooks: Hooks,
    trace: VecDeque<TraceEntry>,
    /// Signals from calls between frames, given with the next frame's.
    pending: Vec<Signal>,
    now: f64,
    cursor: CursorState,
    loupe: Loupe,
    loupe_frame: Option<LoupeFrame>,
    /// The tiles drawn this frame, for the loupe.
    tile_draws: Vec<TileDraw>,
    /// The display stage drawn with this frame.
    stage: Option<Stage>,
}

/// The cursor between frames: the last query and what it resolved to,
/// what was shown, whether a bitmap was handed to the integration.
#[derive(Debug, Default)]
struct CursorState {
    last: Option<(CursorQuery, Cursor)>,
    shown: Option<Shown>,
    image_set: bool,
}

/// A tile's texture, as the loupe draws it again.
#[derive(Clone, Copy, Debug)]
enum TileTexture {
    Egui(egui::TextureId),
    Gpu(u64),
}

/// A tile drawn this frame: its side, texture, size and where it landed
/// (window physical pixels).
#[derive(Clone, Copy, Debug)]
struct TileDraw {
    side: Side,
    texture: TileTexture,
    size: [u32; 2],
    dst_px: [f64; 4],
}

/// The display transform in use (the default: [`LinearSrgb`]).
#[derive(Clone, Debug)]
struct Transform(Arc<dyn DisplayTransform>);

impl Default for Transform {
    fn default() -> Self {
        Transform(Arc::new(LinearSrgb))
    }
}

#[derive(Clone, Copy, Debug)]
struct Pan {
    start: View,
}

impl ViewportState {
    /// An empty state.
    pub fn new() -> ViewportState {
        ViewportState::default()
    }

    /// The mapping of the last frame drawn.
    pub fn mapping(&self) -> Option<Mapping> {
        self.last_mapping
    }

    /// Whether a gesture is in progress.
    pub fn in_gesture(&self) -> bool {
        self.pan.is_some()
    }

    /// Replaces the display transform (the application's colour
    /// management; [`LinearSrgb`] by default).
    pub fn set_display_transform(&mut self, transform: Arc<dyn DisplayTransform>) {
        self.transform = Transform(transform);
    }

    /// The display transform in use.
    pub fn display_transform(&self) -> &Arc<dyn DisplayTransform> {
        &self.transform.0
    }

    /// The display target the last frame was drawn for.
    pub fn display_target(&self) -> Option<DisplayTarget> {
        self.target
    }

    /// What the last frame was drawn for: the wanted target, the
    /// surface's, the one drawn and why it carries less, if it does.
    pub fn display_resolution(&self) -> Option<&display::Resolution> {
        self.resolution.as_ref()
    }

    /// Where the source delivers the image's tiles (give it to the
    /// source when making it).
    pub fn sink(&self) -> TileSink {
        self.tiles.sink(Side::After)
    }

    /// Where a second source delivers the "before" image of a
    /// comparison.
    pub fn before_sink(&self) -> TileSink {
        self.tiles.sink(Side::Before)
    }

    /// What happened to the tiles, counted.
    pub fn stats(&self) -> &Arc<Stats> {
        &self.tiles.stats
    }

    /// No request outstanding and every wanted exact tile on screen
    /// (after the last frame).
    pub fn tiles_complete(&self) -> bool {
        self.tiles.complete()
    }

    /// Requests waiting for an answer.
    pub fn outstanding(&self) -> usize {
        self.tiles.outstanding()
    }

    /// Tiles held.
    pub fn tile_count(&self) -> usize {
        self.tiles.len()
    }

    /// Bytes of tile textures held.
    pub fn texture_bytes(&self) -> usize {
        self.tiles.used()
    }

    /// The side of a tile wanted, in pixels (256 by default; the device's
    /// largest texture side at most).
    pub fn set_tile_size(&mut self, side: u32) {
        self.tiles.wanted_size = side.max(16);
    }

    /// The texture budget, in bytes (256 MB by default). Tiles drawn in
    /// the current frame are never evicted, so a view larger than the
    /// budget still shows whole.
    pub fn set_texture_budget(&mut self, bytes: usize) {
        self.tiles.budget = bytes;
    }

    /// How much one frame uploads at most ([`UploadBudget::DEFAULT`], 8
    /// MiB, by default). Tiles over it wait for the next frames, the
    /// visible area's first, and are kept when the view moves.
    /// [`UploadBudget::UNLIMITED`] opts out: every tile arrived goes up
    /// in the frame that takes it in.
    pub fn set_upload_budget(&mut self, budget: UploadBudget) {
        self.tiles.upload_budget = budget;
    }

    /// The upload budget.
    pub fn upload_budget(&self) -> UploadBudget {
        self.tiles.upload_budget
    }

    /// Converts tiles on the thread that delivers them (the source's
    /// worker) instead of the interface thread: the display transform
    /// of the CPU path, the half-float texels of the GPU path. **On by
    /// default**: the source's thread runs the display transform
    /// ([`DisplayTransform`] is `Send + Sync`), and a tile converted
    /// with a transform replaced meanwhile is shown, marked stale and
    /// asked for again. `false` opts out: the interface thread converts
    /// each tile when it takes it in, for a source whose threads must
    /// not run the transform, or one delivering from the interface
    /// thread anyway (the conversion then happens there either way).
    /// See docs/threading.md for the measurements.
    pub fn set_prepare_on_worker(&mut self, on: bool) {
        self.tiles.prepare_on_worker = on;
    }

    /// Whether tiles are converted on the delivering thread.
    pub fn prepare_on_worker(&self) -> bool {
        self.tiles.prepare_on_worker
    }

    /// What the last frame uploaded, and what waits.
    pub fn upload_report(&self) -> UploadReport {
        self.tiles.report
    }

    /// How long the view stays still before exact tiles are asked for,
    /// in seconds (0.1 by default; 0 asks at once).
    pub fn set_settle(&mut self, seconds: f64) {
        self.tiles.settle = seconds.max(0.0);
    }

    /// The loupe's settings.
    pub fn loupe(&self) -> &Loupe {
        &self.loupe
    }

    /// The loupe's settings, to change (turn it on for a tool, change its
    /// magnification).
    pub fn loupe_mut(&mut self) -> &mut Loupe {
        &mut self.loupe
    }

    /// What the loupe showed in the last frame, if it showed.
    pub fn loupe_frame(&self) -> Option<&LoupeFrame> {
        self.loupe_frame.as_ref()
    }

    /// The cursor resolved for the pointer in the last frame (after the
    /// `on_cursor` hook), while the pointer is over the viewport.
    pub fn cursor(&self) -> Option<&Cursor> {
        self.cursor
            .shown
            .as_ref()
            .and_then(|_| self.cursor.last.as_ref().map(|(_, c)| c))
    }

    /// What the cursor showed in the last frame, while the pointer is
    /// over the viewport.
    pub fn cursor_shown(&self) -> Option<&Shown> {
        self.cursor.shown.as_ref()
    }

    /// Forgets the cursor resolved, so the next frame asks the
    /// `on_cursor` hook again (its answer changed for the same
    /// situation).
    pub fn refresh_cursor(&mut self) {
        self.cursor.last = None;
    }

    /// The hooks, to set or remove.
    pub fn hooks_mut(&mut self) -> &mut Hooks {
        &mut self.hooks
    }

    /// The hooks.
    pub fn hooks(&self) -> &Hooks {
        &self.hooks
    }

    /// What the hooks decided lately, oldest first.
    pub fn trace(&self) -> impl Iterator<Item = &TraceEntry> {
        self.trace.iter()
    }

    pub(crate) fn record(&mut self, hook: &'static str, verdict: &'static str, detail: String) {
        if self.trace.len() == TRACE_KEPT {
            self.trace.pop_front();
        }
        self.trace.push_back(TraceEntry {
            time: self.now,
            hook,
            verdict,
            detail,
        });
    }

    /// Changes the view from outside the widget (a menu's "zoom in"),
    /// through the `before_view` hook like a gesture. Returns whether the
    /// view changed; the signal comes with the next frame's output.
    pub fn propose(&mut self, view: &mut View, to: View) -> bool {
        let Some(m) = self.last_mapping else {
            *view = to;
            return true;
        };
        match self.ask(view, to, Cause::Command, &m) {
            Some(to) if to != *view => {
                *view = to;
                self.pending.push(Signal::View {
                    view: to,
                    phase: Phase::Once,
                    cause: Cause::Command,
                });
                true
            }
            _ => false,
        }
    }

    /// One ladder step up around the centre (a command).
    pub fn zoom_in(&mut self, view: &mut View) -> bool {
        self.zoom_to_with(view, view::zoom_in)
    }

    /// One ladder step down around the centre (a command).
    pub fn zoom_out(&mut self, view: &mut View) -> bool {
        self.zoom_to_with(view, view::zoom_out)
    }

    fn zoom_to_with(&mut self, view: &mut View, f: fn(f64) -> f64) -> bool {
        let Some(m) = self.last_mapping else {
            return false;
        };
        let to = m.zoom_around(m.rect.center(), f(m.zoom));
        self.propose(view, to)
    }

    /// The `before_view` hook's answer for a proposed change, traced.
    fn ask(&mut self, from: &View, to: View, cause: Cause, m: &Mapping) -> Option<View> {
        let Some(hook) = self.hooks.before_view.as_mut() else {
            return Some(to);
        };
        let change = ViewChange {
            from: *from,
            to,
            cause,
        };
        match hook(&change, m) {
            Verdict::Allow => Some(to),
            Verdict::Veto => {
                self.record("before_view", "veto", format!("{} to {to:?}", cause.name()));
                None
            }
            Verdict::Redirect(v) => {
                self.record(
                    "before_view",
                    "redirect",
                    format!("{} to {to:?}, given {v:?}", cause.name()),
                );
                Some(v)
            }
        }
    }
}

/// What a frame of the viewport gives back.
#[derive(Debug)]
pub struct ViewportOutput {
    pub response: Response,
    /// The mapping used this frame (`None` without an image size).
    pub mapping: Option<Mapping>,
    /// The image pixel under the pointer, by the drawing's own rule.
    pub hover_pixel: Option<[i64; 2]>,
    /// The source's values at that pixel, once they arrived.
    pub readout: Option<Readout>,
    /// What changed this frame, in order.
    pub signals: Vec<Signal>,
}

/// The viewport widget, built per frame.
pub struct Viewport<'a> {
    source: Option<&'a dyn TileSource>,
    before: Option<&'a dyn TileSource>,
    compare: Compare,
    overlays: &'a [Overlay],
    readout: bool,
    id: Id,
    name: String,
    image_size: Option<[u32; 2]>,
    label: Option<String>,
    pixel_grid: bool,
    size: Option<Vec2>,
    tool: Tool,
    on_cursor: Option<OnCursor<'a>>,
}

/// A cursor hook: the situation and the default, the cursor to show.
type CursorHook<'a> = dyn FnMut(&CursorQuery, Cursor) -> Cursor + 'a;
type OnCursor<'a> = Box<CursorHook<'a>>;

impl std::fmt::Debug for Viewport<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Viewport")
            .field("name", &self.name)
            .field("source", &self.source.is_some())
            .field("image_size", &self.image_size)
            .finish()
    }
}

static NEXT_SLOT: AtomicU64 = AtomicU64::new(1);

/// From this zoom up, the pixel grid shows (when enabled).
pub const GRID_FROM_ZOOM: f64 = 8.0;

/// One frame's working set: the view, its mapping, what changed.
struct Frame<'a> {
    state: &'a mut ViewportState,
    view: &'a mut View,
    mapping: Mapping,
    signals: Vec<Signal>,
}

impl Frame<'_> {
    /// Proposes a change; applies what the hook allows and signals it.
    fn change(&mut self, to: View, cause: Cause, phase: Phase) -> bool {
        let to = if phase == Phase::Cancel {
            Some(to)
        } else {
            self.state.ask(self.view, to, cause, &self.mapping)
        };
        let Some(to) = to else { return false };
        if to == *self.view && phase != Phase::Cancel {
            return false;
        }
        *self.view = to;
        self.mapping = Mapping::new(
            self.view,
            self.mapping.image,
            self.mapping.rect,
            self.mapping.ppp,
        );
        self.signals.push(Signal::View {
            view: to,
            phase,
            cause,
        });
        true
    }

    fn signal(&mut self, phase: Phase, cause: Cause) {
        self.signals.push(Signal::View {
            view: *self.view,
            phase,
            cause,
        });
    }
}

impl<'a> Viewport<'a> {
    /// A viewport; `id_salt` tells it apart from others in the same `Ui`
    /// and names it in the debug bridge.
    pub fn new(id_salt: impl egui::AsIdSalt) -> Viewport<'a> {
        Viewport {
            source: None,
            before: None,
            compare: Compare::Off,
            overlays: &[],
            readout: false,
            name: format!("{id_salt:?}").trim_matches('"').to_string(),
            id: Id::unique(id_salt),
            image_size: None,
            label: None,
            pixel_grid: true,
            size: None,
            tool: Tool::None,
            on_cursor: None,
        }
    }

    /// Decides the cursor: gets the situation (the tool, the keys held, a
    /// drag, a handle under the pointer) and egui-pop's default for it,
    /// returns the cursor to show. Asked again only when the situation
    /// changes (or after [`ViewportState::refresh_cursor`]);
    /// `egui-pop-imaging`'s `cursor` runs it as the `cursor.resolve`
    /// step.
    pub fn on_cursor(
        mut self,
        hook: impl FnMut(&CursorQuery, Cursor) -> Cursor + 'a,
    ) -> Viewport<'a> {
        self.on_cursor = Some(Box::new(hook));
        self
    }

    /// The tool active over the viewport, which decides the cursor
    /// (default: none, the system's arrow).
    pub fn tool(mut self, tool: Tool) -> Viewport<'a> {
        self.tool = tool;
        self
    }

    /// Where the pixels come from (its size is the image's unless
    /// [`Viewport::image_size`] says otherwise).
    pub fn source(mut self, source: &'a dyn TileSource) -> Viewport<'a> {
        self.source = Some(source);
        self
    }

    /// The "before" image of a comparison (deliver to
    /// [`ViewportState::before_sink`]).
    pub fn before(mut self, source: &'a dyn TileSource) -> Viewport<'a> {
        self.before = Some(source);
        self
    }

    /// How "before" and "after" are shown (the application keeps it and
    /// applies the `Compare` signals).
    pub fn compare(mut self, compare: Compare) -> Viewport<'a> {
        self.compare = compare;
        self
    }

    /// Overlays in image coordinates, drawn over the image, their
    /// handles dragged through `Handle` signals.
    pub fn overlays(mut self, overlays: &'a [Overlay]) -> Viewport<'a> {
        self.overlays = overlays;
        self
    }

    /// Shows the source's values under the pointer in a corner (they
    /// are in the output and signalled either way).
    pub fn readout(mut self, on: bool) -> Viewport<'a> {
        self.readout = on;
        self
    }

    /// The image's size in pixels.
    pub fn image_size(mut self, size: [u32; 2]) -> Viewport<'a> {
        self.image_size = Some(size);
        self
    }

    /// The accessible name (default "Image viewport").
    pub fn label(mut self, label: impl Into<String>) -> Viewport<'a> {
        self.label = Some(label.into());
        self
    }

    /// Draws the pixel grid from [`GRID_FROM_ZOOM`] up (default on).
    pub fn pixel_grid(mut self, on: bool) -> Viewport<'a> {
        self.pixel_grid = on;
        self
    }

    /// The widget's size in points (default: all the space available).
    pub fn size(mut self, size: Vec2) -> Viewport<'a> {
        self.size = Some(size);
        self
    }

    /// Draws the viewport and handles its gestures.
    pub fn show(self, ui: &mut Ui, state: &mut ViewportState, view: &mut View) -> ViewportOutput {
        let size = self.size.unwrap_or_else(|| ui.available_size());
        let (_, rect) = ui.allocate_space(size);
        let response = ui.interact(
            rect,
            ui.make_persistent_id(self.id),
            Sense::click_and_drag(),
        );
        let tokens = ViewportTokens::of(ui.ctx());
        let g = gestures::gestures(ui.ctx());
        let ppp = ui.ctx().pixels_per_point();
        state.now = ui.input(|i| i.time);
        let mut signals = std::mem::take(&mut state.pending);
        // The image's own surface, when the platform gives it one: the
        // target below is then that surface's.
        #[cfg(feature = "wayland")]
        let image_route = if tiles::device(ui.ctx()).gpu {
            crate::image_surface::frame(ui.ctx())
        } else {
            None
        };
        #[cfg(not(feature = "wayland"))]
        let image_route: ImageRoute = ();
        let surface = display::target(ui.ctx());
        if state.target.is_some_and(|t| t != surface) {
            signals.push(Signal::Display { target: surface });
        }
        state.target = Some(surface);
        let resolved = display::resolution(ui.ctx());
        let same = state
            .resolution
            .as_ref()
            .is_some_and(|r| r.wanted == resolved.wanted && r.surface == resolved.surface);
        let resolution = if same {
            state.resolution.clone().unwrap_or(resolved)
        } else {
            resolve_hook(state, resolved)
        };
        let target = resolution.drawn;
        let target_changed = state.resolution.as_ref().is_some_and(|r| r.drawn != target);
        state.resolution = Some(resolution);
        ui.painter().rect_filled(rect, 0.0, tokens.background);

        let size = self
            .image_size
            .or_else(|| self.source.and_then(|s| s.size()));
        let Some(image) = size.map(|s| [f64::from(s[0]), f64::from(s[1])]) else {
            name(&response, self.label.as_deref(), None);
            state.last_mapping = None;
            return ViewportOutput {
                response,
                mapping: None,
                hover_pixel: None,
                readout: None,
                signals,
            };
        };

        let mapping = Mapping::new(view, image, rect, ppp);
        let mut f = Frame {
            state,
            view,
            mapping,
            signals: Vec::new(),
        };
        for to in debug::take_commands(ui.ctx(), &self.name) {
            f.change(to, Cause::Command, Phase::Once);
        }
        let pan_key = response.hovered() && ui.input(|i| i.key_down(g.pan_key));
        let compare = handle_compare(ui, &response, &g, self.compare, &mut f);
        let captured = handle_overlays(ui, &response, &g, pan_key, self.overlays, compare, &mut f);
        if !captured.drag {
            handle_pan(ui, &response, &g, pan_key, &mut f);
        }
        if !captured.double_click {
            handle_double_click(&response, &g, &mut f);
        }
        if !captured.wheel {
            handle_wheel(ui, &response, &g, &mut f);
        }
        handle_keys(ui, &response, &g, &mut f);
        if let Some(k) = f.state.loupe.key
            && (response.hovered() || response.has_focus())
            && ui.input(|i| i.key_pressed(k))
        {
            f.state.loupe.on = !f.state.loupe.on;
            f.signals.push(Signal::Loupe {
                on: f.state.loupe.on,
            });
        }
        if response.clicked() || response.drag_started() {
            response.request_focus();
        }
        let Frame {
            state,
            view,
            mapping,
            signals: frame_signals,
        } = f;
        signals.extend(frame_signals);

        // The image.
        let image_rect = mapping.image_rect_to_screen([0.0, 0.0, image[0], image[1]]);
        let painter = ui.painter_at(rect);
        painter.rect_filled(image_rect, 0.0, tokens.backdrop);
        let split_x = split_x(&mapping, compare);
        draw_tiles(
            ui,
            &painter,
            state,
            [self.source, self.before],
            split_x,
            compare == Compare::Before,
            &mapping,
            &target,
            target_changed,
            &mut signals,
            image_route,
            tokens.backdrop,
        );
        if self.pixel_grid && mapping.zoom >= GRID_FROM_ZOOM {
            draw_grid(&painter, &mapping, tokens.grid);
        }
        painter.rect_stroke(
            image_rect,
            0.0,
            Stroke::new(1.0 / ppp, tokens.frame),
            StrokeKind::Outside,
        );
        if let Some(under) = state.hooks.under_overlays.as_mut() {
            under(&painter, &mapping);
        }
        for o in self.overlays {
            overlay::paint_shape(&painter, &mapping, o, &tokens);
        }
        let hovered_handle = response
            .hover_pos()
            .filter(|_| state.drag.is_none())
            .and_then(|p| overlay::hit(self.overlays, &mapping, p));
        for o in self.overlays {
            for h in &o.handles {
                let active = state
                    .drag
                    .as_ref()
                    .is_some_and(|d| d.overlay == o.id && d.handle.index == h.index);
                let hover = hovered_handle
                    .as_ref()
                    .is_some_and(|(i, hh)| self.overlays[*i].id == o.id && hh.index == h.index);
                let st = if active {
                    HandleState::Active
                } else if hover {
                    HandleState::Hover
                } else {
                    HandleState::Rest
                };
                overlay::paint_handle_styled(
                    &painter,
                    mapping.image_to_screen(h.at),
                    h.kind,
                    st,
                    o.style.emphasis,
                    &tokens,
                );
            }
        }
        if let Some(x) = split_x {
            draw_divider(&painter, rect, x, state.split_drag.is_some(), &tokens);
        }
        if let Some(after) = state.hooks.after_paint.as_mut() {
            after(&painter, &mapping);
        }
        if response.has_focus() {
            Kit::of(ui.ctx()).viewport_focus.paint(
                &KitCx {
                    painter: &painter,
                    ppp: ui.ctx().pixels_per_point(),
                },
                &Part {
                    rect,
                    room: rect,
                    state: State::FOCUS,
                    label: None,
                },
            );
        }

        let hover_pixel = response.hover_pos().and_then(|p| mapping.pixel_at(p));
        let readout = handle_readout(
            &response,
            state,
            hover_pixel,
            split_x,
            compare,
            [self.source, self.before],
            &mut signals,
        );
        if self.readout
            && let Some(r) = &readout
        {
            draw_readout(ui, &painter, rect, state, r);
        }
        let pointer = (response.hovered() || response.dragged())
            .then(|| ui.input(|i| i.pointer.hover_pos()))
            .flatten();
        state.loupe_frame = None;
        if state.loupe.on
            && let Some(p) = pointer.filter(|p| rect.contains(*p))
        {
            let before = state.tile_draws.iter().any(|d| d.side == Side::Before)
                && (state.before_held || split_x.is_some_and(|x| p.x < x));
            let side = if before { Side::Before } else { Side::After };
            draw_loupe(
                ui,
                &painter,
                state,
                &mapping,
                p,
                side,
                readout.as_ref(),
                &tokens,
            );
        }
        let query = CursorQuery {
            tool: self.tool.clone(),
            modifiers: ui.input(|i| i.modifiers),
            dragging: response.dragged() || state.pan.is_some() || state.drag.is_some(),
            panning: state.pan.is_some() || pan_key,
            handle: state
                .drag
                .as_ref()
                .map(|d| d.handle.kind)
                .or(hovered_handle.as_ref().map(|(_, h)| h.kind)),
            over_image: hover_pixel.is_some(),
        };
        let mut on_cursor = self.on_cursor;
        update_cursor(
            ui.ctx(),
            &painter,
            state,
            query,
            on_cursor.as_deref_mut(),
            pointer,
            &mapping,
        );
        name(&response, self.label.as_deref(), Some(mapping.zoom));
        state.last_mapping = Some(mapping);
        debug::publish(ui.ctx(), &self.name, state, view, &mapping, &signals);
        ViewportOutput {
            response,
            mapping: Some(mapping),
            hover_pixel,
            readout,
            signals,
        }
    }
}

fn handle_pan(ui: &Ui, response: &Response, g: &Gestures, pan_key: bool, f: &mut Frame<'_>) {
    let starts = response.drag_started_by(g.pan_button)
        || response.drag_started_by(g.pan_button_anywhere)
        || (pan_key && response.drag_started());
    if starts && f.state.pan.is_none() {
        f.state.pan = Some(Pan { start: *f.view });
        f.signal(Phase::Begin, Cause::Pan);
    }
    let Some(pan) = f.state.pan else { return };
    if ui.input(|i| i.key_pressed(g.cancel)) {
        f.state.pan = None;
        f.change(pan.start, Cause::Pan, Phase::Cancel);
        return;
    }
    let delta = response.drag_delta();
    if delta != Vec2::ZERO {
        let to = f.mapping.panned(delta);
        f.change(to, Cause::Pan, Phase::Update);
    }
    if response.drag_stopped() || !response.dragged() && !response.drag_started() {
        f.state.pan = None;
        f.signal(Phase::End, Cause::Pan);
    }
}

fn handle_double_click(response: &Response, g: &Gestures, f: &mut Frame<'_>) {
    if !response.double_clicked() {
        return;
    }
    let at = response.hover_pos().unwrap_or(f.mapping.rect.center());
    let fit = f.view.is_fit();
    let m = f.mapping;
    let action = g.double_click;
    let mut default = || match action {
        DoubleClick::Nothing => None,
        DoubleClick::Fit => Some(View::fit()),
        DoubleClick::FitOrActual => Some(if fit {
            m.zoom_around(at, 1.0)
        } else {
            View::fit()
        }),
    };
    let to = if let Some(run) = f.state.hooks.on_double_click.as_mut() {
        let to = run(at, &m, &mut default);
        f.state
            .record("on_double_click", "run", format!("gave {to:?}"));
        to
    } else {
        default()
    };
    if let Some(to) = to {
        f.change(to, Cause::DoubleClick, Phase::Once);
    }
}

fn handle_wheel(ui: &Ui, response: &Response, g: &Gestures, f: &mut Frame<'_>) {
    if !response.hovered() {
        f.state.detent = false;
        return;
    }
    let (scroll, pinch, modifiers) =
        ui.input(|i| (i.smooth_scroll_delta, i.zoom_delta(), i.modifiers));
    // The canvas takes the wheel: the panel around it does not scroll.
    ui.ctx().input_mut(|i| i.smooth_scroll_delta = Vec2::ZERO);
    let pointer = response.hover_pos().unwrap_or(f.mapping.rect.center());
    let other = !g.wheel_other.is_none() && modifiers.matches_logically(g.wheel_other);
    let zooms = (g.wheel == WheelAction::Zoom) != other;
    if pinch != 1.0 {
        let z = view::wheel_zoom(
            f.mapping.zoom,
            (f64::from(pinch).ln() / view::WHEEL_RATE) as f32,
        );
        let to = f.mapping.zoom_around(pointer, z);
        f.change(to, Cause::Pinch, Phase::Once);
    } else if scroll == Vec2::ZERO {
        f.state.detent = false;
    } else if zooms {
        if f.state.detent {
            // Held on 100 % until the wheel stops.
            return;
        }
        let dy = if scroll.y != 0.0 { scroll.y } else { scroll.x };
        let z = view::wheel_zoom(f.mapping.zoom, dy);
        f.state.detent = z == 1.0 && f.mapping.zoom != 1.0;
        let to = f.mapping.zoom_around(pointer, z);
        f.change(to, Cause::Wheel, Phase::Once);
    } else {
        let to = f.mapping.panned(scroll);
        f.change(to, Cause::Wheel, Phase::Once);
    }
}

fn handle_keys(ui: &Ui, response: &Response, g: &Gestures, f: &mut Frame<'_>) {
    if !response.has_focus() || f.state.pan.is_some() {
        return;
    }
    let m = f.mapping;
    let center = m.rect.center();
    let pressed = |k: Key| ui.input(|i| i.key_pressed(k));
    let to = if g.zoom_in.iter().any(|k| pressed(*k)) {
        Some(m.zoom_around(center, view::zoom_in(m.zoom)))
    } else if g.zoom_out.iter().any(|k| pressed(*k)) {
        Some(m.zoom_around(center, view::zoom_out(m.zoom)))
    } else if pressed(g.fit) {
        Some(View::fit())
    } else if pressed(g.actual) {
        Some(m.zoom_around(center, 1.0))
    } else if pressed(g.double) {
        Some(m.zoom_around(center, 2.0))
    } else {
        let coarse = !g.coarse.is_none() && ui.input(|i| i.modifiers.matches_logically(g.coarse));
        let step = m.rect.size() * if coarse { 0.5 } else { 0.1 };
        let mut d = Vec2::ZERO;
        if pressed(Key::ArrowLeft) {
            d.x += step.x;
        }
        if pressed(Key::ArrowRight) {
            d.x -= step.x;
        }
        if pressed(Key::ArrowUp) {
            d.y += step.y;
        }
        if pressed(Key::ArrowDown) {
            d.y -= step.y;
        }
        (d != Vec2::ZERO).then(|| m.panned(d))
    };
    if let Some(to) = to {
        f.change(to, Cause::Key, Phase::Once);
    }
}

/// The accessible name, with the zoom as the value.
fn name(response: &Response, label: Option<&str>, zoom: Option<f64>) {
    let label = label.map_or_else(
        || egui_pop_core::strings::tr(&response.ctx, &crate::strings::VIEWPORT),
        str::to_string,
    );
    response.widget_info(move || {
        let mut info = WidgetInfo::labeled(Role::Unknown, true, &label);
        if let Some(z) = zoom {
            info.value = Some(z * 100.0);
            info.current_text_value = Some(format!("{:.0} %", z * 100.0));
        }
        info
    });
}

/// Lines between image pixels, over the visible part.
fn draw_grid(painter: &egui::Painter, m: &Mapping, color: Color32) {
    let Some(v) = m.visible() else { return };
    let stroke = Stroke::new(1.0 / m.ppp, color);
    let (x0, x1) = (v[0].floor() as i64, v[2].ceil() as i64);
    let (y0, y1) = (v[1].floor() as i64, v[3].ceil() as i64);
    if (x1 - x0) > 1000 || (y1 - y0) > 1000 {
        return;
    }
    let top = m.image_to_screen([0.0, v[1]]).y;
    let bottom = m.image_to_screen([0.0, v[3]]).y;
    let left = m.image_to_screen([v[0], 0.0]).x;
    let right = m.image_to_screen([v[2], 0.0]).x;
    for x in x0..=x1 {
        let sx = m.image_to_screen([x as f64, 0.0]).x;
        painter.line_segment([Pos2::new(sx, top), Pos2::new(sx, bottom)], stroke);
    }
    for y in y0..=y1 {
        let sy = m.image_to_screen([0.0, y as f64]).y;
        painter.line_segment([Pos2::new(left, sy), Pos2::new(right, sy)], stroke);
    }
}

/// The zoom of a view as shown, in percent (fit resolved by `mapping`).
pub fn zoom_percent(view: &View, mapping: Option<&Mapping>) -> f64 {
    match (view.zoom, mapping) {
        (Zoom::Px(z), _) => z * 100.0,
        (Zoom::Fit, Some(m)) => m.zoom * 100.0,
        (Zoom::Fit, None) => 100.0,
    }
}

/// Where the split falls on screen (points), when splitting.
fn split_x(m: &Mapping, compare: Compare) -> Option<f32> {
    match compare {
        Compare::Split(t) => Some(m.rect.left() + m.rect.width() * t.clamp(0.0, 1.0)),
        _ => None,
    }
}

/// The image's own surface for a frame, when it has one.
#[cfg(feature = "wayland")]
type ImageRoute = Option<(
    crate::image_surface::ImageSurfaces,
    crate::image_surface::Route,
)>;
#[cfg(not(feature = "wayland"))]
type ImageRoute = ();

/// Takes in what the sources sent, asks for what is missing, draws the
/// tiles (proxy, then exact) of each side shown, evicts past the budget,
/// and signals what moved.
#[allow(clippy::too_many_arguments)]
fn draw_tiles(
    ui: &Ui,
    painter: &egui::Painter,
    state: &mut ViewportState,
    sources: [Option<&dyn TileSource>; 2],
    split_x: Option<f32>,
    before_only: bool,
    mapping: &Mapping,
    target: &DisplayTarget,
    target_changed: bool,
    signals: &mut Vec<Signal>,
    image: ImageRoute,
    backdrop: Color32,
) {
    let ctx = ui.ctx();
    let transform = state.transform.0.clone();
    if target_changed || state.transform_revision != transform.revision() {
        state.transform_revision = transform.revision();
        state.tiles.rebake();
    }
    if state.slot == 0 {
        // Three slots: the image, the "before" image, the loupe.
        state.slot = NEXT_SLOT.fetch_add(3, Ordering::Relaxed);
    }
    let device = tiles::device(ctx);
    state
        .tiles
        .receive(ctx, device, target, &transform, mapping);
    let settled = state.tiles.note_view(mapping, state.now);
    if let Some(left) = state.tiles.settles_in(state.now) {
        ctx.request_repaint_after(Duration::from_secs_f64(left));
    }
    let [after, before] = sources;
    let stage = transform.stage(&target.effective());
    state.stage = Some(stage);
    let mut drawn: Vec<TileDraw> = Vec::new();
    // "Before" alone: the application's choice, or the key held.
    let showing_before = before_only;
    let rect = mapping.rect;
    let mut sides: Vec<(Side, &dyn TileSource, Rect, u64)> = Vec::new();
    match (before, split_x, showing_before) {
        (Some(b), _, true) => sides.push((Side::Before, b, rect, state.slot + 1)),
        (Some(b), Some(x), false) => {
            if let Some(a) = after {
                sides.push((
                    Side::After,
                    a,
                    Rect::from_min_max(pos2(x, rect.top()), rect.max),
                    state.slot,
                ));
            }
            sides.push((
                Side::Before,
                b,
                Rect::from_min_max(rect.min, pos2(x, rect.bottom())),
                state.slot + 1,
            ));
        }
        _ => {
            if let Some(a) = after {
                sides.push((Side::After, a, rect, state.slot));
            }
        }
    }
    #[cfg(feature = "wgpu")]
    let mut paints: Vec<(crate::gpu::TilePaint, Rect)> = Vec::new();
    for (side, source, clip, slot) in sides {
        let clip = clip.intersect(painter.clip_rect());
        let painter = painter.with_clip_rect(clip);
        let mut gpu: Vec<(u64, [f32; 4], [u32; 2])> = Vec::new();
        let ppp = f64::from(mapping.ppp);
        for d in state.tiles.frame(side, source, mapping, settled) {
            let r = d.dst_px;
            drawn.push(TileDraw {
                side,
                texture: match d.texture {
                    Texture::Egui(handle) => TileTexture::Egui(handle.id()),
                    Texture::Gpu(id) => TileTexture::Gpu(*id),
                },
                size: d.size,
                dst_px: r,
            });
            match d.texture {
                Texture::Egui(handle) => {
                    painter.image(
                        handle.id(),
                        Rect::from_min_max(
                            pos2((r[0] / ppp) as f32, (r[1] / ppp) as f32),
                            pos2((r[2] / ppp) as f32, (r[3] / ppp) as f32),
                        ),
                        Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
                        Color32::WHITE,
                    );
                }
                Texture::Gpu(id) => {
                    gpu.push((
                        *id,
                        [r[0] as f32, r[1] as f32, r[2] as f32, r[3] as f32],
                        d.size,
                    ));
                }
            }
        }
        #[cfg(feature = "wgpu")]
        if device.gpu {
            let paint = crate::gpu::TilePaint {
                slot,
                queue: state.tiles.gpu.clone(),
                draws: gpu
                    .into_iter()
                    .map(|(texture, dst, size)| crate::gpu::GpuDraw { texture, dst, size })
                    .collect(),
                stage,
            };
            paints.push((paint, clip));
        }
        #[cfg(not(feature = "wgpu"))]
        let _ = (gpu, slot);
    }
    // The image goes to its own surface when it has one for what is
    // drawn (a hook may have redirected it), else to the window.
    #[cfg(feature = "wayland")]
    let paints = match image {
        Some((surfaces, route)) if route.target() == Some(*target) => {
            let visible = mapping
                .image_rect_to_screen([0.0, 0.0, mapping.image[0], mapping.image[1]])
                .intersect(painter.clip_rect());
            let px = |r: Rect| {
                [
                    r.min.x * mapping.ppp,
                    r.min.y * mapping.ppp,
                    r.max.x * mapping.ppp,
                    r.max.y * mapping.ppp,
                ]
            };
            if visible.is_positive() {
                let paint = crate::image_surface::ImagePaint {
                    ctx: ctx.clone(),
                    surfaces,
                    route,
                    key: state.slot,
                    pass: ctx.cumulative_pass_nr(),
                    rect_px: px(visible),
                    scale: ctx.native_pixels_per_point().unwrap_or(mapping.ppp),
                    background: crate::image_surface::clear_colour(backdrop, target),
                    sides: paints.into_iter().map(|(p, clip)| (p, px(clip))).collect(),
                    shown: std::sync::atomic::AtomicBool::new(false),
                };
                painter
                    .with_clip_rect(visible)
                    .add(egui_wgpu::Callback::new_paint_callback(visible, paint));
            }
            Vec::new()
        }
        _ => paints,
    };
    #[cfg(not(feature = "wayland"))]
    let _ = (image, backdrop);
    #[cfg(feature = "wgpu")]
    for (paint, clip) in paints {
        painter
            .with_clip_rect(clip)
            .add(egui_wgpu::Callback::new_paint_callback(rect, paint));
    }
    state.tiles.evict();
    state.tile_draws = drawn;

    let snap = state.tiles.stats.snapshot();
    let counts = [snap[0].1, snap[1].1, snap[3].1];
    let complete = state.tiles.complete();
    let any = after.is_some() || before.is_some();
    if any && (counts != state.last_counts || complete != state.last_complete) {
        signals.push(Signal::Tiles {
            requested: counts[0] - state.last_counts[0],
            delivered: counts[1] - state.last_counts[1],
            cancelled: counts[2] - state.last_counts[2],
            complete,
        });
        state.last_counts = counts;
        state.last_complete = complete;
    }
}

/// What the overlays took of this frame's input.
#[derive(Clone, Copy, Debug, Default)]
struct Captured {
    drag: bool,
    double_click: bool,
    wheel: bool,
}

/// The `before_handle` hook's answer for a handle move, traced.
fn ask_handle(state: &mut ViewportState, mv: &HandleMove, m: &Mapping) -> Option<[f64; 2]> {
    let Some(hook) = state.hooks.before_handle.as_mut() else {
        return Some(mv.to);
    };
    match hook(mv, m) {
        Verdict::Allow => Some(mv.to),
        Verdict::Veto => {
            state.record(
                "before_handle",
                "veto",
                format!("{} {} to {:?}", mv.overlay, mv.handle, mv.to),
            );
            None
        }
        Verdict::Redirect(p) => {
            state.record(
                "before_handle",
                "redirect",
                format!("{} {} to {:?}, given {p:?}", mv.overlay, mv.handle, mv.to),
            );
            Some(p)
        }
    }
}

/// Handle drags (one gesture each, Escape cancels), double clicks and
/// the wheel over a handle.
fn handle_overlays(
    ui: &Ui,
    response: &Response,
    g: &Gestures,
    pan_key: bool,
    overlays: &[Overlay],
    compare: Compare,
    f: &mut Frame<'_>,
) -> Captured {
    let mut captured = Captured::default();
    let m = f.mapping;
    // A drag on the split moves it.
    if let Some(x) = split_x(&m, compare) {
        let near = |p: Pos2| (p.x - x).abs() <= overlay::HIT_RADIUS;
        if response.drag_started_by(g.pan_button)
            && !pan_key
            && ui.input(|i| i.pointer.press_origin()).is_some_and(near)
        {
            f.state.split_drag = Some(x);
        }
        if f.state.split_drag.is_some() {
            captured.drag = true;
            if let Some(p) = response.interact_pointer_pos() {
                let t = ((p.x - m.rect.left()) / m.rect.width()).clamp(0.0, 1.0);
                let phase = if response.drag_stopped() {
                    Phase::End
                } else {
                    Phase::Update
                };
                f.signals.push(Signal::Compare {
                    compare: Compare::Split(t),
                    phase,
                });
            }
            if response.drag_stopped() || !response.dragged() {
                f.state.split_drag = None;
            }
        }
    }
    if overlays.is_empty() && f.state.drag.is_none() {
        return captured;
    }
    let pointer = response.hover_pos();
    let under = pointer.and_then(|p| overlay::hit(overlays, &m, p));
    if response.drag_started_by(g.pan_button)
        && !pan_key
        && f.state.drag.is_none()
        && let Some(origin) = ui.input(|i| i.pointer.press_origin())
        && let Some((i, h)) = overlay::hit(overlays, &m, origin)
    {
        let d = HandleDrag {
            overlay: overlays[i].id.clone(),
            handle: h,
            press: m.screen_to_image(origin),
            at: h.at,
        };
        f.signals.push(Signal::Handle {
            overlay: d.overlay.clone(),
            handle: h.index,
            kind: h.kind,
            phase: Phase::Begin,
            from: h.at,
            at: h.at,
        });
        f.state.drag = Some(d);
    }
    if let Some(mut d) = f.state.drag.clone() {
        captured.drag = true;
        let signal = |phase, at| Signal::Handle {
            overlay: d.overlay.clone(),
            handle: d.handle.index,
            kind: d.handle.kind,
            phase,
            from: d.handle.at,
            at,
        };
        if ui.input(|i| i.key_pressed(g.cancel)) {
            f.signals.push(signal(Phase::Cancel, d.handle.at));
            f.state.drag = None;
            return captured;
        }
        if let Some(p) = response.interact_pointer_pos()
            && response.drag_delta() != Vec2::ZERO
        {
            let now = m.screen_to_image(p);
            let to = [
                d.handle.at[0] + now[0] - d.press[0],
                d.handle.at[1] + now[1] - d.press[1],
            ];
            let mv = HandleMove {
                overlay: d.overlay.clone(),
                handle: d.handle.index,
                kind: d.handle.kind,
                start: d.handle.at,
                from: d.at,
                to,
                modifiers: ui.input(|i| i.modifiers),
            };
            if let Some(at) = ask_handle(f.state, &mv, &m) {
                d.at = at;
                f.signals.push(signal(Phase::Update, at));
            }
        }
        if response.drag_stopped() || !response.dragged() {
            f.signals.push(signal(Phase::End, d.at));
            f.state.drag = None;
        } else {
            f.state.drag = Some(d);
        }
    }
    // A click (no drag) on a handle, else on an outline.
    if response.clicked_by(g.pan_button)
        && !pan_key
        && let Some(p) = response.interact_pointer_pos().or(pointer)
    {
        let modifiers = ui.input(|i| i.modifiers);
        if let Some((i, h)) = overlay::hit(overlays, &m, p) {
            f.signals.push(Signal::HandleClick {
                overlay: overlays[i].id.clone(),
                handle: h.index,
                kind: h.kind,
                modifiers,
            });
        } else if let Some((i, segment, at)) = overlay::hit_edge(overlays, &m, p) {
            f.signals.push(Signal::EdgeClick {
                overlay: overlays[i].id.clone(),
                segment,
                at,
                modifiers,
            });
        }
    }
    if let Some((i, h)) = under {
        if response.double_clicked() {
            captured.double_click = true;
            f.signals.push(Signal::HandleReset {
                overlay: overlays[i].id.clone(),
                handle: h.index,
                kind: h.kind,
            });
        }
        let scroll = ui.input(|i| i.smooth_scroll_delta.y);
        if response.hovered() && scroll != 0.0 {
            captured.wheel = true;
            ui.ctx().input_mut(|i| i.smooth_scroll_delta = Vec2::ZERO);
            f.signals.push(Signal::HandleWheel {
                overlay: overlays[i].id.clone(),
                handle: h.index,
                kind: h.kind,
                delta: scroll,
            });
        }
    }
    captured
}

/// The `before` key held shows the "before" image (a quasimode); the
/// `before_compare` hook may veto or redirect a change. Returns the
/// comparison to draw this frame.
fn handle_compare(
    ui: &Ui,
    response: &Response,
    g: &Gestures,
    compare: Compare,
    f: &mut Frame<'_>,
) -> Compare {
    let held = (response.hovered() || response.has_focus()) && ui.input(|i| i.key_down(g.before));
    if held != f.state.before_held {
        let proposed = if held { Compare::Before } else { compare };
        let allowed = match f.state.hooks.before_compare.as_mut() {
            None => Some(proposed),
            Some(hook) => match hook(&compare, &proposed) {
                Verdict::Allow => Some(proposed),
                Verdict::Veto => None,
                Verdict::Redirect(c) => Some(c),
            },
        };
        if let Some(c) = allowed {
            f.state.before_held = held && c == Compare::Before;
            f.signals.push(Signal::Compare {
                compare: c,
                phase: if held { Phase::Begin } else { Phase::End },
            });
        }
    }
    if f.state.before_held {
        Compare::Before
    } else {
        compare
    }
}

/// The `before_display` hook on a new resolution.
fn resolve_hook(state: &mut ViewportState, mut r: display::Resolution) -> display::Resolution {
    let Some(hook) = state.hooks.before_display.as_mut() else {
        return r;
    };
    let (verdict, to) = match hook(&r) {
        Verdict::Allow => return r,
        Verdict::Veto => (
            "veto",
            DisplayTarget {
                surface_format: r.surface.surface_format,
                source: r.surface.source,
                ..DisplayTarget::srgb()
            },
        ),
        Verdict::Redirect(t) => ("redirect", t.effective()),
    };
    let detail = format!("{} -> {}", r.drawn.describe(), to.describe());
    r.drawn = to;
    r.fallback = Some(display::Fallback::Hook);
    state.record("before_display", verdict, detail);
    r
}

/// Asks the source shown under the pointer for its values when the
/// pointer moves to another pixel; gives back what arrived for it.
fn handle_readout(
    response: &Response,
    state: &mut ViewportState,
    pixel: Option<[i64; 2]>,
    split_x: Option<f32>,
    compare: Compare,
    sources: [Option<&dyn TileSource>; 2],
    signals: &mut Vec<Signal>,
) -> Option<Readout> {
    let pixel = pixel?;
    let left = split_x
        .zip(response.hover_pos())
        .is_some_and(|(x, p)| p.x < x);
    let side = if (compare == Compare::Before || state.before_held || left) && sources[1].is_some()
    {
        Side::Before
    } else {
        Side::After
    };
    let source = if side == Side::Before {
        sources[1]
    } else {
        sources[0]
    }?;
    if state.asked_readout != Some((side, pixel)) {
        state.asked_readout = Some((side, pixel));
        source.readout(pixel);
    }
    let r = state
        .tiles
        .readout
        .clone()
        .filter(|r| r.side == side && r.pixel == pixel)?;
    if state.signalled_readout != Some((side, pixel)) {
        state.signalled_readout = Some((side, pixel));
        signals.push(Signal::Readout(r.clone()));
    }
    Some(r)
}

/// The readout's text: the `on_readout` hook's, or the pixel and its
/// values.
pub fn readout_text(r: &Readout) -> String {
    let mut s = format!("{}, {}", r.pixel[0], r.pixel[1]);
    for (name, v) in &r.values {
        s.push_str(&format!("  {name} {v:.3}"));
    }
    s
}

fn draw_readout(
    ui: &Ui,
    painter: &egui::Painter,
    rect: Rect,
    state: &mut ViewportState,
    r: &Readout,
) {
    let text = match state.hooks.on_readout.as_mut() {
        Some(hook) => hook(r),
        None => readout_text(r),
    };
    let t = egui_pop_core::theme::Tokens::of(ui.ctx());
    let font = egui::FontId::monospace(t.font.small);
    let galley = painter.layout_no_wrap(text, font, Color32::PLACEHOLDER);
    let kit = Kit::of(ui.ctx());
    let size = kit.readout.measure(galley.size());
    let at = pos2(rect.left() + t.space.s, rect.bottom() - t.space.s - size.y);
    let bg = Rect::from_min_size(at, size);
    kit.readout.paint_part(
        &KitCx {
            painter,
            ppp: ui.ctx().pixels_per_point(),
        },
        &Part {
            rect: bg,
            room: bg,
            state: State::REST,
            label: Some(&galley),
        },
    );
}

fn draw_divider(painter: &egui::Painter, rect: Rect, x: f32, active: bool, t: &ViewportTokens) {
    let top = pos2(x, rect.top());
    let bottom = pos2(x, rect.bottom());
    let w = |x: f32| egui_pop_core::scale::stroke(painter.ctx(), x);
    painter.line_segment(
        [top, bottom],
        Stroke::new(w(overlay::UNDER_WIDTH), t.handle_outline),
    );
    painter.line_segment([top, bottom], Stroke::new(w(overlay::LINE_WIDTH), t.guide));
    let st = if active {
        HandleState::Active
    } else {
        HandleState::Rest
    };
    overlay::paint_handle(painter, pos2(x, rect.center().y), HandleKind::Edge, st, t);
}

/// Resolves the cursor when the query changed (the default, then the
/// `on_cursor` hook), and shows it; clears a bitmap cursor once the
/// pointer has left.
fn update_cursor(
    ctx: &egui::Context,
    painter: &egui::Painter,
    state: &mut ViewportState,
    query: CursorQuery,
    hook: Option<&mut CursorHook<'_>>,
    pointer: Option<Pos2>,
    mapping: &Mapping,
) {
    let Some(p) = pointer else {
        if state.cursor.image_set {
            cursor::clear_image(ctx);
            state.cursor.image_set = false;
        }
        state.cursor.shown = None;
        return;
    };
    let cached = state
        .cursor
        .last
        .as_ref()
        .filter(|(q, _)| *q == query)
        .map(|(_, c)| c.clone());
    let resolved = match cached {
        Some(c) => c,
        None => {
            let default = cursor::default_cursor(&query);
            let given = hook.map(|hook| hook(&query, default.clone()));
            let c = match given {
                Some(c) if c != default => {
                    state.record(
                        "on_cursor",
                        "redirect",
                        format!("{} {} to {}", query.tool.name(), default.name(), c.name()),
                    );
                    c
                }
                _ => default,
            };
            state.cursor.last = Some((query, c.clone()));
            c
        }
    };
    let shown = cursor::shown(ctx, &resolved, mapping.zoom, mapping.ppp);
    let image = cursor::apply(ctx, painter, p, &shown);
    if !image && state.cursor.image_set {
        cursor::clear_image(ctx);
    }
    state.cursor.image_set = image;
    state.cursor.shown = Some(shown);
}

/// Draws the loupe beside the pointer: the side's tiles again through
/// the magnifying sampling (a second paint callback on the GPU path, the
/// same egui textures on the CPU path), its grid, its marker and its
/// readout.
#[allow(clippy::too_many_arguments)]
fn draw_loupe(
    ui: &Ui,
    painter: &egui::Painter,
    state: &mut ViewportState,
    mapping: &Mapping,
    pointer: Pos2,
    side: Side,
    readout: Option<&Readout>,
    tokens: &ViewportTokens,
) {
    let l = state.loupe;
    let ctx = ui.ctx();
    let ppp = mapping.ppp;
    let t = egui_pop_core::theme::Tokens::of(ctx);
    let tr = |k: &egui_pop_core::strings::StrKey| egui_pop_core::strings::tr(ctx, k);
    let labels = [
        tr(&crate::strings::LOUPE_DISPLAY),
        tr(&crate::strings::LOUPE_SCENE),
        tr(&crate::strings::LOUPE_NONE),
    ];
    let lines = loupe::readout_lines(
        mapping.pixel_at(pointer),
        readout,
        l.values,
        state.stage.as_ref(),
        [&labels[0], &labels[1], &labels[2]],
    );
    let galley = (!lines.is_empty()).then(|| {
        painter.layout_no_wrap(
            lines.join("\n"),
            egui::FontId::monospace(t.font.small),
            t.text.default,
        )
    });
    let pad = egui_pop_core::scale::snap_size(t.space.xs, ppp);
    let picture = loupe::picture_size(&l, ppp);
    let text_h = galley.as_ref().map_or(0.0, |g| g.size().y + pad);
    let size = egui_pop_core::scale::snap_size(picture.x + 2.0 * pad, ppp);
    let size = Vec2::new(
        size.max(galley.as_ref().map_or(0.0, |g| g.size().x + 2.0 * pad)),
        picture.y + 2.0 * pad + text_h,
    );
    let placed = loupe::place(pointer, size, mapping.rect, l.offset);
    let snap = |v: f32| (v * ppp).round() / ppp;
    let rect = Rect::from_min_size(pos2(snap(placed.min.x), snap(placed.min.y)), size);
    let pic = Rect::from_min_size(rect.min + Vec2::splat(pad), picture);
    painter.rect_filled(rect, 0.0, tokens.loupe_background);
    painter.rect_filled(pic, 0.0, tokens.background);
    let s = loupe::Sampling::new(mapping, pointer, l.magnification(), pic.center());
    let clip = pic.intersect(painter.clip_rect());
    let lp = painter.with_clip_rect(clip);
    let a = mapping.image_to_px([0.0, 0.0]);
    let b = mapping.image_to_px(mapping.image);
    lp.rect_filled(
        loupe::to_points(s.map_rect([a[0], a[1], b[0], b[1]]), ppp),
        0.0,
        tokens.backdrop,
    );
    let mut gpu: Vec<(u64, [f64; 4], [u32; 2])> = Vec::new();
    let mut tiles = 0;
    let whole = Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0));
    for d in state.tile_draws.iter().filter(|d| d.side == side) {
        tiles += 1;
        let dst = s.map_rect(d.dst_px);
        match d.texture {
            TileTexture::Egui(id) => {
                lp.image(id, loupe::to_points(dst, ppp), whole, Color32::WHITE);
            }
            TileTexture::Gpu(id) => gpu.push((id, dst, d.size)),
        }
    }
    let on_gpu = !gpu.is_empty();
    #[cfg(feature = "wgpu")]
    if let (true, Some(stage)) = (on_gpu, state.stage) {
        let paint = crate::gpu::TilePaint {
            slot: state.slot + 2,
            queue: state.tiles.gpu.clone(),
            draws: gpu
                .into_iter()
                .map(|(texture, dst, size)| crate::gpu::GpuDraw {
                    texture,
                    dst: dst.map(|v| v as f32),
                    size,
                })
                .collect(),
            stage,
        };
        lp.add(egui_wgpu::Callback::new_paint_callback(clip, paint));
    }
    #[cfg(not(feature = "wgpu"))]
    let _ = gpu;
    let p = f64::from(ppp);
    let pic_px = [
        f64::from(pic.left()) * p,
        f64::from(pic.top()) * p,
        f64::from(pic.right()) * p,
        f64::from(pic.bottom()) * p,
    ];
    if l.grid {
        // Hairlines centred on the physical pixel past each edge.
        let stroke = Stroke::new(1.0 / ppp, tokens.loupe_grid.gamma_multiply(0.8));
        let half = 0.5 / ppp;
        let (xs, ys) = s.grid_lines(pic_px);
        for x in xs {
            let x = (x / p) as f32 + half;
            lp.line_segment([pos2(x, pic.top()), pos2(x, pic.bottom())], stroke);
        }
        for y in ys {
            let y = (y / p) as f32 + half;
            lp.line_segment([pos2(pic.left(), y), pos2(pic.right(), y)], stroke);
        }
    }
    let w = |x: f32| egui_pop_core::scale::stroke(ctx, x);
    if l.marker {
        let r = loupe::to_points(s.center_block(), ppp);
        lp.rect_stroke(
            r,
            0.0,
            Stroke::new(w(3.0), tokens.cursor_outline),
            StrokeKind::Outside,
        );
        lp.rect_stroke(
            r,
            0.0,
            Stroke::new(w(1.0), tokens.loupe_marker),
            StrokeKind::Outside,
        );
    }
    painter.rect_stroke(
        rect,
        0.0,
        Stroke::new(w(1.0), tokens.loupe_frame),
        StrokeKind::Inside,
    );
    if let Some(g) = galley {
        painter.galley(pos2(pic.left(), pic.bottom() + pad), g, t.text.default);
    }
    state.loupe_frame = Some(LoupeFrame {
        rect,
        picture: pic,
        sampling: s,
        tiles,
        gpu: on_gpu,
        lines,
    });
}
