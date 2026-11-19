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
//! Built-in modes: [`DetailsMode`] (rows with columns), [`ThumbnailsMode`]
//! (a virtualised grid), [`FilmstripMode`], [`ColumnsMode`] (Finder's),
//! [`CarouselMode`] and [`AgendaMode`] (by day). Others register on a
//! [`ViewModes`] registry.

use std::collections::{BTreeSet, HashMap, HashSet};
use std::fmt;
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::SystemTime;

use egui::{AtomExt as _, Modifiers, Ui};

use egui_pop_core::icon::Icon;
use egui_pop_core::indicator::{Anchor, Indicator};
use egui_pop_core::kit::Kit;
use egui_pop_core::placeholder::SkeletonTokens;
use egui_pop_core::strings;
use egui_pop_core::theme::Tokens;
use egui_pop_core::thumbnail::{self, Thumbnail};
use egui_pop_core::thumbnail::{Band, ThumbnailGrid};
use egui_pop_core::widgets;

use super::fs::{Meta, SharedFs};
use super::groups::{FileGroup, Groupings};
use super::preview::{Previews, Shown};
use super::sort::{PickerSort, SortColumn, SortFacts, sort_by};

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
        // Cancelled before it started: the focus already moved on.
        if out.cancelled() {
            return;
        }
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
            // Each entry's facts cost a call: a listing no longer wanted
            // stops at the next one, not at the end of a page.
            if out.cancelled() {
                return;
            }
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
    /// The thumbnails' side, in points: the browser sets it from its zoom
    /// ([`ThumbZoom::side`]) before each draw.
    pub thumbnail_size: f32,
    /// The agenda's level and its collapsed groups.
    pub agenda: AgendaSettings,
}

impl Default for ModeSettings {
    fn default() -> Self {
        Self {
            thumbnail_size: 128.0,
            agenda: AgendaSettings::default(),
        }
    }
}

/// A thumbnail grid's zoom: how many thumbnails fill a row, the API's
/// side of the library grid's `Zoom` (`egui_pop_views::library::grid`,
/// which converts from and into it).
///
/// The cells share the grid's width, gaps aside, so the zoom survives a
/// resize: a grid made wider shows the same count of larger thumbnails.
/// `PerRow(1)` shows one thumbnail across. A grid too narrow for `n`
/// cells of [`ThumbnailsMode::MIN_SIZE`] shows as many as fit, and keeps
/// `n` for when it grows again. A host turns the zoom into
/// [`ModeSettings::thumbnail_size`] before a mode draws
/// ([`ThumbZoom::side`]), with the room the grid will have
/// ([`ThumbnailGrid::room`](egui_pop_core::thumbnail::ThumbnailGrid::room)).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ThumbZoom {
    /// This many thumbnails per row (at least one).
    PerRow(u32),
    /// A size in points, as an older setting kept it: turned into the
    /// count per row it gives at the grid's width on the next draw
    /// ([`ThumbZoom::from_points`]).
    Points(f32),
}

impl Default for ThumbZoom {
    fn default() -> Self {
        ThumbZoom::PerRow(ThumbZoom::DEFAULT_PER_ROW)
    }
}

impl ThumbZoom {
    /// The zoom at first: this many thumbnails per row.
    pub const DEFAULT_PER_ROW: u32 = 5;
    /// The counts per row the zoom steps through (`Ctrl` and `+` or
    /// `-`); past the last, each step out adds a quarter.
    pub const STEPS: [u32; 13] = [1, 2, 3, 4, 5, 6, 8, 10, 12, 16, 20, 24, 32];
    /// The most thumbnails per row a zoom asks for.
    pub const MOST: u32 = 64;

    /// The count per row a size in `points` gives in a grid `width`
    /// points wide with `gap` points between cells: how a size kept in
    /// points converts, once.
    pub fn from_points(points: f32, width: f32, gap: f32) -> ThumbZoom {
        let points = if points.is_finite() {
            points.max(ThumbnailsMode::MIN_SIZE)
        } else {
            ThumbnailsMode::MIN_SIZE
        };
        ThumbZoom::PerRow(fitting(points, width, gap))
    }

    /// How many thumbnails of at least [`ThumbnailsMode::MIN_SIZE`] fit
    /// in a row `width` points wide with `gap` points between cells (at
    /// least one).
    pub fn most_per_row(width: f32, gap: f32) -> u32 {
        fitting(ThumbnailsMode::MIN_SIZE, width, gap)
    }

    /// The count per row, unless it is still a size in points.
    pub fn per_row(self) -> Option<u32> {
        match self {
            ThumbZoom::PerRow(n) => Some(n.clamp(1, Self::MOST)),
            ThumbZoom::Points(_) => None,
        }
    }

    /// The zoom as a count per row, a size in points converted at
    /// `width`; the count itself, even past what fits.
    #[must_use]
    pub fn resolved(self, width: f32, gap: f32) -> ThumbZoom {
        match self {
            ThumbZoom::PerRow(n) => ThumbZoom::PerRow(n.clamp(1, Self::MOST)),
            ThumbZoom::Points(p) => ThumbZoom::from_points(p, width, gap),
        }
    }

    /// The count a grid `width` points wide shows: the zoom's, or as many
    /// as fit.
    pub fn shown_per_row(self, width: f32, gap: f32) -> u32 {
        let n = self.resolved(width, gap).per_row().unwrap_or(1);
        n.min(Self::most_per_row(width, gap))
    }

    /// The thumbnails' side, in whole points, for the zoom in a grid
    /// `width` points wide with `gap` points between cells: what a host
    /// sets [`ModeSettings::thumbnail_size`] to.
    pub fn side(self, width: f32, gap: f32) -> f32 {
        let n = self.shown_per_row(width, gap) as f32;
        ((width + gap) / n - gap).floor().max(1.0)
    }

    /// One step closer: fewer, larger thumbnails per row (down to one).
    /// A size in points steps from the count it gives at `width`.
    #[must_use]
    pub fn zoomed_in(self, width: f32, gap: f32) -> ThumbZoom {
        let n = self.shown_per_row(width, gap);
        ThumbZoom::PerRow(
            Self::STEPS
                .iter()
                .rev()
                .copied()
                .find(|&s| s < n)
                .unwrap_or(1),
        )
    }

    /// One step out: more, smaller thumbnails per row, while they fit.
    #[must_use]
    pub fn zoomed_out(self, width: f32, gap: f32) -> ThumbZoom {
        let n = self.shown_per_row(width, gap);
        let next = Self::STEPS
            .iter()
            .copied()
            .find(|&s| s > n)
            .unwrap_or_else(|| (n as f32 * 1.25).ceil() as u32);
        ThumbZoom::PerRow(
            next.min(Self::most_per_row(width, gap))
                .clamp(1, Self::MOST),
        )
    }
}

/// How many cells `size` points wide fit in `width` with `gap` between
/// them (at least one): the grid's own rule.
fn fitting(size: f32, width: f32, gap: f32) -> u32 {
    let n = ((width + gap) / (size + gap)).floor();
    if n.is_finite() && n >= 1.0 {
        (n as u32).min(ThumbZoom::MOST)
    } else {
        1
    }
}

/// What an application says of an entry beyond its facts
/// ([`FileBrowser::with_marks`]): short badges drawn as text on its cell
/// and after its name; chips (an icon or a short text in a corner of its
/// thumbnail, [`MarkChip`]); both said with its name to assistive
/// technology; and whether it is dimmed (shown, but not what the person
/// acts on).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct EntryMarks {
    /// Texts stacked from the thumbnail's bottom left corner.
    pub badges: Vec<String>,
    /// Icons and short texts in the thumbnail's corners.
    pub chips: Vec<MarkChip>,
    pub dimmed: bool,
}

impl EntryMarks {
    /// The entry's name with its badges and its chips' names, as a screen
    /// reader says it.
    pub fn label(&self, name: &str) -> String {
        let mut label = name.to_owned();
        let mut said: Vec<&str> = Vec::new();
        for b in self.badges.iter().chain(self.chips.iter().map(|c| &c.name)) {
            // Chips of one kind (a group's, one per member) share a name.
            if said.contains(&b.as_str()) {
                continue;
            }
            said.push(b);
            label.push_str(" · ");
            label.push_str(b);
        }
        label
    }

    /// Whether there is nothing to say of the entry beyond its name.
    fn is_plain(&self) -> bool {
        self.badges.is_empty() && self.chips.is_empty()
    }
}

/// The corner of a thumbnail a [`MarkChip`] sits in. Chips in one corner
/// line up from it towards the middle, in their order.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum MarkCorner {
    TopLeft,
    /// Where states go (in the library, offline, backed up).
    #[default]
    TopRight,
    /// Where facts of the content go (a file type, a pair).
    BottomRight,
}

/// A mark in a corner of an entry's thumbnail: an icon ("in your
/// Library" as a check) or a short text ("R+J" for a RAW and JPEG pair),
/// on a pill of its tone. Its name is said after the entry's name and is
/// its tooltip.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MarkChip {
    /// An icon of the icon registry (`phosphor:check-circle`), drawn
    /// instead of the text.
    pub icon: Option<String>,
    /// The short text drawn without an icon.
    pub text: String,
    /// What it means, in words: its accessible name and its tooltip.
    pub name: String,
    pub tone: egui_pop_core::kit::Tone,
    pub corner: MarkCorner,
}

impl MarkChip {
    /// An icon named `name`, top right.
    pub fn icon(icon: impl Into<String>, name: impl Into<String>) -> MarkChip {
        MarkChip {
            icon: Some(icon.into()),
            text: String::new(),
            name: name.into(),
            tone: egui_pop_core::kit::Tone::Neutral,
            corner: MarkCorner::TopRight,
        }
    }

    /// A short text named `name` (`"R+J"`, "RAW+JPG"), bottom right.
    pub fn text(text: impl Into<String>, name: impl Into<String>) -> MarkChip {
        MarkChip {
            icon: None,
            text: text.into(),
            name: name.into(),
            tone: egui_pop_core::kit::Tone::Neutral,
            corner: MarkCorner::BottomRight,
        }
    }

    #[must_use]
    pub fn tone(mut self, tone: egui_pop_core::kit::Tone) -> MarkChip {
        self.tone = tone;
        self
    }

    #[must_use]
    pub fn corner(mut self, corner: MarkCorner) -> MarkChip {
        self.corner = corner;
        self
    }
}

/// What gives an entry's marks ([`FileBrowser::with_marks`]).
pub type MarkEntries = dyn Fn(&SourceEntry) -> EntryMarks + Send + Sync;

/// What an agenda groups its entries by.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum AgendaLevel {
    #[default]
    Day,
    Month,
    Year,
}

impl AgendaLevel {
    /// The three, finest first.
    pub const ALL: [AgendaLevel; 3] = [AgendaLevel::Day, AgendaLevel::Month, AgendaLevel::Year];

    /// One step coarser (a day to a month); the year stays.
    pub fn coarser(self) -> Self {
        match self {
            AgendaLevel::Day => AgendaLevel::Month,
            _ => AgendaLevel::Year,
        }
    }

    /// One step finer (a year to a month); the day stays.
    pub fn finer(self) -> Self {
        match self {
            AgendaLevel::Year => AgendaLevel::Month,
            _ => AgendaLevel::Day,
        }
    }

    /// The group a day (since 1970-01-01) falls in at this level: the day,
    /// the month (`year * 12 + month - 1`) or the year.
    pub fn group(self, day: i64) -> i64 {
        match self {
            AgendaLevel::Day => day,
            AgendaLevel::Month => {
                let (y, m, _) = civil(day);
                y * 12 + i64::from(m) - 1
            }
            AgendaLevel::Year => civil(day).0,
        }
    }

    /// A group's heading: `2025-11-03`, `2025-11` or `2025`.
    pub fn heading(self, group: i64) -> String {
        match self {
            AgendaLevel::Day => {
                let (y, m, d) = civil(group);
                format!("{y:04}-{m:02}-{d:02}")
            }
            AgendaLevel::Month => {
                format!(
                    "{:04}-{:02}",
                    group.div_euclid(12),
                    group.rem_euclid(12) + 1
                )
            }
            AgendaLevel::Year => format!("{group:04}"),
        }
    }

    fn label(self) -> &'static strings::StrKey {
        match self {
            AgendaLevel::Day => &crate::strings::FILES_AGENDA_DAY,
            AgendaLevel::Month => &crate::strings::FILES_AGENDA_MONTH,
            AgendaLevel::Year => &crate::strings::FILES_AGENDA_YEAR,
        }
    }
}

/// The agenda's settings: its level, and the groups collapsed at that level
/// (`None`: the entries without a time). A change of level expands them all.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AgendaSettings {
    pub level: AgendaLevel,
    pub collapsed: BTreeSet<Option<i64>>,
}

impl AgendaSettings {
    /// Groups by `level`, every group expanded; `false` when it was already.
    pub fn set_level(&mut self, level: AgendaLevel) -> bool {
        if self.level == level {
            return false;
        }
        self.level = level;
        self.collapsed.clear();
        true
    }

