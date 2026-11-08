//! A file browser with pluggable view modes
//! ([design](https://github.com/quazardous/egui-pop/blob/next/docs/design/file-browser.md)).
//!
//! [`FileBrowser`] owns what every view shares: the entries of a place, read
//! from a [`FileSource`]; the selection and the focus; type-ahead; the mode
//! switcher. A [`ViewMode`] draws the entries and answers what the person
//! did; the browser decides what it means. Thumbnails come from a
//! [`ThumbnailProvider`], egui-pop's default being [`PreviewThumbnails`] over
//! the files service's preview providers.
//!
//! Built-in modes: [`DetailsMode`] (rows with columns) and [`ThumbnailsMode`]
//! (a virtualised grid). Others register on a [`ViewModes`] registry.

use std::collections::{BTreeSet, HashMap, HashSet};
use std::fmt;
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::SystemTime;

use egui::{Modifiers, Ui};

use egui_pop_core::icon::Icon;
use egui_pop_core::strings;
use egui_pop_core::thumbnail::ThumbnailGrid;
use egui_pop_core::widgets;

use super::fs::{Meta, SharedFs};
use super::preview::{Previews, Shown};
use super::sort::natural_cmp;

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

// ---------------------------------------------------------------------------
// FileSource
// ---------------------------------------------------------------------------

/// What an entry is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum EntryKind {
    File,
    Folder,
}

/// One entry as a source gives it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceEntry {
    pub path: PathBuf,
    pub name: String,
    pub kind: EntryKind,
    /// Its size in bytes, when known (a folder has none).
    pub size: Option<u64>,
    pub modified: Option<SystemTime>,
    /// The time a view sorts or groups by when it is not the file's own: a
    /// photo's capture time, given by an application's source.
    pub taken: Option<SystemTime>,
    /// The application's own id for the file (a catalogue's image id).
    pub app_id: Option<u64>,
}

impl SourceEntry {
    pub fn is_dir(&self) -> bool {
        self.kind == EntryKind::Folder
    }

    /// The facts the preview providers key their cache by.
    pub fn meta(&self) -> Meta {
        Meta {
            is_dir: self.is_dir(),
            len: self.size.unwrap_or(0),
            modified: self.modified,
        }
    }
}

/// Why a source could not list.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SourceError {
    NotFound(PathBuf),
    Denied(PathBuf),
    Io(String),
    Unsupported,
}

impl fmt::Display for SourceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SourceError::NotFound(p) => write!(f, "{} does not exist", p.display()),
            SourceError::Denied(p) => write!(f, "{} cannot be read", p.display()),
            SourceError::Io(e) => f.write_str(e),
            SourceError::Unsupported => f.write_str("this source cannot do that"),
        }
    }
}

/// What a listing delivers, in order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Page {
    Entries(Vec<SourceEntry>),
    /// Facts learnt later about entries already given (sizes, times).
    Updated(Vec<SourceEntry>),
    Removed(Vec<PathBuf>),
    /// Start again: the place changed in a way the source cannot describe.
    Changed,
    Done,
    Failed(SourceError),
}

/// How to list.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ListOptions {
    /// Hidden entries (a name starting with `.`) too.
    pub hidden: bool,
}

/// A listing under way: its pages arrive through [`Listing::poll`], and the
/// frame never waits. Dropping it cancels it.
#[derive(Debug)]
pub struct Listing {
    rx: mpsc::Receiver<Page>,
    cancel: Arc<AtomicBool>,
}

/// The source's end of a [`Listing`].
#[derive(Clone, Debug)]
pub struct ListingSender {
    tx: mpsc::Sender<Page>,
    cancel: Arc<AtomicBool>,
}

impl ListingSender {
    /// Sends a page; `false` once the listing is cancelled or dropped.
    pub fn send(&self, page: Page) -> bool {
        !self.cancelled() && self.tx.send(page).is_ok()
    }

    pub fn cancelled(&self) -> bool {
        self.cancel.load(Ordering::Relaxed)
    }
}

impl Listing {
    /// A listing and the sender a source fills it through.
    pub fn channel() -> (ListingSender, Listing) {
        let (tx, rx) = mpsc::channel();
        let cancel = Arc::new(AtomicBool::new(false));
        (
            ListingSender {
                tx,
                cancel: cancel.clone(),
            },
            Listing { rx, cancel },
        )
    }

    /// Every page that arrived since the last poll.
    pub fn poll(&self) -> Vec<Page> {
        self.rx.try_iter().collect()
    }

    pub fn cancel(&self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
}

impl Drop for Listing {
    fn drop(&mut self) {
        self.cancel();
    }
}

/// Where a browser's entries come from: a file system, a camera card, a
/// catalogue. Entries are paged and streamed.
pub trait FileSource: Send + Sync + fmt::Debug {
    /// A stable id, for persistence and the collector.
    fn id(&self) -> &str;

    /// Starts a listing of `place`; a refresh is a new listing.
    fn list(&self, place: &Path, options: &ListOptions) -> Listing;

    /// Whether [`FileSource::watch`] means something here.
    fn can_watch(&self) -> bool {
        false
    }

    /// Asks to be told when `place` changes, through the listing's pages.
    fn watch(&self, _place: &Path) -> Result<(), SourceError> {
        Err(SourceError::Unsupported)
    }
}

/// A [`FileSource`] over a [`super::fs::FileSystem`]: in pages of
/// `page_size`, on a thread of its own for the system's file system (off
/// the web), at once for one held in memory.
#[derive(Clone, Debug)]
pub struct FsSource {
    fs: SharedFs,
    page_size: usize,
}

impl FsSource {
    pub const PAGE_SIZE: usize = 512;

    pub fn new(fs: SharedFs) -> Self {
        Self {
            fs,
            page_size: Self::PAGE_SIZE,
        }
    }

    pub fn page_size(mut self, n: usize) -> Self {
        self.page_size = n.max(1);
        self
    }

    fn run(fs: &SharedFs, place: &Path, options: ListOptions, page: usize, out: &ListingSender) {
        let items = match fs.read_dir(place) {
            Ok(items) => items,
            Err(e) => {
                let err = match e.kind() {
                    std::io::ErrorKind::NotFound => SourceError::NotFound(place.to_path_buf()),
                    std::io::ErrorKind::PermissionDenied => {
                        SourceError::Denied(place.to_path_buf())
                    }
                    _ => SourceError::Io(e.to_string()),
                };
                out.send(Page::Failed(err));
                return;
            }
        };
        let mut batch = Vec::with_capacity(page.min(items.len()));
        for item in items {
            if !options.hidden && item.name.starts_with('.') {
                continue;
            }
            let path = place.join(&item.name);
            let meta = fs.metadata(&path).ok();
            batch.push(SourceEntry {
                kind: if item.is_dir {
                    EntryKind::Folder
                } else {
                    EntryKind::File
                },
                size: meta.filter(|m| !m.is_dir).map(|m| m.len),
                modified: meta.and_then(|m| m.modified),
                taken: None,
                app_id: None,
                name: item.name,
                path,
            });
            if batch.len() == page && !out.send(Page::Entries(std::mem::take(&mut batch))) {
                return;
            }
        }
        if !batch.is_empty() && !out.send(Page::Entries(batch)) {
            return;
        }
        out.send(Page::Done);
    }
}

impl FileSource for FsSource {
    fn id(&self) -> &str {
        "egui_pop.fs"
    }

    fn list(&self, place: &Path, options: &ListOptions) -> Listing {
        let (tx, listing) = Listing::channel();
        let (fs, place, options, page) = (
            self.fs.clone(),
            place.to_path_buf(),
            *options,
            self.page_size,
        );
        if self.fs.is_native() && !cfg!(target_arch = "wasm32") {
            std::thread::spawn(move || Self::run(&fs, &place, options, page, &tx));
        } else {
            Self::run(&fs, &place, options, page, &tx);
        }
        listing
    }
}

// ---------------------------------------------------------------------------
// ThumbnailProvider
// ---------------------------------------------------------------------------

/// How soon a thumbnail is wanted.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Priority {
    /// On screen now.
    Visible,
    /// About to be: the next screen.
    Near,
    Background,
}

/// One thumbnail asked for.
#[derive(Clone, Copy, Debug)]
pub struct ThumbRequest<'a> {
    pub entry: &'a SourceEntry,
    /// Pixels on its long side, at least.
    pub size: u32,
    pub priority: Priority,
}

/// A thumbnail's state.
#[derive(Clone)]
pub enum ThumbState {
    Ready(egui::TextureHandle),
    Loading,
    /// No image: an icon stands in (a folder, a file without one).
    None,
    Failed(String),
}

impl fmt::Debug for ThumbState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ThumbState::Ready(t) => write!(f, "Ready({:?})", t.size()),
            ThumbState::Loading => f.write_str("Loading"),
            ThumbState::None => f.write_str("None"),
            ThumbState::Failed(why) => write!(f, "Failed({why:?})"),
        }
    }
}

/// Makes thumbnails off the interface thread. A frame starts with
/// [`ThumbnailProvider::begin_frame`], asks with
/// [`ThumbnailProvider::request`], and ends with
/// [`ThumbnailProvider::end_frame`]: a request not renewed in a frame is
/// cancelled, so what scrolled away leaves the queue without the mode's help.
pub trait ThumbnailProvider: Send + Sync {
    fn begin_frame(&self, _ctx: &egui::Context) {}
    /// Answers at once from what is made; never blocks the frame.
    fn request(&self, ctx: &egui::Context, request: &ThumbRequest<'_>) -> ThumbState;
    fn end_frame(&self, _ctx: &egui::Context) {}
    /// Cancels one request now.
    fn cancel(&self, _path: &Path) {}
}

/// The bookkeeping of renewed requests: what a frame asked, in priority
/// order, and what the frame before asked that this one did not.
#[derive(Debug, Default)]
pub struct Renewals {
    asked: Vec<(Priority, usize, PathBuf, Meta)>,
    before: HashSet<PathBuf>,
}

impl Renewals {
    pub fn begin(&mut self) {
        self.before = self.asked.drain(..).map(|(_, _, p, _)| p).collect();
    }

    pub fn ask(&mut self, entry: &SourceEntry, priority: Priority) {
        if self.asked.iter().any(|(_, _, p, _)| *p == entry.path) {
            return;
        }
        let n = self.asked.len();
        self.asked
            .push((priority, n, entry.path.clone(), entry.meta()));
    }

    /// What was asked this frame, by priority then order of asking.
    pub fn wanted(&mut self) -> Vec<(PathBuf, Meta)> {
        self.asked.sort_by_key(|(p, n, _, _)| (*p, *n));
        self.asked
            .iter()
            .map(|(_, _, p, m)| (p.clone(), *m))
            .collect()
    }

    /// What the frame before asked and this one did not.
    pub fn dropped(&self) -> Vec<PathBuf> {
        let now: HashSet<&PathBuf> = self.asked.iter().map(|(_, _, p, _)| p).collect();
        let mut gone: Vec<PathBuf> = self
            .before
            .iter()
            .filter(|p| !now.contains(p))
            .cloned()
            .collect();
        gone.sort();
        gone
    }
}

/// egui-pop's default thumbnails: the files service's preview providers
/// (PNG and JPEG with `image`, folders, the system's thumbnails when
/// installed), made by their worker, visible first.
#[allow(missing_debug_implementations)]
pub struct PreviewThumbnails {
    previews: Previews,
    fs: SharedFs,
    renewals: Mutex<Renewals>,
}

impl PreviewThumbnails {
    pub fn new(previews: Previews, fs: SharedFs) -> Self {
        Self {
            previews,
            fs,
            renewals: Mutex::default(),
        }
    }
}

impl ThumbnailProvider for PreviewThumbnails {
    fn begin_frame(&self, _ctx: &egui::Context) {
        lock(&self.renewals).begin();
    }

