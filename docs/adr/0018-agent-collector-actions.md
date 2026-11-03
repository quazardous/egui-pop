# 0018: Collector actions say whether agents may call them, through one step

Date: 2026-11-03. Status: accepted (2026-11-03, by the lead
maintainer's decision under GOVERNANCE, without the comment period).

## Context

The debug bridge runs a command for an agent only when the command is
offered (`CommandSpec::no_agent`) and applies where it is called, and the
host sees its origin (ADR 0010). Collector actions had none of this.
`dialogs.answer`, `files.answer`, `plugins.set_enabled`, `store.set`, the
`jobs` and `contexts` actions and the others ran for anyone holding the
bridge, without a step a plugin could veto (ADR 0011). Only the devtools
panel went through one (`devtools.call`), and only for its own calls.
An agent could disconnect the plugin that guards it, or turn on a
setting only its owner writes.

## Decision

- **Every collector action says whether agents may call it**
  (`ActionSpec::agent`). By default it may when the action only reads,
  and may not when it writes. A collector offers a writing action with
  `FnCollector::agent`; egui-pop offers one, `commands.run`, which
  checks the command's own flag.
- **The bridge's calls are the agent's** (`Origin::Agent("debug-bridge")`).
  Only the user (`debug_call_as(.., Origin::User)`) may call an action
  not offered to agents. Anyone else's call is refused before it runs,
  in the bridge's log and on the journal (`debug.call.refused`).
- **One step for every call**: `debug.call` (`collector`, `action`,
  `args`, `origin`). A plugin may veto it for every caller, and a
  redirection cannot change who calls. The devtools panel is the user's.
  It keeps its `devtools.call` step, then calls through `debug.call`, so
  a kiosk's veto covers the panel and agents alike. A call that only
  reads is journaled and traced only when vetoed, so polling fills
  neither.
- **The application may offer a writing action** to agents it trusts
  (`AppBuilder::offer_to_agents`), such as a test harness or its own
  automation. A name that matches no action is reported
  (`agent-offer-unknown`, ADR 0003).

## Consequences

- An agent that wrote through collector actions moves to commands
  (`value.set`, `job.cancel`, `context.hold`...) or is offered the
  action by the application. Code acting as the person calls
  `debug_call_as` with `Origin::User`. This breaks the debug bridge's
  behaviour; the changelog says how to move.
- Collector actions still do not know who called them: `commands.run`
  runs as `debug-bridge`, and `store.set` writes as the framework,
  whoever calls. Carrying the caller into them, the value's writers, and
  the dialogs' target command are later decisions.
- The origin is still a label from code in the process, not a
  credential: a plugin can call as the user. A transport that exposes
  the bridge controls who reaches it.
