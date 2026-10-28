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
proposed name) and `folder`, with a starting `directory` and `filter`s.
A path field of a form dialog (`Descriptor::path`) goes through the
same chooser when its Browse button is pressed; inside a dialog of its
own, an application starts the choice with `Files::choose_inline` and
draws the in-app picker where it wants it (`Files::show_inline`).

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
- Beside the folder: home, the recent places and the bookmarks, kept in
  the host's store (`settings/files.recent`, `settings/files.bookmarks`,
  saved with the settings).
- Below: the request's filters (and all files), hidden files.

`FilePicker` is usable on its own, in any `Ui`.

### Where it reads

The picker reads through a `files::fs::FileSystem`: the system's
(`StdFs`, the default), or a folder held in memory (`MemoryFs`: files
with their bytes and modification times, and folders). The web build
has no file system: an application gives the picker a `MemoryFs` there
(`FilesPlugin::file_system`, or `Files::set_file_system` later), as the
gallery does for its demo folder. `FilePicker::new_in` makes a picker
on any file system.

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
- **What a preview holds.** A body (an image, a text's first lines, a
  document drawn simply, a folder's counts and first thumbnails, or
  nothing) and facts: dimensions, pages, and labelled facts of the
  provider's own.

## Steps

| Step | Arguments | Default |
|---|---|---|
| `file.choose` | `{ "id", "request" }` | opens the system's chooser or the in-app picker |
| `file.chosen` | `{ "id", "request", "paths" }` | remembers the places, gives the paths |

A run-hook on `file.choose` replaces the whole chooser: it answers
`{ "paths": [...] }` or `{ "cancelled": true }` at once, or later
through `Files::deliver`. A before-hook on `file.chosen` refuses a
choice (the `Pending` gives the reason as an error) or redirects it to
other paths. The journal says `file.cancelled`. The debug bridge's
`files` collector shows the choices open, the in-app picker's folder,
field and entries, and answers a choice (`answer`).

The gallery's `files` page opens each kind of choice in a demo folder.