    fn request(&self, ctx: &egui::Context, request: &ThumbRequest<'_>) -> ThumbState {
        if request.entry.is_dir() {
            return ThumbState::None;
        }
        lock(&self.renewals).ask(request.entry, request.priority);
        match self
            .previews
            .thumbnail(&request.entry.path, &request.entry.meta())
        {
            Shown::Ready(ready) => match ready.texture(ctx, 0) {
                Some(t) => ThumbState::Ready(t),
                None => ThumbState::None,
            },
            Shown::Loading => ThumbState::Loading,
            Shown::Nothing => ThumbState::None,
            Shown::Failed(why) => ThumbState::Failed(why),
        }
    }

    fn end_frame(&self, ctx: &egui::Context) {
        let wanted = lock(&self.renewals).wanted();
        // `want` replaces the queue: what is not in it any more is cancelled.
        self.previews.want(ctx, &self.fs, &wanted);
    }
}

// ---------------------------------------------------------------------------
// Selection
// ---------------------------------------------------------------------------

/// The selected entries, by path, and the anchor a range starts from.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Selection {
    pub paths: BTreeSet<PathBuf>,
    pub anchor: Option<PathBuf>,
}

impl Selection {
    pub fn contains(&self, path: &Path) -> bool {
        self.paths.contains(path)
    }

    pub fn len(&self) -> usize {
        self.paths.len()
    }

    pub fn is_empty(&self) -> bool {
        self.paths.is_empty()
    }

    fn only(&mut self, path: &Path) {
        self.paths.clear();
        self.paths.insert(path.to_path_buf());
        self.anchor = Some(path.to_path_buf());
    }
}

// ---------------------------------------------------------------------------
// ViewMode and the registry
// ---------------------------------------------------------------------------

/// A key a mode moves the focus with: its layout decides where it goes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ModeKey {
    Up,
    Down,
    Left,
    Right,
    PageUp,
    PageDown,
    Home,
    End,
}

/// What a mode needs from the browser.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ModeNeeds {
    pub thumbnails: bool,
    /// A time per entry (an agenda).
    pub time: bool,
    /// The listings of the place's ancestors and of the focused folder
    /// ([`ModeContext::ancestors`], [`ModeContext::peek`]): Finder's columns.
    pub hierarchy: bool,
}

/// One folder's listing beside the place's: an ancestor, or the focused
/// folder looked into, for a mode that shows the path as columns.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Column {
    /// The folder listed.
    pub place: PathBuf,
    /// Its entries, sorted as the browser sorts (folders first, then names
    /// as people read them).
    pub entries: Vec<SourceEntry>,
    /// The entry that leads to the next column: the path's next folder.
    pub open: Option<usize>,
    /// Its listing is still under way.
    pub loading: bool,
}

/// Somewhere a mode asks the browser to go: a place, and the entry to focus
/// there (a click in another column than the place's).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Reach {
    pub place: PathBuf,
    pub focus: Option<PathBuf>,
}

/// A key that leaves the place, in a mode that shows the path.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Travel {
    /// Into the focused folder, its first entry focused.
    Into,
    /// Back to the parent, the folder left focused.
    Out,
}

/// What a mode is, for the switcher, persistence, agents and screen readers.
#[derive(Clone, Debug, PartialEq)]
pub struct ModeInfo {
    /// `details`, `thumbnails`, `filmstrip`…
    pub id: String,
    /// Its English name: translated under `egui-pop-files-mode-<id>`
    /// ([`mode_label`]).
    pub label: String,
    /// The name of its icon in the icon registry (`phosphor:list`); one
    /// that does not resolve leaves the label alone.
    pub icon: String,
    /// The keys it moves with, as its tip says them.
    pub keys: Vec<(String, String)>,
    /// The pointer gestures it reads, as its tip says them.
    pub gestures: Vec<String>,
    /// The key its own settings are kept under.
    pub persist: String,
    /// What a screen reader is told the container is.
    pub role: egui::Role,
    pub needs: ModeNeeds,
}

/// A mode's own settings, kept per browser.
#[derive(Clone, Debug, PartialEq)]
pub struct ModeSettings {
    /// The thumbnails' side, in points.
    pub thumbnail_size: f32,
}

impl Default for ModeSettings {
    fn default() -> Self {
        Self {
            thumbnail_size: 128.0,
        }
    }
}

/// What a mode is given to draw.
#[allow(missing_debug_implementations)]
pub struct ModeContext<'a> {
    /// In display order: sorted, filtered.
    pub entries: &'a [SourceEntry],
    pub selection: &'a Selection,
    /// The focused entry's index.
    pub focus: Option<usize>,
    pub thumbnails: &'a dyn ThumbnailProvider,
    pub settings: &'a mut ModeSettings,
    /// The id under which it keeps its scroll and its widgets.
    pub id: egui::Id,
    /// Scroll the focused entry into view this frame.
    pub reveal_focus: bool,
    /// The most the entries take in height; all that is left when `None`.
    pub max_height: Option<f32>,
    /// What draws each entry; the browser's [`PlainDrawer`] when `None`.
    pub drawer: Option<&'a mut dyn EntryDrawer>,
    /// The entries' container for assistive technology, when the mode
    /// makes it: its name (a list box whose current entry is the focus).
    /// The browser makes its own (`None`).
    pub name: Option<String>,
    /// Several entries can be selected: said by the container.
    pub many: bool,
    /// The folder the entries are in.
    pub place: &'a Path,
    /// The place's ancestors' listings, the topmost first, when the mode
    /// needs a hierarchy ([`ModeNeeds::hierarchy`]); empty otherwise.
    pub ancestors: &'a [Column],
    /// The focused folder's listing, when the mode needs a hierarchy and a
    /// folder has the focus.
    pub peek: Option<&'a Column>,
}

/// What the person did in a mode; the browser decides what it means.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ModeOutput {
    pub clicked: Option<(usize, Modifiers)>,
    /// A double click.
    pub activated: Option<usize>,
    /// The entries it drew.
    pub visible: Range<usize>,
    /// The focused entry's widget, when drawn: the container's active
    /// descendant.
    pub focused: Option<egui::Id>,
    /// The entries in one of its rows, as last laid out.
    pub columns: usize,
    /// Another place to show: a click in an ancestor's column or in the
    /// focused folder's.
    pub reach: Option<Reach>,
}

/// A way to show a browser's entries. It draws and reports; selection rules,
/// what Enter does and what a click on a folder does are the browser's.
pub trait ViewMode: Send {
    fn describe(&self) -> ModeInfo;

    fn show(&mut self, ui: &mut Ui, cx: ModeContext<'_>) -> ModeOutput;

    /// Where a key moves the focus from `from`, among `count` entries, in
    /// the layout last drawn.
    fn step(&self, from: usize, count: usize, key: ModeKey) -> usize;

    /// Whether a key leaves the place rather than moving within it (Left and
    /// Right in Finder's columns). `None`, the default, leaves the key to
    /// [`ViewMode::step`].
    fn travel(&self, _key: ModeKey) -> Option<Travel> {
        None
    }
}

type ModeFactory = Arc<dyn Fn() -> Box<dyn ViewMode> + Send + Sync>;

/// The view modes on offer, in the switcher's order: the built-ins first,
/// then those plugins register (and take back when they disconnect).
#[derive(Clone)]
pub struct ViewModes(Arc<Mutex<Vec<(String, ModeFactory)>>>);

impl fmt::Debug for ViewModes {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_list().entries(self.ids()).finish()
    }
}

impl Default for ViewModes {
    fn default() -> Self {
        let modes = ViewModes(Arc::default());
        modes
            .register("details", Arc::new(|| Box::new(DetailsMode::default())))
            .expect("first");
        modes
            .register(
                "thumbnails",
                Arc::new(|| Box::new(ThumbnailsMode::default())),
            )
            .expect("second");
        modes
    }
}

impl ViewModes {
    /// The folder chooser's modes: details and columns.
    pub fn folders() -> Self {
        let modes = ViewModes(Arc::default());
        modes
            .register(MODE_DETAILS, Arc::new(|| Box::new(DetailsMode::default())))
            .expect("first");
        modes
            .register(MODE_COLUMNS, Arc::new(|| Box::new(ColumnsMode::default())))
            .expect("second");
        modes
    }

    /// Every built-in mode: details, thumbnails, filmstrip, columns.
    pub fn built_in() -> Self {
        let modes = ViewModes::default();
        modes
            .register(
                MODE_FILMSTRIP,
                Arc::new(|| Box::new(FilmstripMode::default())),
            )
            .expect("built-in");
        modes
            .register(MODE_COLUMNS, Arc::new(|| Box::new(ColumnsMode::default())))
            .expect("built-in");
        modes
    }

    /// No mode at all.
    pub fn empty() -> Self {
        ViewModes(Arc::default())
    }

    /// Adds a mode; an id taken already is refused.
    pub fn register(&self, id: &str, factory: ModeFactory) -> Result<(), String> {
        let mut modes = lock(&self.0);
        if modes.iter().any(|(m, _)| m == id) {
            return Err(format!("view mode {id} is registered already"));
        }
        modes.push((id.to_owned(), factory));
        Ok(())
    }

    /// Takes a mode back; `false` when it was not there.
    pub fn unregister(&self, id: &str) -> bool {
        let mut modes = lock(&self.0);
        let before = modes.len();
        modes.retain(|(m, _)| m != id);
        modes.len() != before
    }

    pub fn ids(&self) -> Vec<String> {
        lock(&self.0).iter().map(|(m, _)| m.clone()).collect()
    }

    /// A fresh instance of a mode.
    pub fn make(&self, id: &str) -> Option<Box<dyn ViewMode>> {
        let factory = lock(&self.0)
            .iter()
            .find(|(m, _)| m == id)
            .map(|(_, f)| f.clone())?;
        Some(factory())
    }
}

// ---------------------------------------------------------------------------
// The built-in modes
// ---------------------------------------------------------------------------

fn human_size(n: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut v = n as f64;
    let mut u = 0;
    while v >= 1000.0 && u < UNITS.len() - 1 {
        v /= 1000.0;
        u += 1;
    }
    if u == 0 {
        format!("{n} B")
    } else {
        format!("{v:.1} {}", UNITS[u])
    }
}

fn entry_response(resp: &egui::Response, entry: &SourceEntry, selected: bool) {
    let name = entry.name.clone();
    resp.widget_info(|| {
        egui::WidgetInfo::selected(egui::Role::ListBoxOption, true, selected, &name)
    });
}

fn report(resp: &egui::Response, i: usize, out: &mut ModeOutput) {
    if resp.double_clicked() {
        out.activated = Some(i);
    } else if resp.clicked() {
        out.clicked = Some((i, resp.ctx.input(|inp| inp.modifiers)));
    }
}

/// Draws the entries a mode lays out, one at a time: the layer that knows
/// its host. A mode owns the layout (the virtualised rows or cells, the
/// scrolling, the keys' moves, the thumbnails' order); the drawer owns what
/// an entry looks like and does. The browser's own is [`PlainDrawer`]; the
/// in-app picker gives its rows with their columns, its groups' badges, the
/// highlight its field drives and its own clicks.
pub trait EntryDrawer {
    /// Above the entries, `width` wide: column headers that sort. Nothing
    /// by default.
    fn headers(&mut self, _ui: &mut Ui, _width: f32) {}
    /// Said instead of the entries when there are none. Nothing by default.
    fn empty(&mut self, _ui: &mut Ui) {}
    /// A row's size, in `width` of room.
    fn row_size(&self, ui: &Ui, width: f32) -> egui::Vec2;
    /// Draws row `n`.
    fn row(&mut self, ui: &mut Ui, n: usize, size: egui::Vec2) -> egui::Response;
    /// A cell's size for a thumbnail `side` points wide.
    fn cell_size(&self, ui: &Ui, side: f32) -> egui::Vec2;
    /// Draws cell `n`; `thumbnail` is what the provider answered for it.
    fn cell(
        &mut self,
        ui: &mut Ui,
        n: usize,
        size: egui::Vec2,
        thumbnail: ThumbState,
    ) -> egui::Response;
}

