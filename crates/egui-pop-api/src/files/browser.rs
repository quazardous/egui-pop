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
}

/// What a mode is, for the switcher, persistence, agents and screen readers.
#[derive(Clone, Debug, PartialEq)]
pub struct ModeInfo {
    /// `details`, `thumbnails`, `filmstrip`…
    pub id: String,
    pub label: String,
    /// A short glyph for the switcher's button.
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
}

/// What the person did in a mode; the browser decides what it means.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ModeOutput {
    pub clicked: Option<(usize, Modifiers)>,
    /// A double click.
    pub activated: Option<usize>,
    /// The entries it drew.
    pub visible: Range<usize>,
}

/// A way to show a browser's entries. It draws and reports; selection rules,
/// what Enter does and what a click on a folder does are the browser's.
pub trait ViewMode: Send {
    fn describe(&self) -> ModeInfo;

    fn show(&mut self, ui: &mut Ui, cx: ModeContext<'_>) -> ModeOutput;

    /// Where a key moves the focus from `from`, among `count` entries, in
    /// the layout last drawn.
    fn step(&self, from: usize, count: usize, key: ModeKey) -> usize;
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

/// Rows with columns: the name, the size and when it last changed.
#[derive(Debug, Default)]
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
            icon: "☰".into(),
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
        let row = Self::ROW_HEIGHT;
        let mut area = egui::ScrollArea::vertical()
            .id_salt(cx.id.with("details"))
            .auto_shrink([false, false]);
        if cx.reveal_focus
            && let Some(f) = cx.focus
        {
            // Keep the focused row in view, scrolling as little as it takes.
            let spacing = ui.spacing().item_spacing.y;
            let top = f as f32 * (row + spacing);
            let state = egui::scroll_area::State::load(
                ui.ctx(),
                ui.make_persistent_id(cx.id.with("details")),
            );
            let (offset, height) = state
                .map(|s| (s.clamped_offset().y, ui.available_height()))
                .unwrap_or((0.0, ui.available_height()));
            if top < offset {
                area = area.vertical_scroll_offset(top);
            } else if top + row > offset + height {
                area = area.vertical_scroll_offset(top + row - height);
            }
        }
        area.show_rows(ui, row, cx.entries.len(), |ui, range| {
            self.rows_in_view = range.len().max(1);
            out.visible = range.clone();
            for i in range {
                let e = &cx.entries[i];
                let selected = cx.selection.contains(&e.path);
                let focused = cx.focus == Some(i);
                let (rect, resp) = ui.allocate_exact_size(
                    egui::vec2(ui.available_width(), row),
                    egui::Sense::click(),
                );
                entry_response(&resp, e, selected);
                let visuals = ui.visuals();
                if selected {
                    ui.painter()
                        .rect_filled(rect, 2.0, visuals.selection.bg_fill);
                } else if resp.hovered() {
                    ui.painter()
                        .rect_filled(rect, 2.0, visuals.widgets.hovered.weak_bg_fill);
                }
                if focused {
                    ui.painter().rect_stroke(
                        rect.shrink(0.5),
                        2.0,
                        visuals.selection.stroke,
                        egui::StrokeKind::Inside,
                    );
                }
                let text = if selected {
                    visuals.selection.stroke.color
                } else {
                    visuals.text_color()
                };
                let font = egui::TextStyle::Body.resolve(ui.style());
                let w = rect.width();
                let name = if e.is_dir() {
                    format!("🗀 {}", e.name)
                } else {
                    e.name.clone()
                };
                let y = rect.center().y;
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
                report(&resp, i, &mut out);
            }
        });
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

/// A virtualised grid of thumbnails with their names. Only the rows in view
/// ask for a thumbnail (the next row as `Near`), so what scrolls away is
/// cancelled by the provider.
#[derive(Debug, Default)]
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
            icon: "⊞".into(),
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
                time: false,
            },
        }
    }

    fn show(&mut self, ui: &mut Ui, cx: ModeContext<'_>) -> ModeOutput {
        let mut out = ModeOutput::default();
        let side = cx
            .settings
            .thumbnail_size
            .clamp(Self::MIN_SIZE, Self::MAX_SIZE);
        let caption = ui.text_style_height(&egui::TextStyle::Small) + 4.0;
        let cell = egui::vec2(side + 8.0, side + caption + 8.0);
        let columns = ((ui.available_width() / cell.x).floor() as usize).max(1);
        self.columns = columns;
        let n = cx.entries.len();
        let rows = n.div_ceil(columns);
        let pixels = (side * ui.ctx().pixels_per_point()).round() as u32;
        let spacing = ui.spacing().item_spacing.y;
        let mut area = egui::ScrollArea::vertical()
            .id_salt(cx.id.with("thumbnails"))
            .auto_shrink([false, false]);
        if cx.reveal_focus
            && let Some(f) = cx.focus
        {
            let top = (f / columns) as f32 * (cell.y + spacing);
            let state = egui::scroll_area::State::load(
                ui.ctx(),
                ui.make_persistent_id(cx.id.with("thumbnails")),
            );
            let offset = state.map(|s| s.clamped_offset().y).unwrap_or(0.0);
            let height = ui.available_height();
            if top < offset {
                area = area.vertical_scroll_offset(top);
            } else if top + cell.y > offset + height {
                area = area.vertical_scroll_offset(top + cell.y - height);
            }
        }
        let ctx = ui.ctx().clone();
        area.show_rows(ui, cell.y, rows, |ui, range| {
            self.rows_in_view = range.len().max(1);
            let first = range.start * columns;
            let end = (range.end * columns).min(n);
            out.visible = first..end;
            // Visible first, in reading order; then the next row.
            for e in &cx.entries[first..end] {
                cx.thumbnails.request(
                    &ctx,
                    &ThumbRequest {
                        entry: e,
                        size: pixels,
                        priority: Priority::Visible,
                    },
                );
            }
            for e in &cx.entries[end..(end + columns).min(n)] {
                cx.thumbnails.request(
                    &ctx,
                    &ThumbRequest {
                        entry: e,
                        size: pixels,
                        priority: Priority::Near,
                    },
                );
            }
            for row in range {
                ui.horizontal(|ui| {
                    for i in row * columns..((row + 1) * columns).min(n) {
                        let e = &cx.entries[i];
                        let selected = cx.selection.contains(&e.path);
                        let (rect, resp) = ui.allocate_exact_size(cell, egui::Sense::click());
                        entry_response(&resp, e, selected);
                        let visuals = ui.visuals();
                        if selected {
                            ui.painter()
                                .rect_filled(rect, 4.0, visuals.selection.bg_fill);
                        } else if resp.hovered() {
                            ui.painter().rect_filled(
                                rect,
                                4.0,
                                visuals.widgets.hovered.weak_bg_fill,
                            );
                        }
                        if cx.focus == Some(i) {
                            ui.painter().rect_stroke(
                                rect.shrink(0.5),
                                4.0,
                                visuals.selection.stroke,
                                egui::StrokeKind::Inside,
                            );
                        }
                        let image = egui::Rect::from_min_size(
                            rect.min + egui::vec2(4.0, 4.0),
                            egui::vec2(side, side),
                        );
                        let state = cx.thumbnails.request(
                            &ctx,
                            &ThumbRequest {
                                entry: e,
                                size: pixels,
                                priority: Priority::Visible,
                            },
                        );
                        match state {
                            ThumbState::Ready(t) => {
                                let s = t.size_vec2();
                                let scale = (side / s.x.max(s.y)).min(1.0);
                                let r = egui::Rect::from_center_size(image.center(), s * scale);
                                ui.painter().image(
                                    t.id(),
                                    r,
                                    egui::Rect::from_min_max(
                                        egui::pos2(0.0, 0.0),
                                        egui::pos2(1.0, 1.0),
                                    ),
                                    egui::Color32::WHITE,
                                );
                            }
                            other => {
                                let glyph = match other {
                                    _ if e.is_dir() => "🗀",
                                    ThumbState::Failed(_) => "⚠",
                                    ThumbState::Loading => "…",
                                    _ => "🗋",
                                };
                                ui.painter()
                                    .rect_filled(image, 4.0, visuals.extreme_bg_color);
                                ui.painter().text(
                                    image.center(),
                                    egui::Align2::CENTER_CENTER,
                                    glyph,
                                    egui::FontId::proportional(side * 0.3),
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
                        report(&resp, i, &mut out);
                    }
                });
            }
        });
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
}

/// Type-ahead's prefix and when it was last typed.
#[derive(Clone, Debug, Default)]
struct TypeAhead {
    prefix: String,
    at: f64,
}

/// The file browser: entries from a [`FileSource`], shown by a
/// [`ViewMode`] chosen in its toolbar and remembered under its id.
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
            // Folders first, then names as people read them.
            self.entries.sort_by(|a, b| {
                b.is_dir()
                    .cmp(&a.is_dir())
                    .then_with(|| natural_cmp(&a.name, &b.name))
            });
        }
        if done {
            self.listing = None;
        }
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

    fn mode_key() -> &'static str {
        "egui_pop.file_browser.mode"
    }

    /// Draws the toolbar's mode switcher and the entries.
    pub fn show(&mut self, ui: &mut Ui) -> BrowserOutput {
        let mut out = BrowserOutput::default();
        if !self.restored {
            self.restored = true;
            let kept: Option<String> = ui
                .ctx()
                .data_mut(|d| d.get_persisted(self.id.with(Self::mode_key())));
            if let Some(kept) = kept {
                self.set_mode(&kept);
            }
        }
        self.pump();
        if self.loading() {
            ui.ctx().request_repaint();
        }
        // The switcher.
        ui.horizontal(|ui| {
            for id in self.modes.ids() {
                let Some(info) = self.modes.make(&id).map(|m| m.describe()) else {
                    continue;
                };
                let on = id == self.mode_id;
                let tip = info
                    .keys
                    .iter()
                    .map(|(k, what)| format!("{k}: {what}"))
                    .collect::<Vec<_>>()
                    .join("\n");
                let resp = ui
                    .selectable_label(on, format!("{} {}", info.icon, info.label))
                    .on_hover_text(tip);
                if resp.clicked() && !on {
                    self.set_mode(&id);
                    out.mode_changed = true;
                }
            }
            if self
                .modes
                .make(&self.mode_id)
                .is_some_and(|m| m.describe().needs.thumbnails)
            {
                ui.add(
                    egui::Slider::new(
                        &mut self.settings.thumbnail_size,
                        ThumbnailsMode::MIN_SIZE..=ThumbnailsMode::MAX_SIZE,
                    )
                    .show_value(false)
                    .text("Size"),
                );
            }
        });
        if out.mode_changed {
            let id = self.mode_id.clone();
            ui.ctx()
                .data_mut(|d| d.insert_persisted(self.id.with(Self::mode_key()), id));
        }
        if let Some(e) = &self.error {
            ui.label(e.to_string());
            return out;
        }
        // The keyboard, while the entries hold the focus.
        let keys_id = self.id.with("entries");
        let focused = ui.memory(|m| m.has_focus(keys_id));
        if focused {
            let (events, modifiers) = ui.input(|i| (i.events.clone(), i.modifiers));
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
                        }
                    }
                    egui::Event::Text(t) if !modifiers.command => {
                        let now = ui.input(|i| i.time);
                        self.type_ahead(&t, now);
                    }
                    _ => {}
                }
            }
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
                },
            )
        });
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
            out.selection_changed = true;
            ui.memory_mut(|m| m.request_focus(keys_id));
        }
        if let Some(i) = mode_out.activated {
            out.activated = Some(self.entries[i].path.clone());
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
