# egui-pop documentation

egui-pop is an application framework and component library for egui:
themable, accessible, drivable by agents, easy to debug.

**Status: 0.1 prepared, not published.** Design tokens and themes;
plugins that own what they register, commands declared once, layered
keymaps resolved by focus, zones and blocks, overlays, a value store with
undo, a host contract, persistence, a debug bridge and a test kit; the
paved road's components; the workbench (the window's own frame, scale
per window, panels in areas, strips, auxiliary windows, arrangements),
devtools; every feature's moments reachable as steps (ADR 0011).

## Guides

- [Getting started](getting-started.md): the application template, a
  plugin application, the command palette and the paved road.
- [Themes](themes.md): picking and adjusting a theme, every built-in
  theme.
- [Design tokens](tokens.md): the token format, resolution, densities,
  accents, contrast checks, writing a theme.
- [The theme editor](theme-editor.md): a theme's tokens edited live by
  role, colour pickers, contrast readouts (AA, AAA), undo, export and
  import as a token file, steps and agent commands; the survey behind
  it.
- [Plugins, commands and steps](plugins.md): what a plugin registers,
  commands and the host, zones and blocks, hooks, steps.
- [The workbench](workbench.md): panels in areas with placement rules,
  auxiliary windows, arrangements, strips and indicators, the window's
  own frame, scale and the interface zoom, the keymap at run time, hooks
  around the shell.
- [The image viewport](viewport.md): tiles from a source the application
  implements; wide gamut and HDR.
- [Image tools](image-tools.md): a histogram with clipping markers and
  a drag handed back, a curve editor (an application's spline, the
  targeted adjustment), colour wheels, a zoom control and a navigator,
  their steps.
- [Node graph editor](node-graph.md): an application's graph of typed
  ports on egui's `Scene`, edits handed back as requests and run as
  steps, an overview and a detail view with inline sliders, a minimap,
  the keyboard and screen reader paths, agent commands; the survey
  behind it.
- [Cursors and the loupe](cursors.md): tool cursors drawn at the
  window's scale, a brush outline that follows the zoom, the
  `cursor.resolve` step, a GPU loupe on the displayed pixels; the
  survey behind them.
- [Choosing files](files.md): the system's chooser off the interface
  thread, the in-app picker (keyboard, typed paths, places, filters),
  the `file.choose` and `file.chosen` steps.
- [Sliders](sliders.md): darktable-style sliders (soft and hard range,
  typed expressions, fine and coarse, reset, a quad button), logarithmic,
  gradient tracks, range sliders, the `slider.change` step.
- [The tree](tree.md): a live tree over a lazy, asynchronous source,
  edits at any time, selection, keyboard, drag and drop, rename, steps.
- [Lists and tables](table.md): a list and a data table, virtualised,
  sorting and widths in the value store, cells drawn by the
  application, steps.
- [The photo library](library.md): a virtualised thumbnail grid and a
  filmstrip over thumbnails supplied asynchronously, ratings, colour
  labels, flags and stacks, the culling keys, a comparison, tag chips
  and input, a search field with filter chips, steps and agent
  commands; the survey behind them.
- [Text and code editors](editors.md): a text editor and a code
  editor, virtualised, search and replace, a gutter of markers, folding,
  highlighting behind a trait (syntect, tree-sitter), code assist
  providers, steps and agent commands; a terminal over a
  pseudo-terminal or a log console, its keys and focus rules; the survey
  behind them.
- [Jobs and progress](progress.md): the host's background jobs folded
  from its journal, each with an estimated overall percentage, their
  commands, the `jobs.estimate` step, a fake host for tests.
- [Translations](i18n.md): Fluent catalogues, the language setting,
  numbers, dates and plurals, the lookup step, extracting and checking
  keys.
- Platform services, each an opt-in plugin of `egui-pop-platform`:
  [a single instance and deep links](instance.md),
  [notifications](notifications.md), [the tray icon](tray.md),
  [global shortcuts](global-shortcuts.md), [logs and crash reports](logs.md),
  [the focus mode read in-process](focus-mode.md).
- [Tips](tips.md): a tip of the day and contextual hints, registered
  by plugins, translated, dismissed for good through the settings.
- [Debugging](debugging.md): the debug bridge and devtools.
- [Extensions](extensions.md): the known extensions in three tiers
  (official, community, featured), the entry format, how to get listed
  and the checks an entry passes; the extension template
  (`templates/extension`).
- [The gallery](gallery.md): the catalogue application and its options.

## Reference

- [Packaging](packaging.md): desktop files, AppStream, Flatpak, icons,
  the Windows manifest and installers, the macOS bundle, and what each
  platform service needs from the package.
