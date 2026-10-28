# The context stack: places and modes

The context stack says where the user is and how the application
behaves right now. It is one small state machine, global to the
application, owned by the application's kernel and shown by the
interface (ADR 0012). It is also the session state: what comes back,
the same, when the application opens again.

```rust
use egui_pop::api::contexts::{self, ContextsPlugin, FrameSpec};
use egui_pop::api::{App, Ctx, Plugin, PluginError};
use egui_pop::model::ids::{FrameId, GroupId, PanelId, PluginId};
use egui_pop::model::panels::{Area, PanelSpec};

const ALBUM: FrameId = FrameId::from_static("photos.album");
const CULLING: FrameId = FrameId::from_static("photos.culling");
const RATINGS: PanelId = PanelId::from_static("photos.ratings");

struct Photos;

impl Plugin for Photos {
    fn id(&self) -> PluginId {
        PluginId::from_static("photos")
    }

    fn requires(&self) -> Vec<PluginId> {
        vec![contexts::PLUGIN]
    }

    fn connect(&mut self, ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
        ctx.panel(PanelSpec::new(RATINGS, "Ratings", Area::EndSide).closed(), |ui, _| {
            ui.label("Stars and flags");
        })?;
        ctx.frame(FrameSpec::place(ALBUM, "Album"))?.frame(
            FrameSpec::mode(CULLING, GroupId::from_static("orientation"), "Culling")
                .show(RATINGS),
        )?;
        Ok(())
    }
}

let app = App::builder()
    .plugin(ContextsPlugin::default())
    .plugin(Photos)
    .build(&egui_pop::egui::Context::default())
    .unwrap();
let h = app.handle();
contexts::enter(h, ALBUM, egui_pop::model::serde_json::json!("holidays"));
contexts::set_mode(h, CULLING);
assert_eq!(h.context().get("place"), "photos.album");
assert_eq!(h.context().get("mode.orientation"), "photos.culling");
assert!(h.panel_visible(&RATINGS));
```

The gallery's `contexts` page shows it: an album, a photo and a mask to
drill into, an orientation, a compare, keys and panels that follow, and
a guard that refuses.

## Places or modes

| | A place | A mode |
|---|---|---|
| What it is | a drill-down: a group, a composite, a mask, a sub-workspace | a global state: an orientation, compare, read-only, a soft proof, "an agent is driving" |
| Shown as | a crumb of the breadcrumb (`contexts::breadcrumb`) | a chip with a way out (`contexts::mode_chips`), a switch (`mode_switch`), a toggle (`mode_toggle`) |
| Moves | `enter` (with a payload: which group, which mask), `leave`, `jump` to a level, `go` to a path | `set_mode` (sticky), `push_mode` (temporary), `pop_mode`, `clear_mode` |
| How many | a path from the root | one per group; groups combine |
| The projection | the place's arrangement the first time (none declared: the enclosing place's layout), then the layout it was left with | the panels it shows or hides, or an arrangement of its own; a layout change made in a mode is the mode's, gone when it is left |
| Back (`nav.back`, Alt+Left) | leaves it | a pushed mode pops (Escape too), putting back what it replaced; a set mode stays |

Use a place when the user goes *somewhere* and expects to come back to
where they were. Use a mode when the same place should *behave
differently*: other keys, other panels, other defaults. Use a pushed
mode for a moment (compare, a tool mode with a clear way out), a set
mode for an orientation that lasts. Declare a frame **transient** when
it must never come back after a restart (an agent driving, a modal
tool).

Modes are global to the stack: an orientation set at the root stays on
in a sub-place. All windows of the application show the same stack.

A frame's title (`FrameSpec::place(id, "Album")`) is English; crumbs,
chips and switches translate it like every other declaration, under
the key derived from its id: `frame-<id>.title` (`photos.album` gives
`frame-photos-album.title`). The extractor of `egui-pop-i18n` lists
them with the commands and panels. The root's crumb says "Home",
translated, unless the application names it: a text as given
(`ContextsPlugin::default().root("Main")`), or a string of its own
catalogue, translated each time the crumb is said (`root_key`).

The root's crumb can also say what is open, a workspace or a project:
`ContextsPlugin::root_label` is asked each time the crumb is said,
given the translated title and the `egui::Context` to translate with;
`None` keeps the title. The breadcrumb, the status bar, `context.read`
(`projection.crumbs`), the announcement of where the user is and the
undo toast all say it, and follow when the name changes.