/// The browser's own drawing: a plain row (name, size, date) or a cell (the
/// thumbnail and the name), selected and focused as the context says.
#[allow(missing_debug_implementations)]
pub struct PlainDrawer<'a> {
    pub entries: &'a [SourceEntry],
    pub selection: &'a Selection,
    pub focus: Option<usize>,
}

impl PlainDrawer<'_> {
    /// Below this width, a row shows its name alone, without the size and
    /// the date.
    pub const FACTS_WIDTH: f32 = 320.0;

    fn back(&self, ui: &Ui, resp: &egui::Response, n: usize, radius: f32) {
        let e = &self.entries[n];
        let selected = self.selection.contains(&e.path);
        entry_response(resp, e, selected);
        let visuals = ui.visuals();
        let rect = resp.rect;
        if selected {
            ui.painter()
                .rect_filled(rect, radius, visuals.selection.bg_fill);
        } else if resp.hovered() {
            ui.painter()
                .rect_filled(rect, radius, visuals.widgets.hovered.weak_bg_fill);
        }
        if self.focus == Some(n) {
            ui.painter().rect_stroke(
                rect.shrink(0.5),
                radius,
                visuals.selection.stroke,
                egui::StrokeKind::Inside,
            );
        }
    }
}

impl EntryDrawer for PlainDrawer<'_> {
    fn row_size(&self, _ui: &Ui, width: f32) -> egui::Vec2 {
        egui::vec2(width, DetailsMode::ROW_HEIGHT)
    }

    fn row(&mut self, ui: &mut Ui, n: usize, size: egui::Vec2) -> egui::Response {
        let (rect, resp) = ui.allocate_exact_size(size, egui::Sense::click());
        self.back(ui, &resp, n, 2.0);
        let e = &self.entries[n];
        let visuals = ui.visuals();
        let text = if self.selection.contains(&e.path) {
            visuals.selection.stroke.color
        } else {
            visuals.text_color()
        };
        let font = egui::TextStyle::Body.resolve(ui.style());
        let w = rect.width();
        let name = if e.is_dir() {
            format!("{}/", e.name)
        } else {
            e.name.clone()
        };
        let y = rect.center().y;
        // A narrow row (a column of Finder's view) shows the name alone.
        if w < Self::FACTS_WIDTH {
            paint_row_name(ui, rect, e, text);
            return resp;
        }
        ui.painter()
            .with_clip_rect(rect.with_max_x(rect.left() + w * 0.6))
            .text(
                egui::pos2(rect.left() + 4.0, y),
                egui::Align2::LEFT_CENTER,
                name,
                font.clone(),
                text,
            );
        if let Some(s) = e.size {
            ui.painter().text(
                egui::pos2(rect.left() + w * 0.6 + 4.0, y),
                egui::Align2::LEFT_CENTER,
                human_size(s),
                font.clone(),
                text,
            );
        }
        if let Some(t) = e
            .modified
            .and_then(|t| t.duration_since(SystemTime::UNIX_EPOCH).ok())
        {
            let days = t.as_secs() / 86_400;
            let (y_, m, d) = civil(days as i64);
            ui.painter().text(
                egui::pos2(rect.left() + w * 0.78 + 4.0, y),
                egui::Align2::LEFT_CENTER,
                format!("{y_:04}-{m:02}-{d:02}"),
                font,
                text,
            );
        }
        resp
    }

    fn cell_size(&self, ui: &Ui, side: f32) -> egui::Vec2 {
        let caption = ui.text_style_height(&egui::TextStyle::Small) + 4.0;
        egui::vec2(side + 8.0, side + caption + 8.0)
    }

    fn cell(
        &mut self,
        ui: &mut Ui,
        n: usize,
        size: egui::Vec2,
        thumbnail: ThumbState,
    ) -> egui::Response {
        let (rect, resp) = ui.allocate_exact_size(size, egui::Sense::click());
        self.back(ui, &resp, n, 4.0);
        paint_cell(ui, rect, &self.entries[n], thumbnail);
        resp
    }
}

/// A cell's thumbnail (or what stands in for it) and its name, in `rect`.
pub fn paint_cell(ui: &Ui, rect: egui::Rect, e: &SourceEntry, thumbnail: ThumbState) {
    let visuals = ui.visuals();
    let caption = ui.text_style_height(&egui::TextStyle::Small) + 4.0;
    let side = (rect.width() - 8.0)
        .min(rect.height() - caption - 8.0)
        .max(1.0);
    let image = egui::Rect::from_min_size(rect.min + egui::vec2(4.0, 4.0), egui::vec2(side, side));
    match thumbnail {
        ThumbState::Ready(t) => {
            let s = t.size_vec2();
            let scale = (side / s.x.max(s.y)).min(1.0);
            let r = egui::Rect::from_center_size(image.center(), s * scale);
            ui.painter().image(
                t.id(),
                r,
                egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                egui::Color32::WHITE,
            );
        }
        other => {
            let glyph = match other {
                _ if e.is_dir() => "Folder",
                ThumbState::Failed(_) => "!",
                ThumbState::Loading => "...",
                _ => "File",
            };
            ui.painter()
                .rect_filled(image, 4.0, visuals.extreme_bg_color);
            ui.painter().text(
                image.center(),
                egui::Align2::CENTER_CENTER,
                glyph,
                egui::FontId::proportional(side * 0.15),
                visuals.weak_text_color(),
            );
        }
    }
    ui.painter().with_clip_rect(rect).text(
        egui::pos2(rect.center().x, image.bottom() + 2.0 + caption / 2.0),
        egui::Align2::CENTER_CENTER,
        &e.name,
        egui::TextStyle::Small.resolve(ui.style()),
        visuals.text_color(),
    );
}

/// Lays `count` entries out in a virtualised grid of `cell`s through
/// `draw`, reporting what was drawn: the one way every built-in mode lays
/// its entries out. Gives the entries in a row.
#[allow(clippy::too_many_arguments)]
fn lay_out(
    ui: &mut Ui,
    container: Option<(&str, bool)>,
    drawer: &mut dyn EntryDrawer,
    salt: egui::Id,
    count: usize,
    cell: egui::Vec2,
    max_height: Option<f32>,
    reveal: Option<usize>,
    focus: Option<usize>,
    out: &mut ModeOutput,
    mut draw: impl FnMut(&mut dyn EntryDrawer, &mut Ui, usize) -> egui::Response,
) -> usize {
    let mut grid = ThumbnailGrid::new(count, cell).id_salt(salt).reveal(reveal);
    if let Some(h) = max_height {
        grid = grid.max_height(h);
    }
    let mut first = usize::MAX;
    let mut end = 0;
    let mut columns = 1;
    let mut add = |ui: &mut Ui, out: &mut ModeOutput| {
        if count == 0 {
            drawer.empty(ui);
        }
        columns = grid.clone().show(ui, |ui, n| {
            let r = draw(&mut *drawer, ui, n);
            if focus == Some(n) {
                out.focused = Some(r.id);
            }
            report(&r, n, out);
            first = first.min(n);
            end = end.max(n + 1);
        });
    };
    in_container(ui, container, out, &mut add);
    out.visible = if first == usize::MAX {
        0..0
    } else {
        first..end
    };
    columns.max(1)
}

/// Draws the entries through `add`, inside a list box named `container`'s
/// name when the mode makes it, its active descendant the focused entry.
fn in_container(
    ui: &mut Ui,
    container: Option<(&str, bool)>,
    out: &mut ModeOutput,
    add: &mut dyn FnMut(&mut Ui, &mut ModeOutput),
) {
    match container {
        Some((name, many)) => {
            ui.scope_builder(
                egui::UiBuilder::new()
                    .accessibility_role(egui::accesskit::Role::ListBox)
                    .accessibility_label(name),
                |ui| {
                    add(ui, out);
                    let current = out.focused;
                    ui.ctx().accesskit_node_builder(ui.unique_id(), |node| {
                        if many {
                            node.set_multiselectable();
                        }
                        if let Some(id) = current {
                            node.set_active_descendant(id.accesskit_id());
                        }
                    });
                },
            );
        }
        None => add(ui, out),
    }
}

/// Rows with columns: the name, the size and when it last changed.
#[derive(Clone, Debug, Default)]
pub struct DetailsMode {
    rows_in_view: usize,
}

impl DetailsMode {
    pub const ROW_HEIGHT: f32 = 20.0;
}

impl ViewMode for DetailsMode {
    fn describe(&self) -> ModeInfo {
        ModeInfo {
            id: "details".into(),
            label: "Details".into(),
            icon: "phosphor:list".into(),
            keys: vec![
                ("Up, Down".into(), "Previous, next entry".into()),
                ("Page Up, Page Down".into(), "A page up or down".into()),
                ("Home, End".into(), "First, last entry".into()),
            ],
            gestures: vec!["Wheel: scroll".into()],
            persist: "details".into(),
            role: egui::Role::ListBox,
            needs: ModeNeeds::default(),
        }
    }

    fn show(&mut self, ui: &mut Ui, cx: ModeContext<'_>) -> ModeOutput {
        let mut out = ModeOutput::default();
        let mut plain = PlainDrawer {
            entries: cx.entries,
            selection: cx.selection,
            focus: cx.focus,
        };
        let drawer: &mut dyn EntryDrawer = match cx.drawer {
            Some(d) => d,
            None => &mut plain,
        };
        let width = ui.available_width();
        drawer.headers(ui, width);
        let n = cx.entries.len();
        let size = drawer.row_size(ui, width - ui.spacing().scroll.allocated_width());
        let reveal = cx.focus.filter(|_| cx.reveal_focus && n > 0);
        lay_out(
            ui,
            cx.name.as_deref().map(|n| (n, cx.many)),
            drawer,
            cx.id.with("details"),
            n,
            size,
            cx.max_height,
            reveal,
            cx.focus,
            &mut out,
            |d, ui, i| d.row(ui, i, size),
        );
        self.rows_in_view = out.visible.len().max(1);
        out.columns = 1;
        out
    }

    fn step(&self, from: usize, count: usize, key: ModeKey) -> usize {
        let last = count.saturating_sub(1);
        let page = self.rows_in_view.max(1);
        match key {
            ModeKey::Up | ModeKey::Left => from.saturating_sub(1),
            ModeKey::Down | ModeKey::Right => (from + 1).min(last),
            ModeKey::PageUp => from.saturating_sub(page),
            ModeKey::PageDown => (from + page).min(last),
            ModeKey::Home => 0,
            ModeKey::End => last,
        }
    }
}

/// Days since 1970-01-01 to a civil date (UTC).
fn civil(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (yoe + era * 400 + i64::from(m <= 2), m, d)
}

/// A virtualised grid of thumbnails with their names. Only the cells in view
/// ask for a thumbnail, in reading order, then the next row as `Near`, so
/// what scrolls away is cancelled by the provider.
#[derive(Clone, Debug, Default)]
pub struct ThumbnailsMode {
    columns: usize,
    rows_in_view: usize,
}

impl ThumbnailsMode {
    pub const MIN_SIZE: f32 = 64.0;
    pub const MAX_SIZE: f32 = 256.0;
}

impl ViewMode for ThumbnailsMode {
    fn describe(&self) -> ModeInfo {
        ModeInfo {
            id: "thumbnails".into(),
            label: "Thumbnails".into(),
            icon: "phosphor:squares-four".into(),
            keys: vec![
                ("Arrows".into(), "Move in the grid".into()),
                ("Page Up, Page Down".into(), "A page up or down".into()),
                ("Home, End".into(), "First, last entry".into()),
            ],
            gestures: vec!["Wheel: scroll".into()],
            persist: "thumbnails".into(),
            role: egui::Role::ListBox,
            needs: ModeNeeds {
                thumbnails: true,
                ..ModeNeeds::default()
            },
        }
    }

