//! A virtualised thumbnail grid, and the filmstrip (the same view in one
//! row), over images the application lists and thumbnails it supplies.
//!
//! The application gives:
//!
//! - its images ([`LibraryItems`]): how many, each one's key, name,
//!   marks, stack and badges. Only the cells in view are asked for, so a
//!   hundred thousand images cost what a screenful does;
//! - their thumbnails ([`ThumbSource`]), asynchronously: a thumbnail not
//!   there yet is [`Thumb::Pending`] and the cell shows a placeholder
//!   (the image's average colour when known, else a skeleton block); the
//!   source is told which images are in view ([`ThumbSource::wanted`])
//!   so it loads those first, and asks for a repaint when one arrives.
//!
//! The grid does the layout (the thumbnails' size by zoom: Ctrl and the
//! wheel, or + and -), the keyboard (arrows, Page Up and Page Down, Home
//! and End, Space, Ctrl+A, Enter), the selection (a click, Shift for a
//! range, Ctrl to toggle, a rubber band dragged across the cells), the
//! culling keys ([`CullKeymap`]: ratings, flags, labels, auto-advance),
//! the overlays (rating, labels, flag, a stack's count and chevron,
//! badges) and the stacks (collapsed to their top image, or expanded).
//! Every change runs a step (see [`plugin`](super::plugin)); the marks
//! are the application's, which applies the output's signals.
//!
//! A grid and a filmstrip drawn over the same [`LibraryState`] share its
//! selection and its stacks.
//!
//! ```
//! use egui_pop_views::library::grid::{LibraryItems, LibraryState, Thumb, ThumbGrid, ThumbSource};
//! use egui_pop_views::library::marks::Marks;
//!
//! struct Shoot(Vec<(u64, String, u8)>);
//!
//! impl LibraryItems<u64> for Shoot {
//!     fn len(&self) -> usize { self.0.len() }
//!     fn key(&self, i: usize) -> u64 { self.0[i].0 }
//!     fn label(&self, i: usize) -> String { self.0[i].1.clone() }
//!     fn marks(&self, i: usize) -> Marks { Marks { rating: self.0[i].2, ..Marks::default() } }
//! }
//!
//! struct Thumbs;
//!
//! impl ThumbSource<u64> for Thumbs {
//!     fn thumb(&self, _key: &u64, _size: u32) -> Thumb {
//!         Thumb::Pending { average: Some(egui::Color32::from_gray(90)) }
//!     }
//! }
//!
//! fn ui(ui: &mut egui::Ui, state: &mut LibraryState<u64>, shoot: &mut Shoot) {
//!     let out = ThumbGrid::new("grid").show(ui, state, shoot, &Thumbs);
//!     for signal in out.signals {
//!         if let egui_pop_views::library::Signal::Rate { keys, rating } = signal {
//!             for p in shoot.0.iter_mut().filter(|p| keys.contains(&p.0)) {
//!                 p.2 = rating;
//!             }
//!         }
//!     }
//! }
//! ```

use std::collections::{HashMap, HashSet};

use egui::{
    Align2, Color32, Event, EventFilter, Id, Key as EKey, Modifiers, Rect, Response, ScrollArea,
    Sense, Stroke, StrokeKind, TextStyle, TextureId, Ui, Vec2, WidgetInfo, WidgetType, pos2, vec2,
};
use egui_pop_api::AppHandle;
use egui_pop_core::icon::Icon;
use egui_pop_core::kit::{self, BoxStyle, Corners, Kit, KitCx, Part, Side, State};
use egui_pop_core::placeholder::SkeletonTokens;
use egui_pop_core::strings::{tr, tr_args};
use egui_pop_core::theme::Tokens;
use egui_pop_model::ids::StepId;
use serde_json::{Value as Json, json};

use super::marks::{ColorLabel, CullAction, CullKeymap, Flag, MAX_RATING, Marks};
use super::paint::{self, LibraryTokens};
use super::plugin::{self as lib, LIBRARIES, Request};
use crate::select::{self, Key, Mode, Proposal, Selection};
use crate::strings as s;
use crate::{nav, rows};

/// The smallest and largest thumbnails, in points.
pub const MIN_SIZE: f32 = 48.0;
pub const MAX_SIZE: f32 = 512.0;
/// The thumbnails' size at first, in points.
pub const DEFAULT_SIZE: f32 = 128.0;
/// One zoom step (+, -) multiplies the size by this.
pub const ZOOM_STEP: f32 = 1.25;
/// How far the pointer moves before a drag draws a rubber band.
const BAND_MIN: f32 = 4.0;
/// Below this size a cell shows no overlays.
const OVERLAY_MIN: f32 = 64.0;

/// A thumbnail, as the source has it now.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Thumb {
    /// The image: a texture and its size in pixels (drawn fitted).
    Ready { texture: TextureId, size: [u32; 2] },
    /// On its way: the image's average colour when known (a flat
    /// placeholder), else a skeleton block.
    Pending { average: Option<Color32> },
    /// It could not be made: a warning sign stands in.
    Failed,
}

/// Where thumbnails come from; the application implements it, loading
/// them asynchronously (a thread, its engine), caching them, and asking
/// for a repaint when one arrives.
pub trait ThumbSource<K> {
    /// `key`'s thumbnail for a cell `size` pixels square. Not ready yet:
    /// [`Thumb::Pending`], and the source starts making it.
    fn thumb(&self, key: &K, size: u32) -> Thumb;

    /// The images in view this pass (and the size wanted): what to make
    /// first, and what may be dropped from a queue.
    fn wanted(&self, keys: &[K], size: u32) {
        let _ = (keys, size);
    }
}

/// A stack an image belongs to: its top image's key, and how many
/// images it holds. Members follow their top image in the application's
/// order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Stack<K> {
    pub top: K,
    pub count: usize,
}

/// A badge's tone: egui-pop's one tone ([`egui_pop_core::kit::Tone`]).
/// The default kit's `pill` gives `info`, `success`, `warning` and
/// `danger`; a tone it does not give is drawn as its own (neutral) box.
pub use egui_pop_core::kit::Tone;

/// A short text in a corner of a cell: `RAW`, `HDR`, `2×`, an edit mark.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Badge {
    pub text: String,
    pub tone: Tone,
}

