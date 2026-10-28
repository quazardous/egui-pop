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
  `group` and `close`. The host gives it (`Host::history`); without
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
| `history.read` | `scope` | The scope's `undo` steps (oldest first) and `redo` steps (the next one last), each with its `text` in the interface's language; `rev` and `head`. |
| `history.undo`, `history.redo` | `scope`, `if_rev`, `dry_run` | The steps walked, the scope's new `rev` and `head`. |
| `history.jump` | `scope`, `to` (a step's id, 0: the start), `if_rev`, `dry_run` | The steps walked, in the order walked. |

- Without `scope`, a command goes where the keys go.
- `if_rev`: refused (`stale`) unless the scope's revision is still that
  one; read it from `history.read` first.
- `dry_run`: checked and answered, nothing changed and nothing
  published.

From code, `history::ask(app, Walk::Undo, Some(&scope), if_rev,
dry_run, &origin)` is the same request.

The debug bridge's `history` collector shows the scopes and where the
keys go; the runs are in the `steps` collector.

## Not yet

- **Snapshots** (named states pinned in a scope) and the panel's
  **hover preview** wait on owner support: the contract has no snapshot
  nor preview request yet.
- **Dropped redo states** of a durable owner, shown as "replaced" rows
  behind a toggle, wait on an owner that keeps them.
- Undo across processes is the owner's: it refuses with a reason, and
  the interface shows it.

The gallery's `history` page shows the list beside values bound to the
store, with a change made as an agent.
