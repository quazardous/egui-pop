# 0014, appendix: the analysis behind undo

The analysis that led to [ADR 0014](0014-undo.md), kept for the reasons
it gives. Where it differs from the ADR, the ADR wins. Helios, the
application egui-pop is built for, has its own note on the kernel side
(its `docs/design/undo.md`); this appendix summarises what it needs.

## What exists today

Read from the code on `main` (2026-10-14).

### In egui-pop

| History | Holds | Scope | Persisted | Limit | Reached by | Gaps |
|---|---|---|---|---|---|---|
| Value store (`egui-pop-model::values`) | `Step { label, origin, changes: [(path, old, new)] }`, an undo and a redo `Vec` | one store per application | no (`export` saves values only) | none | `edit.undo` (Ctrl+Z, `!textInputFocus && canUndo`) → `Host::undo` → `Store::undo`; also `value.undo`, the debug bridge's `store` collector | unbounded; no coalescing outside a gesture (a bound slider makes a step per arrow press, a bound text field one per keystroke, a colour button one per frame); an undo from the user's own origin may replay into the user's own drag (`holder` only refuses other origins); `value.undo` bypasses the `edit.undo` step |
| Gestures and groups (same store) | `begin_gesture(origin, label)` holds its paths against other origins; `begin_group` holds nothing; `end` makes one step; `cancel` writes the old values back, no step | per gesture | no | n/a | sliders (`binding::edit`: begin on drag start, end on stop, cancel on Escape), `slider.change` step | not in the `Host` contract: a host with its own kernel cannot say "a gesture is open" |
| `ThemeDraft` (`egui-pop-tokens::draft`) | `TokenEdit { path, from, to }`, undo and redo `Vec`s | one theme editor | no | none | the editor's toolbar buttons, hidden `theme.undo` and `theme.redo` commands | Ctrl+Z in the editor reaches the store, not the draft; a second gesture opened before the first ends replaces it unrecorded; any outside theme change clears the history |
| Code editor (`egui-pop-editors::history`) | `Entry { kind, forward, inverse, before, after, time, open }` | one editor | no | 1000 entries (undo only) | its own Ctrl+Z, Ctrl+Shift+Z, Ctrl+Y while focused; `editor.undo`, `editor.redo`; each undo runs the `edit.change` step and rolls back on veto | coalesces typing (under a second, contiguous, no whitespace); the host cannot add entries; Edit > Undo does not reach it |
| egui `TextEdit` undoer | text snapshots | one field | no | egui's defaults | egui, while the field has focus | not configured anywhere; fine as it is |
| Graph widget (`egui-pop-viewport::graph`) | emits `Edit::{Connect, Disconnect, Move, Delete, Add, Param}` as `graph.*` steps | one graph view | no | n/a | n/a | no history and no inverse: `Delete` lists ids only, `Add` has no resulting ids; undo is left to the application |
| Steps and hooks (ADR 0011) | `StepTrace` per run | application | no | 64 | n/a | a log, not a history |
| Command runs | `Run` per command | application | no | 64 | n/a | a log |
| Host journal (`host::Journal`) | `Event { seq, source, cause, name, data }` | application | no | ring of 1024 | n/a | a log; `edit.undone`, `edit.redone`, `edit.undo_failed`, `edit.redo_failed` are published there |
| Context stack stamps (branch `wip/context-stack`) | a place path per undo and redo step, kept parallel to the store's stacks | application | no | follows the store | `ctx.after(edit.undo)` shows "Undone in <place>" with "Go there" | matched by count against the store's stacks only, taken one pass after the change; wrong as soon as the host answers undo from elsewhere |

### In Helios (summary)

- **Value store** (`helios-values`): the same `Step` model, one store
  per window, unbounded, memory only; gestures for drags, groups for
  held keys and the masks panel's edits; transient values (the view,
  the selection, mask editing state) never make a step. Ctrl+Z reaches
  it through `HeliosHost`; a plugin vetoes undo while a gesture is
  open. The stack is not cleared when another image is opened.
