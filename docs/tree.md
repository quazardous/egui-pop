# The tree

`egui-pop-views` (the facade's `views` feature, re-exported as
`egui_pop::views`) has a live tree over data the application owns:

```sh
cargo add egui-pop --features views
```

- branches are loaded **on demand**, the first time they are opened,
  from any thread, with a loading row meanwhile and the reason and a
  retry button when loading fails;
- the application **changes the tree at any time** (insert, remove,
  move, rename, reload) and the view follows, keeping the selection and
  the open branches;
- **single or multiple selection**, the **keyboard**, **drag and drop**
  and **rename in place**;
- **virtualised**: 100 000 nodes cost a screenful.

## The source

The application implements `TreeSource`: given a branch (`None` for the
roots), it answers through the `Sink` now or later, from any thread. A
node has a key (any type that is cloned, compared, hashed and
serialised), a label, and whether it is a branch.

```rust
use egui_pop::egui;
use egui_pop::views::tree::{Node, Sink, TreeSource, TreeState, TreeView};

/// Folders `0` to `2`, each with three files.
struct Folders;

impl TreeSource<String> for Folders {
    fn load(&self, parent: Option<&String>, sink: Sink<String>) {
        let children = match parent {
            None => (0..3).map(|i| Node::branch(format!("{i}"), format!("Folder {i}"))).collect(),
            Some(p) => (0..3).map(|i| Node::leaf(format!("{p}/{i}"), format!("File {p}.{i}"))).collect(),
        };
        // A disk or a network answers from a thread: `sink` is `Send`.
        std::thread::spawn(move || sink.deliver(children));
    }
}

let ctx = egui::Context::default();
let mut state = TreeState::<String>::new();
state.set_expanded(&"1".to_string(), true);
for _ in 0..50 {
    // An application runs eframe; a bare context drops its textures.
    let mut frame = ctx.run_ui(egui::RawInput::default(), |ui| {
        TreeView::new("folders").label("Folders").show(ui, &mut state, &Folders);
    });
    frame.textures_delta.clear();
    if state.model().loaded(Some(&"1".to_string())).is_some() {
        break;
    }
    std::thread::sleep(std::time::Duration::from_millis(5));
}
// The roots, then the open folder's files.
assert_eq!(state.model().rows_now().len(), 6);
```

A branch is asked for once; `TreeHandle::reload` asks again (its
children stay on screen until the new ones come). An answer to a
request that was superseded is dropped.

## Changes at any time

`TreeState::handle` gives a `TreeHandle`: cloned, sent to any thread,
its edits are applied at the next pass, which they wake.

```rust
use egui_pop::views::tree::{Node, TreeState};

let state = TreeState::<String>::new();
let edit = state.handle();
std::thread::spawn(move || {
    edit.insert(Some("photos".into()), Some(0), Node::leaf("photos/new.jpg".into(), "new.jpg"));
    edit.rename("photos/old.jpg".into(), "older.jpg");
    edit.move_to("photos/2023".into(), Some("archive".into()), None);
    edit.remove("photos/tmp".into());
    edit.reload(Some("documents".into()));
})
.join()
.unwrap();
```

The selection and the open branches are kept by key: a renamed or moved
node stays selected; a removed one leaves the selection (the cursor
stays where it was). A node inserted or moved under a branch that was
never loaded is left out: it comes with the branch.

## Selection, keys, pointer

`TreeView::mode` sets the selection: `Mode::Single` (the default),
`Mode::Multiple` (Ctrl toggles, Shift extends a range, Ctrl+A) or
`Mode::None`. The tree takes the keyboard focus as one widget; its keys
come after the application's key bindings (a key bound to a command
runs the command):

| Key | What it does |
|---|---|
| Up, Down, Page Up, Page Down, Home, End | Move the cursor and select; with Shift, extend the selection; with Ctrl, move the cursor alone |
| Left | Close the branch, or go to the parent |
| Right | Open the branch, or go to its first child |
| Space, Ctrl+Space | Select the cursor's node, toggle it |
| Enter | Open or close a branch, open a leaf (`tree.activate`) |
| F2 | Rename in place (with `TreeView::rename(true)`); Enter keeps, Escape cancels |
| Letters | Type-ahead: the next node whose name starts with what was typed in the last second |

A click on the triangle or a double click opens a branch. With
`TreeView::drag_drop(true)`, nodes (the selection, when the dragged node
is in it) are dragged before, after or inside a node; a node never goes
into itself, and the view scrolls near its edges.

## Steps and signals

Each moment is a step ([ADR 0011](adr/0011-lifecycle-steps.md)) when
the tree is given the application (`TreeView::app`) and `ViewsPlugin`
is connected; a plugin may veto, redirect, replace or follow it. The
same moments come back as `Signal`s in `TreeOutput::signals`, with
`Signal::Vetoed` when a step was stopped.

| Step | Arguments | Default |
|---|---|---|
| `tree.load` | `tree`, `key` (`null`: the roots) | asks the source; vetoed, the branch shows the reason |
| `tree.expand` | `tree`, `key`, `expanded` | opens or closes the branch |
| `tree.select` | `tree`, `keys`, `cursor` | sets the selection |
| `tree.drop` | `tree`, `keys`, `target`, `position` (`before`, `after`, `inside`) | moves the nodes in the view |
| `tree.rename` | `tree`, `key`, `from`, `to` | renames the node in the view |
| `tree.activate` | `tree`, `key` | nothing: the application opens the leaf |

The view changes what it shows; the application changes its data, from
the signals or from an after-hook. A before-hook refuses what the
application cannot do:

```rust
use egui_pop::egui;
use egui_pop::prelude::*;
use egui_pop::views::ViewsPlugin;
use egui_pop::views::steps::TREE_DROP;

struct ReadOnlyArchive;

impl Plugin for ReadOnlyArchive {
    fn id(&self) -> PluginId {
        PluginId::from_static("read_only_archive")
    }

    fn connect(&mut self, ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
        ctx.before(TREE_DROP, |_, args| {
            if args["target"] == "archive" {
                Before::Veto("the archive is read-only".into())
            } else {
                Before::Go
            }
        });
        Ok(())
    }
}

let app = App::builder()
    .plugin(ViewsPlugin)
    .plugin(ReadOnlyArchive)
    .build(&egui::Context::default())
    .unwrap();
let out = app.handle().step(
    &TREE_DROP,
    serde_json::json!({ "tree": "files", "keys": ["a.jpg"], "target": "archive", "position": "inside" }),
    |_, _| Ok(serde_json::Value::Null),
);
assert_eq!(out.vetoed(), Some("the archive is read-only"));
```

The gallery's `tree` page shows a simulated drive answered from a
thread, edits made while branches are open, a veto on drops into one
folder, and a branch of 100 000 files.
