# 0001: Crates in layers, no back-edges, checked in CI

Date: 2026-10-11. Status: accepted.

## Context

egui-pop follows egui closely (one minor per egui minor), but most of what
an application framework holds (commands, keymaps, menus, layouts, a value
store, plugin ordering) has nothing to do with drawing. When that logic sits
next to egui code it is tested slowly, through a renderer, and every egui
bump touches it. Libraries that grew without layers (one crate importing
everything) end up with cycles that only a rewrite removes.

## Decision

Crates are stacked; a crate depends only on crates of a lower layer:

| Layer | Crate | Depends on egui |
|---|---|---|
| L0 | `egui-pop-tokens`: design tokens | no |
| L1 | `egui-pop-model`: commands, keymap, menus, panels, layouts, values, ids, diagnostics, documents | no |
| L2 | `egui-pop-core`: theme, focus, keys, overlays, placement, accessibility, strings, icons, persistence, fault boundary, binding, debug bridge | yes |
| L3 | `egui-pop-api`: plugins, the app builder, `Ctx`, zones and blocks, hooks, services, stories | yes |
| L4 | `egui-pop-test`: test kit (dev-dependency only) | yes |
| L5 | `egui-pop`: the facade users depend on, re-exporting the layers | yes |
| L6 | applications: the gallery, `consumer-smoke` | yes |

Components, when they come, sit between L3 and the facade. A test
(`crates/egui-pop/tests/layering.rs`) reads `cargo metadata` and fails on
any dependency that points to the same or a higher layer, on any
workspace crate missing from the table, and on L0 or L1 depending on
egui or eframe.

## Consequences

- The model is unit-tested in milliseconds, without a GPU, and survives
  egui bumps untouched.
- `egui-pop-api` gets its own semver: plugins written against it should
  not break when core internals move.
- Adding a crate means placing it in the table of the layering test.
- A feature that needs an upper layer from a lower one is redesigned
  (a trait in the lower layer, implemented above), never wired backwards.
