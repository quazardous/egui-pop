//! The library grid as a file browser's view mode: a folder shown the
//! way the Library shows photos.
//!
//! [`LibraryGridMode`] lays the browser's entries out as its thumbnails
//! mode does (the zoom as a count per row, the rubber band, the arrows,
//! Page Up and Page Down, Home and End), and draws over each cell the
//! library's marks: the flag top left, the colour labels top right, the
//! rating's stars bottom left, a rejected image dimmed. The marks are the
//! application's ([`LibraryGridMode::new`]); a screen reader hears them as
//! the cell's description.
//!
//! The culling keys ([`CullKeymap`], Lightroom's by default) rate, flag
//! and label the selection, else the focused entry. They do not change
//! the marks: they give [`Signal`]s (`Rate`, `Flag`, `Label`, keyed by
//! path) into a [`LibrarySignals`] handle the application drains and
//! applies. With auto-advance, the focus then moves to the next entry.
//!
//! A browser's grouping ([`FileBrowser::with_grouping`]) stands for the
//! library's stacks: a group is one entry, its members said by its chip.
//!
//! ```
//! use egui_pop_api::files::browser::ViewModes;
//! use egui_pop_views::library::marks::Marks;
//! use egui_pop_views::library::mode::{LibraryGridMode, LibrarySignals};
//!
//! let signals = LibrarySignals::default();
//! let modes = ViewModes::default();
//! LibraryGridMode::new(|_entry| Marks::default())
//!     .signals(signals.clone())
//!     .register(&modes)
//!     .unwrap();
//! assert!(modes.ids().contains(&"library".to_string()));
//! // Each frame, after the browser is drawn:
//! for _signal in signals.drain() { /* apply it to the files' marks */ }
//! ```
//!
//! [`FileBrowser::with_grouping`]: egui_pop_api::files::browser::FileBrowser::with_grouping

use std::fmt;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use egui::{Modifiers, Rect, Ui, vec2};
use egui_pop_api::files::browser::{
    EntryDrawer, ModeContext, ModeInfo, ModeKey, ModeNeeds, ModeOutput, ModeSettings, PlainDrawer,
    SourceEntry, ThumbState, ThumbnailsMode, ViewMode, ViewModes,
};
use egui_pop_core::kit::{self, Kit, KitCx, Part, State};
use egui_pop_core::rating::RatingStars;
use egui_pop_core::strings::tr_with;
use egui_pop_core::theme::Tokens;

use super::grid::{OVERLAY_MIN, Signal, overlay};
use super::marks::{CullAction, CullKeymap, EmptyStars, Flag, Marks};
use super::paint::LibraryTokens;
use crate::strings as s;

/// The library grid's id among a browser's modes.
pub const MODE_LIBRARY: &str = "library";

/// What gives an entry's library marks.
pub type EntryLibraryMarks = dyn Fn(&SourceEntry) -> Marks + Send + Sync;

/// Where a library grid's culling keys say what the person asked for:
/// a shared queue the application drains, each frame, after drawing the
/// browser. Clones share the queue.
#[derive(Clone, Default)]
pub struct LibrarySignals(Arc<Mutex<Vec<Signal<PathBuf>>>>);

impl fmt::Debug for LibrarySignals {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("LibrarySignals").field(&self.len()).finish()
    }
}

impl LibrarySignals {
    fn lock(&self) -> std::sync::MutexGuard<'_, Vec<Signal<PathBuf>>> {
        self.0.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// The signals given since the last drain, oldest first.
    pub fn drain(&self) -> Vec<Signal<PathBuf>> {
        std::mem::take(&mut *self.lock())
    }

    /// How many wait.
    pub fn len(&self) -> usize {
        self.lock().len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    fn push(&self, signal: Signal<PathBuf>) {
        self.lock().push(signal);
    }
}

/// The library grid, a file browser's view mode
/// ([module docs](self)). Register it on a browser's [`ViewModes`] with
/// [`LibraryGridMode::register`].
#[derive(Clone)]
pub struct LibraryGridMode {
    marks: Arc<EntryLibraryMarks>,
    keymap: CullKeymap,
    signals: LibrarySignals,
    grid: ThumbnailsMode,
    /// A culling key taken, with the focus it was pressed on: turned into
    /// a signal when the entries are next at hand (the draw that follows,
    /// in the same frame).
    pending: Vec<(Option<usize>, CullAction)>,
}

impl fmt::Debug for LibraryGridMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("LibraryGridMode")
            .field("keymap", &self.keymap)
            .field("signals", &self.signals)
            .finish_non_exhaustive()
    }
}

