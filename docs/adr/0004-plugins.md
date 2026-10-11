# 0004: Plugins own what they register

Date: 2026-10-11. Status: accepted.

## Context

An application built on egui-pop is a set of features (pages, panels,
commands, settings) that come and go: optional features, features turned
off in settings, test apps with one feature. When features register into
shared registries by hand, removing one leaves dangling commands, menu
items and key bindings behind.

## Decision

A feature is a `Plugin`: an `id`, the ids it `requires`, a `priority`,
`enabled(&Ctx)`, `connect(&mut Ctx)` and `disconnect(&mut Ctx)`. The app
builder sorts plugins topologically at start-up (requirements first, then
higher priority, then id, so the order is stable), and refuses with typed
errors a missing requirement (`PluginError::Missing`), a cycle
(`PluginError::Cycle`, with the path) or a duplicate id.

Everything registered through `Ctx` during `connect` (commands, key
bindings, menu items, panels, zones, blocks, settings, services, event
handlers, hooks, stories) is tagged with the plugin as owner. Disconnecting
a plugin removes all of it (`remove_owner` on every registry) and rebuilds
the derived declarations; `disconnect` itself is only for what the plugin
holds outside the framework.

The topological sort is our own (Kahn's algorithm with a priority queue,
about fifty lines): `petgraph`'s `toposort` neither orders ties by
priority nor reports the whole cycle, and the dependency would be the
largest in the model.

## Consequences

- A plugin can be switched off and on at run time without leaks; tests
  check that the registries return to their previous state.
- Plugins never hold registry handles; they talk to the framework through
  `Ctx` (at connect time) and `AppHandle` (at run time).
- egui has its own `egui::Plugin` (context hooks). egui-pop plugins are a
  different, higher-level concept; the docs say so where both appear.