```rust
# use egui_pop::api::contexts::ContextsPlugin;
# use egui_pop::core::strings::{self, StrKey};
# use std::sync::{Arc, Mutex};
const WORKSPACE: StrKey = StrKey::new("photos-workspace", "Workspace { $name }");
// The workspace open, as the application keeps it.
let open: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));
let name = open.clone();
let plugin = ContextsPlugin::default().root_label(move |c| {
    let name = name.lock().ok()?.clone()?;
    Some(strings::tr_args(c.ctx, &WORKSPACE, &[("name", &name)]))
});
```

Like a place's label, it is called while the interface draws: it reads
what the application keeps, never the context stack itself.

## A place's payload

A place's crumb says its title, and its payload when the payload is a
text or a number (`Album Holidays`, `Photo 3`). A payload that is an
object (a sub-workspace, `{"id": "darkroom"}`) says nothing by itself:
the frame names it with `FrameSpec::label`, given the payload and the
translated title; `None` keeps the default. The breadcrumb,
`context.read` (`projection.crumbs`), the announcement of where the
user is and the undo toast all say the crumb this way.

```rust
# use egui_pop::api::contexts::FrameSpec;
# use egui_pop::model::ids::FrameId;
# fn name_of(_: &str) -> Option<String> { None }
const SPACE: FrameId = FrameId::from_static("photos.space");
let space = FrameSpec::place(SPACE, "Workspace")
    // The sub-workspace's name, read from the application's own state.
    .label(|c| c.payload["id"].as_str().and_then(name_of))
    // The darkroom starts from its own arrangement, the others from the
    // frame's.
    .arrangement_for(|p| (p["id"] == "darkroom").then(|| "Darkroom".to_string()))
    .arrangement("Workspace");
```

The label is called while the interface draws: it reads what the
application keeps (behind an `Arc` it captured), never the context
stack itself.

`FrameSpec::arrangement_for` chooses, from the payload, the arrangement
an entry starts from; `None` falls back to the frame's
`arrangement`. The chosen one is what the place declares for that
payload: the next section's rules apply to it unchanged (the layout a
projection key was left with wins on later entries; the user's choice
for the frame wins over what it declares).

The top place's payload also sets context keys, for `when` clauses: a
text, a number or a boolean as `payload`, an object's fields of those
kinds as `payload.<field>` (nested values set nothing). A key binding
for one sub-workspace only:

```rust,ignore
ctx.keybinding("K", PICK, "place == 'photos.space' && payload.id == 'darkroom'", Layer::Default)?;
```

## Arrangements by place and mode

The current arrangement (the View menu's switch, save and reset) is
the one of where the user is: the **arranged frame**, the last active
mode that declares an arrangement (`FrameSpec::arrangement`), else the
top place, else the root. A frame starts from the arrangement it
declares; when the user switches or saves there, the choice is that
frame's, kept in `settings/workbench.arrangement.frames` (frame id to
name), and every later entry of the frame (another album, another
photo) starts from it. The root's stays in
`settings/workbench.arrangement`, where it always was: a settings file
written before places had their own reads the same, its name the
root's. Deleting an arrangement puts the frames that were on it back on
what they declare.

| Where | The current arrangement |
|---|---|
| the root | `settings/workbench.arrangement` |
| a place | its entry in `settings/workbench.arrangement.frames`, else its `arrangement` |
| a mode declaring an arrangement, on | the same, for the mode; it wins over the place while it is on |

A place that declares no arrangement, the first time it is entered,
starts from the nearest enclosing place's layout: as that place was
left, else the arrangement it starts from, the root's last. A photo
under an album starts from the album's layout, a place at the top
level from the root's, whichever place the user came from: opening
the photo from another place does not carry that place's layout over.
With nothing remembered at all (the first start), it keeps the layout
shown. Debug builds log a warning for such a place; declaring an
arrangement for it says what it should look like.

`arrangements::arranged_frame` says which frame it is.

## Kernel state and view state

The stack has an owner, and only the owner changes it:

