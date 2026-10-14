# 0012: The context stack: owned by the kernel, projected by the interface

Date: 2026-10-13. Status: accepted.

The analysis behind it (three architectures compared, the main flows
step by step) is in the [appendix](0012-context-stack-analysis.md).

## Context

Applications need a global notion of where the user is and how the
application behaves right now: **places** (drill-downs: a group, a
composite, a mask sub-tree, a sub-workspace; leaving one restores the
parent's projection exactly) and **modes** (global states: an
orientation, compare, read-only, "an agent is driving"). Together they
are a small global state machine, and the session state to restore on
reopen. egui-pop had pieces (`WhenContext` keys set by hand, flat
arrangements, the host contract, steps), none stacked. The stack is
held by the application's kernel, reachable by contract from the
interface, and close to the bus.

## Decision

**A typed contract, implemented by an owner that publishes.** The owner
implements `Contexts` (in `egui-pop-api`, reached through
`Host::contexts()`): the frames, the state, `request`, `hold`,
`release`. egui-pop's default owner is `LocalContexts`. The machine is
pure and shared: `egui-pop-model::context` has the frames (`FrameSpec`:
place or mode, a mode's exclusivity group, pushed or set, transient,
guards), the state (place path, modes, `rev`), `apply(state, frames,
holds, envelope) -> Result<Transition, Refusal>` and `restore(saved,
frames, valid) -> (State, dropped)`, and the `context` and `view`
documents. The bus is not the truth (an event-sourced fold has no place
for guards or refusals, and the journal is a ring in memory): the
owner's events (`context.changed` carrying the whole new state and its
revision, `context.left`, `context.entered`, `context.refused`,
`context.restored`, `context.held`, `context.released`) are a typed
sub-protocol of the bus, and every reader is a trivial fold of them.

**The interface reads and asks, never writes.** A transition is the
step `context.request` (ADR 0011): before-hooks hold the interface's
own guards (veto or redirect), the default asks the owner. The
projection changes only when the owner's revision moves: nothing is
applied optimistically, so a refusal changes nothing on screen.

**The projection** keeps, per projection key (the place path and the
active modes), the workbench's layout, the focused panel and the values
components keep (scroll offsets), and sets the context keys `place`,
`placeDepth`, `modePushed`, `mode.<group>` and the frames' own keys.
A frame's key bindings use its `when` clause (`Ctx::frame_keybinding`).
The breadcrumb, the mode chips, the commands `nav.back`, `nav.up`,
`nav.enter` and `context.*` (for agents), and a `contexts` collector
come with `ContextsPlugin`.

**Saving never waits for the exit.** The owner writes the stack inside
each transition, before it answers or publishes. A debounced writer in
`egui-pop-core::persist` (about a second) writes the view state, the
layout, the settings and the keymap; `app.exit` only writes what is
still waiting, and a vetoed exit writes nothing more.

The owner's answers to the open questions:

1. An undo of a change made out of sight undoes it and says where, in
   a toast with "Go there". Transitions are not undo steps.
2. A layout change made in a mode belongs to the mode and is reverted
   when it is left.
3. Modes are global to the stack, not scoped to a place.
4. Pushed modes come back after a restart, unless declared transient.
5. A held request is refused with the hold's reason (not queued).
6. All windows show the same stack; another place means another
   workspace.
7. Each frame has its own keymap layer, through `WhenContext`.

**Survey.** statig (MIT, hierarchical, states known at compile time),
rust-fsm (MIT, a DSL for flat machines), sm (MIT, typestate checked at
compile time) and finny all build machines whose states are types
fixed at compile time; frames here are declared by plugins at run time,
persisted, guarded and held, and the machine is a few hundred lines.
For egui, egui_router (MIT) routes pages with a history but has no
modes, guards or persistence, and no breadcrumb crate exists. **Own**
both the machine and the widgets.

## Consequences

- An application gets drill-downs, modes and session restore from one
  mechanism; a simple one writes nothing of its own (`LocalContexts`).
- Agents reach the stack through the same request path as the user,
  with typed refusals, `if_rev` and `dry_run`.
- A remote owner costs a round trip per transition (`Answer::Later`).
- Plugins must not set the stack's context keys by hand; they declare
  frames and read the keys.
- A crash loses at most the last second of view state; the stack loses
  nothing.
