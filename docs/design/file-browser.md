# One file browser, many views: a design

Status: **proposed** (2026-11-08). Nothing here is decided until the
maintainer answers the questions in section 7.

The request: the file part of a photo import must share one component with
every file dialog, and let the person change how the files are shown:
details, thumbnails, a filmstrip, a carousel, Finder's columns, an agenda by
date, and any view that comes later. So the view is a **contract**, not a
`match` on two cases.

Today egui-pop's in-app picker ([Choosing files](../files.md)) draws a list
and a grid itself (`PickerView::List`, `PickerView::Grid` in
`egui_pop::api::files`), inside a `FilePicker` that also owns the field, the
places, the preview pane, the selection, the sort and the grouping. A Helios
import window that wants a filmstrip would have to fork it. This note splits
the picker into a **file browser** that hosts pluggable **view modes**, fed
by a **file source** and a **thumbnail provider**, and moves the file and
folder choosers onto it.

Scope:

- the browser: entries, selection, focus, type-ahead, sort, quick look,
  drop targets, the mode switcher;
- three contracts: `FileSource`, `ViewMode` (with a registry),
  `ThumbnailProvider`;
- the six built-in modes;
- the move of `FilePicker` onto the browser.

Out of scope: Helios's catalogue (its image map and thumbnail cache is
another piece of work; here we define only the provider contract it will
implement), tags and ratings editing, copying and moving files.

## 1. Survey

Read on 2026-11-08. Sources in section 9. "Unconfirmed" marks what no source
we read states.

### 1.1 View modes

| App | Modes | Switch | Remembered |
|---|---|---|---|
| macOS Finder | Icons, List, Columns, Gallery | Cmd+1..4, a segmented control in the toolbar | Per folder ("Always open in…"), else the last used |
| Windows Explorer | Extra large, large, medium, small icons, List, Details, Tiles, Content | Ctrl+Shift+1..8, Ctrl+wheel steps through them | Per folder type ("Optimize this folder for: Pictures") and per folder |
| GNOME Files (Nautilus) | Grid, List | Ctrl+1, Ctrl+2, a toggle in the header bar | Per folder (metadata), default in the settings |
| KDE Dolphin | Icons, Compact, Details | Ctrl+1..3, toolbar buttons | Per folder (`.directory`) or globally, a setting |
| Lightroom Classic, import | Grid, Loupe | G, E | Last used |
| Lightroom Classic, Library | Grid, Loupe, Compare, Survey, plus the Filmstrip always at the bottom | G, E, C, N | Last used |
| Photo Mechanic | Contact sheet (grid), Preview (one image, with a filmstrip), a "Columns" layout of the contact sheet | Double click or Enter opens Preview | Per contact sheet window |
| darktable, import | A list of files, with an optional thumbnail column | An eye icon per row | The thumbnails' column state |
| darktable, lighttable | File manager (grid), Zoomable lighttable, Culling, Preview; the filmstrip in other views; the timeline at the bottom | A toolbar control, X (culling), W (preview) | Last used |
| digiKam | Thumbnails, Preview, Map, Table; Albums by folder or **by date** (the Dates view, Timeline) | Toolbar, F3 preview | Last used |

Patterns:

- **Every file manager has a list with columns and a grid with a size
  slider.** Those two are the minimum; egui-pop has them.
- **Photo tools add a one-image view with a strip** (Lightroom's Loupe and
  Filmstrip, Photo Mechanic's Preview, darktable's filmstrip, digiKam's
  Preview). The strip is a view of the same entries, the big image a quick
  look of the focused one.
- **Finder's columns** are the one browsing mode that shows the path as
  panes; Dolphin and Explorer do not have it. Some people swear by it for
  deep trees.
- **Time is a view in photo tools** (digiKam's Dates and Timeline,
  darktable's timeline, Lightroom's date folders on import), never in file
  managers, which only group by date (Nautilus's "Group by", Explorer's
  "Group by > Date modified").
