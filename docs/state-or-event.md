# State or event: what to use

A plugin has several ways to share data with the rest of an
application: the value store, commands, steps, the context stack,
services and alter hooks. Each has its own page; this one says which
to pick, and what goes wrong with the wrong one.

There are two kinds of data:

- **State**: a value others read whenever they draw. The last value
  written is the truth; nothing is lost when someone does not look at
  it for a frame. A setting, a parameter, a selection, where the user
  is.
- **Events**: something that happened, or must happen, once. Each one
  is acted on exactly once, in order, whoever happens to be on screen.
  A request to delete, to save, to open; a moment others may stop.

## The per-frame trap

egui is immediate mode: every visible panel's `ui()` runs every frame,
and a hidden one does not run at all. Two mistakes follow from it.

- **State written while drawing.** A panel that copies its own field
  into a shared value each time it draws writes every frame. Two
  panels doing so race: the one drawn last wins, and a change the user
  made in the other is overwritten in the same frame by a stale copy.
- **An event kept as a flag.** A click that sets `delete_requested =
  true`, for another panel to see and reset, is lost when that panel is
  hidden or closed (its `ui()` never runs), acted on twice when two
  panels read it, or late by a frame. A flag has no order and no
  count: two clicks in one frame are one.

egui-pop keeps both out of the frame:

- A bound widget (`Binding::bound`) reads the store every frame but
  writes only when the user changes it, with an origin, and a drag is
  one gesture that holds the path against other writers
  ([ADR 0009](adr/0009-value-binding.md)).
- `AppHandle::run` does not run the command in the middle of a panel:
  it queues the request, and the command runs once, before the next
  frame, whichever panels are visible.
- Components hand edits back instead of changing shared data in place:
  a finished slider change, a node graph edit, a tree rename runs as a
  step, once per edit, not once per frame.

## One row per need

| The need | Use | The mistake it avoids |
|---|---|---|
| A value others display and may edit (a setting, a parameter) | the value store: `Ctx::setting`, or a path declared on `AppHandle::store()`; widgets bind it with `Binding::bound`, views with `Binding::watch` | a copy per panel written back every frame |
| To react when a value changes | `Ctx::on_change(prefix, f)` on the store | comparing with last frame's value in `ui()` |
| A request someone must act on once (delete, save, open) | a command: `Ctx::command`, asked with `AppHandle::run` | a flag in the store that the next frame forgets or reads twice |
| A moment others may observe, veto, replace or extend | a step: `Ctx::step`, run with `AppHandle::step`; others use `Ctx::before`, `replace`, `after` | a callback list of its own, or a flag others poll |
| To follow what happened (steps, context changes) | the journal: each step publishes a signal, read with `AppHandle::events_since` | a "last event" value that two readers race to clear |
| Where the user is, and how the application behaves there | the context stack: `contexts::enter`, `set_mode`; read through `when` clauses and `context.entered` | a context key set by hand (`AppHandle::set_context` on `place` or `mode.*`), which the projection overwrites |
| A flag for `when` clauses that is not a place or a mode | `AppHandle::set_context` with a key of the plugin's own | a store value read in every `when` by hand |
| An object other plugins call (a file chooser, a log, an editor registry) | a service: `Ctx::service` with a `ServiceToken`, got with `AppHandle::service` | a global, or a value in the store that is really an object |
| To change another plugin's declarations (menu items, palette entries, settings form) | an alter hook: `Ctx::hook` with `Hook::menu`, `palette` or `settings`; it runs when the application is built or a plugin comes or goes | rewriting a menu while it is drawn |
| Something a background thread hands over | a queue the thread fills, taken at `On::Pass` (`Ctx::on`) before the frame | reading the thread's data from `ui()` |

Rules of thumb:

- If missing it for a frame is harmless and only the latest value
  matters, it is state: the store, or the context stack.
- If it must happen exactly once, it is an event: a command. If
  others may want to stop or change it, run it as a step inside the
  command.
- A service shares an object, not data that changes: what the object
  holds is either its own state or reached through its methods.
- `ui()` reads state and asks for events. It does not write state
  every frame and does not act on events.

## One example, both ways

A photo browser shows a "Picked" checkbox in two panels, the grid's
toolbar and the info panel, and a "Delete" button in the grid.

The wrong way: each panel keeps its own field, copies it into the
store every frame, and the delete is a flag.