impl Badge {
    pub fn new(text: impl Into<String>) -> Badge {
        Badge {
            text: text.into(),
            tone: Tone::Neutral,
        }
    }

    #[must_use]
    pub fn tone(mut self, tone: Tone) -> Badge {
        self.tone = tone;
        self
    }
}

/// The images a library shows; the application owns them.
pub trait LibraryItems<K: Key> {
    fn len(&self) -> usize;

    fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Image `index`'s key (the selection is kept by key).
    fn key(&self, index: usize) -> K;

    /// Its name: the cell's accessible name and caption.
    fn label(&self, index: usize) -> String;

    /// Its rating, labels and flag.
    fn marks(&self, index: usize) -> Marks {
        let _ = index;
        Marks::default()
    }

    /// The stack it belongs to.
    fn stack(&self, index: usize) -> Option<Stack<K>> {
        let _ = index;
        None
    }

    /// Its badges.
    fn badges(&self, index: usize) -> Vec<Badge> {
        let _ = index;
        Vec::new()
    }

    /// Changes whenever stacks change without the count changing: the
    /// view then works out again which images it shows.
    fn revision(&self) -> u64 {
        0
    }
}

/// Which images a view shows, stacks collapsed: positions to indices.
#[derive(Debug, Default)]
struct Order {
    /// What it was made from: the count, the revision, the stacks'.
    made: Option<(usize, u64, u64)>,
    /// `None`: every image, in order.
    map: Option<Vec<usize>>,
    len: usize,
}

/// One view's scrolling and gestures.
#[derive(Clone, Debug, Default)]
struct ViewMem {
    offset: f32,
    extent: f32,
    reveal: Option<usize>,
    cols: usize,
    /// A rubber band: where it started and where it is, in content
    /// coordinates, and the modifiers it started with.
    band: Option<(Vec2, Vec2, Modifiers)>,
}

/// A library's state between passes: the selection, the stacks
/// expanded, the thumbnails' size, each view's scrolling.
#[derive(Debug)]
pub struct LibraryState<K: Key> {
    id: String,
    selection: Selection<K>,
    expanded: HashSet<K>,
    expanded_gen: u64,
    size: f32,
    order: Order,
    views: HashMap<String, ViewMem>,
    len: Option<usize>,
    published: bool,
}

impl<K: Key> LibraryState<K> {
    /// A library named `id` in steps, signals and commands.
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            selection: Selection::default(),
            expanded: HashSet::new(),
            expanded_gen: 0,
            size: DEFAULT_SIZE,
            order: Order::default(),
            views: HashMap::new(),
            len: None,
            published: false,
        }
    }

    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn selection(&self) -> &Selection<K> {
        &self.selection
    }

    /// Sets the selection without running a step.
    pub fn set_selection(&mut self, keys: impl IntoIterator<Item = K>, cursor: Option<K>) {
        self.selection.set(keys, cursor);
        self.published = false;
    }

    /// The thumbnails' size in points.
    pub fn size(&self) -> f32 {
        self.size
    }

    pub fn set_size(&mut self, size: f32) {
        self.size = size.clamp(MIN_SIZE, MAX_SIZE);
    }

    pub fn is_expanded(&self, top: &K) -> bool {
        self.expanded.contains(top)
    }

    /// Expands or collapses a stack (by its top image) without running
    /// a step.
    pub fn set_expanded(&mut self, top: K, expanded: bool) {
        let changed = if expanded {
            self.expanded.insert(top)
        } else {
            self.expanded.remove(&top)
        };
        if changed {
            self.expanded_gen += 1;
        }
    }

    /// How many cells the views show (stacks collapsed), as of the last
    /// pass.
    pub fn shown(&self) -> usize {
        self.order.len
    }

    /// Brings the cursor into view in every view at the next pass.
    pub fn reveal_cursor(&mut self) {
        for v in self.views.values_mut() {
            v.reveal = Some(usize::MAX);
        }
    }

    fn refresh(&mut self, items: &dyn LibraryItems<K>) {
        let made = (items.len(), items.revision(), self.expanded_gen);
        if self.order.made == Some(made) {
            return;
        }
        self.order.made = Some(made);
        let n = items.len();
        let mut map = Vec::new();
        let mut stacked = false;
        for i in 0..n {
            match items.stack(i) {
                None => map.push(i),
                Some(st) => {
                    stacked = true;
                    if self.expanded.contains(&st.top) || items.key(i) == st.top {
                        map.push(i);
                    }
                }
            }
        }
        self.order.len = if stacked { map.len() } else { n };
        self.order.map = stacked.then_some(map);
    }

    fn index(&self, position: usize) -> usize {
        self.order.map.as_ref().map_or(position, |m| m[position])
    }
}

/// What happened in a library view this pass.
#[derive(Clone, Debug, PartialEq)]
pub enum Signal<K> {
    Select {
        keys: Vec<K>,
        cursor: Option<K>,
    },
    /// The user rated these images; the application rates them.
    Rate {
        keys: Vec<K>,
        rating: u8,
    },
    /// A colour label put on (`on`) or taken off.
    Label {
        keys: Vec<K>,
        label: ColorLabel,
        on: bool,
    },
    Flag {
        keys: Vec<K>,
        flag: Flag,
    },
    Tag {
        keys: Vec<K>,
        add: Vec<String>,
        remove: Vec<String>,
    },
    /// A stack expanded or collapsed.
    Stack {
        top: K,
        expanded: bool,
    },
    /// An image opened (Enter, a double click).
    Activate {
        key: K,
    },
    /// The thumbnails' size changed (zoom).
    Zoom {
        size: f32,
    },
    /// A step was vetoed.
    Vetoed {
        step: StepId,
        reason: String,
    },
}