- **A key per mode** (Cmd/Ctrl+1..n) everywhere; a toolbar control showing
  the current mode everywhere.
- **Remembered per folder** in file managers, **last used** in photo tools
  and in dialogs. A dialog that remembers per folder surprises: GTK's
  chooser keeps one view for all folders.

### 1.2 Thumbnails

| App | Sizes | Loading order |
|---|---|---|
| Finder | 16 to 512 points, a slider | Visible first; QuickLook's thumbnails, cached by the system |
| Explorer | 16, 48, 96, 256 (the icon sizes), Ctrl+wheel in between | Visible first, from `thumbcache_*.db`; a shell extension per type |
| GNOME Files | Grid zoom levels (about 64 to 256 pixels), the setting "show thumbnails: local only, all, never" | Visible first; the freedesktop cache (`~/.cache/thumbnails/normal` 128, `large` 256, `x-large` 512, `xx-large` 1024) |
| Dolphin | Zoom slider 16 to 256 | Visible first, then the rest of the folder in the background (`KFilePreviewGenerator`), size cap for remote files |
| Lightroom import | Thumbnails slider; embedded previews of the raw | The embedded JPEG first, "Standard" and "1:1" previews later (a choice on import) |
| Photo Mechanic | A slider; the embedded JPEG only, never a raw decode | Visible first, then the whole folder; known for being the fastest because it never renders raws |
| darktable import | One small size, on demand per row | Only when asked (the eye) |
| darktable lighttable | Per zoom level, its own mipmap cache (`mipmaps-*` on disk) | Visible first, then a background generation option |
| digiKam | 32 to 256 (and a "high quality" option), its own thumbnail database | Visible first |

Patterns: **visible first, the rest in the background, cancelled when
scrolled away**; a **small number of sizes** cached (the freedesktop cache's
four, darktable's mipmap levels) with drawing scaled in between; **embedded
previews** of raws before any decode. egui-pop's grid already asks only for
what is in view and drops what left it (the `Previews::want` queue).

### 1.3 Selection and keyboard

- Click selects one; Ctrl (Cmd) toggles; Shift extends a range from the
  anchor; Ctrl+A all; a drag on empty space draws a rubber band in grid
  views (Finder, Explorer, Nautilus, Dolphin, Lightroom). egui-pop's picker
  has all but the rubber band.
- Arrows follow the layout: up and down in a list, the four arrows in a
  grid, left and right in a filmstrip, left and right between columns in
  Finder's column view (Right enters a folder, Left goes back to the
  parent's column). Home, End, Page Up, Page Down everywhere.
- In Lightroom's Loupe and Photo Mechanic's Preview, left and right go to
  the previous and next image; the selection follows the focus unless Ctrl
  is held.
- Selection survives a change of view in every app surveyed: the selected
  files in the grid stay selected in the list.

### 1.4 Type-ahead

Finder, Explorer and Dolphin jump to the first name starting with the typed
letters (a short timeout, about one second, resets the prefix); repeating the
same letter cycles through the names that start with it (Explorer). Nautilus
replaced it with a recursive search and never stopped hearing about it (the
file dialogs survey, Nautilus section). egui-pop's picker has one field that
filters (fuzzy) and becomes a path with `/` or `~`; that stays. Type-ahead is
for the browser when the field is not there (the import window, a mode with
the focus in its entries) and for modes that have no field at all.

### 1.5 Sorting and grouping

All sort by name (natural in Explorer and Finder), date, size and kind; photo
tools add capture time, rating and file name with a sequence number. Groups
are headers in the view: Explorer's "Group by", Nautilus has none, Finder's
"Use Groups" (by kind, date, size, tags), Lightroom's import groups by date
folders on the source side. digiKam's albums by date are the agenda: years,
months, days.

### 1.6 Quick look

Finder: Space opens a floating preview, arrows move through the selection
behind it, Space closes it. Dolphin: an information panel (F11), and since
22.x an image preview popup is unconfirmed. Explorer: the preview pane
(Alt+P). Lightroom: E (Loupe), Z (1:1), Space toggles zoom. Photo Mechanic:
Enter or double click. egui-pop's picker has the preview pane (Alt+P); a
quick look adds a large view of one entry over the browser, without leaving
it.

## 2. The browser

`FileBrowser` is the component every file dialog and Helios's import draw.
It owns what is the same in every view:

- the **entries** of the place shown, from a `FileSource` (section 3.1),
  sorted (`files::sort`), filtered (the field, the type filter, hidden
  files) and grouped (`files::groups`): one `Vec<BrowserEntry>` in display
  order;
- the **selection** (a set of entry keys, an anchor, a mode: none, one,
  several, folders only) and the **focus** (one entry key, the highlight of
  today's picker);
- the **mode switcher** in its toolbar and the current mode, kept per
  dialog id (section 7, Q2);
- **type-ahead**, **quick look**, **drop targets** (section 5);
- the **thumbnail provider** (section 3.3), shared by every mode;
- the **steps** and the **collector**: a selection change, a mode change and
  an activation are reported the same way whichever mode drew them.

What it does not own: the places column, the path bar, the field, the
preview pane, the verb buttons and the application's choices. Those are the
**file chooser's** (today's `FilePicker`), which becomes a frame around a
`FileBrowser`. Helios's import window frames it differently (sources on the
left, options on the right, a big preview).