impl LibraryGridMode {
    /// A library grid whose entries' marks `marks` gives, with
    /// Lightroom's culling keys.
    pub fn new(marks: impl Fn(&SourceEntry) -> Marks + Send + Sync + 'static) -> Self {
        LibraryGridMode {
            marks: Arc::new(marks),
            keymap: CullKeymap::lightroom(),
            signals: LibrarySignals::default(),
            grid: ThumbnailsMode::default(),
            pending: Vec::new(),
        }
    }

    /// The culling keys ([`CullKeymap::none`] for none). The stack key
    /// and the zoom keys are left to the browser.
    #[must_use]
    pub fn keymap(mut self, keymap: CullKeymap) -> Self {
        self.keymap = keymap;
        self
    }

    /// Where the culling keys' signals go (a fresh queue by default).
    #[must_use]
    pub fn signals(mut self, signals: LibrarySignals) -> Self {
        self.signals = signals;
        self
    }

    /// Adds the mode to `modes` under [`MODE_LIBRARY`]; each browser
    /// made with them gets its own copy, all sharing the marks and the
    /// signals' queue.
    pub fn register(self, modes: &ViewModes) -> Result<(), String> {
        modes.register(
            MODE_LIBRARY,
            Arc::new(move || Box::new(self.clone()) as Box<dyn ViewMode>),
        )
    }

    /// The culling key's action, if `key` is one this mode takes.
    fn takes(&self, key: egui::Key, modifiers: Modifiers) -> Option<CullAction> {
        if !(modifiers.is_none() || modifiers == Modifiers::SHIFT) {
            return None;
        }
        self.keymap.action(key).filter(|a| {
            matches!(
                a,
                CullAction::Rate(_) | CullAction::Flag(_) | CullAction::Label(_)
            )
        })
    }

    /// Turns the culling keys taken into signals; the entry auto-advance
    /// moves to, if any.
    fn cull(&mut self, cx: &ModeContext<'_>) -> Option<usize> {
        let mut advance = None;
        for (focus, action) in std::mem::take(&mut self.pending) {
            let focus = focus.filter(|&f| f < cx.entries.len());
            // The selection, else the focused entry.
            let keys: Vec<PathBuf> = if cx.selection.is_empty() {
                focus
                    .map(|f| cx.entries[f].path.clone())
                    .into_iter()
                    .collect()
            } else {
                cx.entries
                    .iter()
                    .filter(|e| cx.selection.contains(&e.path))
                    .map(|e| e.path.clone())
                    .collect()
            };
            if keys.is_empty() {
                continue;
            }
            let signal = match action {
                CullAction::Rate(r) => Signal::Rate {
                    keys,
                    rating: r.min(super::marks::MAX_RATING),
                },
                CullAction::Flag(flag) => Signal::Flag { keys, flag },
                CullAction::Label(label) => {
                    // Toggled from the focused entry's labels.
                    let on = focus.is_none_or(|f| !(self.marks)(&cx.entries[f]).labels.has(label));
                    Signal::Label { keys, label, on }
                }
                _ => continue,
            };
            self.signals.push(signal);
            if self.keymap.auto_advance
                && let Some(f) = focus
                && f + 1 < cx.entries.len()
            {
                advance = Some(f + 1);
            }
        }
        advance
    }
}

impl ViewMode for LibraryGridMode {
    fn describe(&self) -> ModeInfo {
        let mut info = self.grid.describe();
        info.id = MODE_LIBRARY.into();
        info.label = "Library grid".into();
        info.icon = "phosphor:images".into();
        info.persist = MODE_LIBRARY.into();
        info.keys.extend([
            ("0 to 5".into(), "Rate".into()),
            ("P, X, U".into(), "Pick, reject, unflag".into()),
            ("6 to 9".into(), "Toggle a colour label".into()),
        ]);
        info.gestures.push("Drag: select with a band".into());
        info.needs = ModeNeeds {
            thumbnails: true,
            ..ModeNeeds::default()
        };
        info
    }

    fn show(&mut self, ui: &mut Ui, cx: ModeContext<'_>) -> ModeOutput {
        let advance = self.cull(&cx);
        if advance.is_some() {
            ui.ctx().request_repaint();
        }
        let ModeContext {
            entries,
            selection,
            focus,
            thumbnails,
            settings,
            id,
            reveal_focus,
            max_height,
            drawer,
            name,
            many,
            place,
            ancestors,
            peek,
            loading,
            generation,
            marks,
        } = cx;
        let mut plain = PlainDrawer {
            entries,
            selection,
            focus,
            marks,
        };
        let inner: &mut dyn EntryDrawer = match drawer {
            Some(d) => d,
            None => &mut plain,
        };
        let mut drawer = MarksDrawer {
            inner,
            entries,
            marks: &*self.marks,
            id,
        };
        let mut out = self.grid.show(
            ui,
            ModeContext {
                entries,
                selection,
                focus,
                thumbnails,
                settings,
                id,
                reveal_focus,
                max_height,
                drawer: Some(&mut drawer),
                name,
                many,
                place,
                ancestors,
                peek,
                loading,
                generation,
                marks,
            },
        );
        // Auto-advance: as a click on the next entry, which the browser
        // selects and focuses.
        if let Some(next) = advance {
            out.clicked = Some((next, Modifiers::NONE));
        }
        out
    }

