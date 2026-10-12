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