    /// Collapses or expands one group.
    pub fn set_collapsed(&mut self, group: Option<i64>, collapsed: bool) -> bool {
        if collapsed {
            self.collapsed.insert(group)
        } else {
            self.collapsed.remove(&group)
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
    /// The place's listing is still under way: more entries may come.
    pub loading: bool,
    /// Changes whenever the entries do (a page arrived, a refresh): what a
    /// mode computes from the entries alone is kept until it changes.
    pub generation: u64,
    /// The application's marks of each entry, drawn by [`PlainDrawer`].
    pub marks: Option<&'a MarkEntries>,
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
    /// The entry under the pointer and its rectangle: where a drop goes.
    pub pointed: Option<(usize, egui::Rect)>,
    /// A folder under the pointer in a column beside the place's (an
    /// ancestor's, the focused folder's), and its rectangle.
    pub pointed_beside: Option<(PathBuf, egui::Rect)>,
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

    /// Draws the mode's own controls in the browser's toolbar, after the
    /// switcher (the agenda's level). `true` when a setting changed.
    fn toolbar(&mut self, _ui: &mut Ui, _settings: &mut ModeSettings) -> bool {
        false
    }

    /// A key of the mode's own, beyond the arrows (`Ctrl+Up` in the agenda),
    /// pressed while the entry `focus` has the focus. `true` when taken: the
    /// browser then brings the focus into view and does nothing else with it.
    fn own_key(
        &mut self,
        _key: egui::Key,
        _modifiers: Modifiers,
        _focus: Option<usize>,
        _settings: &mut ModeSettings,
    ) -> bool {
        false
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

    /// Every built-in mode: details, thumbnails, filmstrip, columns,
    /// carousel, agenda.
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
            .register(
                MODE_CAROUSEL,
                Arc::new(|| Box::new(CarouselMode::default())),
            )
            .expect("built-in");
        modes
            .register(MODE_AGENDA, Arc::new(|| Box::new(AgendaMode::default())))
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
    // While something is dragged too: where a drop would go.
    if resp.contains_pointer() {
        out.pointed = Some((i, resp.rect));
    }
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
    /// The application's marks of each entry: badges and dimming.
    pub marks: Option<&'a MarkEntries>,
}

impl PlainDrawer<'_> {
    /// Below this width, a row shows its name alone, without its facts.
    pub const FACTS_WIDTH: f32 = 320.0;

    /// The application's marks of entry `n`.
    fn marks_of(&self, n: usize) -> EntryMarks {
        self.marks.map(|m| m(&self.entries[n])).unwrap_or_default()
    }

    /// Whether every entry is a folder: the columns then leave out the size
    /// and the type, which say nothing of folders.
    fn folders_only(&self) -> bool {
        self.entries.iter().all(SourceEntry::is_dir)
    }

    /// The ring the focus has, apart from the selection's fill.
    fn ring(&self, ui: &Ui, r: &egui::Response, n: usize) {
        if self.focus != Some(n) {
            return;
        }
        egui_pop_core::focus::own_ring(ui.ctx(), r.id);
        let t = Tokens::of(ui.ctx());
        ui.painter().rect_stroke(
            r.rect,
            t.radius.widget,
            egui::Stroke::new(1.0, t.focus_ring),
            egui::StrokeKind::Inside,
        );
    }
}

/// An entry as its row and its cell show it: a folder's name ends with a
/// separator.
fn shown_name(e: &SourceEntry) -> String {
    if e.is_dir() {
        format!("{}/", e.name)
    } else {
        e.name.clone()
    }
}

/// What a screen reader is told of an entry beyond its name and its marks
/// ([`EntryMarks::label`]): its facts.
fn entry_said(facts: &[String]) -> String {
    facts.join(", ")
}

/// Marks an entry's widget as an option of its list, selected or not.
fn as_option(ui: &Ui, r: &egui::Response, said: &str) {
    let said = said.to_owned();
    ui.ctx().accesskit_node_builder(r.id, |node| {
        node.set_role(egui::accesskit::Role::ListBoxOption);
        if !said.is_empty() {
            node.set_description(said);
        }
    });
}

/// A mark of an entry as an indicator: a chip's icon or text, a badge's
/// text, toned as asked, its tip its name, under an id agents read
/// (`files.badge:<path>:<text>`).
fn mark_indicator(path: &Path, chip: &MarkChip) -> Indicator {
    let base = match &chip.icon {
        Some(icon) => Indicator::icon(icon.as_str()),
        None => Indicator::text(chip.text.as_str()),
    };
    let said = if chip.icon.is_some() {
        chip.name.as_str()
    } else {
        chip.text.as_str()
    };
    base.tone(chip.tone)
        .overhang(false)
        .id(format!("files.badge:{}:{said}", path.display()))
        .tooltip(chip.name.clone())
}

/// An entry's marks as chips, in the order they are drawn: its badges (as
/// neutral text chips), then its chips.
fn mark_chips(marks: &EntryMarks) -> Vec<MarkChip> {
    marks
        .badges
        .iter()
        .map(|b| MarkChip::text(b.clone(), b.clone()))
        .chain(marks.chips.iter().cloned())
        .collect()
}

/// Paints chips from the end of `host`, the last at the end, as many as fit
/// in `room`. They take no click: the entry under them does.
fn paint_mark_row(ui: &mut Ui, path: &Path, host: egui::Rect, anchor: Anchor, chips: &[MarkChip]) {
    let gap = Tokens::of(ui.ctx()).space.xs;
    let mut host = host;
    for chip in chips.iter().rev() {
        let mark = mark_indicator(path, chip);
        let width = mark.measure(ui).x;
        if width > host.width() {
            break;
        }
        mark.paint_at(ui, host, anchor);
        host.set_right(host.right() - width - gap);
    }
}

/// How wide these chips are, side by side.
fn marks_width(ui: &Ui, path: &Path, chips: &[MarkChip]) -> f32 {
    let gap = Tokens::of(ui.ctx()).space.xs;
    chips
        .iter()
        .map(|c| mark_indicator(path, c).measure(ui).x + gap)
        .sum()
}

impl EntryDrawer for PlainDrawer<'_> {
    fn row_size(&self, ui: &Ui, width: f32) -> egui::Vec2 {
        egui::vec2(width, row_height(ui))
    }

    fn row(&mut self, ui: &mut Ui, n: usize, size: egui::Vec2) -> egui::Response {
        let e = &self.entries[n];
        let marks = self.marks_of(n);
        let selected = self.selection.contains(&e.path);
        // A narrow row (a column of Finder's view) shows the name alone.
        if size.x < Self::FACTS_WIDTH {
            let (rect, resp) = ui.allocate_exact_size(size, egui::Sense::click());
            let visuals = ui.visuals();
            if selected {
                ui.painter()
                    .rect_filled(rect, 2.0, visuals.selection.bg_fill);
            } else if resp.hovered() {
                ui.painter()
                    .rect_filled(rect, 2.0, visuals.widgets.hovered.weak_bg_fill);
            }
            let text = if selected {
                visuals.selection.stroke.color
            } else if marks.dimmed {
                visuals.weak_text_color()
            } else {
                visuals.text_color()
            };
            paint_row_name(ui, rect, e, text);
            let name = marks.label(&e.name);
            resp.widget_info(|| {
                egui::WidgetInfo::selected(egui::Role::ListBoxOption, true, selected, &name)
            });
            as_option(ui, &resp, "");
            self.ring(ui, &resp, n);
            return resp;
        }
        let t = Tokens::of(ui.ctx());
        let label = shown_name(e);
        let columns = list_columns(ui, size.x, self.folders_only());
        let height = row_height(ui) - 2.0 * ui.spacing().button_padding.y;
        let name_column = columns.first().map_or(0.0, |c| c.width);
        // The marks keep to the end of the name's column, and at most half
        // of it.
        let chips = mark_chips(&marks);
        let marks_room = marks_width(ui, &e.path, &chips).min(name_column / 2.0);
        let reserved = if chips.is_empty() {
            0.0
        } else {
            marks_room + t.space.m
        };
        let mut atoms = egui::Atoms::default();
        let mut facts = Vec::new();
        let weak = marks.dimmed && !selected;
        for c in &columns {
            let (text, align) = match c.column {
                SortColumn::Name => {
                    let width = (c.width - reserved).max(0.0);
                    let text = if weak {
                        egui::RichText::new(label.clone()).weak()
                    } else {
                        egui::RichText::new(label.clone())
                    };
                    atoms.push_right(
                        text.atom_max_width(width)
                            .atom_size(egui::vec2(width, height))
                            .atom_align(egui::Align2::LEFT_CENTER),
                    );
                    if !chips.is_empty() {
                        // In the place of the marks, painted over it: the
                        // columns after it stay under their headers.
                        atoms.push_right(String::new().atom_size(egui::vec2(marks_room, height)));
                    }
                    continue;
                }
                column => {
                    let fact = fact_text(ui.ctx(), e, column);
                    if !fact.is_empty() {
                        facts.push(fact.clone());
                    }
                    let align = if column == SortColumn::Size {
                        egui::Align2::RIGHT_CENTER
                    } else {
                        egui::Align2::LEFT_CENTER
                    };
                    (fact, align)
                }
            };
            atoms.push_right(
                text.atom_max_width(c.width)
                    .atom_size(egui::vec2(c.width, height))
                    .atom_align(align),
            );
        }
        let font = egui::TextStyle::Body.resolve(ui.style());
        let cut = ui
            .painter()
            .layout_no_wrap(label.clone(), font, egui::Color32::PLACEHOLDER)
            .size()
            .x
            > name_column - reserved;
        // By its path: the focus stays on it as the rows scroll.
        let r = ui
            .push_id(&e.path, |ui| {
                ui.spacing_mut().icon_spacing = t.space.m;
                ui.with_layout(egui::Layout::top_down_justified(egui::Align::Min), |ui| {
                    ui.add(widgets::SelectableRow::new(selected, atoms).name(marks.label(&e.name)))
                })
                .inner
            })
            .inner;
        as_option(ui, &r, &entry_said(&facts));
        if let Some(c) = columns.first().filter(|_| !chips.is_empty()) {
            let left = r.rect.left() + c.left;
            let host = egui::Rect::from_x_y_ranges(left..=left + c.width, r.rect.y_range());
            paint_mark_row(ui, &e.path, host, Anchor::End, &chips);
        }
        self.ring(ui, &r, n);
        if cut { r.on_hover_text(label) } else { r }
    }

    fn cell_size(&self, ui: &Ui, side: f32) -> egui::Vec2 {
        egui::vec2(side, side + thumbnail::caption_height(ui))
    }

    fn cell(
        &mut self,
        ui: &mut Ui,
        n: usize,
        size: egui::Vec2,
        thumbnail: ThumbState,
    ) -> egui::Response {
        let e = &self.entries[n];
        let marks = self.marks_of(n);
        let selected = self.selection.contains(&e.path);
        let mut cell = Thumbnail::new(marks.label(&e.name))
            .caption(e.name.as_str())
            .size(size)
            .selected(selected);
        let mut failed = None;
        cell = match thumbnail {
            _ if e.is_dir() => cell.icon(Icon::Folder),
            ThumbState::Ready(t) => {
                let [w, h] = t.size();
                cell.image(t.id(), [w as u32, h as u32])
            }
            ThumbState::Loading => cell.placeholder(SkeletonTokens::of(ui.ctx()).fill),
            ThumbState::None => cell.icon(Icon::File),
            ThumbState::Failed(why) => {
                failed = Some(why);
                cell.icon(Icon::Warning)
            }
        };
        // By its path: the focus stays on it as the rows scroll.
        let r = ui.push_id(&e.path, |ui| ui.add(cell)).inner;
        as_option(ui, &r, "");
        if marks.dimmed {
            let t = Tokens::of(ui.ctx());
            ui.painter().rect_filled(
                r.rect,
                t.radius.widget,
                ui.visuals().extreme_bg_color.gamma_multiply(0.6),
            );
        }
        let host = r.rect.shrink(Tokens::of(ui.ctx()).space.xs);
        let chips = mark_chips(&marks);
        if !chips.is_empty() {
            paint_mark_row(ui, &e.path, host, Anchor::TopEnd, &chips);
        }
        self.ring(ui, &r, n);
        match failed {
            Some(why) => r.on_hover_text(why),
            None => r,
        }
    }
}

// ---------------------------------------------------------------------------
// Rows with columns
// ---------------------------------------------------------------------------

/// A column of the rows, as it fits the width it has.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct ListColumn {
    pub(crate) column: SortColumn,
    /// Where its text starts, from the row's left.
    pub(crate) left: f32,
    pub(crate) width: f32,
}

/// A row's height, and its headers': a selectable row's.
pub(crate) fn row_height(ui: &Ui) -> f32 {
    let text = ui.text_style_height(&egui::TextStyle::Body);
    ui.spacing()
        .interact_size
        .y
        .max(text + 2.0 * ui.spacing().button_padding.y)
}

/// Lays columns out in a row `room` wide: `wanted` holds the width of each but
/// the name, in the order they are given up, the last first, while the name has
/// less than [`super::NAME_MIN_WIDTH`]. Their order on screen is the name, then
/// `wanted`'s.
pub(crate) fn fit_columns(
    room: f32,
    gap: f32,
    padding: f32,
    wanted: &[(SortColumn, f32)],
) -> Vec<ListColumn> {
    let mut kept = wanted.len();
    let name_width = loop {
        let others: f32 = wanted[..kept].iter().map(|(_, w)| w + gap).sum();
        let name = room - 2.0 * padding - others;
        if name >= super::NAME_MIN_WIDTH || kept == 0 {
            break name.max(0.0);
        }
        kept -= 1;
    };
    let mut out = vec![ListColumn {
        column: SortColumn::Name,
        left: padding,
        width: name_width,
    }];
    let mut left = padding + name_width + gap;
    for &(column, width) in &wanted[..kept] {
        out.push(ListColumn {
            column,
            left,
            width,
        });
        left += width + gap;
    }
    out
}

/// The columns of rows `width` wide: wide enough for their title with its
/// arrow, and for what they say. Folders alone have no size and all one type.
pub(crate) fn list_columns(ui: &Ui, width: f32, folders_only: bool) -> Vec<ListColumn> {
    let ctx = ui.ctx();
    let font = egui::TextStyle::Body.resolve(ui.style());
    let measure = |text: String| {
        ui.painter()
            .layout_no_wrap(text, font.clone(), egui::Color32::PLACEHOLDER)
            .size()
    };
    let inset = header_inset(ui, &Kit::of(ctx), &font);
    let wanted = |column: SortColumn, sample: &str| {
        let title = measure(strings::tr(ctx, column.title()));
        // The arrow goes after the title, a little apart, as high as the text.
        let header = title.x + title.y * 1.25 + inset;
        (column, measure(sample.to_string()).x.max(header).ceil())
    };
    let folder = strings::tr(ctx, &crate::strings::FILES_KIND_FOLDER);
    let mut columns = vec![wanted(SortColumn::Modified, "0000-00-00 00:00 UTC")];
    if !folders_only {
        columns.push(wanted(SortColumn::Size, "999 MB"));
        columns.push(wanted(SortColumn::Type, &folder));
    }
    let gap = Tokens::of(ctx).space.m;
    fit_columns(width, gap, ui.spacing().button_padding.x, &columns)
}

/// How far a header's title starts from the left of its cell.
fn header_inset(ui: &Ui, kit: &Kit, font: &egui::FontId) -> f32 {
    let galley =
        ui.painter()
            .layout_no_wrap("A".to_string(), font.clone(), egui::Color32::PLACEHOLDER);
    let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(100.0, 40.0));
    let part = egui_pop_core::kit::Part {
        rect,
        room: rect,
        state: egui_pop_core::kit::State::REST,
        label: Some(&galley),
    };
    kit.table_header_cell
        .label_rect(&part, galley.size())
        .left()
}

/// What a column says of an entry: empty for a folder's size, or a fact
/// the source does not give.
fn fact_text(ctx: &egui::Context, e: &SourceEntry, column: SortColumn) -> String {
    match column {
        SortColumn::Name => e.name.clone(),
        SortColumn::Size if e.is_dir() => String::new(),
        SortColumn::Size => e.size.map_or_else(String::new, super::preview::size_text),
        SortColumn::Modified => e
            .modified
            .map_or_else(String::new, super::preview::time_text),
        SortColumn::Taken => e
            .taken
            .or(e.modified)
            .map_or_else(String::new, super::preview::time_text),
        SortColumn::Type if e.is_dir() => strings::tr(ctx, &crate::strings::FILES_KIND_FOLDER),
        SortColumn::Type => match e.name.rfind('.').filter(|&dot| dot > 0) {
            Some(dot) => e.name[dot + 1..].to_uppercase(),
            None => strings::tr(ctx, &crate::strings::FILES_KIND_FILE),
        },
    }
}

/// The headers of rows with `columns`, `width` wide: each a button that sorts
/// by its column, the one sorted by with its arrow (a click turns it). With
/// `ranked` the order is a filter's ranking: they are disabled, and say why.
/// Gives the order clicked.
pub(crate) fn sort_headers(
    ui: &mut Ui,
    columns: &[ListColumn],
    width: f32,
    sort: PickerSort,
    ranked: bool,
) -> Option<PickerSort> {
    let ctx = ui.ctx().clone();
    let height = row_height(ui);
    let (rect, _) = ui.allocate_exact_size(egui::vec2(width, height), egui::Sense::hover());
    let kit = Kit::of(&ctx);
    let font = egui::TextStyle::Body.resolve(ui.style());
    let inset = header_inset(ui, &kit, &font);
    let mut clicked = None;
    ui.add_enabled_ui(!ranked, |ui| {
        for (n, c) in columns.iter().enumerate() {
            let left = (rect.left() + c.left - inset).max(rect.left());
            let right = columns
                .get(n + 1)
                .map_or(rect.right(), |next| rect.left() + next.left - inset);
            let at = egui::Rect::from_min_max(
                egui::pos2(left, rect.top()),
                egui::pos2(right, rect.bottom()),
            );
            let title = strings::tr(&ctx, c.column.title());
            let descending = (!ranked && sort.column == c.column).then_some(sort.descending);
            let name = match descending {
                Some(true) => strings::tr_args(
                    &ctx,
                    &crate::strings::FILES_SORTED_DOWN,
                    &[("column", &title)],
                ),
                Some(false) => strings::tr_args(
                    &ctx,
                    &crate::strings::FILES_SORTED_UP,
                    &[("column", &title)],
                ),
                None => title.clone(),
            };
            let id = ui
                .scope_id()
                .with("egui_pop.files.header")
                .with(c.column.as_str());
            let r = widgets::SortHeader {
                title: &title,
                name: &name,
                descending,
                sortable: true,
            }
            .show(ui, &kit, at, id)
            .on_disabled_hover_text(strings::tr(&ctx, &crate::strings::FILES_SORT_RANKED));
            if r.clicked() {
                clicked = Some(sort.click(c.column));
            }
        }
    });
    clicked
}