/// What [`ThumbGrid::show`] returns.
#[derive(Debug)]
pub struct LibraryOutput<K> {
    /// The view as one focusable widget.
    pub response: Response,
    pub signals: Vec<Signal<K>>,
    /// How many cells were built this pass (those in view).
    pub built: usize,
    /// How many cells a row holds.
    pub columns: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Axis {
    Grid,
    Strip,
}

/// What the view does after drawing, in order.
#[derive(Debug)]
enum Op<K> {
    Select(Proposal<K>),
    /// Moves to the next image if the previous mark was not refused.
    Advance(usize),
    Run(StepId, Json),
    /// A request that could not be read.
    Fail(StepId, String),
    Activate(K),
    Zoom(f32),
}

/// The thumbnail grid, or the filmstrip ([`ThumbGrid::filmstrip`]).
#[derive(Debug)]
pub struct ThumbGrid<'a> {
    view: String,
    axis: Axis,
    label: Option<String>,
    mode: Mode,
    app: Option<&'a AppHandle>,
    height: Option<f32>,
    keymap: CullKeymap,
    captions: bool,
    overlays: bool,
}

impl<'a> ThumbGrid<'a> {
    /// A grid named `view` (a library may have several views).
    pub fn new(view: impl Into<String>) -> Self {
        Self {
            view: view.into(),
            axis: Axis::Grid,
            label: None,
            mode: Mode::Multiple,
            app: None,
            height: None,
            keymap: CullKeymap::lightroom(),
            captions: true,
            overlays: true,
        }
    }

    /// A filmstrip: one row, scrolled sideways; its cells fit its height.
    pub fn filmstrip(view: impl Into<String>) -> Self {
        Self {
            axis: Axis::Strip,
            captions: false,
            ..Self::new(view)
        }
    }

