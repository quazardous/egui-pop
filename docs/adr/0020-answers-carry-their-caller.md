# 0020: Answers carry their caller, and a dialog keeps its command's flag

Date: 2026-11-03. Status: accepted (2026-11-03, by the lead
maintainer's decision under GOVERNANCE, without the comment period).

## Context

ADR 0018 lets an application offer `dialogs.answer` and `files.answer`
to agents. Once offered, an agent's acceptance ran the dialog's command
without checking its `agent` flag. A command kept from agents
(`no_agent`) could therefore run because an agent accepted the dialog
that confirms it. File choices reached their consumer without saying
who made them. ADR 0019 gave every action its caller, but these two
still answered as a fixed `debug-bridge`.

## Decision

- `dialogs.answer`, `files.answer` and the `contexts` actions act as
  their caller (`FnCollector::action_as`). A dialog accepted through the
  bridge runs its command with the caller's origin, and a context hold
  is made in the caller's name.
- **A dialog keeps its command's flag.** An answer that would run a
  command not offered to agents is refused to anyone but the user,
  even when the application offers `dialogs.answer`, and whoever gives
  it: the bridge, or code calling `Dialogs::answer` with another
  origin. The journal says `dialog.answer.refused`, from the caller. The
  dialog stays open, and cancelling it stays allowed.
- **A file choice carries its origin** to its consumer: in the result
  (`files::chosen_origin`), in the `file.chosen` step's arguments
  (`origin`), and in the collector's last choices (`by`).
  `Files::deliver_as` names the origin; `Files::deliver` is the user's.
  Nothing refuses an agent's choice: a choice has no command whose flag
  could decide it. The consumer decides, or a hook on `file.chosen`.

## Consequences

- Offering `dialogs.answer` lets an agent answer dialogs. It does not
  let the agent run, through a confirmation, what it could not run
  directly.
- A consumer of a file choice that should act only on a person's choice
  checks `chosen_origin`. Existing consumers, which read only the paths,
  are unchanged.
- Origin stays a label from code in the process. Carrying it through
  the commands a handler starts is the next decision.