- **The image's edit** (`helios-vault`): every committed step (a write,
  a gesture's end, an undo, a redo, an agent's parameter, a decision
  answered) is saved as a content-addressed **version** continuing the
  previous one, a drag every second as "in progress". An undo is a new
  version: the history only grows. Survives `kill -9` (survival
  scenarios gate a release). The undo stacks are not rebuilt on reopen.
  A version holds parameter values and marks, not the network's shape.
- **Network edits** (masks panel): adding a shape, moving or taking out
  a member, rewiring are **not** undo steps and not saved with the
  edit; taking out a group that holds members asks for confirmation for
  that reason. Decided: each becomes one undo step restoring the values
  it moved; not decided: how such a network is saved with the edit.
- **Library**: ratings, rejects, picks, labels and tags are field
  commands with an audit row each (old, new, who); stacks are accepted,
  corrected or declined decisions; import and duplicates are jobs.
  None is undoable.
- **Workspace** (branch `wip/workspace-edit`): the workspace's state is
  one value of the store, so each edit, reset or parts import is one
  Ctrl+Z step; agents on a node have a second, file-based undo (the
  last 20 states, no redo) that the desktop does not share.
- **Agents**: their writes share the user's stack, one step per write,
  attributed in memory but saved under the surface's origin.
- **Node**: saves a version before answering each change; no undo verb
  on its control endpoint or MCP.

### What the inventory says

1. Five histories, one reachable by Ctrl+Z, and that one's routing is
   wrong inside the theme editor.
2. No history is bounded; none survives a restart except Helios's
   versions, which are a record, not an undo stack.
3. Nothing lets a host push an entry it made, so structural edits,
   imports and agent requests cannot be undo steps today.
4. Attribution exists in memory (`Step.origin`) and is lost on save.
5. The place a step was made in is guessed, not recorded.

## Survey

What other applications do, as a user meets it. Behaviour, not code.

**darktable.** Two unrelated histories. The darkroom's *history stack*
is per image, stored in the library database and the XMP sidecar: each
module change appends an item (a module, its parameters, its mask
forms, its blend), the stack has an *end* (`history_end`), moved by a click in the history
module. An item added while the end is not at the top discards the
items above it (no branch). Consecutive changes of the same module
instance are merged into one item. *Compress history stack* rewrites
the stack to one item per module instance, dropping everything above
the end. The stack survives restarts (it is the edit). Separately, a
global undo list (`undo.c`, typed: develop, lighttable ratings, colour
labels, tags, metadata, geotags) answers Ctrl+Z and Ctrl+Y: in the
darkroom each entry is a copy of the whole stack before a change, so an
undo restores it; it is grouped per action, in memory only, cleared
when another image is opened in the darkroom and lost on quit. Masks'
structural edits ride on the history items (the forms list is copied
into each item), so they undo with the stack. *Snapshots* are rendered images to compare
against, not states to return to. Styles copy part of a stack to other
images.

**Lightroom Classic.** A per-photo *History* panel in Develop: every
adjustment is a named row ("Exposure +0.30"), a click previews or jumps,
a new edit after a jump drops the rows above (no branch); the history
is stored in the catalog with the photo and survives restarts; "Clear"
empties it, keeping the current settings. *Snapshots* are named states
of the develop settings, kept per photo, that history clearing does not
touch. Ctrl+Z is an application-wide undo across modules (ratings,
flags, keywords, collections and develop steps), in memory, lost on
quit; in Develop it walks the same steps the History panel shows.
Batch operations (sync settings to 200 photos) are one undo step.

**Photoshop.** One history per open document, in memory, lost on close:
the History panel lists named states (default 50, configurable), a
click jumps, a new action after a jump drops the later states unless
*non-linear history* is on, in which case they stay listed and
reachable. *Snapshots* (named full states) sit at the top of the panel
and are not dropped by the cap. Ctrl+Z is a toggle by default in older
versions, multi-step since 2019 (Ctrl+Alt+Z / Ctrl+Shift+Z). Panel
settings, preferences and selection changes are or are not steps by
type (selections are). The *history brush* paints from an older state.
Actions (scripts) can be wrapped as one history state.

