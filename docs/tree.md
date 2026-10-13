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

## What a row shows

A row is its triangle, an icon, its label and widgets of the
application's at its right end. `TreeView::icon` gives each node's icon
(an `Icon` by role, a glyph or an image, `egui_pop::core::icon::Source`),
drawn in the row's text colour. `TreeView::row_widgets` adds widgets
right to left in the row (a visibility toggle, a lock): they act on
their row and never select it, and the label is cut before them. Both
are called for the rows in view only.

```rust
use std::collections::HashSet;

use egui_pop::egui;
use egui_pop::core::icon::Icon;
use egui_pop::views::tree::{Node, Sink, TreeSource, TreeState, TreeView};

struct Layers;

impl TreeSource<String> for Layers {
    fn load(&self, _: Option<&String>, sink: Sink<String>) {
        sink.deliver(vec![Node::leaf("sky".into(), "Sky"), Node::leaf("ground".into(), "Ground")]);
    }
}

fn layers(ui: &mut egui::Ui, state: &mut TreeState<String>, hidden: &mut HashSet<String>) {
    TreeView::<String>::new("layers")
        .icon(|_| Some(Icon::File.into()))
        .row_widgets(|ui, node| {
            let shown = !hidden.contains(&node.key);
            let icon = if shown { Icon::Visible } else { Icon::Hidden };
            let name = format!("{} {}", if shown { "Hide" } else { "Show" }, node.label);
            if ui.add(icon.label(&name).sense_click()).clicked() && !hidden.remove(&node.key) {
                hidden.insert(node.key.clone());
            }
        })
        .show(ui, state, &Layers);
}
```

The closures take the node: name the key's type (`TreeView::<String>`)
when the builder comes before `show`.

A right click opens a row's context menu **for that row**, whatever is
selected (the selection does not change; the row is outlined while its
menu is open). `TreeView::context_menu(app, menu)` opens a declared
menu (`Ctx::menu_item` or `CommandSpec::menu`): `{"tree": id, "key":
key}` is merged into its commands' arguments, so a command acts on
`args.key`. `TreeView::context_ui` opens a menu the application draws,
given the row's node. Either way the `tree.context` step runs first
(a veto keeps the menu closed) and `Signal::Context` names the node.

```rust
use egui_pop::egui;
use egui_pop::prelude::*;
use egui_pop::views::tree::{Sink, TreeSource, TreeState, TreeView};

const ROW: MenuId = MenuId::from_static("files.row");
const DELETE: CommandId = CommandId::from_static("files.delete");

struct Files;

impl TreeSource<String> for Files {
    fn load(&self, _: Option<&String>, sink: Sink<String>) {
        sink.deliver(Vec::new());
    }
}

/// Declares the row menu's command: it deletes `args.key`.
fn connect(ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
    ctx.command(
        CommandSpec::new(DELETE, "Delete", "Files").hidden().menu(ROW, "9_edit", 0),
        |call| {
            let key = call.args["key"].as_str().ok_or("no row")?;
            Ok(serde_json::json!({ "deleted": key }))
        },
    )?;
    Ok(())
}

fn files(ui: &mut egui::Ui, app: &AppHandle, state: &mut TreeState<String>) {
    TreeView::new("files").app(app).context_menu(app, ROW).show(ui, state, &Files);
}
```

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

A label may carry decorations (a size, a count, a state): the source
then gives the node its plain name apart, `Node::with_name`. A rename
starts from the name and type-ahead searches names; `tree.rename` and
`Signal::Rename` carry names, and the view keeps the decorated label
until the application sends the node again (`TreeHandle::update`).

```rust
use egui_pop::views::tree::Node;

let node = Node::leaf("notes".to_string(), "notes.txt · 2 KB").with_name("notes.txt");
assert_eq!(node.name(), "notes.txt");
```

`TreeState::set_selection` sets the selection without a step (a saved
state, another view following this one). Nodes the view has not loaded
yet are **queued**: the selection takes each as its branch loads, then
brings the cursor's row into view. Open the ancestors so that they
load; the user selecting drops what is queued, and
`TreeState::pending_selection` tells what still waits.

```rust
use egui_pop::views::tree::TreeState;

let mut state = TreeState::<String>::new();
// "2024/0005.raw" is under "photos/2024", not loaded yet.
state.set_expanded(&"photos".into(), true);
state.set_expanded(&"2024".into(), true);
state.set_selection(["2024/0005.raw".to_string()], Some("2024/0005.raw".to_string()));
assert!(state.selection().is_empty());
assert!(state.pending_selection().is_some());
```

A click on the triangle or a double click opens a branch. With
`TreeView::drag_drop(true)`, nodes (the selection, when the dragged node
is in it) are dragged before, after or inside a node; a node never goes
into itself, and the view scrolls near its edges.

A drop is refused, with its reason, in three places: by the source
(`TreeSource::can_drop`, what the data cannot take), at the call site
(`TreeView::can_drop`, what this view does not allow: a locked node),
and by a plugin's before-hook on `tree.drop` (below). The first two are
asked while dragging, where a refused target is marked in the danger
colour under a "not allowed" pointer, and again by `tree.drop`'s
default (after the hooks, so a redirected drop is checked too); a drop
they refuse leaves the nodes where they were and comes back as
`Signal::Vetoed` with the reason.

```rust
use std::collections::HashSet;

use egui_pop::egui;
use egui_pop::views::tree::{Position, Sink, TreeSource, TreeState, TreeView};

struct Layers;

impl TreeSource<String> for Layers {
    fn load(&self, _: Option<&String>, sink: Sink<String>) {
        sink.deliver(Vec::new());
    }

    /// The data's own rule: a layer holds no layers.
    fn can_drop(&self, _: &[String], target: &String, position: Position) -> Result<(), String> {
        match position {
            Position::Inside if !target.starts_with("group") => Err("only groups hold layers".into()),
            _ => Ok(()),
        }
    }
}

fn layers(ui: &mut egui::Ui, state: &mut TreeState<String>, locked: &HashSet<String>) {
    TreeView::new("layers")
        .drag_drop(true)
        // This view's rule: a locked layer stays where it is.
        .can_drop(|keys: &[String], _: &String, _| match keys.iter().find(|k| locked.contains(*k)) {
            Some(k) => Err(format!("{k} is locked")),
            None => Ok(()),
        })
        .show(ui, state, &Layers);
}
```

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
| `tree.drop` | `tree`, `keys`, `target`, `position` (`before`, `after`, `inside`) | asks the source and the view (`can_drop`), then moves the nodes in the view |
| `tree.rename` | `tree`, `key`, `from`, `to` | renames the node in the view |
| `tree.activate` | `tree`, `key` | nothing: the application opens the leaf |
| `tree.context` | `tree`, `key` | opens the row's context menu, for that row (a redirect changes the node it is for) |

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
thread, edits made while branches are open, drops refused by a hook,
by the source and by the page (a locked node), icons and an eye and a lock on every row, a context menu
acting on the row right-clicked, files named apart from their decorated
labels, a photo selected before its folder loads, and a branch of
100 000 files.
