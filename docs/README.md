# egui-pop documentation

egui-pop is an application framework and component library for egui:
themable, accessible, drivable by agents, easy to debug.

**Status: 0.1.1 (2026-10-18), on egui 0.36.** Design tokens (YAML,
compiled at build time) and themes; visual kits, every themable
component drawn from data per state and tone; plugins that own what they
register, commands declared once, layered keymaps resolved by focus,
zones and blocks, overlays, a value store with undo, a host contract,
persistence, a debug bridge and a test kit; the paved road's components;
the workbench (the window's own frame, scale per window, panels in
areas, strips, auxiliary windows, arrangements), devtools; every
feature's moments reachable as steps (ADR 0011).

These pages are online, with search, at
<https://quazardous.github.io/egui-pop/docs/>, beside the
[gallery](https://quazardous.github.io/egui-pop/), which runs in the
browser, and the [API documentation](https://quazardous.github.io/egui-pop/api/)
of every crate with every feature on.

## Guides

- [Getting started](getting-started.md): the application template, a
  plugin application, the command palette and the paved road.
- [Themes](themes.md): picking and adjusting a theme, every built-in
  theme.
- [Visual kits](kit.md): what components look like, as data per state
  and drawing handlers, compiled when the theme is installed; and
  [kits as packages](kit-packages.md): a kit folder, `kit!`,
  `AppBuilder::kit`, user kits.
- [Design tokens](tokens.md): the token format, resolution, densities,
  accents, contrast checks, writing a theme.
- [The theme editor](theme-editor.md): a theme's tokens edited live by
  role, colour pickers, contrast readouts (AA, AAA), undo, export and
  import as a token file, steps and agent commands; the survey behind
  it.
- [Plugins, commands and steps](plugins.md): what a plugin registers,
  commands and the host, zones and blocks, hooks, steps.
- [State or event: what to use](state-or-event.md): how a plugin
  shares data (the value store, commands, steps, the context stack,
  services), the per-frame trap, one example done both ways.
- [The context stack](contexts.md): places (a drill-down and its
  breadcrumb) and modes (global states and their chips) owned by the
  kernel and projected by the interface, kernel state and view state,
  how a plugin reacts.
- [The clipboard](clipboard.md): typed items kept by the owner,
  readable fallbacks on the system clipboard, a paste negotiated with
  its target, types, transforms and sanitisers declared by plugins,
  the Edit commands and the agents' `clipboard.*`.
- [The status bar and the context stack](status-and-context.md)
  (decided, built): the breadcrumb and mode chips in the status bar,
  status items that follow the context, one saved state.
- [The status bar](status-bar.md): status items at the bar's start or
  end, what gives way on a narrow window, live states (busy, error and
  warning counts), a popover per item, and rich hover cards shared with
  other widgets.
- [The workbench](workbench.md): panels in areas with placement rules,
  auxiliary windows, arrangements, strips and indicators, the window's
  own frame, scale and the interface zoom, the keymap at run time, hooks
  around the shell.
- [Hosts and components](hosts.md): what can live where (panels,
  palettes, auxiliary windows, dialogs, overlays, the status bar), and
  why a combination is not supported.
- [The image viewport](viewport.md): tiles from a source the application
  implements; wide gamut and HDR.
- [Image tools](image-tools.md): a histogram with clipping markers and
  a drag handed back, a curve editor (an application's spline, the
  targeted adjustment), colour wheels, a zoom control and a navigator,
  their steps.
- [Node graph editor](node-graph.md): an application's graph of typed
  ports on egui's `Scene`, edits handed back as requests and run as
  steps, an overview and a detail view with inline sliders, a minimap,
  the keyboard and screen reader paths, agent commands, a reference
  model kept in a YAML file that round-trips; the survey behind it.
- [Pointer gestures and profiles](pointer-gestures.md): what the wheel,
  a pinch and each button do on the image, the node graph and sliders,
  as six profiles (Document, darktable, Photoshop, Lightroom Classic,
  Blender / node editors, Trackpad) of keymap bindings the shortcuts editor changes, and a
  widget taking the secondary button for itself.
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
- [Inputs, field states and Tab order](inputs.md): the decorated
  input (placeholder, icons, prefix and suffix, a clear button, attached
  and split buttons, a dropdown), the field states every input shows
  (modified, required, invalid, disabled with a reason, read-only,
  pending), Tab order per focus scope and F6 between panels; the survey
  behind them.
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
- [Micro-indicators](indicators.md): dots and LEDs, count badges,
  activity pips, flags, swatches and icons by state, drawn by the kit,
  passive (their meaning in their host's name) or buttons, inline or on
  a widget's corner, a pulse still under reduced motion, their states
  for agents.
- [Collapsible sections](sections.md): sections stacked in a panel,
  each a band (disclosure mark, title, count) over its own scrolling
  body, folded, resized at the line between two and remembered; drawn
  by the kit; the classic apps compared and the crates surveyed.
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
  [the focus mode read in-process](focus-mode.md),
  [the system's text scaling](fonts.md#starting-from-the-systems-text-size).
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
- [Micro-indicators, the design](design/indicators.md): the note
  behind the indicator module (accepted, with the owner's decisions and
  where the visual kits changed them): the survey, the places of use,
  what waits for the mount contract and the icon registry.
- [Icons: an inventory](icons.md): every place an icon serves in
  egui-pop and in an application such as Helios (command icons and
  object-kind icons, the rule, size, states and accessibility of each,
  what exists and the gap), the platforms' conventions for menu icons,
  and the registry and menu gutter (decided 2026-10-22).
- [Pointer gestures, a study](design/pointer-gestures.md): how
  applications use the wheel, the buttons and the modifiers on canvases
  (darktable in detail), the gesture profiles, and the decisions taken.
- [Fonts](fonts.md): font families from the theme at four weights, the
  size scale and the text size seeded from the system's (built), and
  the design for the rest.
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
plugins, state or event, workbench, viewport, files, tree, lists and tables,
translations, platform services, packaging, debugging and
micro-indicators pages are
compiled and run as doctests by `crates/consumer-smoke`, against the
`egui-pop` crate only, as a user depends on it: `cargo test -p
consumer-smoke --doc`.