    /// Its accessible name.
    pub fn label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }

    /// How many images may be selected (several by default).
    pub fn mode(mut self, mode: Mode) -> Self {
        self.mode = mode;
        self
    }

    /// Runs its moments as steps of this application, and registers the
    /// library for commands.
    pub fn app(mut self, app: &'a AppHandle) -> Self {
        self.app = Some(app);
        self
    }

    /// Its height (default: what is available for a grid, 120 points
    /// for a filmstrip).
    pub fn height(mut self, height: f32) -> Self {
        self.height = Some(height);
        self
    }

    /// The culling keys ([`CullKeymap::lightroom`] by default).
    pub fn keymap(mut self, keymap: CullKeymap) -> Self {
        self.keymap = keymap;
        self
    }

    /// Each image's name under its thumbnail (on in a grid).
    pub fn captions(mut self, on: bool) -> Self {
        self.captions = on;
        self
    }

    /// Ratings, labels, flags, stacks and badges over the thumbnails.
    pub fn overlays(mut self, on: bool) -> Self {
        self.overlays = on;
        self
    }

    pub fn show<K: Key>(
        self,
        ui: &mut Ui,
        state: &mut LibraryState<K>,
        items: &dyn LibraryItems<K>,
        source: &dyn ThumbSource<K>,
    ) -> LibraryOutput<K> {
        let id = Id::new(("egui_pop_views::library", &state.id, &self.view));
        let grid = self.axis == Axis::Grid;
        let len = items.len();
        let before = state.selection.len();
        crate::list::retain(&mut state.selection, &mut state.len, len, &|i| items.key(i));
        if state.selection.len() != before {
            state.published = false;
        }
        state.refresh(items);
        let n = state.order.len;
        let t = Tokens::of(ui.ctx());
        let lt = LibraryTokens::of(ui.ctx());
        let kit = Kit::of(ui.ctx());
        let gap = t.space.s;
        let small = TextStyle::Small.resolve(ui.style());
        let caption_h = if self.captions {
            ui.text_style_height(&TextStyle::Small) + t.space.xs
        } else {
            0.0
        };
        let strip_h = self.height.unwrap_or(120.0);
        let size = if grid {
            state.size
        } else {
            (strip_h - caption_h - 2.0 * gap - ui.spacing().scroll.allocated_width())
                .max(MIN_SIZE / 2.0)
        };
        let cell = vec2(size, size + caption_h);
        let line = if grid { cell.y + gap } else { cell.x + gap };
        let mut mem = state.views.remove(&self.view).unwrap_or_default();
        if grid {
            let width = ui.available_width() - ui.spacing().scroll.allocated_width();
            mem.cols = (((width - gap) / (cell.x + gap)).floor().max(1.0)) as usize;
        } else {
            mem.cols = 1;
        }
        let cols = mem.cols;
        let lines = if grid { n.div_ceil(cols) } else { n };
        let line_of = |p: usize| if grid { p / cols } else { p };
        let key_at = |p: usize| items.key(state.index(p));
        let focused = ui.memory(|m| m.has_focus(id));
        let mut ops: Vec<Op<K>> = Vec::new();

        // Requests from commands, before the user's keys.
        let registry = self.app.and_then(|a| a.service(&LIBRARIES));
        if let Some(reg) = &registry {
            reg.touch(&state.id);
            for r in reg.take(&state.id) {
                ops.push(request(r));
            }
        }

        // The cursor's position, when there is one in view.
        let cursor = state
            .selection
            .cursor()
            .and_then(|c| (0..n).find(|&p| key_at(p) == *c));
        if focused && n > 0 {
            let page_lines = ((mem.extent / line).floor() as usize)
                .saturating_sub(1)
                .max(1);
            let page = if grid { page_lines * cols } else { page_lines };
            let last = n - 1;
            let events = ui.input(|i| i.events.clone());
            for e in events {
                let Event::Key {
                    key,
                    pressed: true,
                    modifiers,
                    ..
                } = e
                else {
                    continue;
                };
                let c = cursor;
                let to = match key {
                    EKey::ArrowLeft => Some(c.map_or(0, |c| c.saturating_sub(1))),
                    EKey::ArrowRight => Some(c.map_or(0, |c| (c + 1).min(last))),
                    EKey::ArrowUp if grid => {
                        Some(c.map_or(last, |c| if c >= cols { c - cols } else { c }))
                    }
                    EKey::ArrowDown if grid => {
                        Some(c.map_or(0, |c| if c + cols <= last { c + cols } else { c }))
                    }
                    EKey::PageUp => Some(c.map_or(0, |c| c.saturating_sub(page))),
                    EKey::PageDown => Some(c.map_or(0, |c| (c + page).min(last))),
                    EKey::Home => Some(0),
                    EKey::End => Some(last),
                    _ => None,
                };
                if let Some(j) = to {
                    ops.push(Op::Select(
                        state.selection.move_to(self.mode, n, &key_at, j, modifiers),
                    ));
                    mem.reveal = Some(j);
                    continue;
                }
                match key {
                    EKey::Space if modifiers.command => {
                        ops.extend(state.selection.toggle_cursor(self.mode).map(Op::Select));
                    }
                    EKey::Space => {
                        if let Some(c) = c {
                            ops.push(Op::Select(state.selection.move_to(
                                self.mode,
                                n,
                                &key_at,
                                c,
                                Modifiers::NONE,
                            )));
                        }
                    }
                    EKey::A if modifiers.command && self.mode == Mode::Multiple => {
                        ops.push(Op::Select(state.selection.all(n, &key_at)));
                    }
                    EKey::Enter => {
                        if let Some(c) = c {
                            ops.push(Op::Activate(key_at(c)));
                        }
                    }
                    _ if !modifiers.command && !modifiers.alt => {
                        if let Some(action) = self.keymap.action(key) {
                            ops.extend(cull(action, &self.keymap, state, items, c, grid));
                        }
                    }
                    _ => {}
                }
            }
        }
        if mem.reveal == Some(usize::MAX) {
            mem.reveal = cursor;
        }

        // The views: rows of cells, or one row; only those in view built.
        let label = self
            .label
            .clone()
            .unwrap_or_else(|| tr(ui.ctx(), if grid { &s::LIBRARY } else { &s::FILMSTRIP }));
        let mut area = if grid {
            ScrollArea::vertical()
        } else {
            ScrollArea::horizontal()
        }
        .id_salt(id.with("scroll"))
        .auto_shrink([false, !grid]);
        if grid && let Some(h) = self.height {
            area = area.max_height(h).min_scrolled_height(h);
        }
        if let Some(p) = mem.reveal.take() {
            let o = nav::reveal(line_of(p), line, mem.offset, mem.extent);
            if o != mem.offset {
                area = if grid {
                    area.vertical_scroll_offset(o.max(0.0))
                } else {
                    area.horizontal_scroll_offset(o.max(0.0))
                };
            }
        }
        let px = (size * ui.ctx().pixels_per_point()).round().max(1.0) as u32;
        let mode = self.mode;
        let overlays = self.overlays && size >= OVERLAY_MIN;
        let captions = self.captions;
        let selection = &state.selection;
        let expanded = &state.expanded;
        let order = &state.order;
        let index = |p: usize| order.map.as_ref().map_or(p, |m| m[p]);
        let mut band = mem.band.take();
        let mut take_focus = false;
        let mut built = 0;
        let mut wanted: Vec<K> = Vec::new();
        let mut cell_ops: Vec<Op<K>> = Vec::new();
        let out = area.show_viewport(ui, |ui, viewport| {
            let origin = ui.max_rect().min;
            if grid {
                ui.set_min_size(vec2(ui.available_width(), lines as f32 * line));
            } else {
                ui.set_min_size(vec2(lines as f32 * line, cell.y + 2.0 * gap));
            }
            let (lo, hi) = if grid {
                (viewport.min.y, viewport.max.y)
            } else {
                (viewport.min.x, viewport.max.x)
            };
            let first = ((lo / line).floor().max(0.0) as usize).min(lines);
            let last = ((hi / line).ceil().max(0.0) as usize).min(lines);
            let rect_of = |p: usize| -> Rect {
                let min = if grid {
                    let (row, col) = (p / cols, p % cols);
                    origin
                        + vec2(
                            gap + col as f32 * (cell.x + gap),
                            gap / 2.0 + row as f32 * line,
                        )
                } else {
                    origin + vec2(gap / 2.0 + p as f32 * line, gap)
                };
                Rect::from_min_size(min, cell)
            };
            // The rubber band, under the cells: a drag from anywhere.
            let visible = Rect::from_min_size(origin + viewport.min.to_vec2(), viewport.size());
            let bg = ui.interact(visible, id.with("band"), Sense::DRAG);
            if mode == Mode::Multiple {
                if bg.drag_started()
                    && let Some(p) = bg.interact_pointer_pos()
                {
                    let at = p - origin;
                    band = Some((at, at, click_modifiers(ui)));
                    take_focus = true;
                }
                if let (Some(b), Some(p)) = (band.as_mut(), bg.interact_pointer_pos())
                    && bg.dragged()
                {
                    b.1 = p - origin;
                }
            }
            // A drag-only area starts dragging on the press: a band is one
            // once the pointer has moved (a click is not a band).
            let band_cells: Vec<usize> = band
                .filter(|(a, b, _)| (*b - *a).length() >= BAND_MIN)
                .map(|(a, b, _)| {
                    cells_in(
                        Rect::from_two_pos(a.to_pos2(), b.to_pos2()),
                        grid,
                        cols,
                        n,
                        cell,
                        gap,
                        line,
                    )
                })
                .unwrap_or_default();
            let painter = ui.painter().clone();
            let cx = KitCx {
                painter: &painter,
                ppp: ui.ctx().pixels_per_point(),
            };
            for l in first..last {
                for col in 0..cols {
                    let p = if grid { l * cols + col } else { l };
                    if p >= n {
                        break;
                    }
                    built += 1;
                    let i = index(p);
                    let key = items.key(i);
                    let rect = rect_of(p);
                    let r = ui.interact(rect, id.with(("cell", &key)), Sense::CLICK);
                    let in_band = band_cells.binary_search(&p).is_ok();
                    let selected = selection.contains(&key) || in_band;
                    let is_cursor = focused && selection.cursor() == Some(&key);
                    let marks = items.marks(i);
                    let stack = items.stack(i);
                    let name = items.label(i);
                    // The cell's box (its outline and the cursor's ring
                    // too: nothing of the content overlaps them), then
                    // its image. A selected cell is not hovered.
                    let cell_state = State::REST
                        .with(State::SELECTED, selected)
                        .with(State::HOVER, r.hovered() && !selected)
                        .with(State::FOCUS, is_cursor);
                    let cell_part = Part {
                        rect,
                        room: rect,
                        state: cell_state,
                        label: None,
                    };
                    kit.thumb_cell.paint(&cx, &cell_part);
                    let cell_box = *kit.thumb_cell.style(cell_state);
                    let image = Rect::from_min_size(rect.min, vec2(size, size)).shrink(t.space.xs);
                    match source.thumb(&key, px) {
                        Thumb::Ready { texture, size } => {
                            painter.image(
                                texture,
                                crate::library::fitted(image, size),
                                Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
                                Color32::WHITE,
                            );
                        }
                        Thumb::Pending { average: Some(c) } => {
                            painter.rect_filled(image.shrink(size * 0.08), 0.0, c);
                        }
                        Thumb::Pending { average: None } => {
                            painter.rect_filled(
                                image.shrink(size * 0.08),
                                t.radius.widget,
                                SkeletonTokens::of(ui.ctx()).fill,
                            );
                        }
                        Thumb::Failed => {
                            let side = (size * 0.25).min(t.size.icon_panel);
                            Icon::Warning.paint(
                                &painter,
                                Rect::from_center_size(image.center(), vec2(side, side)),
                                kit.thumb_layer.style(State::REST).text,
                            );
                        }
                    }
                    wanted.push(key.clone());
                    let layer = |rect: Rect, tone: kit::Tone| {
                        kit.thumb_layer.paint(
                            &cx,
                            &Part {
                                rect,
                                room: rect,
                                state: State::REST.toned(tone),
                                label: None,
                            },
                        );
                    };
                    if marks.flag == Flag::Reject {
                        layer(image, kit::Tone::Muted);
                    }
                    // A member of an expanded stack: a bar on its left
                    // (the layer's indicator), under the cell's outline.
                    let member = stack.as_ref().is_some_and(|st| st.top != key);
                    if member {
                        layer(
                            Rect::from_min_size(rect.min, vec2(size, size)),
                            kit::Tone::Accent,
                        );
                        if cell_box.border_width > 0.0 {
                            let outline = BoxStyle {
                                fill: Color32::TRANSPARENT,
                                indicator_size: 0.0,
                                ..cell_box
                            };
                            kit::paint_box(&cx, rect, &outline, Corners::All, Side::Bottom);
                        }
                    }
                    if captions {
                        painter.with_clip_rect(rect.intersect(ui.clip_rect())).text(
                            pos2(rect.center().x, rect.bottom() - caption_h / 2.0),
                            Align2::CENTER_CENTER,
                            &name,
                            small.clone(),
                            cell_box.text,
                        );
                    }
                    if overlays {
                        let star = (size / 10.0).clamp(10.0, 16.0);
                        overlay(
                            &cx,
                            &kit,
                            &lt,
                            &t,
                            image,
                            &marks,
                            &items.badges(i),
                            r.hovered(),
                            star,
                            &small,
                        );
                        // The stars, clickable.
                        for j in 0..MAX_RATING {
                            let c = pos2(
                                image.left() + star * (f32::from(j) + 0.75),
                                image.bottom() - star * 0.75,
                            );
                            let sr = ui.interact(
                                Rect::from_center_size(c, vec2(star, star)),
                                id.with(("star", &key, j)),
                                Sense::CLICK,
                            );
                            let n_stars = j + 1;
                            let sname = tr_args(
                                ui.ctx(),
                                &s::RATE_STARS,
                                &[("stars", &n_stars.to_string())],
                            );
                            sr.widget_info(move || {
                                WidgetInfo::labeled(WidgetType::Button, true, &sname)
                            });
                            if sr.clicked() {
                                let rating = if marks.rating == n_stars { 0 } else { n_stars };
                                let keys = targets_of(selection, &key);
                                cell_ops.push(Op::Run(
                                    lib::LIBRARY_RATE,
                                    json!({ "keys": keys, "rating": rating }),
                                ));
                            }
                        }
                        if let Some(st) = stack.as_ref().filter(|st| st.top == key && st.count > 1)
                        {
                            let open = expanded.contains(&st.top);
                            let (chip, chip_text) = paint::pill(
                                &painter,
                                &kit.pill,
                                kit::Tone::Neutral,
                                image.left_top() + vec2(t.space.xs, t.space.xs + 18.0),
                                Align2::LEFT_TOP,
                                &format!("{}  ", st.count),
                                small.clone(),
                            );
                            let chevron = Rect::from_min_size(
                                pos2(chip.right() - chip.height(), chip.top()),
                                vec2(chip.height(), chip.height()),
                            );
                            let icon = if open {
                                Icon::ChevronDown
                            } else {
                                Icon::ChevronRight
                            };
                            icon.paint(&painter, chevron, chip_text);
                            let cr = ui.interact(chip, id.with(("stack", &key)), Sense::CLICK);
                            let count = st.count.to_string();
                            let cname = tr_args(
                                ui.ctx(),
                                if open {
                                    &s::COLLAPSE_STACK
                                } else {
                                    &s::EXPAND_STACK
                                },
                                &[("count", &count)],
                            );
                            cr.widget_info(move || {
                                WidgetInfo::labeled(WidgetType::Button, true, &cname)
                            });
                            if cr.clicked() {
                                cell_ops.push(Op::Run(
                                    lib::LIBRARY_STACK,
                                    json!({ "stack": st.top, "expanded": !open }),
                                ));
                            }
                        }
                    }
                    let a11y_name = name.clone();
                    r.widget_info(move || {
                        WidgetInfo::selected(
                            WidgetType::SelectableLabel,
                            true,
                            selected,
                            &a11y_name,
                        )
                    });
                    if r.double_clicked() {
                        cell_ops.push(Op::Activate(key.clone()));
                    } else if r.clicked() {
                        take_focus = true;
                        let mods = click_modifiers(ui);
                        cell_ops.push(Op::Select(selection.click(
                            mode,
                            n,
                            &|q| items.key(index(q)),
                            p,
                            mods,
                        )));
                    }
                }
            }
            if let Some((a, b, _)) = band
                && (b - a).length() >= BAND_MIN
            {
                let r = Rect::from_two_pos(origin + a, origin + b);
                painter.rect_filled(r, 0.0, lt.band);
                painter.rect_stroke(
                    r,
                    0.0,
                    Stroke::new(1.0, lt.band_outline),
                    StrokeKind::Inside,
                );
            }
            if bg.drag_stopped()
                && let Some((_, _, mods)) = band.take()
                && !band_cells.is_empty()
            {
                let mut keys: Vec<K> = if mods.command || mods.shift {
                    selection.keys().to_vec()
                } else {
                    Vec::new()
                };
                for &p in &band_cells {
                    let k = items.key(index(p));
                    if !keys.contains(&k) {
                        keys.push(k);
                    }
                }
                let cursor = band_cells.last().map(|&p| items.key(index(p)));
                cell_ops.push(Op::Select(Proposal {
                    keys,
                    cursor: cursor.clone(),
                    anchor: cursor,
                }));
            }
        });
        ops.extend(cell_ops);
        mem.band = band;
        if grid {
            mem.offset = out.state.offset.y;
            mem.extent = out.inner_rect.height();
        } else {
            mem.offset = out.state.offset.x;
            mem.extent = out.inner_rect.width();
        }
        let response = ui.interact(out.inner_rect, id, Sense::focusable_noninteractive());
        egui_pop_core::a11y::name(&response, WidgetType::Other, &label);
        if take_focus {
            response.request_focus();
        }
        if response.has_focus() {
            ui.memory_mut(|m| {
                m.set_focus_lock_filter(
                    id,
                    EventFilter {
                        horizontal_arrows: true,
                        vertical_arrows: true,
                        ..Default::default()
                    },
                )
            });
        }
        rows::paint_view(
            ui,
            &egui_pop_core::kit::Kit::of(ui.ctx()),
            out.inner_rect,
            response.has_focus(),
        );
        // Zoom: Ctrl and the wheel over a grid.
        if grid && ui.rect_contains_pointer(out.inner_rect) {
            let z = ui.input(|i| i.zoom_delta());
            if (z - 1.0).abs() > f32::EPSILON {
                ops.push(Op::Zoom(state.size * z));
            }
        }
        if !wanted.is_empty() {
            source.wanted(&wanted, px);
        }

        let signals = self.run_ops(ops, state, items, &mut mem);
        if focused && let Some(reg) = &registry {
            reg.set_focused(&state.id);
        }
        if !state.published
            && let Some(reg) = &registry
        {
            let keys = state
                .selection
                .keys()
                .iter()
                .map(|k| serde_json::to_value(k).unwrap_or(Json::Null))
                .collect();
            let cursor = serde_json::to_value(state.selection.cursor()).unwrap_or(Json::Null);
            reg.publish(&state.id, n, keys, cursor);
            state.published = true;
        }
        state.views.insert(self.view.clone(), mem);
        if !signals.is_empty() {
            ui.ctx().request_repaint();
        }
        LibraryOutput {
            response,
            signals,
            built,
            columns: cols,
        }
    }

    fn run_ops<K: Key>(
        &self,
        ops: Vec<Op<K>>,
        state: &mut LibraryState<K>,
        items: &dyn LibraryItems<K>,
        mem: &mut ViewMem,
    ) -> Vec<Signal<K>> {
        let app = self.app;
        let n = state.order.len;
        let mut out = Vec::new();
        let mut last_ok = true;
        for op in ops {
            let p = match op {
                Op::Select(p) => p,
                Op::Advance(from) => {
                    if !last_ok || from + 1 >= n {
                        continue;
                    }
                    mem.reveal = Some(from + 1);
                    let key_at = |q: usize| items.key(state.index(q));
                    state
                        .selection
                        .move_to(self.mode, n, &key_at, from + 1, Modifiers::NONE)
                }
                Op::Run(step, mut args) => {
                    complete(&step, &mut args, state, items);
                    args["library"] = json!(state.id);
                    match lib::report(app, &step, args)
                        .and_then(|a| signal_of(&step, &a, Some(&mut *state)))
                    {
                        Ok(signal) => {
                            last_ok = true;
                            out.push(signal);
                        }
                        Err(reason) => {
                            last_ok = false;
                            out.push(Signal::Vetoed { step, reason });
                        }
                    }
                    continue;
                }
                Op::Fail(step, reason) => {
                    out.push(Signal::Vetoed { step, reason });
                    continue;
                }
                Op::Activate(key) => {
                    out.push(Signal::Activate { key });
                    continue;
                }
                Op::Zoom(size) => {
                    let before = state.size;
                    state.set_size(size);
                    if state.size != before {
                        out.push(Signal::Zoom { size: state.size });
                    }
                    continue;
                }
            };
            if p.keys == state.selection.keys() && p.cursor.as_ref() == state.selection.cursor() {
                continue;
            }
            let id = state.id.clone();
            match select::run(
                app,
                &lib::LIBRARY_SELECT,
                "library",
                &id,
                &mut state.selection,
                p,
            ) {
                Ok(done) => {
                    state.published = false;
                    last_ok = true;
                    out.push(Signal::Select {
                        keys: done.keys,
                        cursor: done.cursor,
                    });
                }
                Err(reason) => {
                    last_ok = false;
                    out.push(Signal::Vetoed {
                        step: lib::LIBRARY_SELECT,
                        reason,
                    });
                }
            }
        }
        out
    }
}