## 3. Contracts

### 3.1 `FileSource`

Where entries come from. Today it is a `FileSystem` and a folder; the import
needs more: a camera card read slowly, a recursive listing ("include
subfolders"), a catalogue that knows which files are already imported, a
remote place.

```rust
pub trait FileSource: Send + Sync + fmt::Debug {
    /// A stable id, for persistence and the collector.
    fn id(&self) -> &str;
    /// Starts (or restarts) a listing of `place`. Entries arrive in pages
    /// through `poll`; the browser never waits.
    fn list(&self, place: &Path, options: &ListOptions) -> Listing;
    /// Whether `watch` means something here.
    fn can_watch(&self) -> bool { false }
    /// Asks to be told when `place` changes; a change is a `Page::Changed`.
    fn watch(&self, place: &Path) -> Result<(), SourceError> { Err(SourceError::Unsupported) }
}

pub struct ListOptions { pub recursive: Option<u8>, pub hidden: bool }

/// A listing under way: pages, then the end, or an error.
pub struct Listing { /* a channel */ }
impl Listing {
    pub fn poll(&self) -> Vec<Page>;   // all that arrived since the last poll
    pub fn cancel(&self);              // also on drop
}

pub enum Page {
    Entries(Vec<SourceEntry>),
    /// The source learnt more about entries it already gave (sizes, times).
    Updated(Vec<SourceEntry>),
    Removed(Vec<PathBuf>),
    /// Start again: the place changed in a way the source cannot describe.
    Changed,
    Done,
    Failed(SourceError),
}

pub struct SourceEntry {
    pub path: PathBuf,
    pub name: String,
    pub kind: EntryKind,             // File, Folder, Link, Other
    pub size: Option<u64>,
    pub modified: Option<SystemTime>,
    pub created: Option<SystemTime>,
    /// The time a view sorts or groups by when it is not the file's:
    /// a photo's capture time, given by the application's source.
    pub taken: Option<SystemTime>,
    /// The application's own id for the file (a catalogue's image id).
    pub app_id: Option<u64>,
}

pub enum SourceError { NotFound(PathBuf), Denied(PathBuf), Io(String), Unsupported }
```

- **Paged and streamed.** A folder of 20 000 raws on a card shows its first
  page at once; the browser grows as pages arrive, and the focus and the
  selection stay on their keys. Sizes and times may come later
  (`Updated`), as today's picker reads them only for the rows in view.
- **Errors** are entries' state or the listing's end, never a panic; the
  browser says them where the entries would be.
- **Refresh** is a new `list`; the browser keeps the focus and the selection
  by path.
- **Watch** is optional (`notify` on desktop, nothing on the web);
  `Page::Changed` or `Removed` comes through the same `poll`.
- **Built in:** `FsSource` over today's `files::fs::FileSystem` (`StdFs`,
  `MemoryFs`), listing on the interface thread for a `MemoryFs` and on a
  worker for `StdFs`, in pages of 512. The web gets `FsSource` over a
  `MemoryFs`, synchronously.

