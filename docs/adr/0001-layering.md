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
| L0 | `egui-pop-win32`: the frame's Windows hit-test layer (amended 2026-10-12) | no |
| L0 | `egui-pop-wayland`: the image viewport's Wayland layer, a tagged subsurface (amended 2026-10-13) | no |
| L1 | `egui-pop-model`: commands, keymap, menus, panels, layouts, values, ids, diagnostics, documents | no |
| L2 | `egui-pop-core`: theme, focus, keys, overlays, placement, accessibility, strings, icons, persistence, fault boundary, binding, debug bridge | yes |
| L3 | `egui-pop-api`: plugins, the app builder, `Ctx`, zones and blocks, hooks, services, stories | yes |
| L3 | `egui-pop-viewport`: the image viewport, thumbnails, the histogram (a component on the egui layer alone) | yes |
| L4 | `egui-pop-test`: test kit (dev-dependency only) | yes |
| L4 | `egui-pop-i18n`: translations, a plugin (a component on the application API) | yes |
| L4 | `egui-pop-platform`: platform services (single instance, notifications, tray, global shortcuts, logs and crash reports, focus mode), opt-in plugins | yes |
| L4 | `egui-pop-views`: data views (a live tree, a list, a data table), a component on the application API | yes |
| L4 | `egui-pop-editors`: text and code editors, a component on the application API (amended 2026-10-13) | yes |
| L4 | `egui-pop-imaging`: the steps of the viewport's image tools (histogram, curves, colour wheels, zoom and navigator), on the application API (amended 2026-10-13) | yes |
| L5 | `egui-pop`: the facade users depend on, re-exporting the layers | yes |
| L6 | applications: the gallery, `consumer-smoke`, `xtask`; third-party extensions, as `templates/extension` (amended 2026-10-13) | yes |

Components sit between L3 and the facade. A component that needs only
the egui layer (`egui-pop-viewport`) sits at L3 beside the application
API, so the test kit (L4) can carry test doubles for it; one that needs
plugins sits above the API, at L4 (`egui-pop-i18n`, amended
2026-10-12; `egui-pop-views`, whose moments are steps, 2026-10-13), so
the facade can re-export it. A platform layer that must call the system
(`egui-pop-win32`, amended 2026-10-12; `egui-pop-wayland`, 2026-10-13)
knows nothing of egui: it sits at L0 as a leaf, may use `unsafe` (in
one module, the only crates that may), and the layer above maps its
plain types. A test
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
