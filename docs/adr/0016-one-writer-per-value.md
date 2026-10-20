# 0016: One writer per value

Date: 2026-10-20. Status: accepted.

## Context

The value store knows each path's owner (`Store::declare` refuses a
second one), but writes were checked against the descriptor alone: any
plugin could write any path, and when two plugins wrote the same value,
nothing said which one should. egui_mobius's composition language
checks a one-writer rule before the application runs; the idea is
borrowed here, not the language.

## Decision

- **A path has writers**: its owner, the ones its descriptor names
  (`Descriptor::writers`), or anyone (`Descriptor::any_writer`).
- **A write names its scope**: `Set::by`, or the scope entered on the
  thread (`values::enter_scope`). The application enters the plugin's
  scope while it connects, while its command handlers run (in the
  interface or through `Dispatch::run_local`) and in its lifecycle
  hooks; `Ctx::writer` gives a writer that names the plugin from
  anywhere else.
- **The framework writes any path, for a person**: the settings form,
  bound widgets, the `value.*` commands, the debug bridge and egui-pop's
  own helpers write as `OwnerId::FRAMEWORK`. A setting therefore stays
  writable by the settings form without being opened to every plugin,
  which `any` would do: the person edits it, other plugins do not.
- **A write from someone else is refused**, as a value out of its
  descriptor is (`Refused::NotWriter`), with a `not-a-writer`
  diagnostic once per path and scope. `WriterCheck::Warn`
  (`AppBuilder::writer_check`) applies it with a warning instead, to
  migrate plugins written before the check.
- **Not checked**: a write naming no scope (the application's own code,
  a host, a test), and the store's own replays: undo, redo, a gesture
  cancelled, saved values restored. They write what was already
  allowed, or what the person saved.
- **Seen**: each entry keeps its last writer (`Entry::writer`); the
  `store` collector (devtools and the debug bridge) lists each path's
  owner, writers and last writer, and the writes refused or warned
  about; `egui_pop_test::assert_writes_declared` fails a test on any.

## Consequences

- A plugin that wrote another plugin's value must be named its writer
  by the owner, or go through the owner's command or step.
- The context stack is not part of it: it does not go through the
  value store. Its owner, the kernel, is already its one writer; the
  interface asks through the `context.request` step and never writes.
- Readers are not declared: a value nobody reads is cheap, and the
  check would have little to say.