### 3.2 `ViewMode` and its registry

```rust
pub trait ViewMode: Send + Sync {
    /// What it is, for the switcher, persistence, agents and screen readers.
    fn describe(&self) -> ModeInfo;
    /// Draws the entries. The mode reads the context and answers what the
    /// person did; it changes nothing itself.
    fn show(&mut self, ui: &mut Ui, cx: &ModeContext<'_>) -> ModeOutput;
    /// Where the focus goes for a key the mode handles (arrows, Page keys,
    /// Home, End); `None` leaves the key to the browser.
    fn step(&self, cx: &ModeContext<'_>, key: ModeKey) -> Option<EntryKey> { None }
}

pub struct ModeInfo {
    pub id: &'static str,              // "details", "thumbnails", "filmstrip"…
    pub label: StringKey,              // translated name
    pub icon: IconId,
    pub keys: &'static [ModeBinding],  // its own keys beyond the common ones
    pub gestures: &'static [Gesture],  // wheel, Ctrl+wheel, drags, pinch
    pub persist: &'static str,         // the key its own settings are kept under
    pub role: AccessRole,              // list box, grid, tree, carousel
    /// What it needs: thumbnails, a time per entry, a hierarchy.
    pub needs: ModeNeeds,
}

pub struct ModeContext<'a> {
    pub entries: &'a [BrowserEntry],   // sorted, filtered, grouped
    pub selection: &'a Selection,
    pub focus: Option<EntryKey>,
    pub thumbnails: &'a dyn ThumbnailProvider,
    pub sort: SortOrder,
    pub place: &'a Path,
    pub settings: &'a mut ModeSettings, // the mode's own, kept under `persist`
}

pub struct ModeOutput {
    pub clicked: Option<(EntryKey, Modifiers)>,
    pub activated: Option<EntryKey>,      // double click, Enter
    pub focus_to: Option<EntryKey>,
    pub band: Option<Vec<EntryKey>>,      // a rubber band's entries
    pub sort_by: Option<SortKey>,         // a header clicked
    pub context_menu: Option<EntryKey>,
    pub dropped: Option<(Option<EntryKey>, DropPayload)>,
    pub visible: Range<usize>,            // what it drew, for type-ahead and the collector
}
```

- **The mode answers, the browser decides.** Selection rules (single,
  several, folders), what Enter does and what a click on a folder does live
  in the browser, once. A mode draws, reports clicks, activations and the
  focus it wants, and asks for thumbnails. So a third-party mode cannot
  break "a click no longer enters a folder".
- **Keys.** The browser binds the common keys (Enter, Space, Ctrl+A,
  Ctrl+Space, Home, End, type-ahead, the mode keys Ctrl+1..9) as commands of
  the keymap; arrows and Page keys go through `step`, since only the mode
  knows its layout. A mode's own keys (`keys`) become commands under
  `file.mode.<id>.*`, `when` the mode is shown.
- **Registry.** `ViewModes` on the files service, as `Previews` and
  `Groupings`: `register(Arc<dyn Fn() -> Box<dyn ViewMode>>)` with the
  plugin's ownership (ADR 0004, taken back in `disconnect`), ordered by
  registration, the built-ins first. A browser asks the registry for the
  modes its request allows (`FileRequest::modes(&["details",
  "thumbnails"])`, all by default).
- **Persistence.** The current mode per dialog id, the mode's own settings
  (thumbnail size, the columns shown) under `settings/files.mode.<persist>`.
