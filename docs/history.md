# History: undo, redo, the history panel

egui-pop's undo is the kernel's: the application's owner keeps its
histories, egui-pop routes the keys to them and shows them. The decision
and its reasons are [ADR 0014](adr/0014-undo.md); this page is how to
use it. Everything below is `egui_pop::api::history` (the contract, the
routing), `egui_pop::api::history_view` (the panel, the agents'
commands) and `egui_pop::model::history` (scopes, steps, lines, no
egui).

## The model

- **The owner** implements `History`: its **scopes** (one history
  each: a document's edit, a collection's marks, the application's
  values), their steps, and the requests `undo`, `redo`, `jump`,
  `group` and `close`; and, when it supports them, snapshots, the
  hover preview, undoing one step out of order and the replaced steps
  (below: [Beyond undo and redo](#beyond-undo-and-redo)). The host gives it (`Host::history`); without
  one, egui-pop's `StoreHistory` answers over the value store's
  application history (`APP`), capped at 500 steps.
- **A step** says what it is, not how to undo it (`StepInfo`): its id,
  scope, label (a message id and its arguments, shown in the
  interface's language), origin (the user, an agent, a script, another
  process), group, the **place** of the context stack it was made in,
  its time, and whether it is still in progress. The inverse stays with
  the owner.
- **Local scopes** are a component's own history, kept in the
  interface: the theme editor's draft, the settings window's values.
  A component registers one for its focus surface
  (`Ctx::local_scope`).
- **Redo is linear**: a new step drops the redo list of its scope.

## Which history Ctrl+Z undoes

Innermost first:

1. a focused text field keeps its own undo;
2. a focused surface with a local scope undoes there;
3. otherwise the newest step among the owner's scopes **active** in the
   window (those whose focus targets are drawn; a scope without any is
   always active). Redo goes back to the scope last undone.

Undo, redo and jump are steps (`edit.undo`, `edit.redo`, `edit.jump`):
a before-hook may veto one or name another scope, and a gesture still
going in the scope refuses them. Each answer is published on the
journal: `history.changed` (the scope, its revision and head) or
`history.refused` (the scope, the kind and the reason).

## Undone out of sight: "Go there"

With the context stack (`ContextsPlugin`, [contexts.md](contexts.md)),
the stack tells the value store where new steps are made
(`Store::set_place`), so each of its steps carries its place. An undo,
a redo or a jump that walks a step made in another place than the one
shown says so in a toast, "Undone in Photo 3", with **Go there**, which
goes back to that place. A host owning its own histories fills
`StepInfo::place` the same way and gets the same toast. A local scope
is undone where it is shown, and says nothing.

## The history panel

`HistoryPlugin` brings the `History` panel (`PANEL_HISTORY`, in the end
side bar, closed until asked for: its `egui_pop.history.toggle`
command, or `AppHandle::set_panel_visible`); `history_view::show(ui, app)` draws the same list anywhere.

- With several scopes, chips pick the one shown; until one is picked,
  the panel shows where Ctrl+Z goes.
- A **Start** row comes first: the state before the first step.
- Then the steps, oldest first. The head is the selected row; the steps
  a redo would make again are muted under it.
- A step made by an agent, a script or another process names its origin
  after its label: "Radius (agent:helper)".
- A click on a row brings the scope there: undos or redos, all or
  nothing, through `edit.jump`, so a hook can veto it as it vetoes an
  undo.
- Hovering a row previews the state there (when the owner answers):
  the tooltip lists what differs, "Exposure: 0.30 EV (now 1.20 EV)".
- The owner's **snapshots** come first, under a "Take a snapshot"
  button; a click on one restores it, its context menu restores or
  deletes it.
- A step's context menu has **Undo this step**, when the owner allows
  it; a refusal says why in a toast.
- With replaced steps, a **Show replaced steps** toggle lists them,
  muted and marked "(replaced)", under the step their line left from;
  a click goes back to one.

```rust
use egui_pop::api::App;
use egui_pop::api::history_view::HistoryPlugin;

let app = App::builder()
    .plugin(HistoryPlugin::default())
    .build(&egui_pop::egui::Context::default())
    .unwrap();
```

`HistoryPlugin::default().without_panel()` keeps the commands only, for
an application drawing the list in a place of its own.

## For agents: `history.*`

The commands run through the same steps as the keys, so a hook vetoing
an undo vetoes an agent's too. They are kept out of the command palette
(Edit > Undo and Redo are there).

| Command | Arguments | Answer |
|---|---|---|
| `history.list` | none | Every scope (id, title, local, durable, cap), whether it is active, its revision, head and counts; where Undo and Redo go and what they would do. |
| `history.read` | `scope` | The scope's `undo` steps (oldest first) and `redo` steps (the next one last), each with its `text` in the interface's language; `rev` and `head`; its `snapshots`, its `replaced` steps (each with the step it came `after`) and the `features` its owner supports. |
| `history.undo`, `history.redo` | `scope`, `if_rev`, `dry_run` | The steps walked, the scope's new `rev` and `head`. |
| `history.jump` | `scope`, `to` (a step's id, 0: the start, or a replaced step), `if_rev`, `dry_run` | The steps walked, in the order walked. |
| `history.snapshot` | `scope`, `name` (none: "Snapshot 3"), `if_rev`, `dry_run` | The `snapshot` taken (id, name, the head it was taken at, origin, time). |
| `history.restore` | `scope`, `snapshot`, `if_rev`, `dry_run` | The step made, the scope's new `rev` and `head`. |
| `history.drop_snapshot` | `scope`, `snapshot`, `if_rev`, `dry_run` | The snapshot `deleted`. |
| `history.undo_step` | `scope`, `step`, `if_rev`, `dry_run` | The step undone; refused with the reason when a later step changed the same values. |

- Without `scope`, a command goes where the keys go.
- `if_rev`: refused (`stale`) unless the scope's revision is still that
  one; read it from `history.read` first.
- `dry_run`: checked and answered, nothing changed and nothing
  published.

From code, `history::ask(app, Walk::Undo, Some(&scope), if_rev,
dry_run, &origin)` is the same request (`Walk::Restore` and
`Walk::UndoStep` too).

The debug bridge's `history` collector shows the scopes and where the
keys go; the runs are in the `steps` collector.

## Beyond undo and redo

Each of these is a request of `History` with a default, so an owner
that implements none of them keeps working unchanged: the panel offers
only what `History::features` says the scope supports.

- **Snapshots**: named states pinned in a scope, never dropped by the
  cap. `snapshots` lists them; `snapshot` takes one (refused as
  `unsupported` by default), `restore` brings the scope back to one as
  a new step (`edit.restore`, vetoable like an undo, so an undo takes
  the restore back), `forget` deletes one. Taking or deleting one is
  published as `history.snapshot_taken` or `history.snapshot_deleted`.
- **The hover preview**: `preview(scope, Some(at))` while the pointer
  is on a row (`At::Step`, 0 the start, or `At::Snapshot`),
  `preview(scope, None)` when it leaves (or the panel is no longer
  drawn), asked once per row. The panel ends the preview right after
  drawing its rows in the pass the pointer leaves them, and a pointer
  resting on a row asks for no repaint: the window idles. The owner may show the state in its own
  views (`Preview::shown`: the image as it was) and list what differs
  (`Preview::changes`), which the tooltip shows. Nothing is committed.
  The default answers `None`: no preview, the panel shows its hint
  only.
- **Undo this step**: `undo_step` undoes one step out of order
  (`edit.undo_step`), or refuses with a reason. VS Code's rule:
  refused when a later step changed one of the same values
  (`history::may_undo_out_of_order` checks it for an owner).
- **Replaced steps**: redo lists a new step dropped, kept by an owner
  whose history keeps every state; `replaced` lists them, each with
  the step its line left from, and `jump` reaches them. Hidden behind
  the panel's toggle.

`StoreHistory`, the default owner, supports all four over the value
store:

- a snapshot pins the value of every path the scope's steps touched
  (a path first changed after it is pinned at the value it had);
  restoring writes them back as one step, "Restore “Before the
  agent”";
- the preview lists the values that differ at the point, with their
  labels, units and precision; it shows nothing itself;
- undoing the newest step is an undo; an older one is undone by a new
  step, "Undo “set doc/n”", writing back the values it found;
- the redo list a new step drops is kept as a replaced line (as many
  steps as the cap); a jump to one of its steps goes back to where it
  left, then along it, and the line left becomes a replaced one in its
  place. A line that left from a step itself replaced is reached after
  going to that line first.

## Not yet

- Undo across processes is the owner's: it refuses with a reason, and
  the interface shows it.
- Comparing two states side by side (a snapshot against now) is the
  owner's view; the panel has no compare toggle yet.
- Agents have no command for the preview: it follows the pointer.

The gallery's `history` page shows the list beside values bound to the
store, with a change made and a snapshot taken as an agent; every
feature above runs there over the default owner.
