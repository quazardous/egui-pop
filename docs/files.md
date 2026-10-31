# Choosing files

`FilesPlugin` (in `DefaultPlugins`, `egui_pop::api::files`) chooses
files and folders without ever holding a frame: `Files::choose` returns
a `Pending`, the host contract's result to come
([ADR 0010](adr/0010-host.md)), which gives `{ "paths": [...] }`, or
`null` when the person cancelled.

- **The system's chooser** (feature `native-dialogs` of the facade,
  through `rfd`): the xdg desktop portal on Linux, the common dialogs on
  Windows, the open and save panels on macOS. It is made on the
  interface thread and awaited on a thread of its own.
- **The in-app picker**, when the feature is off, when no portal can
  answer (no session bus), or when the `files.native` setting is off.

```rust
use egui_pop::api::files::{self, Chosen, FileRequest, FilesPlugin, FILES};
use egui_pop::egui;
use egui_pop::prelude::*;

let mut app = App::builder().plugin(FilesPlugin::in_app()).build(&egui::Context::default()).unwrap();
let h = app.handle().clone();
let f = h.service(&FILES).unwrap();
let pending = f.choose(
    &h,
    FileRequest::open("Open an image").filter("Images", &["png", "jpg"]),
);
// The picker is open; the frame goes on. An agent (or a test) answers it
// as a person would.
let id = f.open_ids()[0];
f.deliver(id, Some(vec!["/photos/harbour.png".into()])).unwrap();

// The next frame finishes the choice (the `file.chosen` step).
let ctx = egui::Context::default();
let mut out = ctx.run_ui(egui::RawInput::default(), |ui| app.show(ui));
out.textures_delta.clear(); // no renderer here
let result = pending.take().unwrap();
assert_eq!(files::chosen(&result), Ok(Chosen::Paths(vec!["/photos/harbour.png".into()])));
```

A request is one of `FileRequest::open`, `open_many`, `save` (with a
proposed name) and `folder`, with a starting `directory`, a
[`purpose`](#the-last-folder-per-purpose) and `filter`s.
A path field of a form dialog (`Descriptor::path`) goes through the
same chooser when its Browse button is pressed; inside a dialog of its
own, an application starts the choice with `Files::choose_inline` and
draws the in-app picker where it wants it (`Files::show_inline`).

## The last folder per purpose

`FileRequest::purpose("import")` names what the choice is for. Its last
folder is kept under that key, so an import and an export do not share
one, as Windows' `SetClientGuid` and the `identifier` of a macOS panel do.

```rust
use egui_pop::api::files::FileRequest;

// Starts in ~/Pictures the first time, then where the last import was made.
let import = FileRequest::open("Import").purpose("import").directory("/home/me/Pictures");
// Always starts in the document's folder; still updates the last folder.
let save_as = FileRequest::save("Save as", "a.png").purpose("export").force_directory("/work/doc");
# let _ = (import, save_as);
```

With a purpose, a choice starts in the first of these that exists:

1. the purpose's last folder (`files::last_folder`);
2. the request's `directory`: a first-time default, as Windows' guidance
   has it (`SetDefaultFolder`, which does not hide the last location,
   rather than `SetFolder`, which does);
3. home.

So an explicit `directory` does not beat a remembered folder on later
openings. A request that means it uses `FileRequest::force_directory`,
which swaps the first two (`SetFolder`).

- **What is kept.** On a choice only (a cancel keeps nothing): the folder
  chosen, or the parent of the first path, as `file.chosen` gives it after
  its hooks. It is in the host's store, saved with the settings
  (`settings/files.last_folders`), for the last `LAST_FOLDERS_KEPT` (32)
  purposes.
- **Recent places stay global.** A purpose without a last folder or a
  `directory` starts at home, not in the last recent place, which may be
  another purpose's.
- **No purpose** keeps nothing per choice: the request's `directory`, else
  the last recent place, else home.
- **The system's chooser** gets the same folder through `rfd`'s
  `set_directory`, and only that: `rfd` has no `SetClientGuid`, and how far a
  system honours a suggested folder is up to it. egui-pop has not run one to
  see it.
- A `FilePicker` drawn on its own has no store and starts in the request's
  `directory`; its caller remembers.

## The in-app picker

- The field filters the folder with the fuzzy filter the choice dialog
  uses; with a `/` (or starting with `~`) it is a path, and `Tab`
  completes it.
- The folder reads as breadcrumbs, beside the row's controls (under them when
  they leave too little room). A segment opens its folder; the chevron after it
  lists its subfolders, by name, hidden ones only when the list shows them. A
  segment on the folder shown reads that folder again. When the path does not
  fit, its start collapses behind a `…` button listing those folders, the nearest
  first: the deepest segments stay, and the deepest alone is cut only when it
  does not fit by itself.
  - Every segment, chevron and `…` is a named stop (`Open folder photos`,
    `Subfolders of photos`, `Folders above`). Enter or Space on a chevron opens
    its menu with the focus on the folder on the path, else on the first; Up and
    Down go round it, Enter chooses, Tab or Escape close it and the focus goes
    back to the chevron. A folder without subfolders or that cannot be read says
    so, in the menu and to a screen reader.
  - Choosing a folder this way hands the keyboard to the field, as a click on a
    row does.