**Blender.** One global undo stack per file session (default 32 steps,
memory-limited), named by operator ("Translate", "Add Cube"), with
*Edit > Undo History* listing them for a jump. Edit mode has its own
undo system (mesh snapshots) while in edit mode; entering or leaving a
mode is itself a step, and leaving flattens the mode's steps into the
global stack. Undo is not saved with the file. An operator's *redo
panel* (adjust last operation) re-runs the last step with new
arguments: a step can be amended while it is the last one. View
changes (orbit, zoom) and most UI changes are not steps; selection is.

**VS Code.** Undo is per text model (per file), not global: each editor
keeps its stack, typing is grouped by word boundaries and pauses, a
cursor move closes a group. A *workspace edit* (rename symbol,
refactoring, search and replace across files) is one undo element that
spans several models; undoing it from one file asks whether to undo it
in all, and refuses when another model moved on since. File operations
(create, delete, rename) join that undo. Text inputs (search box,
settings fields) have their own small undo. Closing a file drops its
stack unless *files.restoreUndoStack* keeps it with hot exit; it is
never written on disk otherwise.

**Krita.** A per-document undo stack (default 30, configurable) with a
docker listing named steps; a *cumulative undo* option merges older
quick steps (strokes within a time window) so the cap holds more; it
lives in memory and is lost on close. Snapshots (a docker) are full
document copies to switch between.

**GIMP.** Per-image undo with a memory cap (levels and size), an *Undo
History* dialog with thumbnails, groups ("Paste", a plug-in run is one
group), lost on close; *Edit > Undo History* jumps. A new action after
undo drops the redo list.

### What carries over