- **Kernel state** (the owner's): the places, the modes, their payloads,
  the holds, a revision. Without a kernel of its own an application
  uses `LocalContexts` (kept by `ContextsPlugin`), which writes the
  stack to the `context` document inside each transition, before
  anyone hears of it. An application with a kernel implements the
  `Contexts` trait there and gives it through `Host::contexts()`; it
  may be in another thread or process (`Answer::Later`).
- **View state** (the interface's): the workbench's layout, the
  focused panel, scroll offsets and whatever a component keeps through
  `contexts::view_value` and `set_view_value` (or
  `contexts::scroll_area`), each kept per projection key (the place path
  and the active modes). It never reaches the kernel; it is written to
  the `view` document about a second after it changes. Values kept per
  place rather than per projection key (`contexts::place_value`,
  `set_place_value`: the place's frame id, `root` at the root, whatever
  its payload and the modes) go into the same document; the status
  bar's choices are kept so.

The interface reads and asks; it never writes the stack. Each
transition is the step `context.request`: a before-hook stops it (the
interface's own guards: unsaved typed input, a drag of a tab) or
redirects it, the default asks the owner. The owner's guards
(`FrameSpec::can_enter`, `can_leave`) refuse or redirect (a deleted
place lands on its parent), and **holds** (`contexts::hold`, a drag, a
batch, an agent holding the image) refuse whatever touches their group
with their reason. The display changes only when the owner's revision
moves, so a refusal leaves it as it was; the user sees why in a toast,
an agent in the answer.

Nothing waits for the exit: the stack is written at each transition,
the view state, the layout, the settings and the keymap a second after
they change (the debounced writer of `egui_pop_core::persist`). A crash
loses at most that second; a vetoed exit loses nothing.

The `context` and `view` documents sit beside the layout's, as the
host names it (`Host::layout_document`): a host keeping a layout per
workspace (`workspaces/photos/layout`) keeps a stack and a view per
workspace too (`workspaces/photos/context`, `.../view`;
`contexts::documents`). When the host opens another workspace in the
same window, `contexts::reload` reads them again.

At start the saved stack comes back against today's frames: an
unknown or transient frame, or a place whose payload is gone
(`ContextsPlugin::valid`), is dropped, a place with the places under
it; a toast says what did not come back.

## How a plugin reacts

Nobody polls a global. Everything keys on the context keys the
projection sets, in `when` clauses (how the stack sits beside the value
store, commands and steps: [State or event](state-or-event.md)):

| Key | Value |
|---|---|
| `place` | the top place's id, `root` at the root |
| `placeDepth` | how many places deep |
| `mode.<group>` | the mode on in the group, empty when none |
| `modePushed` | whether a pushed mode is on |
| `payload`, `payload.<field>` | the top place's payload: a text, a number or a boolean, or an object's fields of those kinds |
| a frame's own keys | `FrameSpec::key`, while the frame is on |

- **Commands, menus, toolboxes**: `when: "mode.orientation == 'photos.culling'"`.
- **Keys**: `Ctx::frame_keybinding(&culling, "P", PICK)` binds keys in
  the frame's own layer, through its clause; the same key can do
  something else in another mode of the same group without a conflict.
  Two frames that can be on together (a place and a mode, modes of two
  groups) binding the same keys clash: while both are on, one never
  fires. `contexts::frame_clashes` lists them, the keymap check of
  `egui-pop-test` (`keymap_problems`) and the `contexts` collector say
  them, and each is a diagnostic at start (`context-frame-key-clash`).
- **Panels**: `FrameSpec::show` and `hide` for a mode, an arrangement
  for a place or a mode; or a panel reads the keys and draws differently.
- **Events**: the journal says `context.changed` (with the whole new
  state and its revision), then `context.left` and `context.entered`
  per frame, `context.refused` with the reason, `context.restored`,
  `context.held` and `context.released`. A plugin that must act on one
  frame reads `context.entered`.
- **Undo**: each step of a history carries the place it was made in
  (`StepInfo::place`, ADR 0014): the stack tells the value store where
  new steps are made (`Store::set_place`), and a host owning its own
  histories fills it the same way. An undo, a redo or a jump in the
  history panel that walks a change made out of sight says where in a
  toast ("Undone in Photo 3"), with "Go there"; a component's local
  history (the theme editor, the settings window) is undone where it
  is shown and says nothing.

The stack's own keys (`place`, `placeDepth`, `modePushed`,
`mode.<group>`, `payload`, `payload.<field>`) are the projection's alone: a plugin setting one by
hand (`AppHandle::set_context`) makes two truths, and the projection
overwrites it at the next transition. Each such write is reported: a
diagnostic the first time per key (`context-key-by-hand`), and the
`contexts` collector's `warnings` (`hand_writes`: the key, the value,
how many times, the command running and its plugin).

Agents use the same path as the user: `context.read` (which also
gives what the status bar shows, in `status`),
`context.request` (any transition, with `if_rev` and `dry_run`),
`context.go`, `context.set_mode`, `context.hold` and `context.release`;
the `contexts` collector of the debug bridge shows the state, the
holds, the projection and the last context events.