```rust
# use egui_pop::core::lock;
# use egui_pop::egui;
# use egui_pop::model::panels::{Area, PanelSpec};
# use egui_pop::model::values::{Change, Set, Value};
# use egui_pop::prelude::*;
# const GRID: PanelId = PanelId::from_static("photos.grid");
# const PICKED: &str = "photos/picked";
# const DELETE_ASKED: &str = "photos/delete_asked";
# struct Photos;
# impl Plugin for Photos {
#     fn id(&self) -> PluginId {
#         PluginId::from_static("photos")
#     }
#     fn connect(&mut self, ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
let mut picked = false;
ctx.panel(PanelSpec::new(GRID, "Grid", Area::Center), move |ui, app| {
    ui.checkbox(&mut picked, "Picked");
    let store = app.store();
    let mut s = lock(&store);
    // Written every frame: the info panel does the same and the last
    // drawn wins, so a click in one panel is undone by the other.
    let _ = s.set(Set::new(PICKED, Change::To(Value::Bool(picked)), Origin::User));
    if ui.button("Delete").clicked() {
        // A flag: lost if the panel that reads it is closed, read
        // twice if two panels act on it.
        let _ = s.set(Set::new(DELETE_ASKED, Change::To(Value::Bool(true)), Origin::User));
    }
})?;
#         Ok(())
#     }
# }
# App::builder().plugin(Photos).build(&egui::Context::default()).unwrap();
```

The right way: the panels bind the stored value, and the delete is a
command, with a step others may veto.

```rust
use egui_pop::core::{binding, lock};
use egui_pop::egui;
use egui_pop::model::panels::{Area, PanelSpec};
use egui_pop::prelude::*;

const GRID: PanelId = PanelId::from_static("photos.grid");
const INFO: PanelId = PanelId::from_static("photos.info");
const PICKED: &str = "photos/picked";
const DELETE: CommandId = CommandId::from_static("photos.delete");
const REMOVE: StepId = StepId::from_static("photos.remove");

struct Photos;

impl Plugin for Photos {
    fn id(&self) -> PluginId {
        PluginId::from_static("photos")
    }

    fn connect(&mut self, ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
        // The state: one path in the store, declared once.
        lock(&ctx.app().store())
            .declare(ctx.owner(), PICKED, Descriptor::bool("Picked", false))
            .map_err(|e| PluginError::Failed(e.to_string()))?;

        // The event: a command, which runs a step others may veto.
        ctx.step(StepSpec::new(REMOVE, "Removes a photo."))?;
        ctx.command(CommandSpec::new(DELETE, "Delete", "Photos"), |call| {
            let out = call.app.step(&REMOVE, call.args.clone(), |_, args| {
                // Remove the photo named in `args`.
                Ok(args.clone())
            });
            match out.vetoed() {
                Some(reason) => Err(reason.to_string()),
                None => Ok(serde_json::Value::Null),
            }
        })?;

        // The same in both panels.
        ctx.panel(PanelSpec::new(GRID, "Grid", Area::Center), picked)?;
        ctx.panel(PanelSpec::new(INFO, "Info", Area::EndSide), picked)?;
        Ok(())
    }
}

/// Reads the store every frame, writes it only on a click.
fn picked(ui: &mut egui::Ui, app: &AppHandle) {
    binding::edit(ui, Binding::bound(&app.store(), PICKED), |ui, v: &mut bool| {
        ui.checkbox(v, "Picked")
    });
    if ui.button("Delete").clicked() {
        // Queued: runs once, before the next frame.
        app.run(DELETE, serde_json::json!({ "photo": "current" }));
    }
}

// Another plugin refuses to delete a locked photo, without a flag of its own.
struct Locks;

impl Plugin for Locks {
    fn id(&self) -> PluginId {
        PluginId::from_static("locks")
    }

    fn requires(&self) -> Vec<PluginId> {
        vec![PluginId::from_static("photos")]
    }

    fn connect(&mut self, ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
        ctx.before(REMOVE, |_, args| {
            if args["locked"] == true { Before::Veto("the photo is locked".into()) } else { Before::Go }
        });
        Ok(())
    }
}

let mut app = App::builder().plugin(Photos).plugin(Locks).build(&egui::Context::default()).unwrap();
let out = app.run(&DELETE, serde_json::json!({ "photo": "a" }), Origin::User);
assert_eq!(out.now(), Some(Ok(serde_json::Value::Null)));
let out = app.run(&DELETE, serde_json::json!({ "photo": "b", "locked": true }), Origin::User);
assert_eq!(out.now(), Some(Err("the photo is locked".to_string())));
```

Both panels now show the same value, a click in either one is the
only write, and it is one undo step with an origin. The delete runs
exactly once per click, whichever panels are open; a plugin that must
refuse it (the photo is locked) adds a before-hook on the step instead
of a flag of its own (`Locks` above), the key, the menu and agents reach the same
command, and the journal says it happened.

## See also

- [Plugins, commands and steps](plugins.md): commands, hooks, steps.
- [The context stack](contexts.md): places, modes, kernel state and
  view state.
- [ADR 0009](adr/0009-value-binding.md) (binding a value) and
  [ADR 0011](adr/0011-lifecycle-steps.md) (steps).
- The "state versus events" question is also settled, for its own
  primitives, in egui_mobius's book.