    fn step(&self, from: usize, count: usize, key: ModeKey) -> usize {
        self.grid.step(from, count, key)
    }

    fn own_key(
        &mut self,
        key: egui::Key,
        modifiers: Modifiers,
        focus: Option<usize>,
        _settings: &mut ModeSettings,
    ) -> bool {
        match self.takes(key, modifiers) {
            Some(action) => {
                self.pending.push((focus, action));
                true
            }
            None => false,
        }
    }
}

/// The cell drawer of the browser (or of its host), with the library's
/// marks over each cell.
struct MarksDrawer<'a> {
    inner: &'a mut dyn EntryDrawer,
    entries: &'a [SourceEntry],
    marks: &'a EntryLibraryMarks,
    id: egui::Id,
}

impl EntryDrawer for MarksDrawer<'_> {
    fn headers(&mut self, ui: &mut Ui, width: f32) {
        self.inner.headers(ui, width);
    }

    fn empty(&mut self, ui: &mut Ui) {
        self.inner.empty(ui);
    }

    fn row_size(&self, ui: &Ui, width: f32) -> egui::Vec2 {
        self.inner.row_size(ui, width)
    }

    fn row(&mut self, ui: &mut Ui, n: usize, size: egui::Vec2) -> egui::Response {
        self.inner.row(ui, n, size)
    }

    fn cell_size(&self, ui: &Ui, side: f32) -> egui::Vec2 {
        self.inner.cell_size(ui, side)
    }

    fn cell(
        &mut self,
        ui: &mut Ui,
        n: usize,
        size: egui::Vec2,
        thumbnail: ThumbState,
    ) -> egui::Response {
        let r = self.inner.cell(ui, n, size, thumbnail);
        let Some(e) = self.entries.get(n) else {
            return r;
        };
        if e.is_dir() {
            return r;
        }
        let marks = (self.marks)(e);
        if marks == Marks::default() {
            return r;
        }
        if let Some(said) = spoken(ui.ctx(), &marks) {
            ui.ctx()
                .accesskit_node_builder(r.id, |node| node.set_description(said));
        }
        let side = size.x.min(r.rect.width());
        if side >= OVERLAY_MIN {
            let t = Tokens::of(ui.ctx());
            let image = Rect::from_min_size(r.rect.min, vec2(side, side)).shrink(t.space.xs);
            paint_marks(ui, image, &marks, self.id.with(("library.stars", &e.path)));
        }
        r
    }
}

/// The library's marks over a cell's `image`: the grid's own overlays
/// (flag, labels, the stars' box), a rejected image dimmed, and the
/// stars, read only.
fn paint_marks(ui: &mut Ui, image: Rect, marks: &Marks, id: egui::Id) {
    let t = Tokens::of(ui.ctx());
    let lt = LibraryTokens::of(ui.ctx());
    let kit = Kit::of(ui.ctx());
    let painter = ui.painter().clone();
    let cx = KitCx {
        painter: &painter,
        ppp: ui.ctx().pixels_per_point(),
    };
    if marks.flag == Flag::Reject {
        kit.thumb_layer.paint(
            &cx,
            &Part {
                rect: image,
                room: image,
                state: State::REST.toned(kit::Tone::Muted),
                label: None,
            },
        );
    }
    let star = (image.width() / 10.0).clamp(10.0, 16.0);
    overlay(&cx, &kit, &lt, &t, image, marks, false, star);
    if marks.rating > 0 {
        let stars = Rect::from_min_size(
            egui::pos2(image.left() + star * 0.25, image.bottom() - star * 1.25),
            RatingStars::size_for(star),
        );
        let _ = RatingStars::new(marks.rating)
            .id(id)
            .focusable(false)
            .read_only(true)
            .empty(EmptyStars::OnHover)
            .item_hovered(false)
            .show_at(ui, stars);
    }
}

/// The marks in words, as a screen reader says them after the entry's
/// name: "Rated 3 of 5, Pick, Red"; `None` without any.
pub fn spoken(ctx: &egui::Context, marks: &Marks) -> Option<String> {
    let mut parts = Vec::new();
    if marks.rating > 0 {
        parts.push(tr_with(
            ctx,
            &s::LIBRARY_RATED,
            &[("stars", i64::from(marks.rating).into())],
        ));
    }
    if marks.flag != Flag::None {
        parts.push(marks.flag.title(ctx));
    }
    parts.extend(marks.labels.iter().map(|l| l.title(ctx)));
    (!parts.is_empty()).then(|| parts.join(", "))
}