- **Accessibility.** `role` sets the container's role (list box, grid,
  tree); every entry is an option with its name, selected state and the
  active descendant, as today.

Built-in modes:

| Mode | Shows | Arrows | Needs | Notes |
|---|---|---|---|---|
| `details` | Rows with columns (name, modified, size, type), headers sort | Up, Down | — | Today's list, virtualised |
| `thumbnails` | A grid of thumbnails with names, a size slider (64 to 256) | Four arrows | thumbnails | Today's grid, virtualised, visible first |
| `filmstrip` | One row (or column) of thumbnails, the focused one large above (a quick look in place) | Left, Right | thumbnails | Lightroom's Loupe and Filmstrip, Photo Mechanic's Preview |
| `carousel` | The focused entry large, its neighbours smaller on each side | Left, Right | thumbnails | Finder's Gallery (macOS mode) |
| `columns` | One column per folder of the path, the last one a preview | Up, Down in a column; Right enters, Left goes back | a hierarchy | Finder's Columns; the place changes as the focus moves |
| `agenda` | Entries grouped by year, month or day of `taken` (else `modified`), each group a grid; a level switch | Four arrows across groups | a time | digiKam's Dates, darktable's timeline, Lightroom's date folders |

### 3.3 `ThumbnailProvider`

```rust
pub trait ThumbnailProvider: Send + Sync {
    /// Asks for a thumbnail at least `size` pixels on its long side.
    /// Answers at once from what is made; never blocks the frame.
    fn request(&self, req: &ThumbRequest) -> ThumbState;
    /// Drops every request not renewed since the last `begin_frame`:
    /// what scrolled away is cancelled.
    fn begin_frame(&self);
    /// Cancels one request (a mode that knows better).
    fn cancel(&self, key: &ThumbKey);
}

pub struct ThumbRequest<'a> {
    pub entry: &'a BrowserEntry,
    pub size: u32,               // pixels, long side
    pub priority: Priority,      // Visible, Near (the next screen), Background
}

pub enum ThumbState {
    Ready(egui::TextureHandle, [u32; 2]),
    Loading,
    /// An icon stands in: a folder, a file without an image.
    None,
    Failed(String),
}
```

- **Priority**: `Visible` before `Near` before `Background`; within one,
  the order of the requests (the mode asks in reading order from the
  focus). Requests renewed each frame stay; one not renewed is cancelled,
  which gives "visible first, cancelled when scrolled away" without the
  mode's help.
- **Sizes**: the provider rounds a size up to its own steps (egui-pop's
  default: 128, 256, 512, as the freedesktop cache) and the mode scales
  down when drawing.
- **egui-pop's default** (`PreviewThumbnails`) wraps today's `Previews`:
  the preview providers (PNG and JPEG with `image`, folders, text) and the
  optional system thumbnails, decoded off the interface thread on a small
  pool (two workers on desktop by default, in the frame on the web), cached
  by path, size and modification time.
- **Helios** implements it over its own image map and thumbnail cache
  (embedded raw previews first, its mipmaps), using `app_id` from its
  `FileSource`.

## 4. Shared behaviour

- **Selection** is the browser's: one, several or folders, as
  [Choosing files](../files.md) says today; a mode change keeps the selection,
  the anchor and the focus, and the new mode scrolls the focus into view.
- **Type-ahead**: when a mode's entry has the focus and there is no field
  (or the field is not focused), printable keys build a prefix (reset after
  one second, `TYPE_AHEAD_RESET`); the focus jumps to the first entry in
  display order whose name starts with it (case-insensitive, natural);
  repeating one letter cycles. A chooser keeps its field: there, typing
  goes to the field as today.
- **Keyboard per mode**: the common keys are the browser's commands;
  arrows go through the mode's `step`. Each mode's keys are listed by
  `describe` and shown in the switcher's tips.