- `Ctrl+L` (the `file.edit_path` command, rebindable, for agents too) puts the
  folder into the field as a path to edit, the cursor at its end, in place of a
  filter; a path being typed stays. Typing `/` or `~` while an entry has the focus
  starts a path there, as in GTK and macOS. They are what is typed, whichever keys
  make them on the layout. There is no second field: the one that filters takes
  the path, and completes it with `Tab`. Saving, `Ctrl+L` keeps the name at the
  path's end with its stem selected, and a path that leads to another folder
  brings that name back there.
- A click on Up, the bookmark button, List or Grid gives the keyboard back to the
  field, where a press would otherwise leave nothing focused; with Enter or Space
  the focus stays on the button. A typed `..` is resolved in the folder's path:
  `go` takes it out by its name, without asking the file system, so `a/link/..` is
  `a`, even if `link` is a link elsewhere.
- A click selects; a double click or Enter opens: Enter on a folder goes
  into it, on a file chooses it. A click no longer enters a folder, so a
  mis-click navigates nowhere. `Ctrl` is Cmd on macOS, here as in the keymap.
- Arrows move the highlight (what the preview is of, and what Enter acts
  on); Backspace in an empty field or `Alt+Up` goes to the parent folder;
  `Ctrl+Enter` chooses the folder (choosing a folder) or the selection;
  `Ctrl+D` bookmarks the folder; Escape cancels (from the field, the first
  Escape lets go of the focus, as everywhere in egui-pop).
- The field keeps the keyboard: a click on a row gives it back, so typing
  goes on filtering. Tab goes on to the rows: a row holding the focus moves
  the highlight there, Space clicks it, and Enter and the arrows work from it
  as from the field.
- **Several files** (`FileRequest::open_many` only). A click selects that
  file alone; `Ctrl`+click adds or takes one away; `Shift`+click and
  `Shift`+arrows select from the last one clicked or moved to, as a range;
  `Ctrl+Space` selects or unselects the highlighted file; `Ctrl+A` selects
  every file shown. Plain arrows leave the selection as it is, for
  `Ctrl+Space` further on. Selected rows are filled and a ring marks the
  highlight, which is a separate thing here. A line under the list counts the
  selection (`Selected: 3`, or `Selected: 3 (2 not shown)` when the filter
  hides some, which stay selected), and each change is announced to screen
  readers. With nothing selected, it says that Enter or the button acts on
  the highlighted entry.
  - Enter or the button then takes the selection, wherever the highlight is
    among files. On a folder Enter goes into it, and going to another folder
    leaves the selection behind, so nothing is chosen unseen. A double click
    opens that file alone.
  - A folder is not selected with the files: `Ctrl`+click or `Ctrl+Space` on
    one says so in the picker, and a range leaves folders out. `Ctrl+A` with
    no file shown says that too. In a typed path, `Ctrl+A` is the field's.
  - `file.select_all` (Ctrl+A) and `file.toggle_selection` (Ctrl+Space) are
    commands: rebindable, and only bound while a picker offers them
    (`filePickerSelectAll`, `filePickerSelects`), so `Ctrl+A` selects the
    field's text in any other request.
- **One file.** Modifiers add nothing: any click or arrow moves the single
  highlight, which is the selection.
