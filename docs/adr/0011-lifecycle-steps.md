# 0011: Every feature's life is reachable: observe, veto, replace, extend

Date: 2026-10-12. Status: accepted.

## Context

egui-pop does the plumbing of an application: shortcuts, commands, the
layout, windows, the frame. An application built on it must still be
able to take part in that plumbing at the moment it happens: refuse to
close while work is unsaved, keep a panel from being detached, save an
arrangement elsewhere, log every rebinding, react when the scale
changes. Without a common way, each feature grows its own callbacks, or
none, and the application forks egui-pop's code to get in.

## Decision

A feature names the moments of its life as **steps** (`StepId`,
`frame.close`, `workbench.detach`, `keys.rebind`), declared through
`Ctx::step` and run through `AppHandle::step(step, args, default)`. At
each step a plugin may, each optional:

- **observe**: the step publishes a signal on the host's journal (ADR
  0010), named after it, with its arguments and result
  (`<step>.vetoed` when stopped);
- **veto or redirect**: a before-hook (`Ctx::before`) returns `Go`,
  `Veto(reason)` or `Redirect(args)`;
- **replace**: a run-hook (`Ctx::replace`) does the step instead of
  egui-pop's default (the last plugin to replace a step wins; two are
  reported, ADR 0003);
- **extend**: an after-hook (`Ctx::after`) gets the arguments and the
  result.

Hooks are owned (ADR 0004). Every run is traced: who redirected,
vetoed, ran and followed; the debug bridge's `steps` collector lists
the declared steps, their hooks and the last runs. A hook on a step
nobody declares is reported.

Every feature egui-pop adds from now on exposes its steps, with a
default doing the sensible thing, and documents them.

## Consequences

- Applications reach egui-pop's lifecycle without forking it, and an
  agent sees in one place why something did or did not happen.
- A feature's default is a closure given at the call site, so the
  plumbing stays readable; hooks never hold a registry borrowed and may
  run other steps.
- Steps are interface moments, not commands: a command (ADR 0010) is
  what the user asks for, a step is how the interface carries it out.
  A command's handler usually runs one step.
