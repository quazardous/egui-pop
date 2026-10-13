# 0012: The context stack: owned by the kernel, projected by the interface

Date: 2026-10-13. Status: **proposed** (draft for discussion; longer than
an ADR usually is, because it carries the analysis behind the choice).
Once the owner settles the open questions, it is cut down to the decision
and its consequences.

## Context

Applications built on egui-pop need a global notion of "where the user
is and how the application behaves right now":

- **places**: drill-downs with their own projection. A sub-workspace that
  holds part of a larger one, a group, a composite, a mask sub-tree, a
  folder. Entering one switches the whole projection; leaving it restores
  the parent's exactly as it was.
- **modes**: global states that change how everything behaves without
  moving anywhere. A work orientation (culling, retouch, review), compare,
  presentation, read-only, soft proof, "an agent is driving".

Together they form a **context stack**: a small global state machine. It
is also, in the end, the session state to restore on reopen, so that the
application comes back the same.

egui-pop already has pieces of it, none stacked:

- `WhenContext` (`egui-pop-model::when`): flat string keys at a focus
  depth, read by keymaps, commands, menus and `checked` clauses. Global
  keys are set imperatively (`AppHandle::set_context`); a change publishes
  nothing and is never persisted.
- arrangements (`ArrangementsPlugin`): named layouts in the store, with a
  flat current name; workspaces choose their layout document through
  `Host::layout_document()` and their keymap layer (`Layer::Workspace`).
- the host contract (ADR 0010): `dispatch` with a deferred `Reply`, a
  journal with `seq`, `source` and `cause`, a store, a waker, undo and
  redo answered by the host.
- steps (ADR 0011): observe, veto, redirect, replace, extend, run
  synchronously in the frame.
- the gallery's `Nav` service: a current page in a service, projected as
  one context key. It is the pattern already in use, without history.
- `docs/widgets.md` lists a breadcrumb as planned, not built.

The owner's brief: the stack is **held by the application kernel** but
**reachable by contract from the presentation layer**, and it is **close
to the bus**. The question this record answers is what "close to the bus"
means: is the stack a projection (fold) of bus events, a peer service
publishing on the bus, or a typed sub-protocol of the bus?

Constraints that come from what exists:

1. **The bus is not a command channel.** In the applications egui-pop
   targets (Helios is the first), every piece of state has one owner;
   commands go to the owner and are answered; events are past-tense facts
   the owner publishes after a change. The journal is a recent ring (1024
   entries by default), in memory, not durable.
2. **The interface kernel stays on the interface thread.** `AppHandle` is
   `Rc` with `RefCell`s, hooks are not `Send`, steps are synchronous.
   The application kernel may be elsewhere: another thread, another
   process, another machine (`Reply::Later`, `Resolver`, `Waker`).
3. **Several surfaces read the same state**: the main window, auxiliary
   viewports (detached panels), headless runs (a node, a script, a test)
   and agents (the debug bridge today, MCP in Helios).
4. **Layers** (ADR 0001): what has no egui goes to `egui-pop-model`; a
   lower layer exposes a trait that an upper one implements.
5. **Survival.** A `kill -9` must not lose more than the last moment.
   Today egui-pop writes settings, keymap and layout only at exit; that
   is a gap this record has to close for the stack, and it touches the
   layout too.

## 1. What the context stack is

A **frame** is one entry on the stack. Frames are declared by plugins or
by the host in a registry, and instantiated with an optional payload.

| Field | Meaning |
|---|---|
| `id` | `FrameId`, a typed id (ADR 0002), e.g. `helios.sub-workspace`, `helios.orientation.culling` |
| `kind` | `Place` or `Mode` |
| `group` | for a mode: its exclusivity group (`orientation`); none for a place |
| `life` | for a mode: `Pushed` (temporary, popped by Escape or `nav.back`) or `Set` (sticky until changed) |
| `transient` | dropped on restore (an agent driving, a modal tool mode, a confirmation) |
| `title`, `icon` | for the breadcrumb and the mode chips (string keys, translated) |
| `projection` | a place: an arrangement (a layout document), optionally a toolbox and a keymap layer; a mode: an optional arrangement delta (panels shown, hidden) |
| `keys` | when-context keys the frame contributes |
| `payload` | opaque, owned by the application (Helios: a sub-workspace id) |