- **Choosing a folder.** Nothing is selected at first and the line under the
  list says what the button takes: the folder shown. A click or an arrow
  selects a folder in the list, and the line says the button now chooses it
  and Enter goes into it; `Ctrl`+click or `Ctrl+Space` unselects it. The change
  is announced. Typing a filter drops the selection for the filter's first
  match.
- Beside the list, a preview pane: the highlighted entry's preview
  (below) and its facts: a file's size and modification time (in UTC),
  a folder's items, dimensions, pages, and the provider's own. `Alt+P` (as in
  the Windows file explorer) or the Preview box shows or hides it; the
  `files.preview` setting remembers. The dialog keeps its size either
  way: the pane takes its width from the list's, the list keeps at
  least `LIST_MIN_WIDTH` (240), and with less room the pane shrinks
  first, down to `PANE_MIN_WIDTH` (120), under which it is not drawn.
- The dialog is yours to size: drag its edges and corners, down to
  `PICKER_MIN_SIZE` (640 by 280 points) and within the window. Its bottom right corner is a
  handle for the keyboard too: Tab reaches it, the arrows resize (with
  Shift, by bigger steps). The size starts at `PICKER_WIDTH` by
  `PICKER_HEIGHT` (960 by 398 points) and is kept in the host's store
  (`settings/files.dialog_size`, read with `files::dialog_size`). A size
  that no longer fits the window when the picker opens (a smaller
  screen) goes back to the default; one the window comes to hold smaller
  while the picker is open stays as it was asked, and comes back when
  the window grows.
- Between the list and the pane, a splitter sets the pane's width: drag
  it, or Tab to it (from the field, past the places; in a typed path Tab
  completes instead) and use the left and right arrows, with Shift for
  bigger steps. It stays within the limits above; a double click or
  Enter gives the default width back (`preview::PANE_WIDTH`, 240). The
  width is kept in `settings/files.pane_width` (`files::pane_width`). The
  places column keeps its fixed 150 points.
- With previews, a switch beside the folder shows the entries as a list or as a
  grid of thumbnails from the same providers (`PickerView`, kept in
  `settings/files.view`, read with `files::view`); a picker without previews
  keeps its list. The thumbnails' size goes from `THUMBNAIL_MIN_SIZE` to
  `THUMBNAIL_MAX_SIZE` (64 to 256 points, 128 at first): the slider beside the
  switch, `Ctrl` and the wheel over the grid, or `Ctrl` and `+` or `-`; it is
  kept in `settings/files.thumbnail_size` (`files::thumbnail_size`). Only the
  cells in view ask for a thumbnail, so a folder of thousands of raws costs what
  a dialog's worth does: what leaves the view leaves the worker's queue (see
  Previews). A placeholder stands in until a thumbnail is made; a folder shows
  its sign and asks for nothing, a file without an image the file's sign, a
  failure a warning with its reason in a tip.
- In the grid, the four arrows move the highlight in two dimensions, `Home` and
  `End` go to the first and the last entry, and `Shift` with any of them extends
  a selection of several files; Enter, a click and a double click are the list's.
  While the field holds text and the focus, `Left`, `Right`, `Home` and `End`
  move its cursor, and only `Up` and `Down` the highlight: `Tab` to a cell gives
  all of them to the grid, and the focus then follows the highlight.
- The list has columns: the name, when it last changed (in UTC, as in the
  pane), the size, and the type (a file's extension; a file without one says
  File, a folder Folder). A folder has no size, so choosing a folder shows the
  name and the time alone. When the list is too narrow for
  them and at least `NAME_MIN_WIDTH` (140) of name, they go: the type first, the
  modification time last; a name cut short shows whole in a tip.
- Only the rows in view are drawn, so a folder of thousands of files costs what
  a dialog's worth does. Tab walks the rows drawn, then leaves the list; the
  arrows go on through the whole folder, scrolling.
- A click on a header sorts by that column, and another turns it. The name and
  the type start upwards, the size and the time with the largest and the newest
  first, as Finder and Nautilus do. There is always an order: unlike the data
  table's headers, these do not go back to none. The headers stay above the grid,
  whose cells follow the same order, and Tab reaches them. The order is kept in
  `settings/files.sort` (`name`, `-name` for downwards, and `size`, `modified`,
  `type` the same way; `files::sort_order`), and the highlighted entry stays
  highlighted, wherever it lands.