    fn show(&mut self, ui: &mut Ui, cx: ModeContext<'_>) -> ModeOutput {
        let mut out = ModeOutput::default();
        let side = cx
            .settings
            .thumbnail_size
            .clamp(Self::MIN_SIZE, Self::MAX_SIZE);
        let mut plain = PlainDrawer {
            entries: cx.entries,
            selection: cx.selection,
            focus: cx.focus,
        };
        let drawer: &mut dyn EntryDrawer = match cx.drawer {
            Some(d) => d,
            None => &mut plain,
        };
        let width = ui.available_width();
        drawer.headers(ui, width);
        let n = cx.entries.len();
        let cell = drawer.cell_size(ui, side);
        let pixels = (side * ui.ctx().pixels_per_point()).round() as u32;
        let ctx = ui.ctx().clone();
        let thumbnails = cx.thumbnails;
        let entries = cx.entries;
        let ask = |i: usize, priority| {
            thumbnails.request(
                &ctx,
                &ThumbRequest {
                    entry: &entries[i],
                    size: pixels,
                    priority,
                },
            )
        };
        let reveal = cx.focus.filter(|_| cx.reveal_focus && n > 0);
        let columns = lay_out(
            ui,
            cx.name.as_deref().map(|n| (n, cx.many)),
            drawer,
            cx.id.with("thumbnails"),
            n,
            cell,
            cx.max_height,
            reveal,
            cx.focus,
            &mut out,
            // Visible first, in reading order.
            |d, ui, i| d.cell(ui, i, cell, ask(i, Priority::Visible)),
        );
        // Then the next row.
        let end = out.visible.end;
        for i in end..(end + columns).min(n) {
            ask(i, Priority::Near);
        }
        self.columns = columns;
        self.rows_in_view = out.visible.len().div_ceil(columns).max(1);
        out.columns = columns;
        out
    }

    fn step(&self, from: usize, count: usize, key: ModeKey) -> usize {
        let last = count.saturating_sub(1);
        let c = self.columns.max(1);
        let page = c * self.rows_in_view.max(1);
        match key {
            ModeKey::Left => from.saturating_sub(1),
            ModeKey::Right => (from + 1).min(last),
            ModeKey::Up if from >= c => from - c,
            ModeKey::Up => from,
            ModeKey::Down if from + c <= last => from + c,
            // The last row may be short: go to its last entry, as file managers do.
            ModeKey::Down if from / c < last / c => last,
            ModeKey::Down => from,
            ModeKey::PageUp => from.saturating_sub(page),
            ModeKey::PageDown => (from + page).min(last),
            ModeKey::Home => 0,
            ModeKey::End => last,
        }
    }
}

/// Finder's columns: one column per folder of the path, the place's last,
/// then the focused entry looked into (a folder's entries, a file's
/// thumbnail and facts). Up and Down move in the place's column; Right
/// enters the focused folder, Left goes back to the parent.
#[derive(Clone, Debug, Default)]
pub struct ColumnsMode {
    rows_in_view: usize,
}

impl ColumnsMode {
    /// A column's width, in points.
    pub const COLUMN_WIDTH: f32 = 200.0;

    /// One column of plain rows: an ancestor's, or the focused folder's.
    /// Answers the entry clicked.
    fn side(ui: &mut Ui, column: &Column, height: f32, salt: egui::Id) -> Option<PathBuf> {
        let mut clicked = None;
        let name = column.place.file_name().map_or_else(
            || column.place.display().to_string(),
            |n| n.to_string_lossy().into_owned(),
        );
        ui.scope_builder(
            egui::UiBuilder::new()
                .accessibility_role(egui::accesskit::Role::ListBox)
                .accessibility_label(name),
            |ui| {
                egui::ScrollArea::vertical()
                    .id_salt(salt)
                    .max_height(height)
                    .auto_shrink([false, false])
                    .show_rows(
                        ui,
                        DetailsMode::ROW_HEIGHT,
                        column.entries.len(),
                        |ui, range| {
                            for i in range {
                                let e = &column.entries[i];
                                let on = column.open == Some(i);
                                let (rect, resp) = ui.allocate_exact_size(
                                    egui::vec2(ui.available_width(), DetailsMode::ROW_HEIGHT),
                                    egui::Sense::click(),
                                );
                                entry_response(&resp, e, on);
                                let visuals = ui.visuals();
                                if on {
                                    ui.painter().rect_filled(
                                        rect,
                                        2.0,
                                        visuals.widgets.active.weak_bg_fill,
                                    );
                                } else if resp.hovered() {
                                    ui.painter().rect_filled(
                                        rect,
                                        2.0,
                                        visuals.widgets.hovered.weak_bg_fill,
                                    );
                                }
                                paint_row_name(ui, rect, e, visuals.text_color());
                                if resp.clicked() {
                                    clicked = Some(e.path.clone());
                                }
                            }
                        },
                    );
            },
        );
        clicked
    }
}

/// A row's name, and a chevron after a folder's.
fn paint_row_name(ui: &Ui, rect: egui::Rect, e: &SourceEntry, color: egui::Color32) {
    let font = egui::TextStyle::Body.resolve(ui.style());
    let y = rect.center().y;
    let chevron = 14.0;
    ui.painter()
        .with_clip_rect(rect.with_max_x(rect.right() - chevron))
        .text(
            egui::pos2(rect.left() + 4.0, y),
            egui::Align2::LEFT_CENTER,
            &e.name,
            font.clone(),
            color,
        );
    if e.is_dir() {
        ui.painter().text(
            egui::pos2(rect.right() - 4.0, y),
            egui::Align2::RIGHT_CENTER,
            ">",
            font,
            color,
        );
    }
}

impl ViewMode for ColumnsMode {
    fn describe(&self) -> ModeInfo {
        ModeInfo {
            id: MODE_COLUMNS.into(),
            label: "Columns".into(),
            icon: "phosphor:columns".into(),
            keys: vec![
                ("Up, Down".into(), "Previous, next entry".into()),
                ("Right".into(), "Into the folder".into()),
                ("Left".into(), "Back to the parent".into()),
            ],
            gestures: vec!["Wheel: scroll".into()],
            persist: MODE_COLUMNS.into(),
            role: egui::Role::ListBox,
            // The focused file's thumbnail is at the column's width: no
            // size slider.
            needs: ModeNeeds {
                hierarchy: true,
                ..ModeNeeds::default()
            },
        }
    }

    fn show(&mut self, ui: &mut Ui, cx: ModeContext<'_>) -> ModeOutput {
        let mut out = ModeOutput::default();
        let w = Self::COLUMN_WIDTH;
        let height = cx
            .max_height
            .unwrap_or_else(|| ui.available_height())
            .max(DetailsMode::ROW_HEIGHT * 3.0);
        let mut plain = PlainDrawer {
            entries: cx.entries,
            selection: cx.selection,
            focus: cx.focus,
        };
        let drawer: &mut dyn EntryDrawer = match cx.drawer {
            Some(d) => d,
            None => &mut plain,
        };
        let n = cx.entries.len();
        let reveal = cx.focus.filter(|_| cx.reveal_focus && n > 0);
        let reveal_column = cx.reveal_focus;
        let ctx = ui.ctx().clone();
        let thumbnails = cx.thumbnails;
        let focused = cx.focus.and_then(|i| cx.entries.get(i));
        egui::ScrollArea::horizontal()
            .id_salt(cx.id.with("columns"))
            .auto_shrink([false, true])
            .show(ui, |ui| {
                ui.horizontal_top(|ui| {
                    let column = |ui: &mut Ui, add: &mut dyn FnMut(&mut Ui)| {
                        ui.allocate_ui_with_layout(
                            egui::vec2(w, height),
                            egui::Layout::top_down(egui::Align::Min),
                            |ui| {
                                ui.set_width(w);
                                ui.set_min_height(height);
                                add(ui);
                            },
                        )
                        .response
                    };
                    for (k, a) in cx.ancestors.iter().enumerate() {
                        column(ui, &mut |ui| {
                            if let Some(p) = Self::side(ui, a, height, cx.id.with(("ancestor", k)))
                            {
                                out.reach = Some(Reach {
                                    place: a.place.clone(),
                                    focus: Some(p),
                                });
                            }
                        });
                        ui.separator();
                    }
                    let here = column(ui, &mut |ui| {
                        let size = drawer.row_size(ui, w - ui.spacing().scroll.allocated_width());
                        lay_out(
                            ui,
                            cx.name.as_deref().map(|n| (n, cx.many)),
                            &mut *drawer,
                            cx.id.with("columns.here"),
                            n,
                            size,
                            Some(height),
                            reveal,
                            cx.focus,
                            &mut out,
                            |d, ui, i| d.row(ui, i, size),
                        );
                    });
                    if let Some(f) = focused {
                        ui.separator();
                        column(ui, &mut |ui| match cx.peek {
                            Some(p) if f.is_dir() => {
                                if let Some(e) = Self::side(ui, p, height, cx.id.with("peek")) {
                                    out.reach = Some(Reach {
                                        place: p.place.clone(),
                                        focus: Some(e),
                                    });
                                }
                            }
                            _ if f.is_dir() => {}
                            _ => {
                                let side = w - 8.0;
                                let pixels = (side * ctx.pixels_per_point()).round() as u32;
                                let state = thumbnails.request(
                                    &ctx,
                                    &ThumbRequest {
                                        entry: f,
                                        size: pixels,
                                        priority: Priority::Visible,
                                    },
                                );
                                let cell = egui::vec2(w, side + DetailsMode::ROW_HEIGHT + 8.0);
                                let (rect, _) = ui.allocate_exact_size(cell, egui::Sense::hover());
                                paint_cell(ui, rect, f, state);
                                if let Some(s) = f.size {
                                    ui.weak(human_size(s));
                                }
                            }
                        });
                    }
                    if reveal_column {
                        ui.scroll_to_rect(here.rect, None);
                    }
                });
            });
        self.rows_in_view = out.visible.len().max(1);
        out.columns = 1;
        out
    }

    fn step(&self, from: usize, count: usize, key: ModeKey) -> usize {
        let last = count.saturating_sub(1);
        let page = self.rows_in_view.max(1);
        match key {
            ModeKey::Up => from.saturating_sub(1),
            ModeKey::Down => (from + 1).min(last),
            ModeKey::Left | ModeKey::Right => from,
            ModeKey::PageUp => from.saturating_sub(page),
            ModeKey::PageDown => (from + page).min(last),
            ModeKey::Home => 0,
            ModeKey::End => last,
        }
    }

    fn travel(&self, key: ModeKey) -> Option<Travel> {
        match key {
            ModeKey::Right => Some(Travel::Into),
            ModeKey::Left => Some(Travel::Out),
            _ => None,
        }
    }
}

/// Lays `count` cells out in one virtualised row, scrolled sideways, `item`
/// drawing cell `i`: the filmstrip's strip.
fn strip_row(
    ui: &mut Ui,
    count: usize,
    cell: egui::Vec2,
    salt: egui::Id,
    reveal: Option<usize>,
    mut item: impl FnMut(&mut Ui, usize),
) {
    let gap = ui.spacing().item_spacing.x;
    let pitch = cell.x + gap;
    egui::ScrollArea::horizontal()
        .id_salt(salt)
        .auto_shrink([false, true])
        .animated(false)
        .show_viewport(ui, |ui, viewport| {
            let total = (pitch * count as f32 - gap).max(0.0);
            ui.set_width(total);
            ui.set_height(cell.y);
            let origin = ui.max_rect().min;
            if let Some(i) = reveal {
                let r = egui::Rect::from_min_size(origin + egui::vec2(i as f32 * pitch, 0.0), cell);
                ui.scroll_to_rect(r, None);
                ui.ctx().request_repaint();
            }
            let first = ((viewport.min.x / pitch).floor().max(0.0) as usize).min(count);
            let end = ((viewport.max.x / pitch).ceil().max(0.0) as usize + 1).min(count);
            for i in first..end {
                let rect =
                    egui::Rect::from_min_size(origin + egui::vec2(i as f32 * pitch, 0.0), cell);
                ui.scope_builder(egui::UiBuilder::new().max_rect(rect), |ui| item(ui, i));
            }
        });
}