/// A change of marks or tags made outside a grid (a panel's stars, a
/// tag input), for [`change`].
#[derive(Clone, Debug, PartialEq)]
pub enum Change<K> {
    Rate {
        keys: Vec<K>,
        rating: u8,
    },
    Label {
        keys: Vec<K>,
        label: ColorLabel,
        on: bool,
    },
    Flag {
        keys: Vec<K>,
        flag: Flag,
    },
    Tag {
        keys: Vec<K>,
        add: Vec<String>,
        remove: Vec<String>,
    },
}

/// Runs a change's step (`library.rate`, `library.label`,
/// `library.flag`, `library.tag`) for library `library`, as the grid
/// does: the signal to apply (with what a before-hook redirected it
/// to), or [`Signal::Vetoed`].
pub fn change<K: Key>(app: Option<&AppHandle>, library: &str, change: Change<K>) -> Signal<K> {
    let (step, mut args) = match change {
        Change::Rate { keys, rating } => (
            lib::LIBRARY_RATE,
            json!({ "keys": keys, "rating": rating.min(MAX_RATING) }),
        ),
        Change::Label { keys, label, on } => (
            lib::LIBRARY_LABEL,
            json!({ "keys": keys, "label": label.name(), "on": on }),
        ),
        Change::Flag { keys, flag } => (
            lib::LIBRARY_FLAG,
            json!({ "keys": keys, "flag": flag.name() }),
        ),
        Change::Tag { keys, add, remove } => (
            lib::LIBRARY_TAG,
            json!({ "keys": keys, "add": add, "remove": remove }),
        ),
    };
    args["library"] = json!(library);
    match lib::report(app, &step, args).and_then(|a| signal_of(&step, &a, None)) {
        Ok(signal) => signal,
        Err(reason) => Signal::Vetoed { step, reason },
    }
}

