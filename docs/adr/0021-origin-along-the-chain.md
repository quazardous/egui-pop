# 0021: The origin goes along the chain of commands; deep links are remote

Date: 2026-11-03. Status: accepted (2026-11-03, by the lead
maintainer's decision under GOVERNANCE, without the comment period).

## Context

ADRs 0018 to 0020 hold the debug bridge's own calls to the `agent` flag
and to the caller's origin. Past the first command, the origin was
lost. A handler that started another command with `AppHandle::run` ran
it as the user, so an agent's offered command could start one kept from
agents. `App::run` and `AppHandle::run_as` took any origin without
checking the flag. Deep links from a launch, which any program of the
user's can send through the single-instance socket, ran the
application's links command as the user.

## Decision

- **A handler acts for its command's origin.** While a command's
  handler runs, or its host dispatch for one the host runs in process,
  `AppHandle::acting_origin` is that origin. Outside a command it is the
  user.
- **What a handler starts keeps that origin.** `AppHandle::run` runs
  for the acting origin.
- **No claim to the user.** While a command runs for anyone but the
  user, `AppHandle::run_as` refuses `Origin::User`. While one runs for
  an agent, it refuses anything but an agent's origin. The refusal is on
  the journal (`command.refused`, from the acting origin) and in the
  log. The explicit escape is to ask the user: a dialog, whose answer
  comes from the user outside the handler and runs its command as the
  user. A plain `run_as(Origin::User)` would let any agent-run handler
  claim the user, so there is no such method.
- **The `agent` flag holds for every run.** `App::run`, and so
  everything queued by `AppHandle::run_as`, refuses a command not
  offered to agents for an agent origin (`command.refused`).
- **Deep links are remote.** A link that comes with a launch runs the
  links command for `Origin::Remote("deeplink")`
  (`instance::deeplink_origin`), with the origin in `deeplink.open`'s
  arguments. The chain and writer rules above apply to it as to any
  origin but the user. A link of the application's scheme clicked
  inside the application runs for the click's origin.

## Consequences

- An application whose handler ran other commands as the user while
  serving an agent, a script or a deep link gets `command.refused`. It
  asks the user, or runs them with the acting origin.
- Code outside a handler still names any origin: a plugin's own code, a
  step's hook, a timer. Work a handler leaves for later (a thread, a
  host's `Reply::Later`) does not inherit the origin, and names it
  itself. The origin stays a label from code in the process, not a
  credential.