- Names sort as people read them (`files::sort::natural_cmp`): `IMG_9` before
  `IMG_10`, whatever the case. It compares characters rather than following a
  language's collation, so `é` comes after `z`. What has no value in a column (a
  folder's size and type, a time the file system does not give) goes last in both
  directions, and the name settles ties, upwards.
- Folders stay first in every order, in the same direction among themselves. The
  Folders first box and the `files.folders_first` setting
  (`files::folders_first`), as GTK's `sort-directories-first`, sort them among the
  files instead.
- While the field filters, its best matches come first, the order settling only
  what ties: the headers are disabled then, their tip says why, and the arrow goes
  until the field is empty. A typed path, and the name when saving, do not rank.
- The size and the time are read off the file system once per listing, for the
  rows in view; sorting by either reads them all then, on the interface thread, as
  listing the folder does. Sorting by name or type reads nothing more.
- Beside the folder: home, the recent places and the bookmarks, kept in
  the host's store (`settings/files.recent`, `settings/files.bookmarks`,
  saved with the settings).
- Below: the request's filters (and all files), hidden files.
- Files that go together can be one entry, by the application's rules: see
  [Grouped files](#grouped-files).

### Saving

- **A folder in the list.** A click or an arrow selects it: Enter, and the
  button, which reads Open then, go into it, the name kept. Typing in the field
  takes the selection back, and Enter saves. A click on a file puts its name in
  the field; an arrow onto a file leaves the name alone.
- **The name.** The field proposes the request's name with its stem
  selected, whenever the field gets the focus: typing replaces the name
  and keeps the extension. A click in the field places the cursor as
  usual.
- **The extension.** Enter or Save appends the active filter's first
  extension to a name that has none (`dawn` gives `dawn.png`). When the
  filter changes, a name whose extension one of the request's filters
  names takes the new filter's first (`dawn.png` gives `dawn.jpg`), as
  Windows' dialog does, unless the new filter accepts it too.
  An extension no filter names is the person's and is never touched
  (`dawn.tiff` stays under PNG), and all files, or a filter without
  extensions, leaves the name as it is. A hidden file's name (`.config`)
  gets no extension; what follows a dot in `Mr. Smith` holds a space and
  is not one, so that name gets one.
- **Replacing a file.** When the name exists, the picker asks first
  through the dialogs plugin, with a `Confirm` in the danger style:
  "Replace a.png?", the focus on Cancel. No key accepts it from there: Enter
  and Space press the focused button whatever the modifiers (egui's rule), so
  Ctrl+Enter or Space cancels; replacing takes a click on Replace, or Tab to
  it first. Cancel or Escape gives the picker back, the keyboard in its field
  and the stem selected for another name; Replace ends the choice
  (`file.replace`). The picker is inert meanwhile. An
  application that asks itself turns this off with
  `FileRequest::confirm_overwrite(false)`.
  - It is the in-app picker's question only, so egui-pop adds no second one
    to a system chooser: a choice answered by the system goes straight to
    `file.chosen`. egui-pop relies on the system's own question there and
    has not run one to see it. `rfd` asks for it in its GTK and zenity
    backends and leaves Windows' default (`FOS_OVERWRITEPROMPT`) alone; with
    the desktop portal and macOS's save panel, the question is up to them.
    `rfd` has no way to turn it off, so the opt-out does not reach them.
    A run-hook on `file.choose`, `Files::deliver` and the bridge's `answer`
    answer for themselves.
  - Without the dialogs plugin nobody can ask: nothing is replaced, and the
    picker says so and wants another name.
  - A `FilePicker` drawn on its own answers `PickerAnswer::Replace(path)`:
    its caller asks.
  - Inside a dialog (`Files::show_inline`), the question comes up in that
    dialog's place, and the dialog comes back after the answer, as it was. Its
    overlay closes and opens again, so it is drawn anew.
- **A new folder**, saving or choosing a folder: the New folder button, or
  the `file.new_folder` command (Ctrl+Shift+N, rebindable, working through
  the picker's modal and while typing). A row under the folder asks the
  name: Enter or Create makes it through the file system
  (`FileSystem::create_dir`) and goes into it, the name to save under
  still in the field; Escape closes the row and leaves the picker open.
  A name that is empty, holds a path or exists, or an error from the
  system, shows in the picker, and the row stays to fix the name. The
  name comes first, as in GTK and macOS, so nothing needs renaming. The
  journal says `file.folder_created`. An agent gives `{ "name" }` to the
  command and the folder is made at once.

`FilePicker` is usable on its own, in any `Ui`: its list and headers are
`LIST_HEIGHT` (280) high, or take the height left with `fill`. Its order is
`FilePicker::sort` and `FilePicker::folders_first` (`set_sort`,
`set_folders_first` keep the highlight on its entry); its caller remembers them. With no
keymap to go through, it reads `Ctrl+A`, `Ctrl+Space` and `Ctrl+L` itself, and they do
not rebind. A
modal of your own that the person resizes is
`OverlaySpec::resizable` (`overlay` module).

How other dialogs do it, and what the picker should take from them:
the [file dialogs survey](file-dialogs-survey.md).

### Grouped files

An application declares which files go together, and they are one entry: a RAW
with its sidecars, as Lightroom shows a RAW and its JPEG, and darktable and
digiKam treat an XMP. egui-pop has the mechanism and no rule: nothing groups
until a grouping is declared, and it has no list of photo extensions of its own.

```rust
use std::sync::Arc;
use egui_pop::api::files::groups::SidecarRule;
use egui_pop::api::files::FilesPlugin;

// x.xmp, x.cr3.xmp and x.jpg belong to x.cr3; the badge says +XMP, +JPEG.
let photos = SidecarRule::new("my_app.photos", &["cr3", "nef", "arw"], &["xmp", "jpg"])
    .badge("jpg", "JPEG");
let files = FilesPlugin::default().grouping(Arc::new(photos));
# let _ = files;
```

A plugin registers on the service instead, `Files::groupings().register(...)`,
and takes it back in its `disconnect` (`Groupings::unregister`). A
`SidecarRule` goes by extension, whatever its case, and an exact stem; the main
extensions come in the order they win, so `&["cr3", "jpg"]` makes `x.jpg` a
member beside `x.cr3` and a main file of its own otherwise. Anything else, such
as darktable's duplicates (`x_01.cr3.xmp`), is a `FileGrouping` of your own: it
is given a folder's file names, without folders or hidden files, and gives groups
of names. It is asked once per listing, on the interface thread, as the folder is
read, and reads no file. What makes no sense in its answer is ignored and the file
stays an entry of its own (a name not there, a file an earlier group or grouping
took); one that panics counts for nothing, and the log says so.

- **The entry.** It stands for the main file: its name, its size, its time and
  its type in the columns, in the grid's cell and in every order. It says the
  others with badges (`+XMP`, `+JPEG`) at the end of the name's column or in the
  cell's corner, `+2` when they do not fit, and its description and the badges'
  tip name them. The pane lists them under the facts. Arrows, ranges and `Ctrl+A`
  go by entries, and `Selected: 2` counts groups.
- **What a choice gives.** `paths` holds the main file, as `files::chosen` reads it,
  so a caller who does not care changes nothing. The result's `groups`, which
  `files::chosen_groups` reads, holds `FileGroup { main, members, badges }` for each
  path that heads a group, and `file.chosen`'s arguments hold them too for
  a before-hook. A redirected choice gives the groups of where it leads. They
  come from the paths at the end of the choice, so a run-hook, `Files::deliver`,
  the bridge's `answer` and the system's chooser give them as well; the system's
  window shows each file apart, and a member chosen there is only that file.
  A `FilePicker` drawn on its own takes `with_grouping`, answers with the main
  file, and tells its caller the rest with `group_of`.
- **Filters.** The group forms over the whole folder, and the filter judges its
  main file: under `Raws (*.cr3)` an entry still has its `+XMP`, and under
  `Sidecars (*.xmp)`, which refuses the RAW, each XMP is an entry of its own.
- **The field.** A group answers to its members' names too, and choosing it still
  gives the main file. A typed path is a path: `/shoot/x.jpg` chooses that file
  alone, and its completions list every file.
- **Previews.** Only the main file is asked for a preview, in the pane and in the
  grid's queue. Where no provider gives it one (`Nothing`, or a failure), its
  members are asked in turn, and the first image stands in, with `Preview from
  x.jpg` under it in the pane; the facts stay the main file's, but for what the
  provider said of the image, such as its dimensions. A sidecar's text does not
  stand in for a photo. The `files` collector says it as `stand_in`.
- **Each file its own entry.** The Group files switch beside the view shows
  every file, to reach a member with the pointer, and a choice then gives no
  groups. It shows only with a grouping declared, and is kept in
  `settings/files.grouped` (`files::grouped`), outside the settings window where
  it would mean nothing without a grouping. Switching back moves the highlight to
  the group that takes its file in, and a member selected leaves the selection.
- **Not grouped.** Saving and choosing a folder never group, as a name to save
  under must see every file it could replace. A hidden file joins no group, even
  shown. `FileRequest::ungrouped()` turns groups off for a choice that wants a
  sidecar as much as a RAW.

### Where it reads

The picker reads through a `files::fs::FileSystem`: the system's
(`StdFs`, the default), or a folder held in memory (`MemoryFs`: files
with their bytes and modification times, and folders). The web build
has no file system: an application gives the picker a `MemoryFs` there
(`FilesPlugin::file_system`, or `Files::set_file_system` later), as the
gallery does for its demo folder. `FilePicker::new_in` makes a picker
on any file system.

It writes one thing, a new folder, and only on a file system that says
it can (`FileSystem::can_create_dirs`, `create_dir`). `StdFs` can. A
`MemoryFs` is read-only, and a `FileSystem` of your own is too unless it
says otherwise: the New folder button is disabled with the reason as its
tip, and the key shows the reason in the picker. So on the web, whose
picker reads from memory, no folder can be made, and it says so; saving
still chooses a path and still asks before replacing a file held there.

## Previews

Previews come from providers (`files::preview::PreviewProvider`) that
plugins register on the files service:

```rust
use std::sync::Arc;
use egui_pop::api::files::preview::{Body, Handles, Preview, PreviewProvider, PreviewRequest};

#[derive(Debug)]
struct Notes;

impl PreviewProvider for Notes {
    fn id(&self) -> &str {
        "my_app.notes"
    }
    fn handles(&self) -> Handles {
        Handles::new().extensions(&["note"]).magic(b"NOTE1")
    }
    fn priority(&self) -> i32 {
        10
    }
    fn preview(&self, request: &PreviewRequest) -> Result<Option<Preview>, String> {
        let bytes = request.read()?;
        let text = String::from_utf8_lossy(&bytes[5.min(bytes.len())..]).into_owned();
        Ok(Some(Preview::new(Body::Text { text, truncated: false }).fact("Format", "Note 1")))
    }
}

// In a plugin's `connect`, after the files plugin's:
// ctx.app().service(&FILES).unwrap().previews().register(Arc::new(Notes));
# let _ = Arc::new(Notes);
```

- **Which provider.** A provider's `Handles` name file extensions,
  MIME types (from the extension: `image/png`, a family `text/*`, any
  file `*/*`), magic bytes a file starts with, and folders. The
  providers that handle a path are tried from the highest `priority`
  down; `accepts` may refuse a path its handles match. The first to
  give a preview wins; one may pass (`Ok(None)`) to the next.
- **A failure is a reason.** An error, or a panic in the provider, shows
  as "no preview" with its reason; nothing crashes.
- **Off the interface thread.** `Previews::get` answers from the cache,
  else hands the path to a worker thread and says `Loading`; the frame
  never waits, and the context is woken when the preview is ready.
  Asking for another path cancels the one asked before (a long provider
  checks `PreviewRequest::cancelled`). On the web, which has no threads,
  previews are made in the frame. `Previews::compute` makes one at once
  (agents, tests).
- **Cached and capped.** Previews are kept by path, size and
  modification time, up to 64 MiB (`Previews::set_cache_bytes`; the
  oldest go first); a provider reads at most 64 MiB of a file
  (`PreviewRequest::read` refuses more, `set_max_bytes`) and draws images
  at most 256 pixels long (`set_max_side`).
- **Built in** (`files::providers`, registered by the files plugin
  unless `FilesPlugin::without_builtin_previews`):
  - images: PNG and JPEG decoded with the `image` crate and scaled down,
    with their dimensions (feature `image-previews` of the facade, on by
    default; `image` is MIT OR Apache-2.0 and eframe already uses it);
  - Markdown, drawn simply: headings, paragraphs, items, quotes, code
    and rules, emphasis and links as plain text. No Markdown renderer
    is in the tree; this small one is egui-pop's own, until the
    renderer the in-app viewer will use is chosen;
  - text and code: the first 40 lines in a monospace font (any file
    that reads as UTF-8 text, at the lowest priority);
  - folders: how many items, and the first images as the providers
    preview them (four at most, among the first sixteen files that are
    images or of no known type).
- **The system's thumbnails** (feature `platform-thumbnails` of the
  facade, part of `platform`; `egui_pop::platform::thumbnails`, an
  opt-in `ThumbnailsPlugin`): for what egui-pop's providers do not
  preview (PDF, office documents, videos, fonts…), at the lowest
  priority and on the system's file system only. On Linux and the BSDs
  it reads the freedesktop thumbnail cache (a thumbnail whose
  `Thumb::MTime` is the file's), else runs the installed thumbnailer
  for the file's MIME type (the shared MIME database's globs) into a
  temporary file, stopped after 10 s (`ThumbnailsPlugin::timeout`) or
  when the selection moves; it reads the cache and does not write it.
  The Windows shell's thumbnails and macOS's QuickLook are not done
  yet: the provider gives that as its reason. The web has none.
- **What a preview holds.** A body (an image, a text's first lines, a
  document drawn simply, a folder's counts and first thumbnails, or
  nothing) and facts: dimensions, pages, and labelled facts of the
  provider's own.

A grid asks differently from the pane: `Previews::want` says each frame which
paths it has in view, in order and with their facts, and `Previews::thumbnail`
reads what is made, neither touching the file system in the frame. The worker
makes them one at a time, behind the preview asked with `get`, which a thumbnail
in the making gives way to; a path that left the view leaves the queue, and is
cancelled in the making. They share the cache and its key with the pane, so the
highlighted cell's preview is there at once; what is in view is kept over the
cache's cap, rather than made again for ever.

## Steps

| Step | Arguments | Default |
|---|---|---|
| `file.choose` | `{ "id", "request" }` | opens the system's chooser or the in-app picker |
| `file.chosen` | `{ "id", "request", "paths" }`, and `"groups"` when some are | remembers the places and the purpose's last folder, gives the paths and their groups |

For agents, the `file.preview` command (`{ "path" }`, kept out of the
palette) gives a path's preview metadata as the pane shows it, made at
once if it is not in the cache: whether it is a folder, its size and
modification time, the provider, the kind of preview, dimensions,
pages, a folder's counts, the provider's facts, or the state and the
reason when there is none. No pixels and no text. The `files`
collector shows the providers, the cache, and the highlighted entry's
preview state in each open picker.

A run-hook on `file.choose` replaces the whole chooser: it answers
`{ "paths": [...] }` or `{ "cancelled": true }` at once, or later
through `Files::deliver`. A before-hook on `file.chosen` refuses a
choice (the `Pending` gives the reason as an error) or redirects it to
other paths. The journal says `file.cancelled`. The debug bridge's
`files` collector shows the choices open, the in-app picker's folder,
field and entries, and answers a choice (`answer`).

The commands `file.new_folder` (`{ "name" }`, optional) and
`file.replace` (`{ "id", "path" }`, hidden) are the picker's new folder and
the yes to its question. The `when` key `filePickerNewFolder` says that an
open picker offers a new folder. `file.select_all` and
`file.toggle_selection` answer `{ "selected": n }`, and the `files` collector
lists the picker's `selected`, its `sort` and `folders_first`, its `grouped` and
`groups`, and the `groupings` declared.

The gallery's `files` page opens each kind of choice in a demo folder
whose files are made by code at start (the repository holds no binary
file): PNG and JPEG images drawn with the `image` crate, a text file, a
Markdown document and a CSV table, written to the temporary folder on
the desktop and given to the picker as a `MemoryFs` on the web, so the
previews show there too. Open an image and Save as each have a purpose:
each comes back to its own last folder, the others stay where they were.
`Photos/2026` holds a pretend RAW and a JPEG with sidecars, grouped by a rule the
page declares for itself.