/// A cell's marks over its thumbnail: dimmed, its badges stacked from the
/// image's bottom left corner, its chips in their corners. Gives where
/// each chip was drawn, in their order.
pub fn paint_marks(ui: &Ui, rect: egui::Rect, marks: &EntryMarks) -> Vec<egui::Rect> {
    if !marks.dimmed && marks.is_plain() {
        return Vec::new();
    }
    let visuals = ui.visuals();
    let caption = ui.text_style_height(&egui::TextStyle::Small) + 4.0;
    let side = (rect.width() - 8.0)
        .min(rect.height() - caption - 8.0)
        .max(1.0);
    let image = egui::Rect::from_min_size(rect.min + egui::vec2(4.0, 4.0), egui::vec2(side, side));
    let painter = ui.painter().with_clip_rect(rect);
    if marks.dimmed {
        painter.rect_filled(image, 4.0, visuals.extreme_bg_color.gamma_multiply(0.6));
    }
    let font = egui::TextStyle::Small.resolve(ui.style());
    let mut bottom = image.bottom() - 4.0;
    for badge in &marks.badges {
        let galley =
            painter.layout_no_wrap(badge.clone(), font.clone(), visuals.strong_text_color());
        let at = egui::pos2(image.left() + 4.0, bottom - galley.size().y);
        painter.rect_filled(
            egui::Rect::from_min_size(at, galley.size()).expand(2.0),
            2.0,
            visuals.widgets.inactive.bg_fill,
        );
        bottom = at.y - 4.0;
        painter.galley(at, galley, visuals.strong_text_color());
    }
    paint_chips(&painter, image, &marks.chips, font)
}

/// The chips over a thumbnail's square `image`: pills of their tone, an
/// icon or a short text on each, lined up from their corner.
fn paint_chips(
    painter: &egui::Painter,
    image: egui::Rect,
    chips: &[MarkChip],
    font: egui::FontId,
) -> Vec<egui::Rect> {
    use egui_pop_core::kit::{Kit, KitCx, Part, State};
    if chips.is_empty() {
        return Vec::new();
    }
    let kit = Kit::of(painter.ctx());
    let cx = KitCx {
        painter,
        ppp: painter.ctx().pixels_per_point(),
    };
    let glyph = font.size;
    let pad = 4.0;
    // How far each corner's line has gone.
    let mut ends = [0.0f32; 3];
    let mut rects = Vec::with_capacity(chips.len());
    for chip in chips {
        let state = State::REST.toned(chip.tone);
        let galley = chip.icon.is_none().then(|| {
            painter.layout_no_wrap(chip.text.clone(), font.clone(), egui::Color32::PLACEHOLDER)
        });
        let inner = galley
            .as_ref()
            .map_or(egui::vec2(glyph, glyph), |g| g.size());
        let size = kit.pill.measure(inner);
        let (slot, anchor, at) = match chip.corner {
            MarkCorner::TopLeft => (
                0,
                egui::Align2::LEFT_TOP,
                image.left_top() + egui::vec2(pad, pad),
            ),
            MarkCorner::TopRight => (
                1,
                egui::Align2::RIGHT_TOP,
                image.right_top() + egui::vec2(-pad, pad),
            ),
            MarkCorner::BottomRight => (
                2,
                egui::Align2::RIGHT_BOTTOM,
                image.right_bottom() - egui::vec2(pad, pad),
            ),
        };
        let shift = if chip.corner == MarkCorner::TopLeft {
            ends[slot]
        } else {
            -ends[slot]
        };
        let r = anchor.anchor_size(at + egui::vec2(shift, 0.0), size);
        ends[slot] += size.x + 3.0;
        let part = Part {
            rect: r,
            room: r,
            state,
            label: galley.as_ref(),
        };
        kit.pill.paint_part(&cx, &part);
        if let Some(icon) = &chip.icon {
            let colour = kit.pill.style(state).text;
            egui_pop_core::icon::registry::resolve(icon, "files.marks")
                .unwrap_or(egui_pop_core::icon::PLACEHOLDER)
                .paint(
                    painter,
                    egui::Rect::from_center_size(r.center(), egui::vec2(glyph, glyph)),
                    colour,
                );
        }
        rects.push(r);
    }
    rects
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
/// its entries out. Gives the entries in a row. With `band`, a drag from
/// the room around the cells draws a rubber band, told to the browser
/// under that key ([`band_key`]).
#[allow(clippy::too_many_arguments)]
fn lay_out(
    ui: &mut Ui,
    band: Option<egui::Id>,
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
    let mut grid = ThumbnailGrid::new(count, cell)
        .id_salt(salt)
        .reveal(reveal)
        .rubber_band(band.is_some());
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
        let shown = grid.clone().show_with_band(ui, |ui, n| {
            let r = draw(&mut *drawer, ui, n);
            if focus == Some(n) {
                out.focused = Some(r.id);
            }
            report(&r, n, out);
            first = first.min(n);
            end = end.max(n + 1);
        });
        columns = shown.per_row;
        if let (Some(key), Some(told)) = (band, shown.band) {
            ui.ctx().data_mut(|d| d.insert_temp(key, told));
        }
    };
    in_container(ui, container, out, &mut add);
    out.visible = if first == usize::MAX {
        0..0
    } else {
        first..end
    };
    columns.max(1)
}

/// Takes the rubber band a mode drew this frame under `id` (its
/// [`ModeContext::id`]): for a host that draws the modes itself (the in-app
/// picker).
pub(crate) fn take_band(ctx: &egui::Context, id: egui::Id) -> Option<Band> {
    ctx.data_mut(|d| {
        let key = band_key(id);
        let band = d.get_temp::<Band>(key);
        d.remove::<Band>(key);
        band
    })
}