/// One entry large: its thumbnail fitted in `rect`, or what stands in for
/// it, named for screen readers by `name`.
fn paint_large(
    ui: &mut Ui,
    rect: egui::Rect,
    e: &SourceEntry,
    state: ThumbState,
    name: &str,
) -> egui::Response {
    let resp = ui.allocate_rect(rect, egui::Sense::hover());
    let label = name.to_owned();
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Image, true, &label));
    let visuals = ui.visuals();
    ui.painter()
        .rect_filled(rect, 4.0, visuals.extreme_bg_color);
    match state {
        ThumbState::Ready(t) => {
            let s = t.size_vec2();
            let scale = (rect.width() / s.x).min(rect.height() / s.y);
            ui.painter().image(
                t.id(),
                egui::Rect::from_center_size(rect.center(), s * scale),
                egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                egui::Color32::WHITE,
            );
        }
        other => {
            let glyph = match other {
                _ if e.is_dir() => "Folder",
                ThumbState::Failed(_) => "!",
                ThumbState::Loading => "...",
                _ => "File",
            };
            ui.painter().text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                glyph,
                egui::FontId::proportional((rect.height() * 0.1).clamp(12.0, 48.0)),
                visuals.weak_text_color(),
            );
        }
    }
    resp
}

/// The pixels a large view asks for: its long side, on the screen.
fn pixels_of(ui: &Ui, rect: egui::Rect) -> u32 {
    (rect.width().max(rect.height()) * ui.ctx().pixels_per_point()).round() as u32
}

/// One row of thumbnails, the focused entry large above it (Lightroom's
/// Loupe and Filmstrip). The large view asks first (`Visible`), then the
/// strip's cells in view, then the next screen of them (`Near`).
#[derive(Clone, Debug, Default)]
pub struct FilmstripMode {
    in_view: usize,
}

impl FilmstripMode {
    /// The strip's thumbnails' most side, in points.
    pub const MAX_STRIP: f32 = 128.0;
}

impl ViewMode for FilmstripMode {
    fn describe(&self) -> ModeInfo {
        ModeInfo {
            id: MODE_FILMSTRIP.into(),
            label: "Filmstrip".into(),
            icon: "phosphor:film-strip".into(),
            keys: vec![
                ("Left, Right".into(), "Previous, next entry".into()),
                ("Page Up, Page Down".into(), "A strip's width".into()),
                ("Home, End".into(), "First, last entry".into()),
            ],
            gestures: vec!["Wheel: scroll the strip".into()],
            persist: MODE_FILMSTRIP.into(),
            role: egui::Role::ListBox,
            needs: ModeNeeds {
                thumbnails: true,
                ..ModeNeeds::default()
            },
        }
    }

    fn show(&mut self, ui: &mut Ui, cx: ModeContext<'_>) -> ModeOutput {
        let mut out = ModeOutput::default();
        let side = cx
            .settings
            .thumbnail_size
            .clamp(ThumbnailsMode::MIN_SIZE, Self::MAX_STRIP);
        let mut plain = PlainDrawer {
            entries: cx.entries,
            selection: cx.selection,
            focus: cx.focus,
        };
        let drawer: &mut dyn EntryDrawer = match cx.drawer {
            Some(d) => d,
            None => &mut plain,
        };
        let n = cx.entries.len();
        let cell = drawer.cell_size(ui, side);
        let height = cx
            .max_height
            .unwrap_or_else(|| ui.available_height())
            .max(cell.y * 2.0);
        let ctx = ui.ctx().clone();
        let thumbnails = cx.thumbnails;
        let entries = cx.entries;
        // The large view: the focused entry, else the first.
        let shown = cx.focus.or((n > 0).then_some(0));
        let big_h = (height
            - cell.y
            - ui.spacing().item_spacing.y * 2.0
            - ui.spacing().scroll.allocated_width())
        .max(cell.y);
        let (big, _) = ui.allocate_exact_size(
            egui::vec2(ui.available_width(), big_h),
            egui::Sense::hover(),
        );
        if let Some(i) = shown {
            let e = &entries[i];
            let state = thumbnails.request(
                &ctx,
                &ThumbRequest {
                    entry: e,
                    size: pixels_of(ui, big),
                    priority: Priority::Visible,
                },
            );
            let name = strings::tr_args(
                &ctx,
                &crate::strings::FILES_PREVIEW_OF,
                &[("name", &e.name)],
            );
            let mut child = ui.new_child(egui::UiBuilder::new().max_rect(big));
            paint_large(&mut child, big, e, state, &name);
        } else {
            drawer.empty(ui);
        }
        let pixels = (side * ctx.pixels_per_point()).round() as u32;
        let ask = |i: usize, priority| {
            thumbnails.request(
                &ctx,
                &ThumbRequest {
                    entry: &entries[i],
                    size: pixels,
                    priority,
                },
            )
        };
        let reveal = cx.focus.filter(|_| cx.reveal_focus && n > 0);
        let mut first = usize::MAX;
        let mut end = 0;
        let focus = cx.focus;
        let mut add = |ui: &mut Ui, out: &mut ModeOutput| {
            strip_row(ui, n, cell, cx.id.with("filmstrip"), reveal, |ui, i| {
                let r = drawer.cell(ui, i, cell, ask(i, Priority::Visible));
                if focus == Some(i) {
                    out.focused = Some(r.id);
                }
                report(&r, i, out);
                first = first.min(i);
                end = end.max(i + 1);
            });
        };
        in_container(
            ui,
            cx.name.as_deref().map(|n| (n, cx.many)),
            &mut out,
            &mut add,
        );
        out.visible = if first == usize::MAX {
            0..0
        } else {
            first..end
        };
        // Then the next screen of the strip.
        let screen = out.visible.len();
        for i in end..(end + screen).min(n) {
            ask(i, Priority::Near);
        }
        self.in_view = screen.max(1);
        out.columns = n.max(1);
        out
    }

    fn step(&self, from: usize, count: usize, key: ModeKey) -> usize {
        let last = count.saturating_sub(1);
        let page = self.in_view.max(1);
        match key {
            ModeKey::Left => from.saturating_sub(1),
            ModeKey::Right => (from + 1).min(last),
            ModeKey::Up | ModeKey::Down => from,
            ModeKey::PageUp => from.saturating_sub(page),
            ModeKey::PageDown => (from + page).min(last),
            ModeKey::Home => 0,
            ModeKey::End => last,
        }
    }
}

// ---------------------------------------------------------------------------
// The mode switcher and the mode keys
// ---------------------------------------------------------------------------

/// The id of the details mode, the first in the switcher.
pub const MODE_DETAILS: &str = "details";
/// The id of the thumbnails mode.
pub const MODE_THUMBNAILS: &str = "thumbnails";
/// The id of the columns mode (Finder's).
pub const MODE_COLUMNS: &str = "columns";
/// The id of the filmstrip mode.
pub const MODE_FILMSTRIP: &str = "filmstrip";
/// Up to this many modes, the switcher is a segmented control; beyond, a
/// menu.
pub const SEGMENTED_MAX: usize = 4;
/// How many modes have a key (`Ctrl+1` to `Ctrl+9`).
pub const MODE_KEYS: usize = 9;

fn request_id() -> egui::Id {
    egui::Id::unique("egui_pop.file_browser.mode_request")
}

fn focused_id() -> egui::Id {
    egui::Id::unique("egui_pop.file_browser.focused")
}

/// A mode's name in the interface's language: the catalogue's
/// `egui-pop-files-mode-<id>`, else its English label.
pub fn mode_label(ctx: &egui::Context, info: &ModeInfo) -> String {
    strings::lookup(
        ctx,
        &format!("egui-pop-files-mode-{}", info.id),
        &info.label,
    )
}

/// Asks the file browser holding the keyboard for its `n`-th mode
/// (from 1, in the switcher's order): what the `file.view.mode<n>`
/// commands do. Taken by the browser's next frame.
pub fn request_mode(ctx: &egui::Context, n: usize) {
    ctx.data_mut(|d| d.insert_temp(request_id(), n));
    ctx.request_repaint();
}

/// Whether a file browser held the keyboard in the frame drawn last; read
/// once (the flag is cleared), for the mode keys' `when` clause.
pub fn take_focused(ctx: &egui::Context) -> bool {
    ctx.data_mut(|d| d.remove_temp::<bool>(focused_id()))
        .unwrap_or(false)
}

/// The `n`-th mode key pressed (`Ctrl+1` is 1), consumed; `None` when
/// none was.
pub fn mode_key(ui: &mut Ui) -> Option<usize> {
    const KEYS: [egui::Key; MODE_KEYS] = [
        egui::Key::Num1,
        egui::Key::Num2,
        egui::Key::Num3,
        egui::Key::Num4,
        egui::Key::Num5,
        egui::Key::Num6,
        egui::Key::Num7,
        egui::Key::Num8,
        egui::Key::Num9,
    ];
    ui.input_mut(|i| {
        KEYS.iter()
            .position(|k| i.consume_key(Modifiers::COMMAND, *k))
            .map(|n| n + 1)
    })
}

/// The mode switcher: a segmented control for up to [`SEGMENTED_MAX`]
/// modes, a menu beyond. Each mode's label is translated
/// ([`mode_label`]), its icon drawn from the icon registry when it
/// resolves, its keys in its tip. Answers the mode pressed, if one was
/// (the one shown too: a press may move the keyboard elsewhere).
pub fn mode_switcher(
    ui: &mut Ui,
    id: egui::Id,
    modes: &[ModeInfo],
    current: &str,
) -> Option<(String, egui::Response)> {
    let ctx = ui.ctx().clone();
    let tip = |n: usize, info: &ModeInfo| {
        let mut lines = Vec::new();
        if n < MODE_KEYS {
            lines.push(format!("Ctrl+{}", n + 1));
        }
        lines.extend(info.keys.iter().map(|(k, what)| format!("{k}: {what}")));
        lines.join("\n")
    };
    let mut picked = None;
    if modes.len() <= SEGMENTED_MAX {
        ui.scope(|ui| {
            ui.spacing_mut().item_spacing.x = 0.0;
            for (n, info) in modes.iter().enumerate() {
                let label = mode_label(&ctx, info);
                let on = info.id == current;
                // Without the icon font in use (a bare test context, the
                // first pass), the label stands alone: a glyph would panic.
                let glyph = Icon::by_name(&info.icon)
                    .and_then(Icon::glyph)
                    .filter(|_| egui_pop_core::icon::font_ready(&ctx));
                let r = match glyph {
                    Some(g) => {
                        let family = egui::FontFamily::Name(egui_pop_core::icon::FAMILY.into());
                        let icon = egui::RichText::new(g).family(family);
                        widgets::selectable_label(ui, on, (icon, label.as_str()))
                    }
                    None => widgets::selectable_label(ui, on, label.as_str()),
                };
                // Its name is the label alone, not the icon's glyph.
                r.widget_info(|| egui::WidgetInfo::selected(egui::Role::Button, true, on, &label));
                let r = r.on_hover_text(tip(n, info));
                if r.clicked() {
                    picked = Some((info.id.clone(), r));
                }
            }
        });
    } else {
        let shown = modes
            .iter()
            .find(|m| m.id == current)
            .map(|m| mode_label(&ctx, m))
            .unwrap_or_default();
        let r = egui::ComboBox::from_id_salt(id.with("modes"))
            .selected_text(shown)
            .show_ui(ui, |ui| {
                for (n, info) in modes.iter().enumerate() {
                    let on = info.id == current;
                    let r = widgets::selectable_label(ui, on, mode_label(&ctx, info))
                        .on_hover_text(tip(n, info));
                    if r.clicked() {
                        picked = Some((info.id.clone(), r));
                    }
                }
            });
        let name = strings::tr(&ctx, &crate::strings::FILES_MODES);
        r.response
            .widget_info(|| egui::WidgetInfo::labeled(egui::Role::ComboBox, true, &name));
    }
    picked
}

