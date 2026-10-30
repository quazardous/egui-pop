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
- Arrows move, Enter enters a folder or chooses a file; Backspace in an
  empty field or `Alt+Up` goes to the parent folder; `Ctrl+Enter`
  chooses the folder (choosing a folder) or the selection;
  `Ctrl+Space` selects a file (choosing several); `Ctrl+D` bookmarks the
  folder; Escape cancels (from the field, the first Escape lets go of
  the focus, as everywhere in egui-pop).
- Beside the list, a preview pane: the highlighted entry's preview
  (below) and its facts: a file's size and modification time (in UTC),
  a folder's items, dimensions, pages, and the provider's own. `Alt+P` (as in
  the Windows file explorer) or the Preview box shows or hides it; the
  `files.preview` setting remembers. The dialog keeps its size either
  way: the pane takes its width from the list's, the list keeps at
  least `LIST_MIN_WIDTH` (240), and with less room the pane shrinks
  first, down to `PANE_MIN_WIDTH` (120), under which it is not drawn.
- The dialog is yours to size: drag its edges and corners, down to
  `PICKER_MIN_SIZE` and within the window. Its bottom right corner is a
  handle for the keyboard too: Tab reaches it, the arrows resize (with
  Shift, by bigger steps). The size starts at `PICKER_WIDTH` by
  `PICKER_HEIGHT` (880 by 398 points) and is kept in the host's store
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
- Beside the folder: home, the recent places and the bookmarks, kept in
  the host's store (`settings/files.recent`, `settings/files.bookmarks`,
  saved with the settings).
- Below: the request's filters (and all files), hidden files.

### Saving

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

`FilePicker` is usable on its own, in any `Ui`: its list is
`LIST_HEIGHT` (280) high, or takes the height left with `fill`. A
modal of your own that the person resizes is
`OverlaySpec::resizable` (`overlay` module).

How other dialogs do it, and what the picker should take from them:
the [file dialogs survey](file-dialogs-survey.md).

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

## Steps

| Step | Arguments | Default |
|---|---|---|
| `file.choose` | `{ "id", "request" }` | opens the system's chooser or the in-app picker |
| `file.chosen` | `{ "id", "request", "paths" }` | remembers the places and the purpose's last folder, gives the paths |

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
open picker offers a new folder.

The gallery's `files` page opens each kind of choice in a demo folder
whose files are made by code at start (the repository holds no binary
file): PNG and JPEG images drawn with the `image` crate, a text file, a
Markdown document and a CSV table, written to the temporary folder on
the desktop and given to the picker as a `MemoryFs` on the web, so the
previews show there too. Open an image and Save as each have a purpose:
each comes back to its own last folder, the others stay where they were.