/// What the marks act on: the selection, else the cursor.
fn targets<K: Key>(selection: &Selection<K>) -> Vec<K> {
    if selection.is_empty() {
        selection.cursor().cloned().into_iter().collect()
    } else {
        selection.keys().to_vec()
    }
}

/// What a click on a cell's overlay acts on: the selection when the
/// cell is in it, else the cell alone.
fn targets_of<K: Key>(selection: &Selection<K>, key: &K) -> Vec<K> {
    if selection.contains(key) {
        selection.keys().to_vec()
    } else {
        vec![key.clone()]
    }
}

/// The operations a culling key asks for.
#[allow(clippy::too_many_arguments)]
fn cull<K: Key>(
    action: CullAction,
    keymap: &CullKeymap,
    state: &LibraryState<K>,
    items: &dyn LibraryItems<K>,
    cursor: Option<usize>,
    grid: bool,
) -> Vec<Op<K>> {
    let keys = targets(&state.selection);
    let mut ops = Vec::new();
    let mark = match action {
        CullAction::Rate(r) => Some((
            lib::LIBRARY_RATE,
            json!({ "keys": keys, "rating": r.min(MAX_RATING) }),
        )),
        CullAction::Flag(f) => Some((lib::LIBRARY_FLAG, json!({ "keys": keys, "flag": f.name() }))),
        CullAction::Label(l) => {
            let on = cursor.is_none_or(|c| !items.marks(state.index(c)).labels.has(l));
            Some((
                lib::LIBRARY_LABEL,
                json!({ "keys": keys, "label": l.name(), "on": on }),
            ))
        }
        CullAction::ToggleStack => {
            if let Some(st) = cursor
                .and_then(|c| items.stack(state.index(c)))
                .filter(|st| st.count > 1)
            {
                let open = state.is_expanded(&st.top);
                ops.push(Op::Run(
                    lib::LIBRARY_STACK,
                    json!({ "stack": st.top, "expanded": !open }),
                ));
            }
            None
        }
        CullAction::ZoomIn if grid => {
            ops.push(Op::Zoom(state.size * ZOOM_STEP));
            None
        }
        CullAction::ZoomOut if grid => {
            ops.push(Op::Zoom(state.size / ZOOM_STEP));
            None
        }
        CullAction::ZoomIn | CullAction::ZoomOut => None,
    };
    if let Some((step, args)) = mark
        && !keys.is_empty()
    {
        ops.push(Op::Run(step, args));
        if keymap.auto_advance
            && let Some(c) = cursor
        {
            ops.push(Op::Advance(c));
        }
    }
    ops
}