// ---------------------------------------------------------------------------
// The browser
// ---------------------------------------------------------------------------

/// How many entries can be selected.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SelectMode {
    One,
    Many,
}

/// What the browser did this frame.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BrowserOutput {
    /// An entry opened: a double click or Enter.
    pub activated: Option<PathBuf>,
    pub selection_changed: bool,
    pub mode_changed: bool,
    /// The place shown changed: a click in another column, or Left and
    /// Right in a mode that shows the path.
    pub place_changed: bool,
}

/// Type-ahead's prefix and when it was last typed.
#[derive(Clone, Debug, Default)]
struct TypeAhead {
    prefix: String,
    at: f64,
}

/// The file browser: entries from a [`FileSource`], shown by a
/// [`ViewMode`] chosen in its toolbar (or with `Ctrl+1` to `Ctrl+9`), kept
/// per dialog in the host's store when drawn with [`FileBrowser::show_in`].
pub struct FileBrowser {
    id: egui::Id,
    source: Arc<dyn FileSource>,
    place: PathBuf,
    options: ListOptions,
    listing: Option<Listing>,
    entries: Vec<SourceEntry>,
    error: Option<SourceError>,
    selection: Selection,
    focus: Option<PathBuf>,
    select: SelectMode,
    modes: ViewModes,
    mode_id: String,
    mode: Box<dyn ViewMode>,
    settings: ModeSettings,
    thumbnails: Arc<dyn ThumbnailProvider>,
    type_ahead: TypeAhead,
    reveal: bool,
    restored: bool,
    root: Option<PathBuf>,
    /// The ancestors' and the focused folder's listings, for a mode that
    /// shows the path.
    ancestors: Vec<Column>,
    ancestor_listings: Vec<Option<Listing>>,
    peek: Option<(Column, Option<Listing>)>,
    /// Focus the first entry once the listing gives one (Right into a
    /// folder).
    focus_first: bool,
    /// Quick look is open over the entries.
    quick_look: bool,
}

/// The browser's order: folders first, then names as people read them.
fn sort_entries(entries: &mut [SourceEntry]) {
    entries.sort_by(|a, b| {
        b.is_dir()
            .cmp(&a.is_dir())
            .then_with(|| natural_cmp(&a.name, &b.name))
    });
}

/// Takes what a side listing delivered into its column; `true` while it is
/// under way.
fn pump_column(column: &mut Column, listing: &mut Option<Listing>) -> bool {
    let Some(l) = listing else {
        return false;
    };
    let mut changed = false;
    let mut done = false;
    for page in l.poll() {
        match page {
            Page::Entries(mut e) => {
                column.entries.append(&mut e);
                changed = true;
            }
            Page::Updated(e) => {
                for u in e {
                    if let Some(old) = column.entries.iter_mut().find(|o| o.path == u.path) {
                        *old = u;
                    }
                }
            }
            Page::Removed(paths) => column.entries.retain(|e| !paths.contains(&e.path)),
            Page::Changed => column.entries.clear(),
            Page::Done | Page::Failed(_) => done = true,
        }
    }
    if changed {
        sort_entries(&mut column.entries);
    }
    if done {
        *listing = None;
    }
    column.loading = listing.is_some();
    column.loading
}

impl fmt::Debug for FileBrowser {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("FileBrowser")
            .field("id", &self.id)
            .field("source", &self.source.id())
            .field("place", &self.place)
            .field("entries", &self.entries.len())
            .field("mode", &self.mode_id)
            .finish_non_exhaustive()
    }
}

/// A provider that gives nothing: icons stand in.
#[derive(Debug, Default)]
pub struct NoThumbnails;

impl ThumbnailProvider for NoThumbnails {
    fn request(&self, _ctx: &egui::Context, _r: &ThumbRequest<'_>) -> ThumbState {
        ThumbState::None
    }
}

impl FileBrowser {
    /// How long type-ahead's prefix lasts after the last key, in seconds.
    pub const TYPE_AHEAD_RESET: f64 = 1.0;

    /// A browser of `place`, keyed by `id` (its mode is remembered under it),
    /// in the first mode of the default registry.
    pub fn new(
        id: impl std::hash::Hash + fmt::Debug,
        source: Arc<dyn FileSource>,
        place: impl Into<PathBuf>,
    ) -> Self {
        let modes = ViewModes::default();
        let mode_id = modes.ids().first().cloned().unwrap_or_default();
        let mode = modes
            .make(&mode_id)
            .unwrap_or_else(|| Box::new(DetailsMode::default()));
        let mut b = Self {
            id: egui::Id::unique(("egui_pop.file_browser", id)),
            source,
            place: place.into(),
            options: ListOptions::default(),
            listing: None,
            entries: Vec::new(),
            error: None,
            selection: Selection::default(),
            focus: None,
            select: SelectMode::Many,
            modes,
            mode_id,
            mode,
            settings: ModeSettings::default(),
            thumbnails: Arc::new(NoThumbnails),
            type_ahead: TypeAhead::default(),
            reveal: false,
            restored: false,
            root: None,
            ancestors: Vec::new(),
            ancestor_listings: Vec::new(),
            peek: None,
            focus_first: false,
            quick_look: false,
        };
        b.reload();
        b
    }

    pub fn with_modes(mut self, modes: ViewModes) -> Self {
        self.modes = modes;
        if let Some(first) = self.modes.ids().first().cloned() {
            self.set_mode(&first);
        }
        self
    }

    pub fn with_thumbnails(mut self, provider: Arc<dyn ThumbnailProvider>) -> Self {
        self.thumbnails = provider;
        self
    }

    pub fn with_select(mut self, select: SelectMode) -> Self {
        self.select = select;
        self
    }

    pub fn id(&self) -> egui::Id {
        self.id
    }

    /// The topmost folder a mode that shows the path goes up to (and Left
    /// goes back to); the file system's root when not set.
    pub fn with_root(mut self, root: impl Into<PathBuf>) -> Self {
        self.root = Some(root.into());
        self
    }

    /// The ancestors' listings, the topmost first, as a mode that shows the
    /// path is given them (kept only while such a mode is shown).
    pub fn ancestors(&self) -> &[Column] {
        &self.ancestors
    }

    /// The place's ancestors a mode that shows the path lists, the topmost
    /// first: up to the root.
    fn wanted_ancestors(&self) -> Vec<PathBuf> {
        if let Some(root) = &self.root
            && !self.place.starts_with(root)
        {
            return Vec::new();
        }
        let mut up: Vec<PathBuf> = Vec::new();
        for a in self.place.ancestors().skip(1) {
            if self.root.as_deref().is_some_and(|r| !a.starts_with(r)) {
                break;
            }
            up.push(a.to_path_buf());
        }
        up.reverse();
        up
    }

    /// Keeps the side listings a mode that shows the path needs: started
    /// when missing, polled, dropped when not wanted. `true` while one is
    /// under way.
    fn sync_columns(&mut self) -> bool {
        let wanted = self.wanted_ancestors();
        if self.ancestors.iter().map(|c| &c.place).ne(wanted.iter()) {
            let mut old: Vec<(Column, Option<Listing>)> = self
                .ancestors
                .drain(..)
                .zip(self.ancestor_listings.drain(..))
                .collect();
            for place in wanted {
                match old.iter().position(|(c, _)| c.place == place) {
                    Some(i) => {
                        let (c, l) = old.swap_remove(i);
                        self.ancestors.push(c);
                        self.ancestor_listings.push(l);
                    }
                    None => {
                        let l = self.source.list(&place, &self.options);
                        self.ancestors.push(Column {
                            place,
                            loading: true,
                            ..Column::default()
                        });
                        self.ancestor_listings.push(Some(l));
                    }
                }
            }
        }
        let mut busy = false;
        for (c, l) in self
            .ancestors
            .iter_mut()
            .zip(self.ancestor_listings.iter_mut())
        {
            busy |= pump_column(c, l);
        }
        // Each column's open entry: the path's next folder.
        let n = self.ancestors.len();
        for k in 0..n {
            let next = if k + 1 < n {
                self.ancestors[k + 1].place.clone()
            } else {
                self.place.clone()
            };
            let c = &mut self.ancestors[k];
            c.open = c.entries.iter().position(|e| e.path == next);
        }
        // The focused folder, looked into.
        let folder = self
            .focus_index()
            .map(|i| &self.entries[i])
            .filter(|e| e.is_dir())
            .map(|e| e.path.clone());
        match folder {
            Some(f) => {
                if self.peek.as_ref().is_none_or(|(c, _)| c.place != f) {
                    let l = self.source.list(&f, &self.options);
                    self.peek = Some((
                        Column {
                            place: f,
                            loading: true,
                            ..Column::default()
                        },
                        Some(l),
                    ));
                }
                if let Some((c, l)) = &mut self.peek {
                    busy |= pump_column(c, l);
                }
            }
            None => self.peek = None,
        }
        busy
    }

    /// Leaves the place as a mode's key asks: into the focused folder, or
    /// back to the parent. `true` when the place changed.
    pub fn travel(&mut self, travel: Travel) -> bool {
        match travel {
            Travel::Into => {
                let Some(folder) = self
                    .focus_index()
                    .map(|i| &self.entries[i])
                    .filter(|e| e.is_dir())
                    .map(|e| e.path.clone())
                else {
                    return false;
                };
                self.open(folder);
                self.focus_first = true;
                self.settle_focus();
                true
            }
            Travel::Out => {
                if self.root.as_deref() == Some(self.place.as_path()) {
                    return false;
                }
                let Some(parent) = self.place.parent().map(Path::to_path_buf) else {
                    return false;
                };
                let left = self.place.clone();
                self.reach(&Reach {
                    place: parent,
                    focus: Some(left),
                });
                true
            }
        }
    }

    /// Shows another place, an entry of it focused (and selected, as a
    /// click selects it).
    pub fn reach(&mut self, reach: &Reach) {
        if reach.place != self.place {
            self.open(reach.place.clone());
        }
        if let Some(f) = &reach.focus {
            self.selection.only(f);
            self.focus = Some(f.clone());
            self.reveal = true;
        }
    }

    /// Whether quick look is open.
    pub fn quick_look(&self) -> bool {
        self.quick_look
    }

    /// Opens or closes quick look, a large view of the focused entry over
    /// the entries (`Ctrl+Y`, or Space while an entry has the focus);
    /// nothing opens without a focused entry.
    pub fn set_quick_look(&mut self, open: bool) {
        self.quick_look = open && self.focus_index().is_some();
    }

    /// Quick look's keys: Escape, `Ctrl+Y` or Space closes it, the arrows
    /// go to the previous or next entry. `true` when a key was taken.
    fn quick_look_keys(&mut self, ui: &mut Ui) -> bool {
        let (close, back, next) = ui.input_mut(|i| {
            let close = i.consume_key(Modifiers::NONE, egui::Key::Escape)
                || i.consume_key(Modifiers::COMMAND, egui::Key::Y)
                || i.consume_key(Modifiers::NONE, egui::Key::Space);
            let back = i.consume_key(Modifiers::NONE, egui::Key::ArrowLeft)
                || i.consume_key(Modifiers::NONE, egui::Key::ArrowUp);
            let next = i.consume_key(Modifiers::NONE, egui::Key::ArrowRight)
                || i.consume_key(Modifiers::NONE, egui::Key::ArrowDown);
            (close, back, next)
        });
        if close {
            self.quick_look = false;
        } else if (back || next)
            && let Some(at) = self.focus_index()
        {
            {
                let to = if next {
                    (at + 1).min(self.entries.len() - 1)
                } else {
                    at.saturating_sub(1)
                };
                let path = self.entries[to].path.clone();
                self.focus = Some(path.clone());
                if self.select == SelectMode::One {
                    self.selection.only(&path);
                }
                self.reveal = true;
            }
        }
        close || back || next
    }

