# 0023: Every command goes through the host's dispatch; history walks are held to the writers

Date: 2026-11-03. Status: accepted (2026-11-03, by the lead
maintainer's decision under GOVERNANCE, without the comment period).

## Context

ADR 0010 makes `Host::dispatch` the one place a command is run, so a
host can guard every command there, refusing agents for instance. 29
commands bypassed it:

- undo and redo (`edit.undo`, `edit.redo`);
- the agents' `history.*`;
- the context stack's `nav.*` and `context.*`;
- the clipboard's `edit.cut/copy/paste/pasteSpecial` and `clipboard.*`.

Their handlers call the host themselves (`Host::history`,
`Host::contexts`, `Host::clipboard`), and a dispatch holds the host, so
they ran in the interface without being dispatched. A guard in the host
did not see them. Undoing replayed values as the framework, so an
agent's undo took back what its own write would have been refused (ADR
0019).

## Decision

- **Every command is dispatched.** For one whose handler calls the host
  (`Dispatch::deferred()` says so), the dispatch gets a stand-in
  in-process handler. `run_local` approves the command and answers
  `null` at once; once the dispatch returns and the host is back, the
  interface runs the real handler, and the command's result is the
  handler's.
  - A host refuses such a command as it refuses any other: by answering
    an error without calling `run_local`.
  - A host that answers on its own without calling `run_local` gives the
    command's result itself.
  - Running the handler inside the dispatch would not be sound: the host
    it calls is held there, and re-entering it would deadlock or fail.
    Approving first and running after keeps one dispatch per command and
    leaves the host free.
- **History walks are held to the writers.** Undo, redo, jump, restoring
  a snapshot, undoing one step out of order and walking to a replaced
  line all write by the walker's writer (`Origin::writer`, ADR 0019).
  This covers `edit.undo`, `edit.redo` and `history.*` with the store's
  owner. Every path the walk changes is checked first, all or nothing.
  The user's writer, the framework, may write every path, so nothing
  changes for the user.

## Consequences

- A host that refuses agents in `dispatch` refuses them everywhere.
- A host that wrapped `run_local` to time or journal it sees `null` at
  once for the 29 deferred commands. It reads `Dispatch::deferred`.
- An agent's undo of a value it may not write is refused (`not_writer`).
  An application names the agent among the value's writers to let it
  undo there.
- Collector actions, code calling a step or `AppHandle::undo` directly,
  and a text field's own undo inside egui are not commands and are not
  dispatched. The `debug.call` step (ADR 0018) and the steps' hooks cover
  them.