/// Where a mode's rubber band is told to the browser, for the browser's
/// `id` ([`ModeContext::id`]).
fn band_key(id: egui::Id) -> egui::Id {
    id.with("egui_pop.file_browser.band")
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
            marks: cx.marks,
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
            None,
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
    /// The smallest thumbnails: a zoom shows no more per row than fit.
    pub const MIN_SIZE: f32 = 64.0;
    /// The largest thumbnails of the modes drawn at a size (the agenda);
    /// the grid's follow its zoom ([`ThumbZoom`]), up to one across.
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
        // As large as the zoom makes it: one thumbnail across at most.
        let side = cx.settings.thumbnail_size.max(Self::MIN_SIZE);
        let mut plain = PlainDrawer {
            entries: cx.entries,
            selection: cx.selection,
            focus: cx.focus,
            marks: cx.marks,
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
            Some(band_key(cx.id)),
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

    /// One column of plain rows: an ancestor's, or the focused folder's,
    /// its listing's busy row last while it is under way. Answers the entry
    /// clicked; the folder under the pointer goes to `out`.
    fn side(
        ui: &mut Ui,
        column: &Column,
        height: f32,
        salt: egui::Id,
        out: &mut ModeOutput,
    ) -> Option<PathBuf> {
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
                        column.entries.len() + usize::from(column.loading),
                        |ui, range| {
                            for i in range {
                                let Some(e) = column.entries.get(i) else {
                                    busy_row(ui);
                                    continue;
                                };
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
                                if resp.contains_pointer() && e.is_dir() {
                                    out.pointed_beside = Some((e.path.clone(), rect));
                                }
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

/// The row a column ends with while its listing is under way: a spinner
/// and "Listing…", said to assistive technology.
fn busy_row(ui: &mut Ui) {
    let text = strings::tr(ui.ctx(), &crate::strings::FILES_LISTING);
    let (rect, resp) = ui.allocate_exact_size(
        egui::vec2(ui.available_width(), DetailsMode::ROW_HEIGHT),
        egui::Sense::hover(),
    );
    let name = text.clone();
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::ProgressIndicator, true, &name));
    let side = DetailsMode::ROW_HEIGHT - 6.0;
    let spinner = egui::Rect::from_center_size(
        egui::pos2(rect.left() + 4.0 + side / 2.0, rect.center().y),
        egui::vec2(side, side),
    );
    egui::Spinner::new().size(side).paint_at(ui, spinner);
    ui.painter().text(
        egui::pos2(spinner.right() + 6.0, rect.center().y),
        egui::Align2::LEFT_CENTER,
        text,
        egui::TextStyle::Body.resolve(ui.style()),
        ui.visuals().weak_text_color(),
    );
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
            marks: cx.marks,
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
        // Room for the busy row under the place's entries.
        let busy = if cx.loading {
            DetailsMode::ROW_HEIGHT
        } else {
            0.0
        };
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
                            if let Some(p) =
                                Self::side(ui, a, height, cx.id.with(("ancestor", k)), &mut out)
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
                            None,
                            cx.name.as_deref().map(|n| (n, cx.many)),
                            &mut *drawer,
                            cx.id.with("columns.here"),
                            n,
                            size,
                            Some(height - busy),
                            reveal,
                            cx.focus,
                            &mut out,
                            |d, ui, i| d.row(ui, i, size),
                        );
                        // The place's listing streams in: more may come.
                        if cx.loading {
                            busy_row(ui);
                        }
                    });
                    if let Some(f) = focused {
                        ui.separator();
                        column(ui, &mut |ui| match cx.peek {
                            Some(p) if f.is_dir() => {
                                if let Some(e) =
                                    Self::side(ui, p, height, cx.id.with("peek"), &mut out)
                                {
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
            marks: cx.marks,
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

/// The focused entry large in the middle, at the thumbnail size (smaller
/// when the room is short), its neighbours smaller and smaller on each
/// side (Finder's Gallery), spaced by their cells' own widths. Left and Right bring the
/// previous or next entry to the middle; a click on a neighbour does too.
/// The middle one asks for its thumbnail first, then the neighbours
/// outwards, then the next one on each side (`Near`).
#[derive(Clone, Debug, Default)]
pub struct CarouselMode {
    per_side: usize,
}

impl CarouselMode {
    /// Each neighbour's side, against the one nearer the middle.
    pub const SHRINK: f32 = 0.55;
    /// The smallest neighbour drawn, in points.
    pub const MIN_SIDE: f32 = 24.0;
    /// The middle one's most width, against the room: what leaves room
    /// for neighbours.
    pub const MIDDLE: f32 = 0.3;

    /// The middle one's side: `wanted` (the thumbnail size), made smaller
    /// until the drawer's cell for it fits in `room`, and never under
    /// [`Self::MIN_SIDE`].
    fn fit(drawer: &dyn EntryDrawer, ui: &Ui, wanted: f32, room: egui::Vec2) -> f32 {
        let mut side = wanted.max(Self::MIN_SIDE);
        // A cell grows with its side, though not always in proportion: a
        // few passes settle it.
        for _ in 0..8 {
            let cell = drawer.cell_size(ui, side);
            let over = (cell.x - room.x).max(cell.y - room.y);
            if over <= 0.5 || side <= Self::MIN_SIDE {
                break;
            }
            side = (side - over).max(Self::MIN_SIDE);
        }
        side
    }
}

impl ViewMode for CarouselMode {
    fn describe(&self) -> ModeInfo {
        ModeInfo {
            id: MODE_CAROUSEL.into(),
            label: "Carousel".into(),
            icon: "phosphor:slideshow".into(),
            keys: vec![
                ("Left, Right".into(), "Previous, next entry".into()),
                (
                    "Page Up, Page Down".into(),
                    "The neighbours on one side".into(),
                ),
                ("Home, End".into(), "First, last entry".into()),
            ],
            gestures: vec!["Click a neighbour: bring it to the middle".into()],
            persist: MODE_CAROUSEL.into(),
            role: egui::Role::ListBox,
            needs: ModeNeeds {
                thumbnails: true,
                ..ModeNeeds::default()
            },
        }
    }

    fn show(&mut self, ui: &mut Ui, cx: ModeContext<'_>) -> ModeOutput {
        let mut out = ModeOutput::default();
        let mut plain = PlainDrawer {
            entries: cx.entries,
            selection: cx.selection,
            focus: cx.focus,
            marks: cx.marks,
        };
        let drawer: &mut dyn EntryDrawer = match cx.drawer {
            Some(d) => d,
            None => &mut plain,
        };
        let n = cx.entries.len();
        let width = ui.available_width();
        let height = cx
            .max_height
            .unwrap_or_else(|| ui.available_height())
            .max(4.0 * Self::MIN_SIDE);
        // The middle one at the thumbnail size, made smaller until its cell
        // fits the room.
        let middle = Self::fit(
            drawer,
            ui,
            cx.settings.thumbnail_size,
            egui::vec2(width * Self::MIDDLE, height),
        );
        let gap = ui.spacing().item_spacing.x;
        // Outwards, a step smaller each time, while there is room: each
        // neighbour's side and its centre's distance from the middle, spaced
        // by the cells' own widths.
        let mut steps = Vec::new();
        let mut side = middle;
        let mut reach = drawer.cell_size(ui, middle).x / 2.0;
        loop {
            let next = side * Self::SHRINK;
            let w = drawer.cell_size(ui, next).x;
            if next < Self::MIN_SIDE || reach + gap + w > width / 2.0 {
                break;
            }
            steps.push((next, reach + gap + w / 2.0));
            side = next;
            reach += gap + w;
        }
        let beyond = side;
        let ctx = ui.ctx().clone();
        let thumbnails = cx.thumbnails;
        let entries = cx.entries;
        let ask = |i: usize, side: f32, priority| {
            thumbnails.request(
                &ctx,
                &ThumbRequest {
                    entry: &entries[i],
                    size: (side * ctx.pixels_per_point()).round() as u32,
                    priority,
                },
            )
        };
        let shown = cx.focus.or((n > 0).then_some(0));
        let focus = cx.focus;
        let mut per_side = 0;
        let mut first = usize::MAX;
        let mut end = 0;
        let mut add = |ui: &mut Ui, out: &mut ModeOutput| {
            let (area, _) = ui.allocate_exact_size(egui::vec2(width, height), egui::Sense::hover());
            let Some(at) = shown else {
                drawer.empty(ui);
                return;
            };
            let mut place = |ui: &mut Ui, i: usize, side: f32, x: f32, priority| {
                let size = drawer.cell_size(ui, side);
                let rect = egui::Rect::from_center_size(egui::pos2(x, area.center().y), size);
                let mut child = ui.new_child(egui::UiBuilder::new().max_rect(rect));
                let r = drawer.cell(&mut child, i, size, ask(i, side, priority));
                if focus == Some(i) {
                    out.focused = Some(r.id);
                }
                report(&r, i, out);
                first = first.min(i);
                end = end.max(i + 1);
            };
            place(ui, at, middle, area.center().x, Priority::Visible);
            for (k, &(next, offset)) in (1..).zip(&steps) {
                if at + k < n {
                    place(
                        ui,
                        at + k,
                        next,
                        area.center().x + offset,
                        Priority::Visible,
                    );
                }
                if let Some(i) = at.checked_sub(k) {
                    place(ui, i, next, area.center().x - offset, Priority::Visible);
                }
                per_side = k;
            }
            // The next one beyond each side.
            let k = steps.len() + 1;
            if at + k < n {
                ask(at + k, beyond, Priority::Near);
            }
            if let Some(i) = at.checked_sub(k) {
                ask(i, beyond, Priority::Near);
            }
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
        self.per_side = per_side;
        out.columns = n.max(1);
        out
    }

    fn step(&self, from: usize, count: usize, key: ModeKey) -> usize {
        let last = count.saturating_sub(1);
        let page = self.per_side.max(1);
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

/// When an agenda files an entry: when it was taken, else when it last
/// changed, in nanoseconds since 1970-01-01 (UTC); `None` without either.
fn agenda_time(e: &SourceEntry) -> Option<i128> {
    let t = e.taken.or(e.modified)?;
    Some(match t.duration_since(SystemTime::UNIX_EPOCH) {
        Ok(d) => d.as_nanos() as i128,
        Err(e) => -(e.duration().as_nanos() as i128),
    })
}

const NANOS_PER_DAY: i128 = 86_400 * 1_000_000_000;

/// One line of an agenda: a group's heading, or a row of its entries.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum AgendaLine {
    /// The group's number.
    Heading(usize),
    /// Positions in the order shown.
    Cells(usize, usize),
}

/// What an agenda computes from its entries alone, once per listing (per
/// [`ModeContext::generation`]): each entry's day and the agenda's order;
/// then once per level, the groups.
#[derive(Clone, Debug, Default)]
struct AgendaIndex {
    built: bool,
    generation: u64,
    count: usize,
    /// The entries' indices, the newest first, the undated last, the
    /// browser's order within one time.
    order: Vec<usize>,
    /// Each entry's day since 1970-01-01, by index.
    days: Vec<Option<i64>>,
    level: AgendaLevel,
    /// Each group: its key at the level (`None`: undated) and its span of
    /// `order`.
    groups: Vec<(Option<i64>, Range<usize>)>,
}

impl AgendaIndex {
    /// Brings it up to date; `true` when the dates and the order were
    /// computed again.
    fn update(&mut self, entries: &[SourceEntry], generation: u64, level: AgendaLevel) -> bool {
        let fresh = !self.built || self.generation != generation || self.count != entries.len();
        if fresh {
            let times: Vec<Option<i128>> = entries.iter().map(agenda_time).collect();
            let mut order: Vec<usize> = (0..entries.len()).collect();
            order.sort_by(|&a, &b| match (times[a], times[b]) {
                (Some(x), Some(y)) => y.cmp(&x).then(a.cmp(&b)),
                (Some(_), None) => std::cmp::Ordering::Less,
                (None, Some(_)) => std::cmp::Ordering::Greater,
                (None, None) => a.cmp(&b),
            });
            self.days = times
                .iter()
                .map(|t| t.map(|t| t.div_euclid(NANOS_PER_DAY) as i64))
                .collect();
            self.order = order;
            self.built = true;
            self.generation = generation;
            self.count = entries.len();
        }
        if fresh || self.level != level || self.groups.is_empty() != self.order.is_empty() {
            self.level = level;
            self.groups.clear();
            let key = |i: usize| self.days[i].map(|d| level.group(d));
            let mut start = 0;
            while start < self.order.len() {
                let k = key(self.order[start]);
                let len = self.order[start..]
                    .iter()
                    .take_while(|&&i| key(i) == k)
                    .count();
                self.groups.push((k, start..start + len));
                start += len;
            }
        }
        fresh
    }
}

/// What an agenda's lines depend on: the entries (generation, count), the
/// level, the entries in a row, the groups collapsed.
type LayoutKey = (u64, usize, AgendaLevel, usize, BTreeSet<Option<i64>>);

/// The agenda's lines for a width and the groups collapsed, kept until one
/// of them, the level or the entries change.
#[derive(Clone, Debug, Default)]
struct AgendaLayout {
    key: Option<LayoutKey>,
    /// The entries shown (those of expanded groups), in the agenda's order.
    shown: Vec<usize>,
    /// Each entry's position in `shown`, by index; `None` when its group
    /// is collapsed.
    position: Vec<Option<usize>>,
    /// Each entry's group, by index.
    group_of: Vec<usize>,
    /// Each group's heading line.
    heading_of: Vec<usize>,
    /// Each position's line.
    line_of: Vec<usize>,
    lines: Vec<AgendaLine>,
}

impl AgendaLayout {
    fn update(&mut self, index: &AgendaIndex, columns: usize, collapsed: &BTreeSet<Option<i64>>) {
        let key = (
            index.generation,
            index.count,
            index.level,
            columns,
            collapsed.clone(),
        );
        if self.key.as_ref() == Some(&key) {
            return;
        }
        let n = index.count;
        let mut shown = Vec::with_capacity(n);
        let mut position = vec![None; n];
        let mut group_of = vec![0; n];
        let mut heading_of = Vec::with_capacity(index.groups.len());
        let mut line_of = Vec::with_capacity(n);
        let mut lines = Vec::new();
        for (g, (k, span)) in index.groups.iter().enumerate() {
            heading_of.push(lines.len());
            lines.push(AgendaLine::Heading(g));
            for &i in &index.order[span.clone()] {
                group_of[i] = g;
            }
            if collapsed.contains(k) {
                continue;
            }
            let start = shown.len();
            for &i in &index.order[span.clone()] {
                position[i] = Some(shown.len());
                shown.push(i);
            }
            let end = shown.len();
            let mut at = start;
            while at < end {
                let to = (at + columns).min(end);
                line_of.extend(std::iter::repeat_n(lines.len(), to - at));
                lines.push(AgendaLine::Cells(at, to));
                at = to;
            }
        }
        *self = AgendaLayout {
            key: Some(key),
            shown,
            position,
            group_of,
            heading_of,
            line_of,
            lines,
        };
    }
}

/// The entries by day, month or year, the newest first: one heading per
/// group (when the entry was taken, given by the application's source, else
/// when it last changed; UTC), which a click collapses or expands, its
/// entries under it in a grid, the newest first, those without a time last.
/// The arrows move across the groups' grids; `Ctrl+Up` and `Ctrl+Down`
/// change the level, `Ctrl+Left` and `Ctrl+Right` collapse and expand the
/// focused entry's group. The dates and the groups are computed once per
/// listing, not every frame.
#[derive(Clone, Debug, Default)]
pub struct AgendaMode {
    index: AgendaIndex,
    layout: AgendaLayout,
    rows_in_view: usize,
}

impl AgendaMode {
    fn arranged_id() -> egui::Id {
        egui::Id::unique("egui_pop.file_browser.agenda_arranged")
    }

    /// How many times an agenda drawn in `ctx` computed its entries' dates
    /// and order: once per listing, for tests and agents to check.
    pub fn arrangements(ctx: &egui::Context) -> u64 {
        ctx.data(|d| d.get_temp::<u64>(Self::arranged_id()))
            .unwrap_or(0)
    }

    /// The position `rows` rows above (negative) or below `p`, the same
    /// column as near as it goes; `p` itself past the first or the last row.
    fn rows_from(&self, p: usize, rows: isize) -> usize {
        let l = &self.layout;
        let Some(&line) = l.line_of.get(p) else {
            return p;
        };
        let AgendaLine::Cells(start, _) = l.lines[line] else {
            return p;
        };
        let column = p - start;
        let mut at = line;
        let mut target = None;
        let mut left = rows.unsigned_abs();
        while left > 0 {
            let next = if rows < 0 {
                at.checked_sub(1)
            } else {
                Some(at + 1).filter(|&k| k < l.lines.len())
            };
            let Some(next) = next else { break };
            at = next;
            if let AgendaLine::Cells(s, e) = l.lines[at] {
                target = Some(s + column.min(e - s - 1));
                left -= 1;
            }
        }
        target.unwrap_or(p)
    }
}

impl ViewMode for AgendaMode {
    fn describe(&self) -> ModeInfo {
        ModeInfo {
            id: MODE_AGENDA.into(),
            label: "Agenda".into(),
            icon: "phosphor:calendar".into(),
            keys: vec![
                ("Arrows".into(), "Move across the groups".into()),
                ("Page Up, Page Down".into(), "A page up or down".into()),
                ("Home, End".into(), "Newest, oldest entry".into()),
                ("Ctrl+Up, Ctrl+Down".into(), "By year, month or day".into()),
                (
                    "Ctrl+Left, Ctrl+Right".into(),
                    "Collapse, expand the group".into(),
                ),
            ],
            gestures: vec![
                "Wheel: scroll".into(),
                "Click on a heading: collapse or expand".into(),
            ],
            persist: MODE_AGENDA.into(),
            role: egui::Role::ListBox,
            needs: ModeNeeds {
                thumbnails: true,
                time: true,
                ..ModeNeeds::default()
            },
        }
    }

    fn toolbar(&mut self, ui: &mut Ui, settings: &mut ModeSettings) -> bool {
        let ctx = ui.ctx().clone();
        let mut changed = false;
        ui.separator();
        ui.scope(|ui| {
            ui.spacing_mut().item_spacing.x = 0.0;
            for level in AgendaLevel::ALL {
                let label = strings::tr(&ctx, level.label());
                let on = settings.agenda.level == level;
                let r = widgets::selectable_label(ui, on, label.as_str());
                r.widget_info(|| egui::WidgetInfo::selected(egui::Role::Button, true, on, &label));
                let r = r.on_hover_text("Ctrl+Up, Ctrl+Down");
                if r.clicked() {
                    changed |= settings.agenda.set_level(level);
                }
            }
        });
        changed
    }

    fn own_key(
        &mut self,
        key: egui::Key,
        modifiers: Modifiers,
        focus: Option<usize>,
        settings: &mut ModeSettings,
    ) -> bool {
        if !modifiers.command || modifiers.shift || modifiers.alt {
            return false;
        }
        let a = &mut settings.agenda;
        match key {
            egui::Key::ArrowUp => {
                a.set_level(a.level.coarser());
            }
            egui::Key::ArrowDown => {
                a.set_level(a.level.finer());
            }
            egui::Key::ArrowLeft | egui::Key::ArrowRight => {
                let group = focus
                    .and_then(|i| self.layout.group_of.get(i))
                    .and_then(|&g| self.index.groups.get(g))
                    .map(|(k, _)| *k);
                if let Some(g) = group {
                    a.set_collapsed(g, key == egui::Key::ArrowLeft);
                }
            }
            _ => return false,
        }
        true
    }

    fn show(&mut self, ui: &mut Ui, cx: ModeContext<'_>) -> ModeOutput {
        let mut out = ModeOutput::default();
        let ctx = ui.ctx().clone();
        let level = cx.settings.agenda.level;
        if self.index.update(cx.entries, cx.generation, level) {
            ctx.data_mut(|d| {
                *d.get_temp_mut_or_default::<u64>(Self::arranged_id()) += 1;
            });
        }
        let side = cx
            .settings
            .thumbnail_size
            .clamp(ThumbnailsMode::MIN_SIZE, ThumbnailsMode::MAX_SIZE);
        let mut plain = PlainDrawer {
            entries: cx.entries,
            selection: cx.selection,
            focus: cx.focus,
            marks: cx.marks,
        };
        let drawer: &mut dyn EntryDrawer = match cx.drawer {
            Some(d) => d,
            None => &mut plain,
        };
        let n = cx.entries.len();
        let cell = drawer.cell_size(ui, side);
        let gap = ui.spacing().item_spacing;
        let width = ui.available_width() - ui.spacing().scroll.allocated_width();
        let columns = (((width + gap.x) / (cell.x + gap.x)).floor() as usize).max(1);
        self.layout
            .update(&self.index, columns, &cx.settings.agenda.collapsed);
        let (index, layout) = (&self.index, &self.layout);
        let heading_h = ui.text_style_height(&egui::TextStyle::Heading) + 2.0 * gap.y;
        let row_h = cell.y + gap.y;
        let mut tops = Vec::with_capacity(layout.lines.len() + 1);
        let mut y = 0.0;
        for line in &layout.lines {
            tops.push(y);
            y += match line {
                AgendaLine::Heading(..) => heading_h,
                AgendaLine::Cells(..) => row_h,
            };
        }
        tops.push(y);
        let total = y;
        let thumbnails = cx.thumbnails;
        let entries = cx.entries;
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
        let undated = strings::tr(&ctx, &crate::strings::FILES_AGENDA_UNDATED);
        // The focused entry's row, or its group's heading when collapsed.
        let reveal = cx
            .focus
            .filter(|&i| cx.reveal_focus && i < layout.position.len())
            .map(|i| match layout.position[i] {
                Some(p) => layout.line_of[p],
                None => layout.heading_of[layout.group_of[i]],
            });
        let focus = cx.focus;
        let collapsed = &cx.settings.agenda.collapsed;
        let mut toggled = None;
        let mut first = usize::MAX;
        let mut end = 0;
        let mut drawn_rows = 0;
        let mut near = None;
        let mut area = egui::ScrollArea::vertical()
            .id_salt(cx.id.with("agenda"))
            .auto_shrink([false, false])
            .animated(false);
        if let Some(h) = cx.max_height {
            area = area.max_height(h);
        }
        let mut add = |ui: &mut Ui, out: &mut ModeOutput| {
            if n == 0 {
                drawer.empty(ui);
            }
            area.clone().show_viewport(ui, |ui, viewport| {
                ui.set_height(total);
                ui.set_width(width);
                let origin = ui.max_rect().min;
                if let Some(l) = reveal {
                    let r = egui::Rect::from_min_max(
                        origin + egui::vec2(0.0, tops[l]),
                        origin + egui::vec2(width, tops[l + 1]),
                    );
                    ui.scroll_to_rect(r, None);
                    ui.ctx().request_repaint();
                }
                for (l, line) in layout.lines.iter().enumerate() {
                    if tops[l + 1] < viewport.min.y {
                        continue;
                    }
                    if tops[l] > viewport.max.y {
                        // The next row, for the next screen.
                        near = layout.lines[l..].iter().find_map(|line| match *line {
                            AgendaLine::Cells(s, e) => Some(s..e),
                            AgendaLine::Heading(..) => None,
                        });
                        break;
                    }
                    let top = origin + egui::vec2(0.0, tops[l]);
                    match *line {
                        AgendaLine::Heading(g) => {
                            let (key, span) = &index.groups[g];
                            let shut = collapsed.contains(key);
                            let text = match key {
                                Some(k) => index.level.heading(*k),
                                None => undated.clone(),
                            };
                            let rect = egui::Rect::from_min_size(top, egui::vec2(width, heading_h));
                            let resp = ui.interact(
                                rect,
                                cx.id.with(("agenda.heading", *key)),
                                egui::Sense::click(),
                            );
                            let name = text.clone();
                            resp.widget_info(|| {
                                egui::WidgetInfo::labeled(egui::Role::Heading, true, &name)
                            });
                            ui.ctx().accesskit_node_builder(resp.id, |node| {
                                node.set_expanded(!shut);
                            });
                            if resp.clicked() {
                                toggled = Some((*key, !shut));
                            }
                            let visuals = ui.visuals();
                            if resp.hovered() {
                                ui.painter().rect_filled(
                                    rect,
                                    2.0,
                                    visuals.widgets.hovered.weak_bg_fill,
                                );
                            }
                            let color = visuals.strong_text_color();
                            // The disclosure triangle: down when expanded.
                            let c = egui::pos2(rect.left() + 10.0, rect.center().y);
                            let r = 4.5;
                            let points = if shut {
                                vec![
                                    c + egui::vec2(-r * 0.6, -r),
                                    c + egui::vec2(r, 0.0),
                                    c + egui::vec2(-r * 0.6, r),
                                ]
                            } else {
                                vec![
                                    c + egui::vec2(-r, -r * 0.6),
                                    c + egui::vec2(r, -r * 0.6),
                                    c + egui::vec2(0.0, r),
                                ]
                            };
                            ui.painter().add(egui::Shape::convex_polygon(
                                points,
                                color,
                                egui::Stroke::NONE,
                            ));
                            let font = egui::TextStyle::Heading.resolve(ui.style());
                            let at = egui::pos2(rect.left() + 22.0, rect.center().y);
                            let galley = ui.painter().layout_no_wrap(text, font, color);
                            let after = at.x + galley.size().x + 8.0;
                            ui.painter().galley(
                                at - egui::vec2(0.0, galley.size().y / 2.0),
                                galley,
                                color,
                            );
                            ui.painter().text(
                                egui::pos2(after, rect.center().y),
                                egui::Align2::LEFT_CENTER,
                                span.len().to_string(),
                                egui::TextStyle::Body.resolve(ui.style()),
                                visuals.weak_text_color(),
                            );
                        }
                        AgendaLine::Cells(s, e) => {
                            drawn_rows += 1;
                            for (c, &i) in layout.shown[s..e].iter().enumerate() {
                                let rect = egui::Rect::from_min_size(
                                    top + egui::vec2(c as f32 * (cell.x + gap.x), 0.0),
                                    cell,
                                );
                                let mut child = ui.new_child(egui::UiBuilder::new().max_rect(rect));
                                let r = drawer.cell(&mut child, i, cell, ask(i, Priority::Visible));
                                if focus == Some(i) {
                                    out.focused = Some(r.id);
                                }
                                report(&r, i, out);
                                first = first.min(s);
                                end = end.max(e);
                            }
                        }
                    }
                }
            });
        };
        in_container(
            ui,
            cx.name.as_deref().map(|n| (n, cx.many)),
            &mut out,
            &mut add,
        );
        if let Some(r) = near {
            for &i in &layout.shown[r] {
                ask(i, Priority::Near);
            }
        }
        if let Some((key, shut)) = toggled {
            cx.settings.agenda.set_collapsed(key, shut);
            ctx.request_repaint();
        }
        // In the agenda's order: positions, not indices.
        out.visible = if first == usize::MAX {
            0..0
        } else {
            first..end
        };
        self.rows_in_view = drawn_rows.max(1);
        out.columns = columns;
        out
    }

    fn step(&self, from: usize, count: usize, key: ModeKey) -> usize {
        let last = count.saturating_sub(1);
        let l = &self.layout;
        // Not laid out for these entries yet: their own order.
        if l.position.len() != count || from >= count {
            return match key {
                ModeKey::Left | ModeKey::Up | ModeKey::PageUp => from.saturating_sub(1),
                ModeKey::Right | ModeKey::Down | ModeKey::PageDown => (from + 1).min(last),
                ModeKey::Home => 0,
                ModeKey::End => last,
            };
        }
        let (Some(&first), Some(&final_)) = (l.shown.first(), l.shown.last()) else {
            // Every group collapsed.
            return from;
        };
        let back = matches!(
            key,
            ModeKey::Left | ModeKey::Up | ModeKey::PageUp | ModeKey::Home
        );
        let Some(p) = l.position[from] else {
            // In a collapsed group: the nearest entry shown that way.
            let g = l.group_of[from];
            return match key {
                ModeKey::Home => first,
                ModeKey::End => final_,
                _ if back => l
                    .shown
                    .iter()
                    .rev()
                    .find(|&&i| l.group_of[i] < g)
                    .copied()
                    .unwrap_or(first),
                _ => l
                    .shown
                    .iter()
                    .find(|&&i| l.group_of[i] > g)
                    .copied()
                    .unwrap_or(final_),
            };
        };
        let last_p = l.shown.len() - 1;
        let page = self.rows_in_view.max(1) as isize;
        let to = match key {
            ModeKey::Left => p.saturating_sub(1),
            ModeKey::Right => (p + 1).min(last_p),
            ModeKey::Up => self.rows_from(p, -1),
            ModeKey::Down => self.rows_from(p, 1),
            ModeKey::PageUp => self.rows_from(p, -page),
            ModeKey::PageDown => self.rows_from(p, page),
            ModeKey::Home => 0,
            ModeKey::End => last_p,
        };
        l.shown[to]
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
/// The id of the carousel mode.
pub const MODE_CAROUSEL: &str = "carousel";
/// The id of the agenda mode.
pub const MODE_AGENDA: &str = "agenda";
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

/// Which kind of entry can be selected ([`SelectionPolicy`]).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Selectable {
    /// Files alone: a folder is focused and opened, never selected.
    Files,
    /// Folders alone: a file is shown, never selected.
    Folders,
    /// Files and folders.
    #[default]
    Any,
}

impl Selectable {
    /// Whether an entry of this kind can be selected.
    pub fn accepts(self, kind: EntryKind) -> bool {
        match self {
            Selectable::Files => kind == EntryKind::File,
            Selectable::Folders => kind == EntryKind::Folder,
            Selectable::Any => true,
        }
    }
}

/// What a browser lets the person select ([`FileBrowser::set_selection_policy`]):
/// which kind of entry, and one or several. A click, the keys, a rubber band,
/// select all and [`FileBrowser::set_selection`] all keep to it; an entry it
/// refuses is still focused and opened.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct SelectionPolicy {
    pub selectable: Selectable,
    pub mode: SelectMode,
}

impl Default for SelectionPolicy {
    /// Any entry, several at once: the browser's default.
    fn default() -> Self {
        Self::any(SelectMode::Many)
    }
}

impl SelectionPolicy {
    pub fn new(selectable: Selectable, mode: SelectMode) -> Self {
        Self { selectable, mode }
    }

    /// Files alone (a file picker).
    pub fn files(mode: SelectMode) -> Self {
        Self::new(Selectable::Files, mode)
    }

    /// Folders alone (a folder picker).
    pub fn folders(mode: SelectMode) -> Self {
        Self::new(Selectable::Folders, mode)
    }

    /// Files and folders.
    pub fn any(mode: SelectMode) -> Self {
        Self::new(Selectable::Any, mode)
    }
}

/// Which entries a browser shows ([`FileBrowser::set_filter`]): files by
/// extension, and one kind of entry or both. Folders pass an extension
/// filter (the person goes through them), unless the kind leaves them out.
/// A group is shown when its main file passes, its members with it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct EntryFilter {
    /// The files' extensions, without the dot, in any case; empty: every
    /// file.
    pub extensions: Vec<String>,
    /// Only entries of this kind; none: both.
    pub kind: Option<EntryKind>,
}

impl EntryFilter {
    /// Every entry.
    pub fn all() -> Self {
        Self::default()
    }

    /// Files with one of these extensions (without the dot), and folders.
    pub fn extensions(extensions: &[&str]) -> Self {
        Self {
            extensions: extensions.iter().map(|e| e.to_lowercase()).collect(),
            kind: None,
        }
    }

    /// Entries of this kind alone.
    pub fn kind(kind: EntryKind) -> Self {
        Self::all().with_kind(kind)
    }

    /// The same filter, for entries of this kind alone.
    pub fn with_kind(mut self, kind: EntryKind) -> Self {
        self.kind = Some(kind);
        self
    }

    /// Whether it lets every entry through.
    pub fn is_all(&self) -> bool {
        self.extensions.is_empty() && self.kind.is_none()
    }

    /// Whether an entry passes.
    pub fn accepts(&self, entry: &SourceEntry) -> bool {
        if self.kind.is_some_and(|k| k != entry.kind) {
            return false;
        }
        entry.is_dir() || has_extension(&self.extensions, &entry.path)
    }
}

impl From<&super::FileFilter> for EntryFilter {
    fn from(f: &super::FileFilter) -> Self {
        Self {
            extensions: f.extensions.clone(),
            kind: None,
        }
    }
}

/// Whether a file's name has one of `extensions` (any case; any extension,
/// or none, when `extensions` is empty).
pub(crate) fn has_extension(extensions: &[String], path: &Path) -> bool {
    extensions.is_empty()
        || path
            .extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| extensions.iter().any(|x| x.eq_ignore_ascii_case(e)))
}

/// The items a fuzzy query matches, best first, equals in their order: each
/// answers to its names (an entry's own, and a group's members' too).
pub(crate) fn rank_by_names<T>(
    items: &[T],
    query: &str,
    names: impl Fn(&T) -> Vec<String>,
) -> Vec<usize> {
    let mut all = Vec::with_capacity(items.len());
    for (n, it) in items.iter().enumerate() {
        all.extend(names(it).into_iter().map(|name| (n, name)));
    }
    let mut seen = HashSet::new();
    egui_pop_model::fuzzy::rank(&all, query, |(_, name)| name.clone())
        .into_iter()
        .map(|(i, _)| all[i].0)
        .filter(|n| seen.insert(*n))
        .collect()
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
    /// A drop its target took ([`FileBrowser::with_drops`]).
    pub dropped: Option<DropOffer>,
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
///
/// Need (dialog): a file or folder picker.
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
    /// How many thumbnails fill a row; turned into the settings' size at
    /// each draw.
    zoom: ThumbZoom,
    /// The grid's room at the last draw (width, gap): what the toolbar's
    /// zoom reads.
    grid_room: (f32, f32),
    thumbnails: Arc<dyn ThumbnailProvider>,
    type_ahead: TypeAhead,
    reveal: bool,
    restored: bool,
    root: Option<PathBuf>,
    /// The ancestors' and the focused folder's listings, for a mode that
    /// shows the path.
    columns: ColumnListings,
    /// Bumped whenever the entries change: [`ModeContext::generation`].
    generation: u64,
    /// Whether, and where, drops are taken.
    drops: Option<DropPolicy>,
    drop_targets: Vec<AppDropTarget>,
    /// What a drag over the browser would do, as last drawn.
    drop_hover: Option<(DropOffer, bool)>,
    /// Focus the first entry once the listing gives one (Right into a
    /// folder).
    focus_first: bool,
    /// Quick look is open over the entries.
    quick_look: bool,
    /// The application's marks of each entry.
    marks: Option<Arc<MarkEntries>>,
    /// The entries' order ([`FileBrowser::set_sort`]).
    sort: PickerSort,
    folders_first: bool,
    /// The selection a rubber band adds to while it is drawn.
    band_base: Option<BTreeSet<PathBuf>>,
    /// The groupings files go together by ([`FileBrowser::with_grouping`]).
    groupings: Option<Groupings>,
    /// Whether files go together now, with groupings declared.
    grouped: bool,
    /// Finds the place's groups as its pages come.
    grouper: Option<Grouper>,
    /// The place's groups, by main file (shared with a frame's marks).
    groups: Arc<HashMap<PathBuf, FileGroup>>,
    /// The groups' members, out of the entries while their group shows.
    absorbed: HashMap<PathBuf, SourceEntry>,
    /// Which entries are shown ([`FileBrowser::set_filter`]).
    filter: EntryFilter,
    /// The fuzzy query the shown entries match, best first
    /// ([`FileBrowser::set_query`]).
    query: String,
    /// Which kind of entry can be selected.
    selectable: Selectable,
    /// The entries listed that the filter or the query leaves out.
    filtered_out: Vec<SourceEntry>,
}

/// The browser's default order: folders first, then names as people read
/// them; a side column's order whatever the place's.
fn sort_entries(entries: &mut [SourceEntry]) {
    sort_in(entries, PickerSort::default(), true);
}

/// Puts entries in `sort`'s order, folders first when asked
/// ([`FileBrowser::set_sort`]).
fn sort_in(entries: &mut [SourceEntry], sort: PickerSort, folders_first: bool) {
    sort_by(entries, sort, folders_first, |e| SortFacts {
        name: &e.name,
        is_dir: e.is_dir(),
        size: e.size,
        modified: e.modified,
        taken: e.taken,
    });
}

/// Takes what a side listing delivered into its column (its folders alone
/// with `folders_only`); `true` while it is under way.
fn pump_column(column: &mut Column, listing: &mut Option<Listing>, folders_only: bool) -> bool {
    let Some(l) = listing else {
        return false;
    };
    let mut changed = false;
    let mut done = false;
    for page in l.poll() {
        match page {
            Page::Entries(mut e) => {
                if folders_only {
                    e.retain(SourceEntry::is_dir);
                }
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

/// The listings of the columns beside the place's, for a mode that shows
/// the path (Finder's columns): the place's ancestors and the focused folder
/// looked into. Each lists off the interface thread when its source does
/// (a [`FsSource`] over the system's file system), its entries streaming in
/// page by page, [`Column::loading`] until the end; one no longer wanted
/// (the focus moved on, the place changed) is dropped, which cancels it.
#[derive(Debug, Default)]
pub struct ColumnListings {
    ancestors: Vec<Column>,
    listings: Vec<Option<Listing>>,
    peek: Option<(Column, Option<Listing>)>,
    options: ListOptions,
    folders_only: bool,
}

impl ColumnListings {
    /// Columns that list the folders alone (a folder chooser's).
    pub fn folders_only(mut self, on: bool) -> Self {
        self.folders_only = on;
        self
    }

    /// Lists `wanted` (the ancestors to show, the topmost first) and `peek`
    /// (the focused folder), reusing the columns already there, starting
    /// what is missing, dropping what is not wanted; then takes what each
    /// listing delivered. `place` is the folder after the last ancestor:
    /// each column's open entry leads to the next. `true` while a listing
    /// is under way.
    pub fn sync(
        &mut self,
        source: &dyn FileSource,
        options: &ListOptions,
        wanted: &[PathBuf],
        place: &Path,
        peek: Option<&Path>,
    ) -> bool {
        if self.options != *options {
            self.clear();
            self.options = *options;
        }
        if self.ancestors.iter().map(|c| &c.place).ne(wanted.iter()) {
            let mut old: Vec<(Column, Option<Listing>)> = self
                .ancestors
                .drain(..)
                .zip(self.listings.drain(..))
                .collect();
            for place in wanted {
                match old.iter().position(|(c, _)| &c.place == place) {
                    Some(i) => {
                        let (c, l) = old.swap_remove(i);
                        self.ancestors.push(c);
                        self.listings.push(l);
                    }
                    None => {
                        let l = source.list(place, options);
                        self.ancestors.push(Column {
                            place: place.clone(),
                            loading: true,
                            ..Column::default()
                        });
                        self.listings.push(Some(l));
                    }
                }
            }
            // What is left of `old` drops here: cancelled.
        }
        let mut busy = false;
        for (c, l) in self.ancestors.iter_mut().zip(self.listings.iter_mut()) {
            busy |= pump_column(c, l, self.folders_only);
        }
        // Each column's open entry: the path's next folder.
        let n = self.ancestors.len();
        for k in 0..n {
            let next = if k + 1 < n {
                self.ancestors[k + 1].place.clone()
            } else {
                place.to_path_buf()
            };
            let c = &mut self.ancestors[k];
            c.open = c.entries.iter().position(|e| e.path == next);
        }
        match peek {
            Some(f) => {
                if self.peek.as_ref().is_none_or(|(c, _)| c.place != f) {
                    // The folder focused before is dropped: cancelled.
                    let l = source.list(f, options);
                    self.peek = Some((
                        Column {
                            place: f.to_path_buf(),
                            loading: true,
                            ..Column::default()
                        },
                        Some(l),
                    ));
                }
                if let Some((c, l)) = &mut self.peek {
                    busy |= pump_column(c, l, self.folders_only);
                }
            }
            None => self.peek = None,
        }
        busy
    }

    /// The ancestors' columns, the topmost first.
    pub fn ancestors(&self) -> &[Column] {
        &self.ancestors
    }

    /// The focused folder's column.
    pub fn peek(&self) -> Option<&Column> {
        self.peek.as_ref().map(|(c, _)| c)
    }

    /// Whether a listing is under way.
    pub fn loading(&self) -> bool {
        self.listings.iter().any(Option::is_some)
            || self.peek.as_ref().is_some_and(|(_, l)| l.is_some())
    }

    /// Drops every column and cancels their listings: a refresh lists them
    /// again.
    pub fn clear(&mut self) {
        self.ancestors.clear();
        self.listings.clear();
        self.peek = None;
    }
}

// ---------------------------------------------------------------------------
// Grouping
// ---------------------------------------------------------------------------

/// What the grouping worker is told of a place's files.
#[derive(Debug)]
enum GroupWork {
    Add(Vec<String>),
    Remove(Vec<String>),
}

/// The thread a large place's groups are found on.
#[derive(Debug)]
struct GroupWorker {
    tx: mpsc::Sender<GroupWork>,
    rx: mpsc::Receiver<(u64, Vec<FileGroup>)>,
}

/// Finds a place's groups ([`FileBrowser::with_grouping`]) as its pages
/// come: each page's file names are told to it, and it answers the groups
/// among every name so far. A small place is grouped in the frame, at
/// once; past [`Grouper::INLINE_NAMES`] names, on a thread of its own (off
/// the web), whose answers coalesce: a slow grouping skips pages, never the
/// last one. Dropping it ends its thread.
#[derive(Debug)]
struct Grouper {
    dir: PathBuf,
    groupings: Groupings,
    /// The names so far, while it groups in the frame.
    names: Vec<String>,
    worker: Option<GroupWorker>,
    /// Pieces of work told to the worker, and the last it answered.
    sent: u64,
    answered: u64,
    /// The last groups found in the frame, not taken yet.
    ready: Option<Vec<FileGroup>>,
}

impl Grouper {
    /// The most names a place is grouped with in the frame.
    const INLINE_NAMES: usize = 2048;

    fn new(dir: PathBuf, groupings: Groupings) -> Self {
        Self {
            dir,
            groupings,
            names: Vec::new(),
            worker: None,
            sent: 0,
            answered: 0,
            ready: None,
        }
    }

    /// The names a grouping reads among these entries: files, not hidden.
    fn names_of<'a>(entries: impl IntoIterator<Item = &'a SourceEntry>) -> Vec<String> {
        entries
            .into_iter()
            .filter(|e| !e.is_dir() && !e.name.starts_with('.'))
            .map(|e| e.name.clone())
            .collect()
    }

    fn tell(&mut self, work: GroupWork) {
        if let Some(w) = &self.worker {
            if w.tx.send(work).is_ok() {
                self.sent += 1;
            }
            return;
        }
        match work {
            GroupWork::Add(mut names) => self.names.append(&mut names),
            GroupWork::Remove(names) => {
                let gone: HashSet<String> = names.into_iter().collect();
                self.names.retain(|n| !gone.contains(n));
            }
        }
        if self.names.len() > Self::INLINE_NAMES && self.spawn() {
            return;
        }
        let names: Vec<&str> = self.names.iter().map(String::as_str).collect();
        self.ready = Some(self.groupings.in_folder(&self.dir, &names));
    }

    /// Moves the work to a thread of its own, the names so far with it;
    /// `false` where there are no threads.
    fn spawn(&mut self) -> bool {
        if cfg!(target_arch = "wasm32") {
            return false;
        }
        let (tx, work) = mpsc::channel::<GroupWork>();
        let (answer, rx) = mpsc::channel();
        let (dir, groupings) = (self.dir.clone(), self.groupings.clone());
        let spawned = std::thread::Builder::new()
            .name("egui-pop-file-groups".into())
            .spawn(move || {
                let mut names: Vec<String> = Vec::new();
                let mut seen = 0u64;
                while let Ok(first) = work.recv() {
                    // What came meanwhile is taken in one answer.
                    for w in std::iter::once(first).chain(work.try_iter()) {
                        seen += 1;
                        match w {
                            GroupWork::Add(mut more) => names.append(&mut more),
                            GroupWork::Remove(gone) => {
                                let gone: HashSet<String> = gone.into_iter().collect();
                                names.retain(|n| !gone.contains(n));
                            }
                        }
                    }
                    let refs: Vec<&str> = names.iter().map(String::as_str).collect();
                    if answer
                        .send((seen, groupings.in_folder(&dir, &refs)))
                        .is_err()
                    {
                        return;
                    }
                }
            });
        if spawned.is_err() {
            return false;
        }
        let names = std::mem::take(&mut self.names);
        self.worker = Some(GroupWorker { tx, rx });
        self.ready = None;
        self.tell(GroupWork::Add(names));
        true
    }

    /// The groups last found, when they are new.
    fn poll(&mut self) -> Option<Vec<FileGroup>> {
        let Some(w) = &self.worker else {
            return self.ready.take();
        };
        let mut last = None;
        for (seen, groups) in w.rx.try_iter() {
            self.answered = seen;
            last = Some(groups);
        }
        last
    }

    /// Whether the worker has not answered all it was told yet.
    fn pending(&self) -> bool {
        self.worker.is_some() && self.answered < self.sent
    }
}

/// A frame's thumbnails with groups: a group's entry without an image of
/// its own (a RAW the provider cannot read) shows its first member's that
/// has one (its JPEG).
pub(crate) struct GroupThumbnails<'a> {
    pub(crate) inner: &'a dyn ThumbnailProvider,
    pub(crate) groups: &'a HashMap<PathBuf, FileGroup>,
    pub(crate) absorbed: &'a HashMap<PathBuf, SourceEntry>,
}

impl ThumbnailProvider for GroupThumbnails<'_> {
    fn begin_frame(&self, ctx: &egui::Context) {
        self.inner.begin_frame(ctx);
    }

    fn request(&self, ctx: &egui::Context, request: &ThumbRequest<'_>) -> ThumbState {
        let own = self.inner.request(ctx, request);
        if !matches!(own, ThumbState::None | ThumbState::Failed(_)) {
            return own;
        }
        let Some(group) = self.groups.get(&request.entry.path) else {
            return own;
        };
        for member in group.members.iter().filter_map(|m| self.absorbed.get(m)) {
            let state = self.inner.request(
                ctx,
                &ThumbRequest {
                    entry: member,
                    ..*request
                },
            );
            if matches!(state, ThumbState::Ready(_) | ThumbState::Loading) {
                return state;
            }
        }
        own
    }

    fn end_frame(&self, ctx: &egui::Context) {
        self.inner.end_frame(ctx);
    }

    fn cancel(&self, path: &Path) {
        self.inner.cancel(path);
    }
}

/// The chips a group's entry carries: one per kind of member (`+XMP`,
/// `+JPEG`), else how many they are, each named by its members for screen
/// readers and in its tip.
pub(crate) fn group_chips(ctx: &egui::Context, group: &FileGroup) -> Vec<MarkChip> {
    let members = group.member_names().join(", ");
    let name = strings::tr_args(
        ctx,
        &crate::strings::FILES_GROUP_WITH,
        &[("members", &members)],
    );
    if group.badges.is_empty() {
        return vec![MarkChip::text(format!("+{}", group.members.len()), name)];
    }
    group
        .badges
        .iter()
        .map(|b| MarkChip::text(format!("+{b}"), name.clone()))
        .collect()
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
            zoom: ThumbZoom::default(),
            grid_room: (800.0, 4.0),
            thumbnails: Arc::new(NoThumbnails),
            type_ahead: TypeAhead::default(),
            reveal: false,
            restored: false,
            root: None,
            columns: ColumnListings::default(),
            generation: 0,
            drops: None,
            drop_targets: Vec::new(),
            drop_hover: None,
            focus_first: false,
            quick_look: false,
            marks: None,
            sort: PickerSort::default(),
            folders_first: true,
            band_base: None,
            groupings: None,
            grouped: true,
            grouper: None,
            groups: Arc::default(),
            absorbed: HashMap::new(),
            filter: EntryFilter::all(),
            query: String::new(),
            selectable: Selectable::Any,
            filtered_out: Vec::new(),
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

    /// Lets the person select what `policy` says, from the start.
    pub fn with_selection_policy(mut self, policy: SelectionPolicy) -> Self {
        self.set_selection_policy(policy);
        self
    }

    /// What can be selected: which kind of entry, one or several. What the
    /// selection holds against it leaves it (the first entry kept, for one).
    pub fn set_selection_policy(&mut self, policy: SelectionPolicy) {
        self.select = policy.mode;
        self.selectable = policy.selectable;
        let kept: Vec<PathBuf> = self.selection.paths.iter().cloned().collect();
        self.set_selection(kept);
    }

    /// What can be selected.
    pub fn selection_policy(&self) -> SelectionPolicy {
        SelectionPolicy::new(self.selectable, self.select)
    }

    /// Shows only the entries `filter` passes, from the start.
    pub fn with_filter(mut self, filter: EntryFilter) -> Self {
        self.set_filter(filter);
        self
    }

    /// Shows only the entries `filter` passes (by extension, by kind): the
    /// others leave [`FileBrowser::entries`], the selection and the focus,
    /// and come back when the filter lets them.
    pub fn set_filter(&mut self, filter: EntryFilter) {
        if filter != self.filter {
            self.filter = filter;
            self.resort();
        }
    }

    pub fn filter(&self) -> &EntryFilter {
        &self.filter
    }

    /// Shows only the entries a fuzzy query matches (a group by its members'
    /// names too), best first, equals in the browser's order; the focus goes
    /// to the best. An empty query shows them all again, in that order.
    pub fn set_query(&mut self, query: &str) {
        if query == self.query {
            return;
        }
        self.query = query.to_owned();
        self.arrange();
        if !self.query.trim().is_empty()
            && let Some(first) = self.entries.first().map(|e| e.path.clone())
        {
            self.focus = Some(first.clone());
            if self.select == SelectMode::One {
                self.select_only(&first);
            }
            self.reveal = true;
        }
    }

    /// The fuzzy query the entries shown match.
    pub fn query(&self) -> &str {
        &self.query
    }

    /// Whether this entry can be selected; one not listed (yet) can.
    fn can_select(&self, path: &Path) -> bool {
        self.entries
            .iter()
            .find(|e| e.path == path)
            .is_none_or(|e| self.selectable.accepts(e.kind))
    }

    /// Selects this entry alone, or nothing when it cannot be selected; the
    /// anchor goes to it either way.
    fn select_only(&mut self, path: &Path) {
        if self.can_select(path) {
            self.selection.only(path);
        } else {
            self.selection.paths.clear();
            self.selection.anchor = Some(path.to_path_buf());
        }
    }

    /// Puts the entries listed in the order asked, those the filter and the
    /// query leave out set aside; the selection and the focus leave those.
    fn arrange(&mut self) {
        self.entries.append(&mut self.filtered_out);
        let query = self.query.trim().to_owned();
        if !self.filter.is_all() {
            let filter = &self.filter;
            let (keep, out): (Vec<_>, Vec<_>) = std::mem::take(&mut self.entries)
                .into_iter()
                .partition(|e| filter.accepts(e));
            self.entries = keep;
            self.filtered_out = out;
        }
        sort_in(&mut self.entries, self.sort, self.folders_first);
        if !query.is_empty() {
            let groups = Arc::clone(&self.groups);
            let order = rank_by_names(&self.entries, &query, |e| {
                let mut names = vec![e.name.clone()];
                if let Some(g) = groups.get(&e.path) {
                    names.extend(g.member_names());
                }
                names
            });
            let mut slots: Vec<Option<SourceEntry>> = std::mem::take(&mut self.entries)
                .into_iter()
                .map(Some)
                .collect();
            self.entries = order.iter().filter_map(|&i| slots[i].take()).collect();
            self.filtered_out.extend(slots.into_iter().flatten());
        }
        if !self.filtered_out.is_empty() {
            let shown: HashSet<&Path> = self.entries.iter().map(|e| e.path.as_path()).collect();
            self.selection.paths.retain(|p| shown.contains(p.as_path()));
            if self
                .selection
                .anchor
                .as_ref()
                .is_some_and(|a| !shown.contains(a.as_path()))
            {
                self.selection.anchor = None;
            }
            if self
                .focus
                .as_ref()
                .is_some_and(|f| !shown.contains(f.as_path()))
            {
                self.focus = None;
            }
        }
        self.generation += 1;
    }

    /// Marks entries as the application sees them: badges on their cells
    /// and after their names, said to assistive technology with the name,
    /// and dimmed entries (a photo already imported). Every mode that
    /// draws through the browser's [`PlainDrawer`] shows them.
    pub fn with_marks(
        mut self,
        marks: impl Fn(&SourceEntry) -> EntryMarks + Send + Sync + 'static,
    ) -> Self {
        self.marks = Some(Arc::new(marks));
        self
    }

    /// Sorts the entries by `sort` from the start: by name upwards unless
    /// told otherwise.
    pub fn with_sort(mut self, sort: PickerSort) -> Self {
        self.set_sort(sort);
        self
    }

    /// Whether folders come before files whatever the order (they do
    /// unless told otherwise).
    pub fn with_folders_first(mut self, on: bool) -> Self {
        self.set_folders_first(on);
        self
    }

    /// The entries' order: by name, when they last changed, when they were
    /// taken ([`SourceEntry::taken`], else when they last changed), size or
    /// type, up or down. What has no value in that order goes last either
    /// way, and names settle ties. The toolbar's sort menu sets it too, in
    /// every mode that does not order by time on its own (an agenda). The
    /// selection and the focus stay on their entries, and the focus is
    /// brought into view.
    pub fn set_sort(&mut self, sort: PickerSort) {
        if sort != self.sort {
            self.sort = sort;
            self.resort();
        }
    }

    pub fn sort(&self) -> PickerSort {
        self.sort
    }

    /// Folders before files in every order, or among them.
    pub fn set_folders_first(&mut self, on: bool) {
        if on != self.folders_first {
            self.folders_first = on;
            self.resort();
        }
    }

    pub fn folders_first(&self) -> bool {
        self.folders_first
    }

    /// Puts the entries in the order asked, the focus brought into view.
    fn resort(&mut self) {
        self.arrange();
        self.reveal = self.focus.is_some();
    }

    /// Files that go together are one entry, by these groupings
    /// ([`super::groups`]; an application's [`Files::groupings`](super::Files::groupings)):
    /// a RAW with its sidecars and its JPEG shows as the RAW, its members'
    /// badges on it (`+XMP +JPEG`), named for screen readers ("Grouped with
    /// …"), and showing its first member's thumbnail when it has none of its
    /// own. The members leave [`FileBrowser::entries`]; the group of an entry
    /// is [`FileBrowser::group_of`]. The groups are found as the listing's
    /// pages come, a large place's off the interface thread. egui-pop declares
    /// no grouping: what goes together is the application's rule.
    pub fn with_grouping(mut self, groupings: Groupings) -> Self {
        self.groupings = Some(groupings);
        self.regroup();
        self
    }

    /// Groups files, with groupings declared, or lets each be an entry of
    /// its own (they are grouped unless told otherwise). The toolbar's sort
    /// menu turns it too. A member selected or focused leaves it for its
    /// group's entry.
    pub fn set_grouped(&mut self, on: bool) {
        if on != self.grouped {
            self.grouped = on;
            self.regroup();
        }
    }

    pub fn grouped(&self) -> bool {
        self.grouped
    }

    /// Whether files go together now: grouped, with a grouping declared.
    fn grouping_on(&self) -> bool {
        self.grouped && self.groupings.as_ref().is_some_and(|g| !g.is_empty())
    }

    /// The group an entry heads, as last found.
    pub fn group_of(&self, path: &Path) -> Option<&FileGroup> {
        self.groups.get(path)
    }

    /// Whether the groups of what was listed are still being looked for.
    pub fn groups_pending(&self) -> bool {
        self.grouper.as_ref().is_some_and(Grouper::pending)
    }

    /// Starts finding the groups again, from the entries listed so far:
    /// the members come back among the entries until it answers.
    fn regroup(&mut self) {
        self.grouper = None;
        self.apply_groups(Vec::new());
        if self.grouping_on()
            && let Some(groupings) = self.groupings.clone()
        {
            let mut grouper = Grouper::new(self.place.clone(), groupings);
            grouper.tell(GroupWork::Add(Grouper::names_of(&self.entries)));
            self.grouper = Some(grouper);
            self.take_groups();
        }
    }

    /// Takes the groups the grouper last found.
    fn take_groups(&mut self) {
        if let Some(found) = self.grouper.as_mut().and_then(Grouper::poll) {
            self.apply_groups(found);
        }
    }

    /// Shows these groups: their members leave the entries, the members of
    /// groups no longer found come back, and a member selected or focused
    /// leaves it for its group's entry.
    fn apply_groups(&mut self, found: Vec<FileGroup>) {
        let groups: HashMap<PathBuf, FileGroup> =
            found.into_iter().map(|g| (g.main.clone(), g)).collect();
        if groups == *self.groups {
            return;
        }
        let main_of: HashMap<&PathBuf, &PathBuf> = groups
            .values()
            .flat_map(|g| g.members.iter().map(move |m| (m, &g.main)))
            .collect();
        let back: Vec<PathBuf> = (self.absorbed.keys())
            .filter(|p| !main_of.contains_key(p))
            .cloned()
            .collect();
        let restored = !back.is_empty();
        for p in back {
            if let Some(e) = self.absorbed.remove(&p) {
                self.entries.push(e);
            }
        }
        let absorbed = &mut self.absorbed;
        let mut absorb = |e: &SourceEntry| {
            if main_of.contains_key(&e.path) {
                absorbed.insert(e.path.clone(), e.clone());
                false
            } else {
                true
            }
        };
        self.entries.retain(&mut absorb);
        self.filtered_out.retain(&mut absorb);
        let to_main = |p: &mut PathBuf| {
            if let Some(main) = main_of.get(p) {
                *p = (*main).clone();
            }
        };
        if let Some(f) = &mut self.focus {
            to_main(f);
        }
        if let Some(a) = &mut self.selection.anchor {
            to_main(a);
        }
        if self.selection.paths.iter().any(|p| main_of.contains_key(p)) {
            let paths = std::mem::take(&mut self.selection.paths);
            self.selection.paths = paths
                .into_iter()
                .map(|mut p| {
                    to_main(&mut p);
                    p
                })
                .collect();
        }
        drop(main_of);
        self.groups = Arc::new(groups);
        if restored || !self.filter.is_all() || !self.query.trim().is_empty() {
            self.arrange();
        }
        self.generation += 1;
    }

    /// Selects these entries alone (the first of them with
    /// [`SelectMode::One`]); the paths not listed are dropped, the focus
    /// stays. An application's own "select all of that day".
    pub fn set_selection(&mut self, paths: impl IntoIterator<Item = PathBuf>) {
        let listed: HashSet<&Path> = self.entries.iter().map(|e| e.path.as_path()).collect();
        let selectable: HashSet<&Path> = (self.entries.iter())
            .filter(|e| self.selectable.accepts(e.kind))
            .map(|e| e.path.as_path())
            .collect();
        let mut chosen: BTreeSet<PathBuf> = paths
            .into_iter()
            .filter(|p| listed.contains(p.as_path()) && selectable.contains(p.as_path()))
            .collect();
        if self.select == SelectMode::One {
            chosen = chosen.into_iter().take(1).collect();
        }
        if self
            .selection
            .anchor
            .as_ref()
            .is_some_and(|a| !chosen.contains(a))
        {
            self.selection.anchor = chosen.iter().next().cloned();
        }
        self.selection.paths = chosen;
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
        self.columns.ancestors()
    }

    /// The focused folder's listing, as a mode that shows the path is given
    /// it.
    pub fn peek(&self) -> Option<&Column> {
        self.columns.peek()
    }

    /// Takes drops: files and folders dragged from inside the application
    /// (a [`DraggedPaths`] payload of `egui::DragAndDrop`) or from the system
    /// (egui's hovered and dropped files, where the platform gives them). A
    /// drop goes to a folder entry under the pointer, else to the folder
    /// shown, else to one of the application's own targets
    /// ([`FileBrowser::with_drop_targets`]); `accept` says whether that
    /// target takes it, shown while hovering, and an accepted drop comes
    /// out as [`BrowserOutput::dropped`]. A folder never takes itself or a
    /// folder it holds.
    pub fn with_drops(
        mut self,
        accept: impl Fn(&DropOffer) -> bool + Send + Sync + 'static,
    ) -> Self {
        self.drops = Some(Arc::new(accept));
        self
    }

    /// The application's own drop targets, drawn as zones under the
    /// toolbar (an import's "Add to the catalogue"); taken only with
    /// [`FileBrowser::with_drops`].
    pub fn with_drop_targets(mut self, targets: Vec<AppDropTarget>) -> Self {
        self.drop_targets = targets;
        self
    }

    /// What a drag over the browser would do, as last drawn: the offer and
    /// whether its target takes it. `None` when nothing is dragged over it.
    pub fn drop_hover(&self) -> Option<&(DropOffer, bool)> {
        self.drop_hover.as_ref()
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
    /// when missing, polled, dropped (cancelled) when not wanted. `true`
    /// while one is under way.
    fn sync_columns(&mut self) -> bool {
        let wanted = self.wanted_ancestors();
        let folder = self
            .focus_index()
            .map(|i| &self.entries[i])
            .filter(|e| e.is_dir())
            .map(|e| e.path.clone());
        self.columns.sync(
            self.source.as_ref(),
            &self.options,
            &wanted,
            &self.place,
            folder.as_deref(),
        )
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
            self.select_only(f);
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
                    self.select_only(&path);
                }
                self.reveal = true;
            }
        }
        close || back || next
    }

    /// Draws quick look over `rect`: the focused entry as large as it fits,
    /// its name under it.
    fn show_quick_look(&self, ui: &mut Ui, rect: egui::Rect, thumbnails: &dyn ThumbnailProvider) {
        let Some(e) = self.focus_index().map(|i| &self.entries[i]) else {
            return;
        };
        let ctx = ui.ctx().clone();
        let name = strings::tr_args(
            &ctx,
            &crate::strings::FILES_QUICK_LOOK,
            &[("name", &e.name)],
        );
        // An overlay over the browser (ADR 0026); above it still when the
        // browser is in a dialog.
        egui_pop_core::layers::rank_from(
            &ctx,
            egui::LayerId::new(egui::Order::Foreground, self.id.with("quick_look")),
            egui_pop_core::layers::Rank::Overlay,
            ui.layer_id(),
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
                let state = thumbnails.request(
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
            self.select_only(&first);
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
        self.filtered_out.clear();
        self.absorbed.clear();
        self.groups = Arc::default();
        self.generation += 1;
        self.error = None;
        self.listing = Some(self.source.list(&self.place, &self.options));
        self.regroup();
        self.pump();
    }

    /// Takes what the listing delivered.
    pub fn pump(&mut self) {
        let Some(listing) = &self.listing else {
            self.take_groups();
            return;
        };
        let mut done = false;
        let mut changed = false;
        for page in listing.poll() {
            match page {
                Page::Entries(mut e) => {
                    if let Some(g) = &mut self.grouper {
                        g.tell(GroupWork::Add(Grouper::names_of(&e)));
                    }
                    self.entries.append(&mut e);
                    changed = true;
                }
                Page::Updated(e) => {
                    for u in e {
                        if let Some(old) = self.absorbed.get_mut(&u.path) {
                            *old = u;
                        } else if let Some(old) = (self.entries.iter_mut())
                            .chain(self.filtered_out.iter_mut())
                            .find(|o| o.path == u.path)
                        {
                            *old = u;
                        }
                    }
                    // A time learnt later moves an entry in an agenda.
                    changed = true;
                }
                Page::Removed(paths) => {
                    if let Some(g) = &mut self.grouper {
                        let names = paths
                            .iter()
                            .filter_map(|p| p.file_name())
                            .map(|n| n.to_string_lossy().into_owned())
                            .collect();
                        g.tell(GroupWork::Remove(names));
                    }
                    self.entries.retain(|e| !paths.contains(&e.path));
                    self.filtered_out.retain(|e| !paths.contains(&e.path));
                    self.generation += 1;
                    for p in &paths {
                        self.absorbed.remove(p);
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
            self.arrange();
        }
        if done {
            self.listing = None;
        }
        self.take_groups();
        self.settle_focus();
    }

    /// Whether the listing is still under way.
    pub fn loading(&self) -> bool {
        self.listing.is_some()
    }

    pub fn error(&self) -> Option<&SourceError> {
        self.error.as_ref()
    }

    /// The entries shown, in order: a group's members are not among them
    /// ([`FileBrowser::group_of`]).
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

    /// A size of thumbnails in points (an older setting's): the next draw
    /// turns it into the count per row it gives there ([`ThumbZoom`]).
    pub fn set_thumbnail_size(&mut self, side: f32) {
        self.zoom = ThumbZoom::Points(side.max(ThumbnailsMode::MIN_SIZE));
    }

    /// The thumbnails' zoom: how many fill a row.
    pub fn zoom(&self) -> ThumbZoom {
        self.zoom
    }

    /// Sets the thumbnails' zoom; it survives a resize.
    pub fn set_zoom(&mut self, zoom: ThumbZoom) {
        self.zoom = zoom;
        self.reveal = true;
    }

    /// This many thumbnails per row (one: a thumbnail across).
    pub fn set_thumbnails_per_row(&mut self, per_row: u32) {
        self.set_zoom(ThumbZoom::PerRow(per_row.clamp(1, ThumbZoom::MOST)));
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
                .filter(|e| self.selectable.accepts(e.kind))
                .map(|e| e.path.clone())
                .collect();
        } else if self.select == SelectMode::One {
            self.select_only(path);
        }
    }

    /// What a click does: selects that entry alone, or with Ctrl adds or
    /// takes it away, or with Shift selects the range.
    pub fn click(&mut self, path: &Path, modifiers: Modifiers) {
        if modifiers.shift && self.select == SelectMode::Many {
            self.focus_on(path, true);
        } else if modifiers.command && self.select == SelectMode::Many {
            if !self.selection.paths.remove(path) && self.can_select(path) {
                self.selection.paths.insert(path.to_path_buf());
            }
            self.selection.anchor = Some(path.to_path_buf());
            self.focus = Some(path.to_path_buf());
        } else {
            self.select_only(path);
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

    /// Selects what a rubber band touches: alone, or added to the
    /// selection it started from with Ctrl or Shift held. `true` when the
    /// selection changed.
    fn apply_band(&mut self, band: &Band) -> bool {
        let keep = band.modifiers.command || band.modifiers.shift;
        let base = self
            .band_base
            .get_or_insert_with(|| {
                if keep {
                    self.selection.paths.clone()
                } else {
                    BTreeSet::new()
                }
            })
            .clone();
        let mut paths = base;
        paths.extend(
            band.cells
                .iter()
                .filter_map(|&i| self.entries.get(i))
                .filter(|e| self.selectable.accepts(e.kind))
                .map(|e| e.path.clone()),
        );
        let changed = paths != self.selection.paths;
        self.selection.paths = paths;
        if band.finished {
            self.band_base = None;
            if let Some(last) = band.cells.last().and_then(|&i| self.entries.get(i)) {
                self.focus = Some(last.path.clone());
                self.selection.anchor = Some(last.path.clone());
            }
        }
        changed
    }

    pub fn select_all(&mut self) {
        if self.select == SelectMode::Many {
            self.selection.paths = (self.entries.iter())
                .filter(|e| self.selectable.accepts(e.kind))
                .map(|e| e.path.clone())
                .collect();
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
                    self.select_only(&path);
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

    /// The toolbar's sort menu: each order, the one shown ticked (a click
    /// on it turns it the other way), and whether folders come first; then
    /// the direction, a button that turns it.
    fn sort_menu(&mut self, ui: &mut Ui) {
        let ctx = ui.ctx().clone();
        let said = |sort: PickerSort| {
            let column = strings::tr(&ctx, sort.column.title());
            let key = if sort.descending {
                &crate::strings::FILES_SORTED_DOWN
            } else {
                &crate::strings::FILES_SORTED_UP
            };
            strings::tr_args(&ctx, key, &[("column", &column)])
        };
        let name = strings::tr(&ctx, &crate::strings::FILES_SORT_BY);
        let shown = strings::tr(&ctx, self.sort.column.title());
        let mut sort = self.sort;
        let mut folders_first = self.folders_first;
        let mut grouped = self.grouped;
        let can_group = self.groupings.as_ref().is_some_and(|g| !g.is_empty());
        let r = widgets::combo_box(self.id.with("sort"))
            .selected_text(shown)
            .show_ui(ui, |ui| {
                for column in SortColumn::ALL {
                    let on = sort.column == column;
                    let label = strings::tr(&ctx, column.title());
                    let r = widgets::selectable_label(ui, on, label.as_str());
                    r.widget_info(|| {
                        egui::WidgetInfo::selected(egui::Role::MenuItemRadio, true, on, &label)
                    });
                    if r.clicked() {
                        sort = sort.click(column);
                    }
                }
                ui.separator();
                widgets::checkbox(
                    ui,
                    &mut folders_first,
                    strings::tr(&ctx, &crate::strings::FILES_FOLDERS_FIRST),
                );
                if can_group {
                    widgets::checkbox(
                        ui,
                        &mut grouped,
                        strings::tr(&ctx, &crate::strings::FILES_GROUP),
                    )
                    .on_hover_text(strings::tr(&ctx, &crate::strings::FILES_GROUP_TIP));
                }
            });
        let current = said(self.sort);
        r.response.widget_info(|| {
            egui::WidgetInfo::labeled(egui::Role::ComboBox, true, format!("{name}: {current}"))
        });
        r.response.on_hover_text(name);
        // The direction, a button of its own that turns it: the kit's sort
        // glyph, named by the order it shows.
        let side = ui.spacing().interact_size.y;
        let (rect, r) = ui.allocate_exact_size(egui::vec2(side, side), egui::Sense::click());
        r.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, &current));
        let visuals = ui.style().interact(&r);
        if r.hovered() {
            ui.painter()
                .rect_filled(rect, visuals.corner_radius, visuals.bg_fill);
        }
        let glyph = egui_pop_core::kit::Kit::of(&ctx).glyph(if self.sort.descending {
            egui_pop_core::kit::Glyph::SortDescending
        } else {
            egui_pop_core::kit::Glyph::SortAscending
        });
        glyph.paint(ui.painter(), rect.shrink(side * 0.25), visuals.text_color());
        if r.clicked() {
            sort.descending = !sort.descending;
        }
        self.set_sort(sort);
        self.set_folders_first(folders_first);
        self.set_grouped(grouped);
    }

    /// Draws the toolbar's mode switcher and the entries.
    pub fn show(&mut self, ui: &mut Ui) -> BrowserOutput {
        let mut out = BrowserOutput::default();
        self.pump();
        if self.loading() || self.groups_pending() {
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
                // Thumbnails per row, fewer to the right: larger ones.
                let name = strings::tr(ui.ctx(), &crate::strings::FILES_THUMBNAIL_SIZE);
                let (width, gap) = self.grid_room;
                let most = ThumbZoom::most_per_row(width, gap);
                let mut per_row = self.zoom.shown_per_row(width, gap);
                let r = ui.add(
                    egui::Slider::new(&mut per_row, most..=1)
                        .show_value(false)
                        .text(name),
                );
                if r.changed() {
                    self.set_thumbnails_per_row(per_row);
                }
            }
            // The order, unless the mode orders by time on its own.
            if !infos.iter().any(|m| m.id == self.mode_id && m.needs.time) {
                self.sort_menu(ui);
            }
            // The mode's own controls (the agenda's level).
            if self.mode.toolbar(ui, &mut self.settings) {
                self.reveal = true;
            }
        });
        if let Some(e) = &self.error {
            ui.label(e.to_string());
            return out;
        }
        // What is dragged, if anything: from the system, or from the app.
        let drag = self.drops.as_ref().and_then(|_| dragged(ui.ctx()));
        // The application's own drop targets.
        let zones = self.drop_zones(ui);
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
                        key,
                        pressed: true,
                        modifiers: mods,
                        ..
                    } if self
                        .mode
                        .own_key(key, mods, self.focus_index(), &mut self.settings) =>
                    {
                        self.reveal = true;
                    }
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
        } else {
            // Not shown: dropped, which cancels what is under way.
            self.columns.clear();
        }
        let focus = self.focus_index();
        let reveal = std::mem::take(&mut self.reveal);
        // A group's entry carries its members' chip, after the
        // application's marks, and stands in with a member's thumbnail.
        let (groups, app_marks) = (self.groups.clone(), self.marks.clone());
        let ctx = ui.ctx().clone();
        let with_groups = move |e: &SourceEntry| {
            let mut marks = app_marks.as_ref().map(|m| m(e)).unwrap_or_default();
            if let Some(g) = groups.get(&e.path) {
                marks.chips.extend(group_chips(&ctx, g));
            }
            marks
        };
        let grouped_thumbnails = GroupThumbnails {
            inner: self.thumbnails.as_ref(),
            groups: &self.groups,
            absorbed: &self.absorbed,
        };
        let (marks, thumbnails): (Option<&MarkEntries>, &dyn ThumbnailProvider) =
            if self.groups.is_empty() {
                (self.marks.as_deref(), self.thumbnails.as_ref())
            } else {
                (Some(&with_groups), &grouped_thumbnails)
            };
        thumbnails.begin_frame(ui.ctx());
        let inner = ui.scope(|ui| {
            // The zoom, a count per row, as the side the modes draw at.
            let (width, gap) = ThumbnailGrid::room(ui);
            self.grid_room = (width, gap);
            self.zoom = self.zoom.resolved(width, gap);
            self.settings.thumbnail_size = self.zoom.side(width, gap);
            self.mode.show(
                ui,
                ModeContext {
                    entries: &self.entries,
                    selection: &self.selection,
                    focus,
                    thumbnails,
                    settings: &mut self.settings,
                    id: self.id,
                    reveal_focus: reveal,
                    max_height: None,
                    drawer: None,
                    name: None,
                    many: false,
                    place: &self.place,
                    ancestors: self.columns.ancestors(),
                    peek: self.columns.peek(),
                    loading: self.listing.is_some(),
                    generation: self.generation,
                    marks,
                },
            )
        });
        // Quick look asks within the frame, so its thumbnail is renewed.
        if self.quick_look && self.focus_index().is_none() {
            self.quick_look = false;
        }
        if self.quick_look {
            self.show_quick_look(ui, inner.response.rect, thumbnails);
        }
        thumbnails.end_frame(ui.ctx());
        let mode_out = inner.inner;
        // A rubber band selects as it goes, the focus on its last entry once
        // let go.
        let band = ui.ctx().data_mut(|d| {
            let key = band_key(self.id);
            let band = d.get_temp::<Band>(key);
            d.remove::<Band>(key);
            band
        });
        match band {
            Some(band) if self.select == SelectMode::Many => {
                out.selection_changed |= self.apply_band(&band);
                if band.finished {
                    ui.memory_mut(|m| m.request_focus(keys_id));
                }
            }
            _ => self.band_base = None,
        }
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
        // Drops: the target under the pointer, shown, and taken on release.
        self.drop_hover = None;
        if let Some((paths, origin, released)) = drag {
            let at = ui.input(|i| i.pointer.latest_pos());
            let entries_rect = inner.response.rect;
            let target = self.drop_target(at, entries_rect, &zones, &mode_out);
            if let Some((target, rect)) = target {
                let offer = DropOffer {
                    target,
                    paths,
                    origin,
                };
                let accepted = self.accepts(&offer);
                self.paint_drop(ui, entries_rect, rect, &offer, accepted);
                if origin == DropOrigin::System {
                    // The window shows nothing more over it.
                    crate::bus::files::shown(ui.ctx());
                }
                if released {
                    if accepted {
                        match origin {
                            DropOrigin::App => egui::DragAndDrop::clear_payload(ui.ctx()),
                            // Said on the window's `pop.files.drop` by the
                            // browser's listener (`pop.files.browser`).
                            DropOrigin::System => crate::bus::files::take(
                                ui.ctx(),
                                match &offer.target {
                                    DropTarget::Folder(p) | DropTarget::Place(p) => {
                                        format!("a file browser took them into `{}`", p.display())
                                    }
                                    DropTarget::App(id) => {
                                        format!("a file browser took them for `{id}`")
                                    }
                                },
                            ),
                        }
                        out.dropped = Some(offer);
                    }
                } else {
                    self.drop_hover = Some((offer, accepted));
                }
            }
        }
        if let Some(r) = mode_out.reach {
            self.reach(&r);
            out.selection_changed = true;
            out.place_changed = true;
            ui.memory_mut(|m| m.request_focus(keys_id));
        }
        out
    }

    /// Draws the application's drop targets, each a zone named by its
    /// label, lit while a drag is over it. Answers their rectangles.
    fn drop_zones(&self, ui: &mut Ui) -> Vec<(String, egui::Rect)> {
        if self.drops.is_none() || self.drop_targets.is_empty() {
            return Vec::new();
        }
        let mut zones = Vec::new();
        ui.horizontal(|ui| {
            for t in &self.drop_targets {
                let lit = self
                    .drop_hover
                    .as_ref()
                    .filter(|(o, _)| o.target == DropTarget::App(t.id.clone()));
                let visuals = ui.visuals();
                let (stroke, fill) = match lit {
                    Some((_, true)) => (
                        egui::Stroke::new(2.0, visuals.selection.stroke.color),
                        visuals.selection.bg_fill.gamma_multiply(0.5),
                    ),
                    Some((_, false)) => (
                        egui::Stroke::new(2.0, visuals.error_fg_color),
                        egui::Color32::TRANSPARENT,
                    ),
                    None => (
                        egui::Stroke::new(1.0, visuals.widgets.noninteractive.bg_stroke.color),
                        egui::Color32::TRANSPARENT,
                    ),
                };
                let r = egui::Frame::new()
                    .inner_margin(egui::Margin::symmetric(10, 4))
                    .corner_radius(4.0)
                    .stroke(stroke)
                    .fill(fill)
                    .show(ui, |ui| ui.label(&t.label))
                    .response;
                zones.push((t.id.clone(), r.rect));
            }
        });
        zones
    }

    /// Where a drop at `at` goes: an application's zone, a folder entry
    /// (in the place's column or beside it), else the folder shown when the
    /// pointer is over the entries, or not known (a system drag the
    /// platform gives no position for). With the rectangle to light.
    fn drop_target(
        &self,
        at: Option<egui::Pos2>,
        entries: egui::Rect,
        zones: &[(String, egui::Rect)],
        mode_out: &ModeOutput,
    ) -> Option<(DropTarget, egui::Rect)> {
        let Some(at) = at else {
            return Some((DropTarget::Place(self.place.clone()), entries));
        };
        if let Some((id, r)) = zones.iter().find(|(_, r)| r.contains(at)) {
            return Some((DropTarget::App(id.clone()), *r));
        }
        if !entries.contains(at) {
            return None;
        }
        if let Some((i, r)) = mode_out.pointed
            && let Some(e) = self.entries.get(i).filter(|e| e.is_dir())
        {
            return Some((DropTarget::Folder(e.path.clone()), r));
        }
        if let Some((p, r)) = &mode_out.pointed_beside {
            return Some((DropTarget::Folder(p.clone()), *r));
        }
        Some((DropTarget::Place(self.place.clone()), entries))
    }

    /// Whether the target takes the offer: never a folder into itself or
    /// into a folder it holds, else as the application says.
    fn accepts(&self, offer: &DropOffer) -> bool {
        let into = match &offer.target {
            DropTarget::Folder(f) | DropTarget::Place(f) => Some(f),
            DropTarget::App(_) => None,
        };
        if into.is_some_and(|f| offer.paths.iter().any(|p| f.starts_with(p))) {
            return false;
        }
        self.drops.as_ref().is_some_and(|accept| accept(offer))
    }

    /// The feedback of a drag: the target's outline (the selection's
    /// colour when it takes the drop, the error's when it refuses), the
    /// cursor, and a caption over the entries that says what a release
    /// does, for assistive technology too.
    fn paint_drop(
        &self,
        ui: &mut Ui,
        entries: egui::Rect,
        rect: egui::Rect,
        offer: &DropOffer,
        accepted: bool,
    ) {
        let ctx = ui.ctx().clone();
        let visuals = ui.visuals().clone();
        let color = if accepted {
            visuals.selection.stroke.color
        } else {
            visuals.error_fg_color
        };
        let painter = ctx.layer_painter(egui::LayerId::new(
            egui::Order::Foreground,
            self.id.with("drop"),
        ));
        if accepted {
            painter.rect_filled(rect, 4.0, visuals.selection.bg_fill.gamma_multiply(0.35));
        }
        painter.rect_stroke(
            rect.shrink(1.0),
            4.0,
            egui::Stroke::new(2.0, color),
            egui::StrokeKind::Inside,
        );
        ctx.set_cursor_icon(if accepted {
            egui::CursorIcon::Copy
        } else {
            egui::CursorIcon::NoDrop
        });
        let text = if accepted {
            let name = match &offer.target {
                DropTarget::Folder(p) | DropTarget::Place(p) => p.file_name().map_or_else(
                    || p.display().to_string(),
                    |n| n.to_string_lossy().into_owned(),
                ),
                DropTarget::App(id) => self
                    .drop_targets
                    .iter()
                    .find(|t| &t.id == id)
                    .map_or_else(|| id.clone(), |t| t.label.clone()),
            };
            strings::tr_args(&ctx, &crate::strings::FILES_DROP_INTO, &[("name", &name)])
        } else {
            strings::tr(&ctx, &crate::strings::FILES_DROP_REFUSED)
        };
        let font = egui::TextStyle::Body.resolve(ui.style());
        let galley = painter.layout_no_wrap(text.clone(), font, visuals.strong_text_color());
        let size = galley.size() + egui::vec2(16.0, 8.0);
        // Over the part of the entries in view: they may run on below.
        let shown = entries.intersect(ui.clip_rect());
        let shown = if shown.is_positive() { shown } else { entries };
        let caption = egui::Rect::from_center_size(
            egui::pos2(shown.center().x, shown.bottom() - size.y / 2.0 - 8.0),
            size,
        );
        painter.rect_filled(caption, 4.0, visuals.window_fill);
        painter.rect_stroke(
            caption,
            4.0,
            egui::Stroke::new(1.0, color),
            egui::StrokeKind::Inside,
        );
        painter.galley(
            caption.center() - galley.size() / 2.0,
            galley,
            visuals.strong_text_color(),
        );
        let r = ui.interact(caption, self.id.with("drop.caption"), egui::Sense::hover());
        r.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Label, true, &text));
    }
}

/// Paths dragged inside the application, for a file browser to take: the
/// payload a widget sets with `egui::DragAndDrop::set_payload` as a drag
/// starts.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DraggedPaths(pub Vec<PathBuf>);

/// Where a drop goes.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum DropTarget {
    /// A folder entry: in the place's column, or beside it in Finder's
    /// columns.
    Folder(PathBuf),
    /// The folder shown: a drop on a file or on empty space.
    Place(PathBuf),
    /// One of the application's own targets, by its id.
    App(String),
}

/// Where what is dropped comes from.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum DropOrigin {
    /// A [`DraggedPaths`] payload set inside the application.
    App,
    /// Files dragged from another program, through egui's hovered and
    /// dropped files.
    System,
}

/// A drop on offer: what the application's `accept` is asked about while
/// hovering, and what [`BrowserOutput::dropped`] gives once it is taken.
/// While a system drag hovers, `paths` may be empty: some platforms say the
/// files only on the drop.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DropOffer {
    pub target: DropTarget,
    pub paths: Vec<PathBuf>,
    pub origin: DropOrigin,
}

/// One of the application's own drop targets: an id it is reported by, and
/// the label its zone shows.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AppDropTarget {
    pub id: String,
    pub label: String,
}

impl AppDropTarget {
    pub fn new(id: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
        }
    }
}