    /// Draws quick look over `rect`: the focused entry as large as it fits,
    /// its name under it.
    fn show_quick_look(&self, ui: &mut Ui, rect: egui::Rect) {
        let Some(e) = self.focus_index().map(|i| &self.entries[i]) else {
            return;
        };
        let ctx = ui.ctx().clone();
        let name = strings::tr_args(
            &ctx,
            &crate::strings::FILES_QUICK_LOOK,
            &[("name", &e.name)],
        );
        egui::Area::new(self.id.with("quick_look"))
            .order(egui::Order::Foreground)
            .fixed_pos(rect.min)
            .show(&ctx, |ui| {
                ui.set_min_size(rect.size());
                let visuals = ui.visuals().clone();
                ui.painter()
                    .rect_filled(rect, 6.0, visuals.window_fill.gamma_multiply(0.97));
                let caption = ui.text_style_height(&egui::TextStyle::Body) + 8.0;
                let image = egui::Rect::from_min_max(
                    rect.min + egui::vec2(12.0, 12.0),
                    egui::pos2(rect.right() - 12.0, rect.bottom() - caption - 12.0),
                );
                if image.width() < 1.0 || image.height() < 1.0 {
                    return;
                }
                let state = self.thumbnails.request(
                    &ctx,
                    &ThumbRequest {
                        entry: e,
                        size: pixels_of(ui, image),
                        priority: Priority::Visible,
                    },
                );
                paint_large(ui, image, e, state, &name);
                ui.painter().text(
                    egui::pos2(rect.center().x, rect.bottom() - caption / 2.0 - 6.0),
                    egui::Align2::CENTER_CENTER,
                    &e.name,
                    egui::TextStyle::Body.resolve(ui.style()),
                    visuals.text_color(),
                );
            });
    }

    /// Focuses the first entry when asked to and one is there.
    fn settle_focus(&mut self) {
        if !self.focus_first {
            return;
        }
        if let Some(first) = self.entries.first().map(|e| e.path.clone()) {
            self.focus_first = false;
            self.focus = Some(first.clone());
            self.selection.only(&first);
            self.reveal = true;
        } else if !self.loading() {
            self.focus_first = false;
        }
    }

    pub fn place(&self) -> &Path {
        &self.place
    }

    /// Shows another place; the selection and the focus are left behind.
    pub fn open(&mut self, place: impl Into<PathBuf>) {
        self.place = place.into();
        self.selection = Selection::default();
        self.focus = None;
        self.reload();
    }

    /// Lists the place again, keeping the selection and the focus by path.
    pub fn reload(&mut self) {
        self.entries.clear();
        self.error = None;
        self.listing = Some(self.source.list(&self.place, &self.options));
        self.pump();
    }

    /// Takes what the listing delivered.
    pub fn pump(&mut self) {
        let Some(listing) = &self.listing else {
            return;
        };
        let mut done = false;
        let mut changed = false;
        for page in listing.poll() {
            match page {
                Page::Entries(mut e) => {
                    self.entries.append(&mut e);
                    changed = true;
                }
                Page::Updated(e) => {
                    for u in e {
                        if let Some(old) = self.entries.iter_mut().find(|o| o.path == u.path) {
                            *old = u;
                        }
                    }
                }
                Page::Removed(paths) => {
                    self.entries.retain(|e| !paths.contains(&e.path));
                    for p in &paths {
                        self.selection.paths.remove(p);
                    }
                }
                Page::Changed => {
                    self.reload();
                    return;
                }
                Page::Done => done = true,
                Page::Failed(e) => {
                    self.error = Some(e);
                    done = true;
                }
            }
        }
        if changed {
            sort_entries(&mut self.entries);
        }
        if done {
            self.listing = None;
        }
        self.settle_focus();
    }

    /// Whether the listing is still under way.
    pub fn loading(&self) -> bool {
        self.listing.is_some()
    }

    pub fn error(&self) -> Option<&SourceError> {
        self.error.as_ref()
    }

    pub fn entries(&self) -> &[SourceEntry] {
        &self.entries
    }

    pub fn selection(&self) -> &Selection {
        &self.selection
    }

    pub fn focus(&self) -> Option<&Path> {
        self.focus.as_deref()
    }

    fn focus_index(&self) -> Option<usize> {
        let f = self.focus.as_ref()?;
        self.entries.iter().position(|e| &e.path == f)
    }

    /// The modes this browser offers, in the switcher's order.
    pub fn modes(&self) -> Vec<String> {
        self.modes.ids()
    }

    pub fn mode(&self) -> &str {
        &self.mode_id
    }

    /// Shows the entries in another mode, keeping the selection and the
    /// focus; `false` when no such mode is registered.
    pub fn set_mode(&mut self, id: &str) -> bool {
        if id == self.mode_id {
            return true;
        }
        let Some(mode) = self.modes.make(id) else {
            return false;
        };
        self.mode = mode;
        self.mode_id = id.to_owned();
        self.reveal = true;
        true
    }

    /// The thumbnails' side, in points.
    pub fn set_thumbnail_size(&mut self, side: f32) {
        self.settings.thumbnail_size =
            side.clamp(ThumbnailsMode::MIN_SIZE, ThumbnailsMode::MAX_SIZE);
    }

    /// Focuses an entry, by path; with `extend`, selects the range from the
    /// anchor (several only).
    pub fn focus_on(&mut self, path: &Path, extend: bool) {
        let Some(at) = self.entries.iter().position(|e| e.path == path) else {
            return;
        };
        self.focus = Some(path.to_path_buf());
        self.reveal = true;
        if extend && self.select == SelectMode::Many {
            let anchor = self
                .selection
                .anchor
                .as_ref()
                .and_then(|a| self.entries.iter().position(|e| &e.path == a))
                .unwrap_or(at);
            let (lo, hi) = (anchor.min(at), anchor.max(at));
            self.selection.paths = self.entries[lo..=hi]
                .iter()
                .map(|e| e.path.clone())
                .collect();
        } else if self.select == SelectMode::One {
            self.selection.only(path);
        }
    }

    /// What a click does: selects that entry alone, or with Ctrl adds or
    /// takes it away, or with Shift selects the range.
    pub fn click(&mut self, path: &Path, modifiers: Modifiers) {
        if modifiers.shift && self.select == SelectMode::Many {
            self.focus_on(path, true);
        } else if modifiers.command && self.select == SelectMode::Many {
            if !self.selection.paths.remove(path) {
                self.selection.paths.insert(path.to_path_buf());
            }
            self.selection.anchor = Some(path.to_path_buf());
            self.focus = Some(path.to_path_buf());
        } else {
            self.selection.only(path);
            self.focus = Some(path.to_path_buf());
        }
    }

    /// Moves the focus by a key, as the mode's layout says.
    pub fn key(&mut self, key: ModeKey, shift: bool) -> bool {
        if let Some(t) = self.mode.travel(key) {
            return self.travel(t);
        }
        let n = self.entries.len();
        if n == 0 {
            return false;
        }
        let to = match self.focus_index() {
            Some(from) => self.mode.step(from, n, key),
            None => 0,
        };
        let path = self.entries[to].path.clone();
        let before = self.selection.clone();
        self.focus_on(&path, shift);
        if !shift && self.select == SelectMode::Many && self.selection.anchor.is_none() {
            self.selection.anchor = Some(path);
        }
        before != self.selection
    }

    pub fn select_all(&mut self) {
        if self.select == SelectMode::Many {
            self.selection.paths = self.entries.iter().map(|e| e.path.clone()).collect();
        }
    }

    /// Type-ahead: `text` typed at `now` (seconds). The focus goes to the
    /// first entry whose name starts with the prefix; one letter typed
    /// again goes round the entries that start with it.
    pub fn type_ahead(&mut self, text: &str, now: f64) -> bool {
        if text.is_empty() {
            return false;
        }
        if now - self.type_ahead.at > Self::TYPE_AHEAD_RESET {
            self.type_ahead.prefix.clear();
        }
        self.type_ahead.at = now;
        let lower = text.to_lowercase();
        let same_letter = {
            let p = &self.type_ahead.prefix;
            !p.is_empty() && p.chars().all(|c| lower.chars().all(|t| t == c))
        };
        let starts = |e: &SourceEntry, p: &str| e.name.to_lowercase().starts_with(p);
        let found = if same_letter {
            // Cycle among the names starting with that letter.
            let p = lower.clone();
            let from = self.focus_index().map_or(0, |i| i + 1);
            let n = self.entries.len();
            (0..n)
                .map(|k| (from + k) % n)
                .find(|&i| starts(&self.entries[i], &p))
        } else {
            self.type_ahead.prefix.push_str(&lower);
            let p = self.type_ahead.prefix.clone();
            self.entries.iter().position(|e| starts(e, &p))
        };
        match found {
            Some(i) => {
                let path = self.entries[i].path.clone();
                self.focus = Some(path.clone());
                if self.select == SelectMode::One {
                    self.selection.only(&path);
                }
                self.reveal = true;
                true
            }
            None => false,
        }
    }

    /// What each of its modes is, in the switcher's order.
    pub fn mode_infos(&self) -> Vec<ModeInfo> {
        self.modes
            .ids()
            .iter()
            .filter_map(|id| self.modes.make(id).map(|m| m.describe()))
            .collect()
    }

    /// Shows the `n`-th mode (from 1, in the switcher's order), as
    /// `Ctrl+n` does; `false` when there is none or it is shown already.
    pub fn set_mode_index(&mut self, n: usize) -> bool {
        let Some(id) = n
            .checked_sub(1)
            .and_then(|i| self.modes.ids().get(i).cloned())
        else {
            return false;
        };
        id != self.mode_id && self.set_mode(&id)
    }

    /// Draws it as [`FileBrowser::show`], its mode kept in the host's store
    /// under `dialog` (the last used for that dialog: a request's purpose,
    /// a window's name), read when it is first drawn.
    pub fn show_in(
        &mut self,
        ui: &mut Ui,
        app: &crate::app::AppHandle,
        dialog: &str,
    ) -> BrowserOutput {
        if !self.restored {
            self.restored = true;
            if let Some(kept) = super::view_for(app, dialog) {
                self.set_mode(&kept);
            }
        }
        let out = self.show(ui);
        if out.mode_changed {
            super::want_view_for(app, dialog, &self.mode_id);
        }
        out
    }