- A **per-document history that is the edit itself** (darktable,
  Lightroom) survives restarts; an **application undo** of the session
  (Lightroom, darktable's lighttable, Blender) does not. Users accept
  that split; they do not accept losing the image's history.
- **Jump, named rows, one row per gesture or per batch**: every editor
  that has a panel does this; the batch-as-one is universal.
- **Branching** is rare (Photoshop's non-linear history, off by
  default); snapshots answer the same need in a way people understand.
- **Compress** exists only where the history is the persisted edit
  (darktable); it is mostly a size and noise answer.
- **Focus picks the stack** (VS Code, browsers, every toolkit): a text
  field's Ctrl+Z is the field's while it has focus.
- **A multi-document edit is one step in each document, undone
  together or refused** (VS Code's workspace edit).

## The questions, answered

### 1. What is undoable

| Change | Undo step? | Why |
|---|---|---|
| A value the user or an agent sets (a parameter, a token, a label) | yes | the core of undo everywhere |
| A structural edit (add, remove, move, rewire a node or member) | yes, one per edit, with the values it moved | darktable carries masks in its history; without it a destructive action needs a confirmation dialog |
| Marks on a collection (rating, reject, pick, colour label, tag) | yes, in the collection's scope, for the session | Lightroom and darktable both undo them; mistakes in culling are frequent and fast |
| Accepting, correcting or declining a proposal (a stack) | yes, one step | it is a decision with an exact inverse (dissolve, reopen) |
| A workspace edit, reset or parts import | yes, one step each | already so on the workspace branch |
| A long job's result (an import, a batch) | the job is cancelled, not undone; its library effect may be one step if it has an exact inverse that deletes nothing the user did not create | files written stay; the inverse of a copy-import removes the entries, never the files, without asking |
| Files written (export, a template saved) | no | cannot be unwritten safely; cancellable while running |
| Selection, view (zoom, pan, scroll), focus, panel layout | no | every surveyed application except Photoshop (selection) and Blender (selection) leaves them out; they are view state, restored by the context stack |
| Context stack transitions | no | ADR 0012 |
| Settings and theme | yes, in their own local scope | they are edited in their own window |

### 2. Scopes, and which Ctrl+Z wins

**Recommendation:** one history per document (an image's edit, a
collection, a workspace), plus local scopes for components that edit
something of their own (theme editor, settings window, code editor,
text fields). Focus picks: text input, then a focused local scope, then
the newest step among the scopes active in the window. Steps of a
document not shown are not reachable by Ctrl+Z.

Why not one global stack (Blender, Lightroom's Ctrl+Z): in a photo
editor, undo after switching images would edit the image left behind,
out of sight; Lightroom does it and switches photo, which surprises.
Why not strictly per focused panel (pure VS Code): rating a picture in
the grid, then pressing Ctrl+Z with the viewport focused, should undo
the rating; the newest step among the active scopes gives that,
without making the user aim the focus.

### 3. Grouping and naming

- A gesture is one step; Escape cancels it and leaves nothing.
- Repeats merge: same merge key (the target path, or the component's
  own key), less than a second apart, focus unchanged.
- An explicit group: an agent's request (one control or MCP request,
  or several under one `group` id), an import, a parts import, a reset.
- A label is a message id and its arguments, rendered in the
  interface's language when shown: "Exposure +0.30 EV", "Take out
  ‘Face’", "Import 120 images", "Agent ‘retouch’: 6 changes".
- The origin is part of the step and is shown.

### 4. Redo, branching, snapshots

- Linear redo: a new step after an undo drops the redo list.
- No branches in the interface. An owner with an append-only store
  (Helios's versions) keeps every state anyway; the history panel can
  show the dropped ones as "replaced" rows, reachable by jump.
- Snapshots: a named state pinned in a scope, outside the cap, used by
  the panel's compare ("before" = a snapshot or a step).

### 5. Persistence, limits, compress

- The scope says. Documents the owner stores durably (an image's edit,
  a workspace) rebuild their undo and redo lists on reopen; a
  collection's marks may (from an audit trail) or may not; local scopes
  never do.
- After `kill -9`, a durable scope has lost at most the step being
  made (Helios already guarantees it for the image's edit).
- In memory: a cap per scope (500 by default), the oldest dropped from
  memory first; the owner may page them back in for the panel.
- Compress belongs to the owner: what it rewrites is its storage.

### 6. A history panel, and its MCP twin

- `HistoryView`: the active scope's steps, newest at the top, each with
  its label, origin badge, time and place; groups folded; "in
  progress" rows hidden; a click jumps (itself a step, so it can be
  undone); a hover asks the owner for a preview; a snapshot button; a
  compare toggle using the viewport's before and after. A scope picker
  when more than one is active.
- Agents: `history.list {scope, from, limit}`, `history.undo {scope,
  group?, if_rev, dry_run}`, `history.redo`, `history.jump {scope,
  step, if_rev, dry_run}`, `history.snapshot {scope, name}`, all
  answered with the same refusals as the interface.

### 7. Across processes

- The contract never assumes the owner is in the interface's process
  (`Answer::Later`, like dispatch).
- A step made by another process is not in this window's Ctrl+Z; if the
  owner sees it (a shared store), the panel shows it as another
  process's, and an undo that would cross it is refused with that
  reason.

### 8. The contract

Sketch (names to settle when built):

```rust
// egui-pop-api::history
pub trait History {
    fn scopes(&self) -> Vec<ScopeInfo>;              // id, title, focus targets, durable, cap
    fn steps(&self, scope: &ScopeId, range: Range) -> Page<StepInfo>;
    fn rev(&self, scope: &ScopeId) -> u64;
    fn undo(&mut self, r: Request, origin: &Origin) -> Answer;   // Request: scope, group?, if_rev, dry_run
    fn redo(&mut self, r: Request, origin: &Origin) -> Answer;
    fn jump(&mut self, r: Request, to: StepId, origin: &Origin) -> Answer;
    fn group(&mut self, origin: &Origin, label: Label) -> GroupToken;
    fn close(&mut self, t: GroupToken);
    fn snapshot(&mut self, scope: &ScopeId, name: &str) -> Answer;
    fn gesture_open(&self) -> Option<Label>;          // vetoes undo while set
}

pub struct StepInfo {
    pub id: StepId, pub scope: ScopeId, pub label: Label, pub origin: Origin,
    pub group: Option<GroupId>, pub place: Vec<FrameRef>, pub at: u64,
    pub in_progress: bool, pub replaced: bool,
}

// a component's own history, in the interface
pub trait LocalScope { fn undo(&mut self) -> Option<Label>; fn redo(&mut self) -> Option<Label>;
                         fn labels(&self) -> (Option<Label>, Option<Label>); }
```

`Answer` is `Done(Change)`, `Refused { reason, by }` or `Later(Pending)`.
The owner publishes `history.changed {scope, rev, head}` and
`history.refused` on the journal. `StoreHistory` (the default owner)
wraps the value store in one scope, capped, and vetoes undo during the
user's own gesture.

## Flows

**H** is the owner, **UI** egui-pop's projection.

### A slider drag

```
user : presses on the exposure slider     → H: gesture opens (label "Exposure")
user : drags                              → values written under the gesture
user : Ctrl+Z meanwhile                   → edit.undo vetoed: "Exposure is in progress"
user : Escape                             → H: gesture cancelled, old value back, no step
user : drags again, releases              → H: one step "Exposure +0.30 EV", history.changed
UI   : Edit menu "Undo Exposure +0.30 EV"; panel row added
```

### Taking a member out of a mask group

```
user : selects the group "Face", presses Delete
UI   : no confirmation any more (it can be undone)
H    : one step "Take out ‘Face’": the nodes, edges and the opacities and
       inversions on the ports it frees, kept with the step
UI   : toast "‘Face’ taken out", with Undo
user : Ctrl+Z → H restores nodes, edges and values together
```

### An agent's batch undone as one

```
agent: history.group {label "Retouch skin"} → token g7
agent: six parameter writes under g7, then close g7
H    : one step "Agent ‘retouch’: Retouch skin (6 changes)", origin agent
user : touches Exposure meanwhile → its own step, after g7
user : history.undo {group g7} from the panel's row
H    : g7's paths untouched since? Exposure is not one of them → undone;
       otherwise refused: "Exposure changed after it", with "Undo both"
```

### A workspace import

```
user : Add from a workspace…, picks parts, sides for conflicts, confirms
H    : group "Add parts from ‘Wedding’": the merged state, one step
user : Ctrl+Z (focus in the viewport) → the newest active step is the
       import → the workspace goes back to its state before, as one step
```

### Undo after reopening the app

```
user : edits, quits (or the process is killed)
start: H rebuilds the image's undo and redo lists from its store
user : Ctrl+Z → the last step before the quit is undone
```

### Ctrl+Z in a text field versus the viewport

```
user : renames a member (F2), types "Fac", Ctrl+Z → the field's undo:
       "Fa" (egui's undoer); the history is untouched
user : Enter → one step "Rename to ‘Fa’"
user : focus on the viewport, Ctrl+Z → the rename is undone
user : in the theme editor, Ctrl+Z → the draft's last token edit
```

### Undo in another place of the context stack

```
user : in sub-workspace s1, changes a parameter → step with place root›s1
user : leaves to root, Ctrl+Z
H    : undoes it (the image's scope is still active)
UI   : toast "Undone in s1" with "Go there", read from the step's place
```

## Open questions

1. A collection's marks: undo for the session only (Lightroom), or
   rebuilt from the audit trail after a restart?
2. The newest step among active scopes, or always the focused panel's
   scope with a fallback? The first is recommended.
3. Should the panel show the dropped redo states ("replaced") of a
   durable owner, or hide them behind a filter?
4. Default cap per scope: 500 entries, or by memory?
5. The code editor: register its `History` as a local scope (Ctrl+Z
   when focused, as today) or let a host take its entries into a
   document scope?

## Steps

1. `egui-pop-model::history` (pure types, merge rules) and the
   `History` contract with `StoreHistory`; `Host::undo` and
   `Host::redo` replaced; the veto during the user's own gesture; the
   cap. Tests: the current `tests/undo.rs`, plus merge and veto.
2. Focus routing: local scopes (theme editor, settings window, code
   editor) and the active-scopes rule; the theme editor's Ctrl+Z.
3. Steps carry their place; the context stack's stamps go.
4. `HistoryPlugin`: menu labels, `HistoryView`, the toast, the
   `history.*` commands, the debug collector.
5. Coalescing of repeats in the value store (bound sliders, colour
   buttons, text fields).