/// The operation a command's request asks for: its keys filled in
/// (else the selection), a label's `on` (else toggled from the
/// cursor's image).
fn request<K: Key>(r: Request) -> Op<K> {
    let Request { step, args } = r;
    if step != lib::LIBRARY_SELECT {
        return Op::Run(step, args);
    }
    let parsed = lib::parse::<Vec<K>>(&args["keys"], "keys").and_then(|keys| {
        let cursor: Option<K> = lib::parse(&args["cursor"], "cursor")?;
        Ok(Proposal {
            keys,
            cursor: cursor.clone(),
            anchor: cursor,
        })
    });
    match parsed {
        Ok(p) => Op::Select(p),
        Err(e) => Op::Fail(step, e),
    }
}

/// Completes a mark's arguments when it runs: its keys (else the
/// selection), a label's `on` (else toggled from the cursor's image).
fn complete<K: Key>(
    step: &StepId,
    args: &mut Json,
    state: &LibraryState<K>,
    items: &dyn LibraryItems<K>,
) {
    if args.get("keys").is_none() {
        args["keys"] = json!(targets(&state.selection));
    }
    if *step == lib::LIBRARY_LABEL && args.get("on").is_none() {
        let label = args["label"].as_str().and_then(ColorLabel::parse);
        let cursor = state
            .selection
            .cursor()
            .and_then(|c| (0..state.order.len).find(|&p| items.key(state.index(p)) == *c));
        let on = match (label, cursor) {
            (Some(l), Some(c)) => !items.marks(state.index(c)).labels.has(l),
            _ => true,
        };
        args["on"] = json!(on);
    }
}

/// The signal a mark's step gives, from the arguments it ran with; a
/// stack's step expands or collapses it.
fn signal_of<K: Key>(
    step: &StepId,
    a: &Json,
    state: Option<&mut LibraryState<K>>,
) -> Result<Signal<K>, String> {
    let keys = || lib::parse::<Vec<K>>(&a["keys"], "keys");
    if *step == lib::LIBRARY_RATE {
        let rating = a["rating"]
            .as_u64()
            .filter(|r| *r <= u64::from(MAX_RATING))
            .ok_or("`rating` is 0 to 5")? as u8;
        Ok(Signal::Rate {
            keys: keys()?,
            rating,
        })
    } else if *step == lib::LIBRARY_LABEL {
        let label = a["label"]
            .as_str()
            .and_then(ColorLabel::parse)
            .ok_or("`label` is red, yellow, green, blue or purple")?;
        let on = a["on"].as_bool().unwrap_or(true);
        Ok(Signal::Label {
            keys: keys()?,
            label,
            on,
        })
    } else if *step == lib::LIBRARY_FLAG {
        let flag = a["flag"]
            .as_str()
            .and_then(Flag::parse)
            .ok_or("`flag` is pick, reject or none")?;
        Ok(Signal::Flag {
            keys: keys()?,
            flag,
        })
    } else if *step == lib::LIBRARY_TAG {
        let list = |k: &str| -> Result<Vec<String>, String> {
            if a[k].is_null() {
                Ok(Vec::new())
            } else {
                lib::parse(&a[k], k)
            }
        };
        Ok(Signal::Tag {
            keys: keys()?,
            add: list("add")?,
            remove: list("remove")?,
        })
    } else if *step == lib::LIBRARY_STACK {
        let top: K = lib::parse(&a["stack"], "stack")?;
        let expanded = a["expanded"]
            .as_bool()
            .ok_or("`expanded` is true or false")?;
        if let Some(state) = state {
            state.set_expanded(top.clone(), expanded);
        }
        Ok(Signal::Stack { top, expanded })
    } else {
        Err(format!("`{step}` is not a library mark"))
    }
}