    /// Draws the toolbar's mode switcher and the entries.
    pub fn show(&mut self, ui: &mut Ui) -> BrowserOutput {
        let mut out = BrowserOutput::default();
        self.pump();
        if self.loading() {
            ui.ctx().request_repaint();
        }
        // The switcher.
        ui.horizontal(|ui| {
            let infos = self.mode_infos();
            if let Some((id, _)) = mode_switcher(ui, self.id, &infos, &self.mode_id)
                && id != self.mode_id
                && self.set_mode(&id)
            {
                out.mode_changed = true;
            }
            if infos
                .iter()
                .any(|m| m.id == self.mode_id && m.needs.thumbnails)
            {
                let name = strings::tr(ui.ctx(), &crate::strings::FILES_THUMBNAIL_SIZE);
                ui.add(
                    egui::Slider::new(
                        &mut self.settings.thumbnail_size,
                        ThumbnailsMode::MIN_SIZE..=ThumbnailsMode::MAX_SIZE,
                    )
                    .show_value(false)
                    .text(name),
                );
            }
        });
        if let Some(e) = &self.error {
            ui.label(e.to_string());
            return out;
        }
        // The keyboard, while the entries hold the focus.
        let keys_id = self.id.with("entries");
        let focused = ui.memory(|m| m.has_focus(keys_id));
        // Quick look holds the keys while it is open.
        let looking = self.quick_look && self.quick_look_keys(ui);
        if self.quick_look && focused {
            ui.ctx().data_mut(|d| d.insert_temp(focused_id(), true));
        }
        if focused && !self.quick_look && !looking {
            ui.ctx().data_mut(|d| d.insert_temp(focused_id(), true));
            // A mode key: pressed here, or run as a command of the keymap.
            let asked = mode_key(ui)
                .or_else(|| ui.ctx().data_mut(|d| d.remove_temp::<usize>(request_id())));
            if let Some(n) = asked
                && self.set_mode_index(n)
            {
                out.mode_changed = true;
            }
            let (events, modifiers) = ui.input(|i| (i.events.clone(), i.modifiers));
            let before = self.place.clone();
            // Space is type-ahead's while a prefix is being typed, quick
            // look's otherwise.
            let typing = !self.type_ahead.prefix.is_empty()
                && ui.input(|i| i.time) - self.type_ahead.at <= Self::TYPE_AHEAD_RESET;
            let mut spaced = false;
            for ev in events {
                match ev {
                    egui::Event::Key {
                        key, pressed: true, ..
                    } => {
                        let mk = match key {
                            egui::Key::ArrowUp => Some(ModeKey::Up),
                            egui::Key::ArrowDown => Some(ModeKey::Down),
                            egui::Key::ArrowLeft => Some(ModeKey::Left),
                            egui::Key::ArrowRight => Some(ModeKey::Right),
                            egui::Key::PageUp => Some(ModeKey::PageUp),
                            egui::Key::PageDown => Some(ModeKey::PageDown),
                            egui::Key::Home => Some(ModeKey::Home),
                            egui::Key::End => Some(ModeKey::End),
                            _ => None,
                        };
                        if let Some(mk) = mk {
                            out.selection_changed |= self.key(mk, modifiers.shift);
                        } else if key == egui::Key::Enter {
                            out.activated = self.focus.clone();
                        } else if key == egui::Key::A && modifiers.command {
                            self.select_all();
                            out.selection_changed = true;
                        } else if key == egui::Key::Space
                            && modifiers.command
                            && let Some(f) = self.focus.clone()
                        {
                            self.click(&f, Modifiers::COMMAND);
                            out.selection_changed = true;
                        } else if (key == egui::Key::Y && modifiers.command)
                            || (key == egui::Key::Space && modifiers.is_none() && !typing)
                        {
                            self.set_quick_look(true);
                            spaced |= key == egui::Key::Space;
                        }
                    }
                    egui::Event::Text(t) if !(modifiers.command || spaced && t == " ") => {
                        let now = ui.input(|i| i.time);
                        self.type_ahead(&t, now);
                    }
                    _ => {}
                }
            }
            out.place_changed |= self.place != before;
            // Keep the arrows, Tab excepted, for the entries.
            ui.memory_mut(|m| {
                m.set_focus_lock_filter(
                    keys_id,
                    egui::EventFilter {
                        horizontal_arrows: true,
                        vertical_arrows: true,
                        tab: false,
                        escape: false,
                    },
                )
            });
        }
        let hierarchy = self.mode.describe().needs.hierarchy;
        if hierarchy {
            if self.sync_columns() {
                ui.ctx().request_repaint();
            }
        } else if !self.ancestors.is_empty() || self.peek.is_some() {
            self.ancestors.clear();
            self.ancestor_listings.clear();
            self.peek = None;
        }
        let focus = self.focus_index();
        let reveal = std::mem::take(&mut self.reveal);
        self.thumbnails.begin_frame(ui.ctx());
        let inner = ui.scope(|ui| {
            self.mode.show(
                ui,
                ModeContext {
                    entries: &self.entries,
                    selection: &self.selection,
                    focus,
                    thumbnails: self.thumbnails.as_ref(),
                    settings: &mut self.settings,
                    id: self.id,
                    reveal_focus: reveal,
                    max_height: None,
                    drawer: None,
                    name: None,
                    many: false,
                    place: &self.place,
                    ancestors: &self.ancestors,
                    peek: self.peek.as_ref().map(|(c, _)| c),
                },
            )
        });
        // Quick look asks within the frame, so its thumbnail is renewed.
        if self.quick_look && self.focus_index().is_none() {
            self.quick_look = false;
        }
        if self.quick_look {
            self.show_quick_look(ui, inner.response.rect);
        }
        self.thumbnails.end_frame(ui.ctx());
        let mode_out = inner.inner;
        // The entries' container: a stop for Tab that takes the keys.
        let container = ui.interact(
            inner.response.rect,
            keys_id,
            egui::Sense::focusable_noninteractive(),
        );
        let label = format!("Entries, {}", self.mode_id);
        container.widget_info(|| egui::WidgetInfo::labeled(egui::Role::ListBox, true, &label));
        if let Some((i, mods)) = mode_out.clicked {
            let path = self.entries[i].path.clone();
            self.click(&path, mods);
            // An entry clicked at the edge, partly scrolled away, comes
            // whole into view (the filmstrip's next cell, a grid's last row).
            self.reveal = true;
            out.selection_changed = true;
            ui.memory_mut(|m| m.request_focus(keys_id));
        }
        if let Some(i) = mode_out.activated {
            out.activated = Some(self.entries[i].path.clone());
            ui.memory_mut(|m| m.request_focus(keys_id));
        }
        if let Some(r) = mode_out.reach {
            self.reach(&r);
            out.selection_changed = true;
            out.place_changed = true;
            ui.memory_mut(|m| m.request_focus(keys_id));
        }
        out
    }
}

/// What a recording [`ThumbnailProvider`] saw, for tests and agents.
#[derive(Debug, Default)]
pub struct RecordedThumbnails {
    /// Each frame's requests, in order of asking, with their priority.
    pub frames: Vec<Vec<(PathBuf, Priority)>>,
    /// Each frame's cancellations: asked the frame before, not renewed.
    pub cancelled: Vec<Vec<PathBuf>>,
}

/// A provider that makes nothing and records what it is asked, to check a
/// mode's loading order.
#[derive(Debug, Default)]
pub struct RecordingThumbnails {
    renewals: Mutex<Renewals>,
    current: Mutex<Vec<(PathBuf, Priority)>>,
    pub record: Mutex<RecordedThumbnails>,
}

impl ThumbnailProvider for RecordingThumbnails {
    fn begin_frame(&self, _ctx: &egui::Context) {
        lock(&self.renewals).begin();
        lock(&self.current).clear();
    }

    fn request(&self, _ctx: &egui::Context, r: &ThumbRequest<'_>) -> ThumbState {
        let mut cur = lock(&self.current);
        if !cur.iter().any(|(p, _)| *p == r.entry.path) {
            cur.push((r.entry.path.clone(), r.priority));
        }
        lock(&self.renewals).ask(r.entry, r.priority);
        ThumbState::Loading
    }

    fn end_frame(&self, _ctx: &egui::Context) {
        let mut renewals = lock(&self.renewals);
        let _ = renewals.wanted();
        let dropped = renewals.dropped();
        let mut rec = lock(&self.record);
        rec.frames.push(lock(&self.current).clone());
        rec.cancelled.push(dropped);
    }
}

/// Thumbnails per path, for a mode's own use.
pub type ThumbnailCache = HashMap<PathBuf, ThumbState>;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::files::fs::MemoryFs;

    fn source() -> Arc<dyn FileSource> {
        let mut fs = MemoryFs::new().dir("/p").dir("/p/sub");
        for i in 1..=12 {
            fs = fs.file(format!("/p/IMG_{i}.png"), vec![0; i]);
        }
        Arc::new(FsSource::new(Arc::new(fs)).page_size(5))
    }

    #[test]
    fn the_source_pages_and_the_browser_sorts() {
        let b = FileBrowser::new("t", source(), "/p");
        assert!(!b.loading());
        let names: Vec<_> = b.entries().iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names[0], "sub");
        assert_eq!(names[1], "IMG_1.png");
        assert_eq!(names[2], "IMG_2.png");
        assert_eq!(names.len(), 13);
    }

    #[test]
    fn a_missing_place_is_an_error() {
        let b = FileBrowser::new("t", source(), "/nope");
        assert_eq!(b.error(), Some(&SourceError::NotFound("/nope".into())));
    }

    #[test]
    fn the_registry_refuses_a_taken_id_and_gives_modes_back() {
        let m = ViewModes::default();
        assert_eq!(m.ids(), ["details", "thumbnails"]);
        assert!(
            m.register("details", Arc::new(|| Box::new(DetailsMode::default())))
                .is_err()
        );
        assert!(m.unregister("thumbnails"));
        assert_eq!(m.ids(), ["details"]);
        assert!(m.make("thumbnails").is_none());
    }

    #[test]
    fn switching_modes_keeps_selection_and_focus() {
        let mut b = FileBrowser::new("t", source(), "/p");
        b.click(Path::new("/p/IMG_2.png"), Modifiers::NONE);
        b.click(Path::new("/p/IMG_4.png"), Modifiers::SHIFT);
        let sel = b.selection().clone();
        assert_eq!(sel.len(), 3);
        assert!(b.set_mode("thumbnails"));
        assert_eq!(b.selection(), &sel);
        assert_eq!(b.focus(), Some(Path::new("/p/IMG_4.png")));
        assert!(b.set_mode("details"));
        assert_eq!(b.selection(), &sel);
        assert!(!b.set_mode("nope"));
    }

    #[test]
    fn type_ahead_jumps_resets_and_cycles() {
        let mut b = FileBrowser::new("t", source(), "/p");
        assert!(b.type_ahead("i", 0.0));
        assert_eq!(b.focus(), Some(Path::new("/p/IMG_1.png")));
        assert!(b.type_ahead("mg_1", 0.2));
        assert_eq!(b.focus(), Some(Path::new("/p/IMG_1.png")));
        // After the pause, a new prefix.
        assert!(b.type_ahead("s", 5.0));
        assert_eq!(b.focus(), Some(Path::new("/p/sub")));
        // One letter again goes round.
        assert!(b.type_ahead("i", 10.0));
        assert_eq!(b.focus(), Some(Path::new("/p/IMG_1.png")));
        assert!(b.type_ahead("i", 10.1));
        assert_eq!(b.focus(), Some(Path::new("/p/IMG_2.png")));
    }

    #[test]
    fn each_mode_steps_by_its_layout() {
        let d = DetailsMode { rows_in_view: 4 };
        assert_eq!(d.step(3, 13, ModeKey::Down), 4);
        assert_eq!(d.step(3, 13, ModeKey::PageDown), 7);
        assert_eq!(d.step(0, 13, ModeKey::Up), 0);
        let t = ThumbnailsMode {
            columns: 4,
            rows_in_view: 2,
        };
        assert_eq!(t.step(1, 13, ModeKey::Down), 5);
        assert_eq!(t.step(5, 13, ModeKey::Up), 1);
        assert_eq!(t.step(5, 13, ModeKey::Right), 6);
        // Down from the row above a short last row goes to its last entry.
        assert_eq!(t.step(10, 13, ModeKey::Down), 12);
        assert_eq!(t.step(0, 13, ModeKey::PageDown), 8);
        assert_eq!(t.step(12, 13, ModeKey::End), 12);
    }

    #[test]
    fn renewals_order_by_priority_and_drop_what_was_not_asked_again() {
        let e = |n: &str| SourceEntry {
            path: n.into(),
            name: n.into(),
            kind: EntryKind::File,
            size: None,
            modified: None,
            taken: None,
            app_id: None,
        };
        let mut r = Renewals::default();
        r.begin();
        r.ask(&e("near"), Priority::Near);
        r.ask(&e("a"), Priority::Visible);
        r.ask(&e("b"), Priority::Visible);
        let w: Vec<_> = r.wanted().into_iter().map(|(p, _)| p).collect();
        assert_eq!(w, [PathBuf::from("a"), "b".into(), "near".into()]);
        r.begin();
        r.ask(&e("b"), Priority::Visible);
        assert_eq!(r.dropped(), [PathBuf::from("a"), "near".into()]);
    }
}
