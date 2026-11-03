# 0019: Collector actions and value writes act as their caller

Date: 2026-11-03. Status: accepted (2026-11-03, by the lead
maintainer's decision under GOVERNANCE, without the comment period).

## Context

ADR 0018 settled who may call a collector action, but the action
itself did not know who called it. `commands.run` ran every command as
`debug-bridge`, the user's call from devtools included. `store.set` and
the `value.set`, `value.reset`, `value.undo` and `value.redo` commands
wrote as the framework, which may write any path (ADR 0016). An agent
could therefore turn on a setting that only its owner writes, through
`value.set`, or through `store.set` once offered.

## Decision

- **An action gets its caller.** `Collector::call` takes the call's
  origin. `FnCollector::action_as` gives it to the action, and
  `FnCollector::action` stays for actions that do not need it.
  `commands.run` runs the command as its caller. The user, from
  devtools, may run a command kept from agents; anyone else may not.
- **A write is made by its caller's writer** (`Origin::writer`). The
  user writes through the framework, as the settings form does. Anyone
  else writes as itself (`agent:debug-bridge`, `script:<name>`...). The
  value's declared writers then apply, under the store's writer check:
  refused by default, warned about under `WriterCheck::Warn`. This holds
  for `store.set` and for the `value.set` and `value.reset` commands.
- **An undo or a redo by a writer is held to the same check**
  (`Store::undo_by`, `Store::redo_by`): all or nothing, every path the
  step changed. `store.undo`, `value.undo` and `value.redo` go through
  it. Undo through the history (`edit.undo`, `history.*`) is unchanged.
- To let an agent write a value, its owner names the agent among the
  value's writers (`Descriptor::writers`, `Origin::writer()` as the
  id), or lets anyone write it.

## Consequences

- An agent that set values it does not own is refused. The application
  names it among those values' writers, or acts as the user.
- The rest of the plan uses the caller: `dialogs.answer` and
  `files.answer` answer as their caller and can check the target
  command's flag for it; the `contexts` actions request as their
  caller; a command's handler can carry its origin into the commands
  it starts. These still act as `debug-bridge` today.
