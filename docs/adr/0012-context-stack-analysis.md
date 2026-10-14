# 0012, appendix: the analysis behind the context stack

The analysis that led to [ADR 0012](0012-context-stack.md), kept for
the reasons it gives. Where it differs from the ADR, the ADR wins.

## Three architectures

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

## Flows

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
act on. The owner refuses rather than queues (decided). An application whose kernel already
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
      → undo it, and show a toast "Undone in Sub" with "Go there"
```

Undo stamps the frame path on each step; the host answers `undo` as
today (ADR 0010).