- **Quick look**: Space on the focused entry (Finder) opens a large view
  over the browser, from the thumbnail provider at the largest size and
  then the preview providers; arrows move the focus behind it; Space or
  Escape closes it. In a chooser with a field, Space is the field's: quick
  look is then `Ctrl+Y` (unconfirmed default; section 7, Q5).
- **Drop targets**: the browser accepts files dropped from outside (egui's
  `dropped_files`) and reports them with the entry under the pointer
  (a folder) or none; a chooser goes to the dropped folder or selects the
  dropped file (JetBrains' chooser), an import adds the files.
- **Canvas-like modes** (carousel, agenda zoomed out, a future map) follow
  the [navigation model](navigation.md): the wheel scrolls, Ctrl and the
  wheel or a pinch zooms the thumbnails, hand mode and `view.fit` where the
  mode has a canvas. The mode declares it through `gestures`.

## 5. The file and folder choosers

1. **The contracts and the registry** land in `egui_pop::api::files::browser`
   beside today's picker, with `FsSource`, `PreviewThumbnails` and the
   `details` and `thumbnails` modes made from today's list and grid code.
2. **`FilePicker` draws a `FileBrowser`**: its list and grid are replaced by
   the browser and the two modes; the switcher replaces the List and Grid
   buttons, `files.view` reads `list` as `details` and `grid` as
   `thumbnails` (a migration of the setting), `PickerView` stays as a
   deprecated alias for one release. The field, places, pane, filters,
   saving and choices do not move. Every test of today's picker keeps
   passing; the snapshots change only where the switcher replaces the two
   buttons.
3. **The folder chooser** is the same picker with `ChooseKind::Folder`; it
   gets `details` and `columns` (thumbnails are of little use for folders).
4. **Helios's import** draws a `FileBrowser` with its `FileSource` (the
   card, recursion, "already imported" from `app_id`) and its
   `ThumbnailProvider`, framed by its own panels.
5. The other built-in modes follow one by one, each with its kittests.

## 6. Testing

- Switching modes keeps the selection and the focus (every pair of modes).
- The thumbnails mode requests visible entries first and cancels those
  scrolled away (a recording `ThumbnailProvider`).
- Type-ahead: prefix, reset, cycling.
- Keyboard per mode: each arrow's result on a known layout.
- A `FileSource` that pages slowly: the focus and the selection hold while
  pages arrive.

## 7. Open questions

**Q1. Where the browser lives.**
- A. `egui-pop-api` (`files::browser`), beside the picker.
- B. `egui-pop-views`, with the list and the table.
- C. A new crate, `egui-pop-files`.
- Recommended: **A**. The picker, the previews and the files service are in
  the API crate; moving them is a separate decision and would break every
  `egui_pop::api::files` path now.

