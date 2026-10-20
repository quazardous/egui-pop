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
| The projection | the place's arrangement the first time, then the layout it was left with | the panels it shows or hides, or an arrangement of its own; a layout change made in a mode is the mode's, gone when it is left |
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
them with the commands and panels.

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
  the `view` document about a second after it changes.

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
- **Undo**: undo steps remember the place they were made in. An undo
  of a change made out of sight says where in a toast, with "Go there".

The stack's own keys (`place`, `placeDepth`, `modePushed`,
`mode.<group>`) are the projection's alone: a plugin setting one by
hand (`AppHandle::set_context`) makes two truths, and the projection
overwrites it at the next transition. Each such write is reported: a
diagnostic the first time per key (`context-key-by-hand`), and the
`contexts` collector's `warnings` (`hand_writes`: the key, the value,
how many times, the command running and its plugin).

Agents use the same path as the user: `context.read`,
`context.request` (any transition, with `if_rev` and `dry_run`),
`context.go`, `context.set_mode`, `context.hold` and `context.release`;
the `contexts` collector of the debug bridge shows the state, the
holds, the projection and the last context events.