The **state** is:

- the place path: `root › place › place …` (a list, the breadcrumb);
- the active modes: at most one per group, each with its life;
  modes of different groups combine;
- a revision `rev: u64`, incremented by every transition;
- the holds currently active (see guards).

Modes are attached to the stack as a whole, not to a place, by default:
"culling" stays on when entering a sub-workspace. A frame may declare
itself **place-scoped** (leaving the place leaves the mode); that is an
open question (Q3) for which the default matters.

**Transitions** are requests: `enter(place, payload)`, `leave()`,
`jump(depth)`, `set_mode(group, mode)`, `push_mode(mode)`, `pop_mode()`,
`clear_mode(group)`. Each carries its origin (user, agent, script,
device, remote), optionally the revision it was computed against
(`if_rev`, like the value store's `if_seq`), and a `dry_run` flag.

**Guards** decide whether a transition may happen:

- `can_enter` and `can_leave` on a frame, evaluated by the owner;
- **holds**: a short-lived lock that someone places on a group or on the
  whole stack ("a drag is in progress", "an export is writing", "an
  agent holds the image"). A transition that touches a held group is
  refused or deferred (Q5), with the hold's reason;
- the interface's own guards (unsaved typed input would be lost, a modal
  is open) are before-hooks on the interface step, see section 3.

The answer to a transition is one of:

- `Applied { rev }`;
- `Refused { reason, by }`: a guard or a hold said no; `Stale { rev }`
  when `if_rev` no longer matches;
- `Redirected { to, rev }`: a guard applied another transition instead
  (entering a deleted place lands on its parent; entering "review" on a
  read-only catalogue lands on "review, read-only").

**Session state** is the stack itself plus the view state each frame
had (section 4). **Transient frames** are part of the live stack, shown
like any other, but never written.

## 2. Its relation to the bus: three architectures

### A. A state service on the kernel, transitions published as events

The kernel holds a context service, the owner. Requests are commands to
it, answered at once or later. After each transition it publishes facts
(`context.changed`, `context.refused`) on the bus. Readers keep their
own copy by applying the events, or query the service.

### B. An event-sourced fold of bus messages, with snapshots

The state is `fold(log)`: transitions are written to the bus as events,
and every reader (the kernel included) computes the stack by folding
them from the last snapshot. Persistence is the log plus snapshots;
replay rebuilds any past state.

### C. A kernel state machine behind a typed host contract, the bus as transport

The kernel implements a typed contract (`Contexts`): read the state,
request a transition, get a typed answer. The machine itself (frames,
groups, guards, the pure transition function) is a shared library. The
bus only transports the contract's notifications to remote readers; in
process, the contract is called directly.

### Comparison

| Criterion | A: service + events | B: fold of the log | C: typed contract, bus as transport |
|---|---|---|---|
| **Ownership** | One owner, clear. Fits "commands go to the owner". | Nobody owns it: whoever publishes a transition event changes the state. Guards have no natural home: an event is a fact, it cannot be refused after it is published. A "request" event plus an "accepted" event brings back an owner, that is A. | One owner behind a trait, clear. |
| **Consistency across windows** | All windows of a process read the same events, same order. A window may lag one event behind. | Same, as long as every reader folds the same log. | Same as A in process; windows read the owner directly. |
| **Consistency across nodes** | Events relay between nodes like `job.*`; a mirror needs snapshot then subscribe from its `seq`. | Needs a total, durable, shared log across nodes: the bus offers one order per process, not across them. | Same as A: the contract's notifications are relayed; a remote mirror reads a snapshot then follows. |
| **Persistence** | The owner writes the state (small) on each transition. | The log must become durable and unbounded, or snapshotted carefully. The bus journal is a ring in memory: making it the truth changes what the bus is. | Same as A; the shared library defines the document and its schema. |
| **Replay** | Not needed: the state is small and written whole. A trace of transitions is in the journal for debugging. | Native, and its real strength: "how did we get here". But the stack's history has little value beyond the last state. | Same as A. |
| **Undo** | The owner decides what is undoable (see flows). | A tempting "undo = fold without the last event", but the log mixes transitions with everything else; undo needs its own model anyway. | Same as A. |
| **Survival after `kill -9`** | Write before publishing, in the transition: the last applied transition survives. | Requires the log write to be synchronous and durable before any reader acts; a ring in memory loses everything. | Same as A. |
| **Threading** | The owner is `Send` (a lock or its own thread); the interface receives events through its journal and the waker. | Every reader folds; the interface folds on its thread. Cheap but duplicated. | The interface calls the contract on its thread; a remote owner answers `Later`. |
| **UI latency** | In process: the answer and the event arrive in the same frame. Remote: one round trip, the interface shows "switching". | Same as A once the event is in; a local publish is immediate, but the guard problem makes local publishing unsafe. | Same as A. |
| **Headless testability** | Good: the owner is plain code with no window. | Good: a fold is a pure function. | Best: the pure transition function is unit-tested without an owner, the owner without a window, the projection with a fake contract. |
| **Agents and MCP** | Agents send requests to the owner (the same path as a key press), read the state, wait for `context.changed`. | Agents would publish events directly: no guard, no answer, no refusal. | Same as A, with typed requests, `dry_run` and `if_rev`. |
| **Complexity** | Low. | High: durable log, snapshots, compaction, schema of every past event forever. | Low to moderate: one trait, one library, one event family. |

### What "close to the bus" should mean

B is rejected as the source of truth. It contradicts the bus's own rules
(events are facts published by an owner, the bus carries no commands,
the journal is not durable), it has no place for guards or refusals, and
the property it buys (replaying the past) has little value for a state
this small. Its two good ideas are kept:

- **state-carrying events**: every `context.changed` carries the whole
  new state and its `rev`, not only the delta. A reader that missed
  events folds nothing: the last event is the state. A late reader takes
  a snapshot (`describe`) and subscribes from its `seq`, as the jobs view
  already does;
- **the journal as a trace**: the sequence of `context.*` events with
  their `cause` is how an agent or a test sees why the stack moved.

A and C are not rivals: C is the contract, A is how an owner implements
it. The recommendation is **C, implemented as A**: an owner-held state
machine, reached through a typed contract, whose notifications are a
typed sub-protocol of the bus (`context.*`). So, answering the question
asked:

- the context stack is a **peer service** (an owner), not a fold of the
  bus;
- its events form a **typed sub-protocol** of the bus;
- every **reader** (the projection, a window, a relay, an agent's
  mirror) is a fold of that sub-protocol, and the fold is trivial
  because each event carries the state.

## 3. The contract boundary

### What the presentation may do

| | Kernel state (owner) | View state (presentation) |
|---|---|---|
| Content | places, modes, holds, `rev`, payloads | focus, scroll, panel sizes and folds, the viewport's zoom and pan, which auxiliary windows are open and where |
| Read | yes: `state()`, and every `context.changed` in the journal | its own |
| Request | `request(transition)`: enter, leave, jump, set, push, pop; `hold` and `release` | n/a |
| Write | **never**: the presentation never sets a frame itself, never edits the stored stack | yes, keyed by frame path |

`AppHandle::set_context` stays for keys that are pure view state
(`windowFullscreen`, `galleryPage`). Keys that come from the stack
(`place`, `mode.<group>`) are written only by the projection, from the
owner's state; a plugin setting them directly is reported (ADR 0003).

### The flow of a request

1. The user presses `nav.back`, clicks a crumb or a mode chip, or a
   command runs `context.*`.
2. The interface runs the step `context.request` (ADR 0011). Its
   before-hooks hold the **interface's guards**: unsaved typed input, an
   open modal, a drag of a tab. They can veto (nothing is sent) or
   redirect (a different transition is sent).
3. The default of the step calls `Contexts::request` on the host, which
   answers `Reply::Done` (owner in process) or `Reply::Later` (remote).
   While it is pending, the crumb or chip shows "switching", like a
   running command.
4. The owner applies, refuses or redirects, writes, then publishes.
5. The projection applies **only** what arrives in `context.changed`.
   It never applies a transition optimistically: a refusal leaves the
   display unchanged with nothing to roll back; a redirect shows where
   the owner went; a transition requested by someone else (an agent,
   another window) is projected the same way.
6. A refusal is published (`context.refused`) and returned. The
   interface shows it where the request came from: a toast for a key
   press, the step's veto in the trace, the answer for an agent.

### What the projection does with a new state

In one pass, on the interface thread, from the `rev` it last applied:

1. save the view state of every frame being left (focus, scroll,
   sizes, open auxiliary windows), keyed by its frame path;
2. switch the arrangement for the new top place (layout document,
   keymap layer, toolbox), then apply each mode's arrangement delta;
3. rebuild the stack keys of `WhenContext`: `place` (the top place's
   id), `placeDepth`, `mode.<group>` for each active mode, and the
   frame's own `keys`;
4. restore the view state saved for the new top frame, if any;
5. redraw the breadcrumb and the mode chips; announce the change
   (AccessKit live region).

### Several windows and auxiliary viewports

There is one stack per application session (per workspace in Helios),
not one per window. All viewports of a process share the one
`WhenContext`, so one projection pass updates them all in the same
frame. An auxiliary window holding a panel the new arrangement does not
show is closed, and its place is kept in the left frame's view state so
re-entering reopens it. A per-window place (a second window showing the
parent while the first is in a sub-place) is not part of this proposal
(Q6).

## 4. Persistence and restore

| What | Who writes | When | Where |
|---|---|---|---|
| The stack (places, `Set` and `Pushed` modes, payloads, `rev`) | the owner | inside each transition, before the answer and the event | the host's choice; `LocalContexts`: a versioned `context` document (ADR 0008) |
| The view state per frame path | the projection | debounced (about one second after the last change), and on each transition for the frame being left | a versioned document next to the layout (`view`), one per workspace |
| Transient frames | nobody | never | the live stack only |
| Holds | nobody | never | live only; a restart clears them |

- **Write before publish.** A reader never sees a state that a crash
  would lose.
- **Restore.** At start the owner loads the document, then checks every
  frame against the registry: a place whose payload no longer exists (a
  deleted sub-workspace), a mode whose plugin is gone or disabled, a
  transient frame. The stack is cut at the first invalid place (its
  children go with it); an invalid mode is dropped alone. The result is
  published as `context.restored`, with what was dropped and why; the
  interface shows a toast. Restore never fails the start.
- **Scope.** One stack per workspace (the application decides what a
  workspace is; `LocalContexts` has one). The session (which workspaces
  and windows were open) is the application's, outside this record.
- **Prerequisite.** egui-pop's persistence writes only at exit. The view
  state, and the layout with it, need a debounced writer in
  `egui-pop-core::persist` (a dirty flag, a deadline, an atomic write
  from the `Pass` lifecycle). It is useful on its own: a crash today
  loses every layout change of the session.

## 5. Flows

Notation: **UI** is the interface thread (egui-pop), **K** the owner
(the application kernel, or `LocalContexts`), **J** the journal.

### Entering a sub-place

```
user: double-click a group, or `nav.enter`
UI  : step context.request {enter, frame: sub, payload: id, if_rev: 7}
UI  :   before-hooks: no unsaved input → Go
UI  : host.contexts().request(...)            → Reply::Done or Later
K   : can_enter(sub, id)? yes; no hold on places
K   : write stack rev 8
K   : publish context.changed {rev 8, state, entered:[sub], left:[], cause}
UI  : (same pass if Done, next pass if Later) projection:
UI  :   save view state of "root"; switch arrangement to sub's layout;
UI  :   keys place=sub, placeDepth=1; restore sub's view state; crumb
```

`nav.back` is the same with `leave`; the parent's view state is restored
from what step 1 of the projection saved.

### An agent switches the orientation while the user drags

```
user: starts dragging a slider (a gesture in the value store)
UI  : step context.request is not involved; the drag places a hold:
UI  :   host.contexts().hold {token: drag-3, groups: [*], reason: "drag"}
agent: context.set_mode {group: orientation, mode: review, origin: agent}
K   : hold drag-3 covers orientation → Refused {by: "user drag", reason}
K   : publish context.refused {request, reason, hold: drag-3}
agent: waits for context.released, then retries with if_rev
user: releases the slider → UI: release {token: drag-3}
K   : publish context.released {token: drag-3}
agent: retries → Applied {rev 9} → context.changed → projection
```

The user's drag is never interrupted and the agent gets a reason it can
act on. Whether the owner should queue the request and apply it at
release instead of refusing is Q5. An application whose kernel already
knows the gesture (Helios's value store is kernel-side) does not need
the interface to place this hold; a tab drag, which only egui knows,
does.

### Two windows

```
main window and a detached "Histogram" window, same process, same stack
user (in the detached window): Alt+Left → context.request {leave}
K   : applied, context.changed rev 10
UI  : one projection pass; both viewports read the same WhenContext;
UI  :   the parent's arrangement has no detached Histogram → its window
UI  :   closes, its geometry saved in the sub-place's view state
user: re-enters the sub-place → the detached window reopens where it was
```

### A headless run in "review"

```
script or agent, no window: context.set_mode {orientation: review}
K   : applied, written, context.changed on the bus
K   : the batch starts and places hold {token: batch-12, groups: [orientation]}
agent: context.set_mode {orientation: culling} → Refused {by: batch-12}
K   : the batch ends → release → context.released
```

The machine works without egui; nothing in this flow needs a projection.

### `kill -9` and reopen

```
user: enters sub (rev 8), sets culling (rev 9), pushes compare (rev 10)
K   : each transition wrote the stack before publishing
UI  : view state written one second after each change
kill -9
start: K loads the stack rev 10; registry check: all valid; no transients
K   : publish context.restored {rev 10, dropped: []}
UI  : projection applies rev 10, then the saved view state of sub
       → same places, same modes, same panels, same zoom
```

If the sub-place was deleted meanwhile, the stack is cut at the root,
the modes survive, and a toast says what was dropped.

### Undo across a transition

Transitions are **not undo steps**: navigation has its own inverse
(`nav.back`, the crumbs, the chips), and mixing them would make Ctrl+Z
walk the user around instead of undoing their work. But an undo step
made in another frame must not apply out of sight:

```
user: in sub, changes a parameter (undo step 41, stamped with frame path root›sub)
user: leaves to root (rev 11)
user: Ctrl+Z
host: step 41 was made in root›sub, the current path is root
      → default: undo it, and show a toast "Undone in Sub" with "Go there"
      (alternative, Q1: first enter root›sub, then undo)
```

Undo stamps the frame path on each step; the host answers `undo` as
today (ADR 0010).

## 6. Decision (proposed)

### Where each piece lives

| Crate | What |
|---|---|
| `egui-pop-model` | `FrameId`, `FrameSpec`, `ContextState`, `Transition`, `Answer`, `Hold`; the pure function `apply(state, registry, holds, request) -> Result<(new state, diff), Refusal>`; `restore(doc, registry) -> (state, notes)`; the `context` and `view` document schemas. No egui, no I/O, unit-tested. |
| `egui-pop-api` | the `Contexts` trait in `host` and `Host::contexts()`; `LocalContexts`, the default owner built on the model; the projection plugin; the `context.request` step; the `nav.*` and `context.*` commands; the `context` debug collector; the gallery page. |
| `egui-pop-core` | the debounced persistence writer; the breadcrumb and mode chip widgets (or `egui-pop-views`, if they need plugins). |
| the application | its own owner, implementing `Contexts`, when it has a kernel; its frame registry entries; its guards. |

### The minimal contract (pseudo-signatures)

```rust
// egui-pop-api::host
pub trait Contexts {
    fn registry(&self) -> &FrameRegistry;          // what frames exist
    fn state(&self) -> ContextState;               // a snapshot, with rev
    fn request(&mut self, r: Request, origin: &Origin) -> Reply; // Answer as JSON
    fn hold(&mut self, h: Hold) -> Result<HoldToken, String>;
    fn release(&mut self, t: HoldToken);
}

pub trait Host {
    // ... ADR 0010
    fn contexts(&mut self) -> Option<&mut dyn Contexts> { None } // None: LocalContexts
}

// egui-pop-model::context
pub enum Request {
    Enter { frame: FrameId, payload: Json }, Leave, Jump { depth: usize },
    SetMode { group: GroupId, mode: FrameId }, PushMode { mode: FrameId },
    PopMode, ClearMode { group: GroupId },
}
pub struct Envelope { pub request: Request, pub if_rev: Option<u64>, pub dry_run: bool }
pub enum Answer {
    Applied { rev: u64 }, Redirected { to: Request, rev: u64 },
    Refused { reason: String, by: String }, Stale { rev: u64 },
}
```

Events, published by the owner on the journal (names under `context.`,
data as JSON):

- `context.changed { rev, state, entered: [frame], left: [frame] }`;
- `context.refused { request, reason, by, rev }`;
- `context.restored { rev, state, dropped: [{frame, why}] }`;
- `context.held { token, groups, reason }`, `context.released { token }`.

`entered` and `left` let a plugin react to one frame without diffing;
there is no separate `context.entered` event, so a reader never sees a
half-applied transition.

### Migration

1. Model: types, the pure machine, the schemas, unit tests (groups,
   pushed versus set, guards, holds, `if_rev`, restore with fallback,
   transient frames).
2. Persistence: the debounced writer, used first by the layout.
3. API: `Contexts`, `LocalContexts`, the projection, the step, the
   commands, the collector; kittest tests (enter and leave restore the
   projection, a refusal changes nothing, two viewports agree, keys by
   context, round-trip of stack and view state).
4. Widgets and the gallery page: a three-level drill-down and two mode
   groups whose panels and keys react.
5. Arrangements: the current arrangement becomes the top place's (the
   flat `workbench.arrangement` setting stays as the root's).
6. Applications move their hand-made context keys onto frames.

### Consequences

- An application gets drill-downs, modes and session restore from one
  mechanism; a simple one writes nothing (`LocalContexts`).
- Agents reach the stack through the same request path as the user,
  with typed refusals; the debug bridge shows the state, the holds and
  the last transitions.
- The interface never decides a transition; it may only veto its own
  part before asking.
- Remote owners cost one round trip per transition, shown as pending.

### Risks

- **Two truths.** A plugin keeps setting `mode`-like keys by hand. The
  projection owns the stack keys and reports writes to them.
- **Holds left behind.** A hold whose holder crashed blocks a group.
  Holds carry an owner and die with it (a plugin's on disconnect, a
  remote's on lost link), and the collector lists them.
- **Arrangement deltas versus user edits.** A user moving a panel while
  in a mode: does it change the mode's delta or the base layout? (Q2.)
- **Keymap precedence.** Stack keys sit at depth 0; a focused component
  still wins (ADR 0007). Whether a frame's bindings need a layer of
  their own between `Workspace` and `User` is Q7.

## Survey

To do before code: state-machine and statechart crates (`statig`,
`rust-fsm`, `sm`) and egui navigation crates. The expected outcome is
"own": the value is in the contract, the guards and the projection, and
the machine is a few hundred lines; a statechart crate would bring
hierarchy the stack does not need and nothing for persistence or holds.

## Open questions for the owner

1. **Undo across frames**: undo out of sight with a "Go there" toast, or
   navigate first then undo? Are some transitions undoable (a mode set
   by mistake)?
2. **Arrangement deltas**: does a layout change made in a mode belong to
   the mode or to the base layout?
3. **Mode scope**: are modes global to the stack by default, or scoped
   to the place they were set in?
4. **Pushed modes on restore**: restored (the current plan) unless
   transient. Confirm that a pushed compare should come back after a
   restart.
5. **Holds**: refuse (the agent retries) or queue (the owner applies at
   release)? Queuing is friendlier, refusing is simpler and never
   surprises the user.
6. **Per-window places**: should a second window be able to sit at
   another place of the same stack?
7. **Frame keymaps**: does a frame's keymap need its own layer, or is
   `when: mode.orientation == 'culling'` at depth 0 enough?