- [Testing](testing.md): how egui-pop is tested.
- [Threading](threading.md): what egui allows on which thread, egui-pop's
  threads, the kernel on its own thread, delegated displays.
- [Threading survey](threading-survey.md): how darktable, vkdt, GIMP,
  Krita, Blender and others split work between threads, and the
  patterns that fit egui-pop.
- [API stability](stability.md): what each release promises.
- [Governance](../GOVERNANCE.md): who decides, decisions as ADRs and
  requests for comments, what breaks an extension, deprecation windows.
- [Upstream workarounds](upstream.md): what egui-pop compensates for in
  its dependencies, and how a limit is handled.
- [Architecture decisions](adr/README.md): one page per decision.
- [Advanced widgets](widgets.md): a survey of advanced widgets and how
  egui-pop gets them.
- [Micro-indicators](design/indicators.md): a design note (proposed)
  for dots, badges, activity pips, dynamic icons, flags and swatches,
  passive or interactive; the survey behind it.
- [Fonts](fonts.md): the plan for proportional and monospace fonts.
- [CREDO.md](../CREDO.md): the principles.

## The crates

| Crate | Layer | What it holds |
|---|---|---|
| [`egui-pop-tokens`](../crates/egui-pop-tokens) | L0 | DTCG token files: parser, resolver, themes, densities, accents, contrast checks. No egui. |
| [`egui-pop-win32`](../crates/egui-pop-win32) | L0 | The window frame's Windows layer: subclasses a window to answer `WM_NCHITTEST` (Snap Layouts, Aero Snap). No egui; `unsafe` in one module. |
| [`egui-pop-wayland`](../crates/egui-pop-wayland) | L0 | The image viewport's Wayland layer: a subsurface under a window, tagged through `wp_color_management_v1`, and the compositor's preferred image description. No egui; `unsafe` in one module. |
| [`egui-pop-model`](../crates/egui-pop-model) | L1 | The logic without egui: typed ids, diagnostics, `when` clauses, commands, keymap (layers, chords, focus depth, traces), menus, panels, layouts, the value store, versioned documents, plugin ordering. |
| [`egui-pop-core`](../crates/egui-pop-core) | L2 | The egui layer: theme, focus scopes, key dispatcher, overlay stack, placement, accessibility, strings, icons, persistence, fault boundaries, value binding, the debug bridge. |
| [`egui-pop-api`](../crates/egui-pop-api) | L3 | The application API (its own semver): `Plugin`, `Ctx`, `AppBuilder` over eframe or headless, zones and blocks, hooks, services, stories. |
| [`egui-pop-viewport`](../crates/egui-pop-viewport) | L3 | A generic image viewport (zoom in physical pixels, tiles from an asynchronous source, a display-transform hook, overlays with handles, before/after, a pixel readout), thumbnails, a histogram with clipping markers and a drag, a curve editor, colour wheels, a zoom control and a navigator (feature `viewport` of the facade). |
| [`egui-pop-test`](../crates/egui-pop-test) | L4 | Test kit: harnesses, "every interactive node has a name", accessibility tree dump. |
| [`egui-pop-i18n`](../crates/egui-pop-i18n) | L4 | Translations as a plugin: Fluent catalogues, fallback chains, the language setting, locale formatting, the `i18n.resolve` step, the key extractor and checks (feature `i18n` of the facade). |
| [`egui-pop-platform`](../crates/egui-pop-platform) | L4 | Platform services as opt-in plugins: a single instance and deep links, notifications, a tray icon, global shortcuts, logs and crash reports, the focus mode (feature `platform` of the facade). |
| [`egui-pop-imaging`](../crates/egui-pop-imaging) | L4 | The image tools in an application: each finished change of the histogram's drag, the curve editor, the colour wheels, the zoom control and the navigator runs as a step (feature `imaging` of the facade). |
| [`egui-pop-views`](../crates/egui-pop-views) | L4 | Data views: a live tree over a lazy, asynchronous source, a list and a data table (on `egui_table`), virtualised, keyboard-driven, every moment a step (feature `views` of the facade). |
| [`egui-pop`](../crates/egui-pop) | L5 | The crate to depend on: re-exports the layers, `egui`, and a `prelude`. |
| [`egui-pop-gallery`](../crates/egui-pop-gallery) | app | The catalogue, itself a plugin application (not published). |

Layers only depend downwards; a test reading `cargo metadata` keeps it so
([ADR 0001](adr/0001-layering.md)).

## Code in these pages

The Rust blocks of the README and of the getting started, themes,
plugins, workbench, viewport, files, tree, lists and tables,
translations, platform services, packaging and debugging pages are
compiled and run as doctests by `crates/consumer-smoke`, against the
`egui-pop` crate only, as a user depends on it: `cargo test -p
consumer-smoke --doc`.
