# 0024: The debug bridge has a mode; commands say when they only read

Date: 2026-11-03. Status: accepted (2026-11-03, by the lead
maintainer's decision under GOVERNANCE, without the comment period).

## Context

ADRs 0018 to 0023 hold the bridge's callers to the `agent` flags, their
origin and the values' writers. The bridge itself was always fully on.
An application could not turn it off, or make it read-only, for a kiosk,
a release build or an agent that should only look.

`commands.run` always said it mutates, so an agent could not tell a
command that reads from one that writes, and nothing could allow the
first kind alone.

## Decision

- **The bridge has a mode,** set by `AppBuilder::debug_bridge(BridgeMode)`.
  It holds anyone but the user:
  - `On` (the default): every action and command offered to agents;
  - `ReadOnly`: only calls that change nothing, meaning actions with
    `mutates: false` and `commands.run` of a read-only command;
  - `Off`: no call at all.
- **The user's calls are never held by the mode** (the devtools panel,
  `debug_call_as` with `Origin::User`). A refusal goes on the journal as
  `debug.call.refused`, as for an action not offered (ADR 0018).
  Snapshots stay readable by code in the process.
- **Commands say when they only read.** `CommandSpec::read_only` marks
  them, and egui-pop marks its own readers.
  - `commands.run` reports `mutates` from that flag, through a per-call
    `Collector::mutates`, and a read-only bridge runs only those
    commands.
  - The flag is the command author's word. Nothing checks that a
    read-only command does not write.
- **A failed read goes on the journal.** A call that only reads is
  published as `debug.call` when it fails or is vetoed, and not when it
  succeeds. So a refused `commands.run` is on the journal exactly once,
  whatever its command.

## Consequences

- A kiosk, or a release build, turns the bridge off or makes it
  read-only in one line. Its devtools panel keeps working for the user.
- An application's own reading commands add `.read_only()` so that a
  read-only bridge can run them.
- Still open, and documented:
  - redacted snapshots;
  - a rate limit;
  - a blocking wait;
  - in-process code calling as the user.