**Q2. Which mode a dialog opens in.**
- A. The last used, global (today's `files.view`).
- B. The last used per dialog id (the request's `purpose`, else the kind).
- C. Per folder, as file managers.
- Recommended: **B**. An import wants thumbnails, a save-as wants details;
  per folder surprises in a dialog (GTK keeps one view).

**Q3. The mode switcher's look.**
- A. A segmented control of icons in the toolbar (Finder, Nautilus).
- B. One button with a menu of modes (Explorer's "View").
- C. A segmented control for up to four modes, a menu beyond.
- Recommended: **C**. A chooser with details and thumbnails stays as today
  (two buttons); the import with six modes does not fill its toolbar.

**Q4. The mode keys.**
- A. Ctrl+1..9, in the order of the switcher (Finder, Nautilus, Dolphin).
- B. Ctrl+Shift+1..9 (Explorer).
- C. None by default; rebindable commands only.
- Recommended: **A**, as commands (`file.mode.1`…), since Ctrl+digits are
  free in the picker.

**Q5. Quick look's key in a chooser with a field.**
- A. Space, as Finder, only when an entry (not the field) has the focus.
- B. Ctrl+Y (Finder's menu key for Quick Look) everywhere, plus A.
- C. No quick look in choosers, only in the import.
- Recommended: **B**.

**Q6. Thumbnail sizes cached by the default provider.**
- A. One size per request, cached as asked.
- B. Steps of 128, 256, 512, the mode scaling down (freedesktop's cache).
- C. Steps and the freedesktop cache written too.
- Recommended: **B**. The cache stays small and a size slider never decodes
  again; writing the system's cache is a separate choice (today we only
  read it).

**Q7. Built-in modes beyond details and thumbnails.**
- A. All six in egui-pop.
- B. Details, thumbnails, filmstrip and columns in egui-pop; carousel and
  agenda in Helios until a second user wants them.
- C. Only details and thumbnails; the rest are Helios's plugins.
- Recommended: **A**. The contract is proven by modes unlike each other;
  the agenda and the carousel are the ones that test it (groups, a canvas).

## 8. Steps

1. This note.
2. The contracts, the registry, `FsSource`, `PreviewThumbnails`, the
   `details` and `thumbnails` modes, the switcher, the picker on the
   browser, a gallery demo, kittests.
3. Filmstrip, columns, then carousel and agenda, each with its tests.
4. Quick look and drop targets.
5. Helios's import window on the browser, with its source and provider.

## 9. Sources (read 2026-11-08)

- macOS Finder: [Change how folders are displayed](https://support.apple.com/guide/mac-help/change-how-folders-are-displayed-in-the-finder-mchlp1016/mac),
  [Finder keyboard shortcuts](https://support.apple.com/en-us/102650),
  [Quick Look](https://support.apple.com/guide/mac-help/view-and-edit-files-with-quick-look-mh14119/mac).
- Windows Explorer: [Change the view of files](https://support.microsoft.com/en-us/windows/file-explorer-in-windows-ef370130-1cca-9dc5-e0df-2f7416fe1cb1),
  [Keyboard shortcuts in Windows](https://support.microsoft.com/en-us/windows/keyboard-shortcuts-in-windows-dcc61a57-8ff0-cffe-9796-cb9706c75eec),
  [Thumbnail handlers](https://learn.microsoft.com/en-us/windows/win32/shell/thumbnail-providers).
- GNOME Files: [Files preferences](https://help.gnome.org/users/gnome-help/stable/nautilus-views.html),
  [Thumbnail Managing Standard](https://specifications.freedesktop.org/thumbnail-spec/latest/).
- KDE Dolphin: [Dolphin handbook, view modes](https://docs.kde.org/stable5/en/dolphin/dolphin/view-modes.html),
  [`KFilePreviewGenerator`](https://api.kde.org/frameworks/kio/html/classKFilePreviewGenerator.html).
- Lightroom Classic: [Import photos](https://helpx.adobe.com/lightroom-classic/help/import-photos-video-catalog.html),
  [Library views](https://helpx.adobe.com/lightroom-classic/help/library-module-views.html),
  [Keyboard shortcuts](https://helpx.adobe.com/lightroom-classic/desktop/introduction-to-lightroom-classic/keyboard-shortcuts.html).
- Photo Mechanic: [User guide](https://docs.camerabits.com/support/solutions/48000449929).
- darktable: [Import module](https://docs.darktable.org/usermanual/development/en/module-reference/utility-modules/lighttable/import/),
  [Lighttable overview](https://docs.darktable.org/usermanual/development/en/lighttable/overview/),
  [Timeline](https://docs.darktable.org/usermanual/development/en/lighttable/timeline/),
  [Filmstrip](https://docs.darktable.org/usermanual/development/en/overview/user-interface/filmstrip/).
- digiKam: [Main window views](https://docs.digikam.org/en/main_window.html),
  [Dates view](https://docs.digikam.org/en/left_sidebar/dates_view.html),
  [Timeline view](https://docs.digikam.org/en/left_sidebar/timeline_view.html).
- egui-pop: [Choosing files](../files.md), the
  [file dialogs survey](../file-dialogs-survey.md), the
  [navigation study](navigation.md).