/// The positions of the cells a rectangle (in content coordinates)
/// touches, in order.
fn cells_in(
    rect: Rect,
    grid: bool,
    cols: usize,
    n: usize,
    cell: Vec2,
    gap: f32,
    line: f32,
) -> Vec<usize> {
    let range =
        |lo: f32, hi: f32, start: f32, extent: f32, step: f32, max: usize| -> (usize, usize) {
            let first = ((lo - start - extent) / step).ceil().max(0.0) as usize;
            let last = ((hi - start) / step).floor();
            if last < 0.0 {
                return (1, 0);
            }
            (first, (last as usize).min(max))
        };
    let mut out = Vec::new();
    if n == 0 {
        return out;
    }
    if grid {
        let (c0, c1) = range(rect.min.x, rect.max.x, gap, cell.x, cell.x + gap, cols - 1);
        let (r0, r1) = range(
            rect.min.y,
            rect.max.y,
            gap / 2.0,
            cell.y,
            line,
            (n - 1) / cols,
        );
        for r in r0..=r1 {
            for c in c0..=c1 {
                let p = r * cols + c;
                if p < n {
                    out.push(p);
                }
            }
        }
    } else {
        if rect.max.y < gap || rect.min.y > gap + cell.y {
            return out;
        }
        let (p0, p1) = range(rect.min.x, rect.max.x, gap / 2.0, cell.x, line, n - 1);
        out.extend(p0..=p1);
    }
    out
}

/// A cell's overlays: its flag, labels, rating and badges.
#[allow(clippy::too_many_arguments)]
fn overlay(
    cx: &KitCx<'_>,
    kit: &Kit,
    lt: &LibraryTokens,
    t: &Tokens,
    image: Rect,
    marks: &Marks,
    badges: &[Badge],
    hovered: bool,
    star: f32,
    small: &egui::FontId,
) {
    let painter = cx.painter;
    let badge = |rect: Rect| {
        kit.thumb_overlay_badge.paint(
            cx,
            &Part {
                rect,
                room: rect,
                state: State::REST,
                label: None,
            },
        );
    };
    let pad = t.space.xs;
    let icon = 16.0;
    if marks.flag != Flag::None {
        let r = Rect::from_min_size(image.left_top() + vec2(pad, pad), vec2(icon, icon));
        badge(r);
        match marks.flag {
            Flag::Pick => paint::flag(painter, r.shrink(2.0), lt.pick),
            _ => paint::cross(painter, r.shrink(2.0), lt.reject),
        }
    }
    let labels: Vec<ColorLabel> = marks.labels.iter().collect();
    if !labels.is_empty() {
        let w = labels.len() as f32 * 13.0 + 3.0;
        let back = Rect::from_min_size(
            pos2(image.right() - pad - w, image.top() + pad),
            vec2(w, icon),
        );
        badge(back);
        for (k, l) in labels.iter().enumerate() {
            let c = pos2(back.left() + 8.0 + k as f32 * 13.0, back.center().y);
            paint::label(painter, *l, c, 5.0, lt.label(*l));
        }
    }
    if marks.rating > 0 || hovered {
        let back = Rect::from_min_size(
            pos2(image.left() + star * 0.25, image.bottom() - star * 1.25),
            vec2(star * f32::from(MAX_RATING) + star * 0.5, star),
        );
        badge(back);
        for j in 0..MAX_RATING {
            let c = pos2(
                image.left() + star * (f32::from(j) + 0.75),
                image.bottom() - star * 0.75,
            );
            let fill = (j < marks.rating).then_some(lt.star);
            if fill.is_some() || hovered {
                paint::star(painter, c, star * 0.45, fill, lt.star_empty);
            }
        }
    }
    let mut right = image.right() - pad;
    for b in badges.iter().rev() {
        let tone = b.tone;
        let (r, _) = paint::pill(
            painter,
            &kit.pill,
            tone,
            pos2(right, image.bottom() - pad),
            Align2::RIGHT_BOTTOM,
            &b.text,
            small.clone(),
        );
        right = r.left() - 3.0;
    }
}

/// The modifiers of this pass's last pointer button event (a click's
/// own, even when the keys were let go in the same pass), else those
/// held now.
fn click_modifiers(ui: &Ui) -> Modifiers {
    ui.input(|i| {
        i.events
            .iter()
            .rev()
            .find_map(|e| match e {
                Event::PointerButton { modifiers, .. } => Some(*modifiers),
                _ => None,
            })
            .unwrap_or(i.modifiers)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_band_touches_the_cells_it_crosses() {
        let cell = vec2(100.0, 100.0);
        // Cells at x 10..110, 120..220, 230..330; rows 5..105, 115..215.
        let r = Rect::from_min_max(pos2(105.0, 50.0), pos2(125.0, 120.0));
        assert_eq!(cells_in(r, true, 3, 9, cell, 10.0, 110.0), [0, 1, 3, 4]);
        // In a gap: nothing.
        let r = Rect::from_min_max(pos2(111.0, 10.0), pos2(119.0, 20.0));
        assert!(cells_in(r, true, 3, 9, cell, 10.0, 110.0).is_empty());
        // Past the last image.
        let r = Rect::from_min_max(pos2(0.0, 0.0), pos2(400.0, 400.0));
        assert_eq!(cells_in(r, true, 3, 5, cell, 10.0, 110.0), [0, 1, 2, 3, 4]);
    }
}