type DropPolicy = Arc<dyn Fn(&DropOffer) -> bool + Send + Sync>;

/// What is being dragged this frame: its paths, where from, and whether it
/// is released (dropped) this frame.
fn dragged(ctx: &egui::Context) -> Option<(Vec<PathBuf>, DropOrigin, bool)> {
    // Files from another program: as the window's pass read them (the
    // bus's files module, the one reader of egui's dropped files).
    if let Some((paths, dropped)) = crate::bus::files::system_drag(ctx) {
        return Some((paths, DropOrigin::System, dropped));
    }
    let released = ctx.input(|i| i.pointer.any_released());
    let payload = egui::DragAndDrop::payload::<DraggedPaths>(ctx)?;
    Some((payload.0.clone(), DropOrigin::App, released))
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
    fn the_carousel_steps_along_one_row() {
        let c = CarouselMode { per_side: 3 };
        assert_eq!(c.step(5, 13, ModeKey::Right), 6);
        assert_eq!(c.step(5, 13, ModeKey::Left), 4);
        assert_eq!(c.step(5, 13, ModeKey::Up), 5);
        assert_eq!(c.step(5, 13, ModeKey::PageDown), 8);
        assert_eq!(c.step(1, 13, ModeKey::PageUp), 0);
        assert_eq!(c.step(5, 13, ModeKey::End), 12);
    }

    fn dated(name: &str, day: u64, taken: Option<u64>) -> SourceEntry {
        let at =
            |d: u64| SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(d * 86_400 + 3600);
        SourceEntry {
            path: PathBuf::from(format!("/p/{name}")),
            name: name.into(),
            kind: EntryKind::File,
            size: None,
            modified: (day > 0).then(|| at(day)),
            taken: taken.map(at),
            app_id: None,
        }
    }

    /// An agenda laid out for `entries`, `columns` to a row.
    fn laid(
        entries: &[SourceEntry],
        columns: usize,
        level: AgendaLevel,
        collapsed: &[Option<i64>],
    ) -> AgendaMode {
        let mut a = AgendaMode {
            rows_in_view: 2,
            ..AgendaMode::default()
        };
        a.index.update(entries, 1, level);
        a.layout
            .update(&a.index, columns, &collapsed.iter().copied().collect());
        a
    }

    #[test]
    fn the_agenda_files_entries_by_day_the_newest_first_the_taken_time_first() {
        let entries = [
            dated("a", 10, None),
            dated("b", 12, None),
            // Changed on day 12, taken on day 3.
            dated("c", 12, Some(3)),
            dated("d", 10, None),
            dated("e", 0, None),
            dated("f", 10, None),
        ];
        let a = laid(&entries, 2, AgendaLevel::Day, &[]);
        assert_eq!(a.index.order, [1, 0, 3, 5, 2, 4]);
        let keys: Vec<_> = a.index.groups.iter().map(|(k, r)| (*k, r.len())).collect();
        assert_eq!(
            keys,
            [(Some(12), 1), (Some(10), 3), (Some(3), 1), (None, 1)]
        );
        assert_eq!(
            a.layout.lines,
            [
                AgendaLine::Heading(0),
                AgendaLine::Cells(0, 1),
                AgendaLine::Heading(1),
                AgendaLine::Cells(1, 3),
                AgendaLine::Cells(3, 4),
                AgendaLine::Heading(2),
                AgendaLine::Cells(4, 5),
                AgendaLine::Heading(3),
                AgendaLine::Cells(5, 6),
            ]
        );
    }

    #[test]
    fn the_agenda_groups_by_month_and_by_year() {
        // 1970-01-11, 1970-01-13, 1970-02-15, 1971-01-01.
        let entries = [
            dated("a", 10, None),
            dated("b", 12, None),
            dated("c", 45, None),
            dated("d", 365, None),
        ];
        let a = laid(&entries, 4, AgendaLevel::Month, &[]);
        let heads: Vec<_> = a
            .index
            .groups
            .iter()
            .map(|(k, r)| (AgendaLevel::Month.heading(k.unwrap()), r.len()))
            .collect();
        assert_eq!(
            heads,
            [
                ("1971-01".into(), 1),
                ("1970-02".into(), 1),
                ("1970-01".into(), 2)
            ]
        );
        let a = laid(&entries, 4, AgendaLevel::Year, &[]);
        let heads: Vec<_> = a
            .index
            .groups
            .iter()
            .map(|(k, r)| (AgendaLevel::Year.heading(k.unwrap()), r.len()))
            .collect();
        assert_eq!(heads, [("1971".into(), 1), ("1970".into(), 3)]);
        assert_eq!(AgendaLevel::Day.coarser().coarser(), AgendaLevel::Year);
        assert_eq!(AgendaLevel::Year.coarser(), AgendaLevel::Year);
        assert_eq!(AgendaLevel::Year.finer().finer(), AgendaLevel::Day);
    }

    #[test]
    fn the_agenda_steps_across_its_days() {
        let entries = [
            dated("a", 10, None),
            dated("b", 12, None),
            dated("c", 3, None),
            dated("d", 10, None),
            dated("e", 10, None),
        ];
        // Day 12: b. Day 10: a d / e. Day 3: c.
        let a = laid(&entries, 2, AgendaLevel::Day, &[]);
        // Right and Left in the agenda's order, across days.
        assert_eq!(a.step(1, 5, ModeKey::Right), 0);
        assert_eq!(a.step(4, 5, ModeKey::Right), 2);
        assert_eq!(a.step(0, 5, ModeKey::Left), 1);
        // Down keeps the column as near as it goes, into the next day.
        assert_eq!(a.step(1, 5, ModeKey::Down), 0);
        assert_eq!(a.step(3, 5, ModeKey::Down), 4);
        assert_eq!(a.step(4, 5, ModeKey::Down), 2);
        assert_eq!(a.step(3, 5, ModeKey::Up), 1);
        assert_eq!(a.step(2, 5, ModeKey::Down), 2);
        assert_eq!(a.step(1, 5, ModeKey::PageDown), 4);
        assert_eq!(a.step(0, 5, ModeKey::Home), 1);
        assert_eq!(a.step(0, 5, ModeKey::End), 2);
        // Laid out for other entries: their own order.
        assert_eq!(a.step(1, 9, ModeKey::Right), 2);
    }

    #[test]
    fn the_agenda_skips_a_collapsed_group() {
        let entries = [
            dated("a", 10, None),
            dated("b", 12, None),
            dated("c", 3, None),
            dated("d", 10, None),
        ];
        // Day 12: b. Day 10 (collapsed): a d. Day 3: c.
        let a = laid(&entries, 2, AgendaLevel::Day, &[Some(10)]);
        assert_eq!(a.layout.shown, [1, 2]);
        assert_eq!(a.step(1, 4, ModeKey::Right), 2);
        assert_eq!(a.step(1, 4, ModeKey::Down), 2);
        // From inside the collapsed group: the nearest entry shown that way.
        assert_eq!(a.step(0, 4, ModeKey::Down), 2);
        assert_eq!(a.step(3, 4, ModeKey::Up), 1);
        assert_eq!(a.step(0, 4, ModeKey::End), 2);
        let all = laid(
            &entries,
            2,
            AgendaLevel::Day,
            &[Some(12), Some(10), Some(3)],
        );
        assert_eq!(all.step(0, 4, ModeKey::Down), 0);
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
