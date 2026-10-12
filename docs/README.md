# egui-pop documentation

egui-pop is an application framework and component library for egui:
themable, accessible, drivable by agents, easy to debug.

**Status: phase 2, the framework.** Design tokens and themes (phase 1);
plugins that own what they register, commands declared once, layered
keymaps resolved by focus, zones and blocks, overlays, a value store with
undo, persistence, a debug bridge and a test kit. Components come next.

## Guides

- [Getting started](getting-started.md): the application template, a
  plugin application, the command palette and the paved road.
- [Themes](themes.md): picking and adjusting a theme, every built-in
  theme.
- [Design tokens](tokens.md): the token format, resolution, densities,
  accents, contrast checks, writing a theme.
- [Plugins, commands and steps](plugins.md): what a plugin registers,
  commands and the host, zones and blocks, hooks, steps.
- [The image viewport](viewport.md): tiles from a source the application
  implements.
- [Debugging](debugging.md): the debug bridge.
- [The gallery](gallery.md): the catalogue application and its options.

## Reference

- [Testing](testing.md): how egui-pop is tested.
- [API stability](stability.md): what each release promises.
- [Upstream workarounds](upstream.md): what egui-pop compensates for in
  its dependencies.
- [Architecture decisions](adr/README.md): one page per decision.
- [Advanced widgets](widgets.md): a survey of advanced widgets and how
  egui-pop gets them.
- [Fonts](fonts.md): the plan for proportional and monospace fonts.
- [CREDO.md](../CREDO.md): the principles.

## The crates

| Crate | Layer | What it holds |
|---|---|---|
| [`egui-pop-tokens`](../crates/egui-pop-tokens) | L0 | DTCG token files: parser, resolver, themes, densities, accents, contrast checks. No egui. |
| [`egui-pop-model`](../crates/egui-pop-model) | L1 | The logic without egui: typed ids, diagnostics, `when` clauses, commands, keymap (layers, chords, focus depth, traces), menus, panels, layouts, the value store, versioned documents, plugin ordering. |
| [`egui-pop-core`](../crates/egui-pop-core) | L2 | The egui layer: theme, focus scopes, key dispatcher, overlay stack, placement, accessibility, strings, icons, persistence, fault boundaries, value binding, the debug bridge. |
| [`egui-pop-api`](../crates/egui-pop-api) | L3 | The application API (its own semver): `Plugin`, `Ctx`, `AppBuilder` over eframe or headless, zones and blocks, hooks, services, stories. |
| [`egui-pop-viewport`](../crates/egui-pop-viewport) | L3 | A generic image viewport (zoom in physical pixels, tiles from an asynchronous source, a display-transform hook, overlays with handles, before/after, a pixel readout), thumbnails and a histogram (feature `viewport` of the facade). |
| [`egui-pop-test`](../crates/egui-pop-test) | L4 | Test kit: harnesses, "every interactive node has a name", accessibility tree dump. |
| [`egui-pop`](../crates/egui-pop) | L5 | The crate to depend on: re-exports the layers, `egui`, and a `prelude`. |
| [`egui-pop-gallery`](../crates/egui-pop-gallery) | app | The catalogue, itself a plugin application (not published). |

Layers only depend downwards; a test reading `cargo metadata` keeps it so
([ADR 0001](adr/0001-layering.md)).

## Code in these pages

The Rust blocks of the README and of the getting started, themes,
plugins, viewport and debugging pages are compiled and run as doctests
by `crates/consumer-smoke`, against the `egui-pop` crate only, as a user
depends on it: `cargo test -p consumer-smoke --doc`.
