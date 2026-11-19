# 0027: Every public module belongs to one component family, which says what it owns and refuses

Date: 2026-11-15. Status: accepted (2026-11-15).

## Context

egui-pop covers a lot: windows, docking, focus, undo, the clipboard, the
context stack, selection, editors, a terminal, an image viewport, a node
graph, jobs. Each crate has its place in the layering (ADR 0001), the
interactions between families are decided in ADRs 0012 to 0017 and 0025,
and `docs/stability.md` gives tiers crate by crate and item by item. What
was missing (an outside review, `docs/review-2026-11.md`, point 1): one
place that says, per family, what egui-pop owns, what it refuses, how
firmly it is kept, and where its contract is written. Without it a new
module lands without anyone deciding which promise it falls under, and a
reader cannot tell a refusal from a gap.

## Decision

- **Families, not crates, carry the promise.** A family is a set of
  public modules across the layers (its pure model in `egui-pop-model`,
  its drawing in `egui-pop-core`, its plugin in `egui-pop-api`, or a crate
  of its own). The table below is the list.
- **Every public module of every published crate is in exactly one
  family row.** A crate without public modules is listed by its name; a
  crate with public modules is listed by its modules only. `cargo xtask
  families --check` reads the table's Modules column and the crates'
  `src/lib.rs`, and fails on a module in no row, in two rows, or a row
  naming a module that does not exist. pr-lint runs it, so a new module
  comes with its family.
- **The tier is `docs/stability.md`'s**, quoted here, never decided here:
  when the two disagree, stability.md wins and this table is fixed.
  "Not listed" would mean stability.md says nothing yet, a gap to fix there.
- **Refuses** is a decision: a request in that list is answered with
  "out of scope, build it on top", not a new module.
- **A new family** needs an application adopting it, an owner, frame
  tests, a contract doc and a row here before its first release.
- **A family may leave the repository** when it has no egui-pop
  dependency it could not take from crates.io (as `ortho-router` and
  `egui-pop-hooks` already could) and its own release rhythm: it moves
  to its own repository, keeps its crate name, and its row here becomes
  a dependency line.

The gallery and `consumer-smoke` (not published), `xtask` and the
templates are examples and tools, outside the table.

## Families

| Family | Owns | Refuses | Tier | Modules | Contract |
|---|---|---|---|---|---|
| Foundations | Typed ids, diagnostics, owner-tagged registries, plugin order, clocks, fuzzy matching, versioned documents and their atomic persistence, egui-memory slots, settings folders | A storage engine or database; sync over a network; egui types in the model | Stable (ids, documents, persistence); Supported (configuration folder) | `egui-pop-model::{ids, diagnostics, registry, order, clock, fuzzy, doc}`; `egui-pop-core::{persist, state}`; `egui-pop-api::dirs` | ADR 0002, 0003, 0008, 0028; `docs/dirs.md` |
| Values and binding | The value store (descriptors, `Set`, gestures, writers), the three binding modes, late values, latest-wins loads | Reactive signals or a general state-management library; deriving UI from structs | Stable (value store, binding); Experimental (late, latest) | `egui-pop-model::values`; `egui-pop-core::{binding, late, latest}` | ADR 0009, 0016, 0028; `docs/state-or-event.md`, `docs/linked-panels.md`, `docs/late-values.md` |
| Tokens and themes | DTCG tokens, the resolver, contrast checks, palettes, theme packages, egui visuals and fonts from tokens, the motion clock, text and scroll-bar settings, the theme editor | Literal colours and sizes in components; a CSS engine; per-widget style overrides outside tokens | Stable (tokens, theme); Supported (scroll-bar setting); theme editor Experimental | `egui-pop-tokens::{apca, cache, contract, contrast, package, palette}`; `egui-pop-core::{theme, fonts, motion}`; `egui-pop-api::{text, scroll_bars, theme_editor}` | ADR 0006; `docs/tokens.md`, `docs/themes.md`, `docs/fonts.md`, `docs/theme-editor.md` |
| Visual kits | A component's boxes per state and tone as data, kit folders and packages | Arbitrary drawing code in a kit; images other than the kit's own | Experimental | `egui-pop-tokens::kit`; `egui-pop-core::kit` | `docs/kit.md`, `docs/kit-packages.md` |
| Controls | egui's small widgets through the kit, the decorated input, numbers, field states, sliders and their gestures, knobs, the parameter widget, ratings, compact controls for dense rows, role-named layout helpers, busy state on command-bound controls | A second widget toolkit beside egui's; date pickers and rich text fields | Supported (slider, parameter widget, busy state); Experimental (inputs, field states, slider gestures, compact, knob, rating, number) | `egui-pop-core::{widgets, input, number, field, slider, knob, param, rating, compact, ui}`; `egui-pop-model::field`; `egui-pop-api::{sliders, busy}` | `docs/widgets.md`, `docs/inputs.md`, `docs/sliders.md`, `docs/knob-interaction.md`, `docs/rating.md`, `docs/compact.md` |
| Feedback, hints and icons | Progress, placeholders, micro-indicators, tooltips, hover cards, the area registry (an edit area's grips at rest and hover card), icons by role and an application's icon set, toasts, tips | Animated illustrations; an icon font other than the bundled one as a default | Supported (toasts, icons, icon set, placeholders); Experimental (progress, micro-indicators, tips, tooltips, hover cards, the area registry) | `egui-pop-core::{progress, placeholder, indicator, tooltip, hover_card, areas, icon, icon_set}`; `egui-pop-api::{toasts, tips}` | ADR 0028; `docs/progress.md`, `docs/indicators.md`, `docs/design/indicators.md`, `docs/icons.md` (the icon set: section 8), `docs/tips.md`, `docs/workbench.md` (interactive areas) |
| Overlays, placement and layers | The overlay stack, placement of a floating element next to its anchor, layer ranks, header bands of movable surfaces | A z-index beyond the ranks; windows of the system drawn as overlays | Stable (overlays); Supported (placement, band); Experimental (layers) | `egui-pop-core::{overlay, placement, layers, band}` | ADR 0026, 0028, 0029; `docs/overlays.md` |
| Focus and keyboard | Focus scopes and the focus chain, Tab order, F6 regions, the key dispatcher, the keymap and its resolution, `when` clauses, the focus mode, keys in the system scope | Input methods and text editing keys (egui's); OS-wide hotkeys (platform services) | Stable (focus, dispatcher, keymap, `when`); Supported (keymap at run time, F6 regions); Experimental (focus mode, system scope) | `egui-pop-core::{focus, tab, regions, keys, focus_mode}`; `egui-pop-model::{tab, keymap, when}`; `egui-pop-api::system_keys` | ADR 0007, 0028; `docs/regions.md`, `docs/focus-mode.md`, `docs/design/navigation.md` |
| Commands, menus and toolbars | Commands as the one action vocabulary, declared menus, the command palette, declared toolbars | Menus or toolbars drawn by whoever fills them; actions that are not commands | Stable (commands, menus); Supported (palette, menus drawn); Experimental (toolbars) | `egui-pop-model::{commands, menus}`; `egui-pop-api::{menus, palette, toolbars}` | ADR 0023, 0028; `docs/toolbars.md`, `docs/plugins.md` |
| Pointer and canvas | Gesture profiles (wheel, scroll, pinch, drag), the table canvases read, the canvas in use and its navigation commands | Gesture recognition beyond egui's events; touch-first layouts | Experimental | `egui-pop-model::pointer`; `egui-pop-core::{pointer, canvas}`; `egui-pop-api::pointer` | `docs/pointer-gestures.md` (the canvas in use), `docs/design/pointer-gestures.md`, `docs/cursors.md` |
| Workbench and layout | Declared panels, zones and blocks, the workbench's areas over egui_stack, layout controls, arrangements, strips, floating palettes, collapsible sections, folds inside a panel and a hand-built tree's rows | A docking engine of its own (egui_stack is it); free-form canvases of panels | Stable (zones and blocks); Experimental (workbench, arrangements, strips, sections, folds) | `egui-pop-model::{panels, layout, sections}`; `egui-pop-core::{sections, fold}`; `egui-pop-api::{workbench, layout_controls, arrangements, strips, palettes, zones}` | ADR 0005, 0022, 0028; `docs/workbench.md`, `docs/sections.md`, `docs/linked-panels.md`, `docs/status-bar.md` |
| Windows, frame and scale | The window's own frame and hit regions, the window's place, application windows, scale per window, the interface zoom | A windowing system of its own (eframe and winit are it); multi-process windows | Supported (frame, scale, zoom, window's place); Experimental (application windows, what draws a window's own surfaces) | `egui-pop-core::{frame, scale, window_surface}`; `egui-pop-api::{frame, window, windows, zoom}` | ADR 0025, 0029; `docs/workbench.md` |
| Operating system layers | The few `unsafe` calls the frame and the viewport need: Win32 hit testing, a Wayland subsurface, winit's raw handles | Anything a platform crate (winit, raw-window-handle) already offers; egui types | Not covered | `egui-pop-win32::{ht, wm, dwm}`; `egui-pop-wayland`; `egui-pop-winit` | ADR 0001; `docs/wayland-colour.md` |
| Application API | `Plugin`, `Ctx`, `AppBuilder`, `App`, services, steps, `alter` hooks, the bus (the window lifecycle's requests, listeners and registry, and the window invariants checked against it), the host, reads, waiting on the application, the native runner and the main window's frames owed (`main_wake`), gallery stories | An application's kernel (the host wraps it); async runtimes; a plugin ABI across dynamic libraries | Stable (plugin, ctx, app, services, steps, hooks); Experimental (bus, window invariants, host, shell hooks, runner, main_wake, reads, wait) | `egui-pop-api::{app, ctx, plugin, services, steps, hooks, bus, window_invariants, host, reads, wait, runner, main_wake, stories}` | ADR 0004, 0010, 0011, 0028, 0029; `docs/plugins.md`, `docs/extensions.md`, `docs/first-app.md`, `docs/threading.md`, `docs/runner.md` |
| Hook kernel | Typed hook points, ordered listeners, `alter` and `collect`, the per-listener error boundary, introspection, without egui | UI of any kind; event buses across processes | Experimental | `egui-pop-hooks` | ADR 0028, 0029; `docs/hook-kernel.md`; crate docs; `docs/standalone.md` |
| Slot layout | Plain boxes packed into a grid in standard slot sizes: forms, orientations, pins, groups and their bands, growth, hysteresis, logical and visual order, without egui (formerly the separate `egui_fit`) | Drawing, widgets or values; a constraint solver or general box model (egui's layouts and `UiPop`'s rows are those) | Experimental | `egui-pop-fit` | crate docs (`crates/egui-pop-fit/README.md`); `docs/standalone.md` |
| Containment | Fault boundaries around panels, the main window's paint guard | Recovering application state after a panic; crash reporting (platform services) | Supported (fault boundaries); Experimental (paint guard) | `egui-pop-core::fault`; `egui-pop-api::paint_guard` | ADR 0003, 0029; `docs/containment.md` |
| Mount contract | What components need and hosts offer, checked across crates | Dependency injection beyond the declared offers | Supported | `egui-pop-model::mount`; `egui-pop-api::mounts`; `egui-pop-views::mounts`; `egui-pop-editors::mounts`; `egui-pop-viewport::mounts`; `egui-pop::mounts` | ADR 0013, 0028; `docs/hosts.md` |
| Context and selection | The context stack projected from the kernel, the application-wide selection | Owning the application's places or selection model (the kernel does) | Experimental | `egui-pop-model::{context, selection}`; `egui-pop-api::{contexts, selection}` | ADR 0012; `docs/contexts.md`, `docs/selection.md`, `docs/status-and-context.md` |
| Undo | Histories routed by focus, one step for many writes, the history panel | Owning the application's history (the kernel does); collaborative or branching undo | Stable (value store undo); Experimental (histories, history panel) | `egui-pop-model::history`; `egui-pop-api::{history, history_view, group}` | ADR 0014, 0028; `docs/history.md` |
| Clipboard | Typed items in several formats, hooked at each boundary | Owning the application's items (the kernel does); drag and drop between applications | Experimental | `egui-pop-model::clipboard`; `egui-pop-api::clipboard` | ADR 0015; `docs/clipboard.md` |
| Links, files and the viewer | One service opening links through one policy, choosing files, the in-app viewer for safe content | A web browser engine; a file manager | Experimental | `egui-pop-model::links`; `egui-pop-api::{links, files}`; `egui-pop-views::viewer` | ADR 0017; `docs/links.md`, `docs/files.md` |
| Jobs | The host's background work folded from `job.*` events, the jobs centre | A thread pool or scheduler (the host runs jobs) | Experimental | `egui-pop-model::jobs`; `egui-pop-api::{jobs, jobs_centre}` | ADR 0028; `docs/progress.md`, `docs/threading.md` |
| Standard windows and dialogs | Dialogs, the settings form, window and custom pages, About | Wizards spanning several applications; a form builder for arbitrary data (admin forms) | Supported (dialogs, settings window); Experimental (About) | `egui-pop-api::{dialogs, settings, settings_pages, settings_window, about}` | ADR 0028, 0029; `docs/dialogs.md`, `docs/about.md`, `docs/plugins.md` |
| Admin forms | The administration app: full-page forms declared as data | Business logic behind the forms | Experimental | `egui-pop-model::admin`; `egui-pop-api::admin` | `docs/design/admin-forms.md` |
| Strings and translations | Fluent catalogues and fallback chains, locale formatting, the extractor and checks, each crate's own strings with English fallbacks | Machine translation; translating the application's data | Supported | `egui-pop-i18n::{catalog, check, extract, format, locale, plugin}`; `egui-pop-core::strings`; `egui-pop-api::strings`; `egui-pop-viewport::strings`; `egui-pop-views::strings`; `egui-pop-editors::strings`; `egui-pop-platform::strings`; `egui-pop-agent::strings` | `docs/i18n.md` |
| Agent bridge and devtools | Collectors describing the running app, the element picker, devtools, the socket served out of process, its agents' pairing and grants, the user's confirmation of their destructive calls, argument schemas checked before a run, its MCP server | Remote control over a network; authentication beyond the socket's permissions | Supported (collector names, snapshot shapes); Experimental (devtools) | `egui-pop-core::{debug, inspect, schema}`; `egui-pop-api::devtools`; `egui-pop-agent::{confirm, grants, mcp, pairing}` | ADR 0018, 0019, 0020, 0021, 0024, 0029; `docs/agent-socket.md`, `docs/agents.md`, `docs/debugging.md`, `docs/inspection.md`, `docs/design/agent-control.md` |
| Accessibility | Names for custom widgets, the announcer | A screen reader of its own; platform bridges (AccessKit is it) | Supported | `egui-pop-core::a11y` | `docs/accessibility.md`; `CREDO.md` (point 7) |
| Image viewport | The view model and mapping, tile sources and the cache, the GPU path, the display transform, the Wayland image surface, gestures, hooks, cursors, the loupe, overlays in image coordinates, navigation, the debug collector, a demo source | Decoding image formats; colour management beyond the display transform; raw development | Experimental | `egui-pop-viewport::{view, widget, source, tiles, device, gpu, display, image_surface, gestures, hooks, cursor, loupe, overlay, navigation, tokens, debug, demo}` | `docs/viewport.md` (the contract), `docs/wayland-colour.md`, `docs/cursors.md` |
| Image tools | The histogram, curves, colour wheels, zoom control and navigator, their shared change and steps, a preview dialog | Computing histograms or applying curves to pixels (the application does) | Experimental | `egui-pop-viewport::{change, curve, histogram, wheel, navigator}`; `egui-pop-imaging::preview` | `docs/image-tools.md` |
| Node graph | A node graph editor over the application's graph, its steps, orthogonal wire routing | Evaluating the graph; a visual programming language | Experimental | `egui-pop-viewport::graph`; `egui-pop-imaging::graph`; `ortho-router` | `docs/node-graph.md` |
| Thumbnails | Thumbnails and their virtualised grid | Generating thumbnails from files (the application or platform services do) | Experimental | `egui-pop-core::thumbnail`; `egui-pop-viewport::thumbnail` | `docs/thumbnails.md`, `docs/library.md` |
| Data views | The live tree, lists, list items, property rows, the data table, the text log, selection by key, charts, the timeline | A spreadsheet; a query engine over the data | Experimental | `egui-pop-views::{item, list, property, select, table, text_log, tree, steps, plot, timeline}` | `docs/tree.md`, `docs/table.md`, `docs/list-items.md`, `docs/plot.md`, `docs/timeline.md` |
| Media and library | Video played in place, a photo library's import, culling and sorting widgets | Codecs; a photo catalogue's storage | Experimental | `egui-pop-views::{video, library}` | `docs/video.md`, `docs/library.md` |
| Text and code editors | The line buffer, selection and history, virtualised drawing, search and replace, markers, folding, a pluggable highlighter, code assist providers, the editors' steps and tokens | A language server; an IDE's project model | Experimental | `egui-pop-editors::{assist, buffer, code, fold, highlight, history, markers, model, search, selection, steps, text, tokens}` | `docs/editors.md` (the contract) |
| Terminal | vt100's emulator drawn over a source (a PTY or a log) | A shell; terminal multiplexing | Experimental | `egui-pop-editors::terminal` | `docs/editors.md` |
| Platform services | A single instance and deep links, notifications, a tray icon, global shortcuts, logs and crash reports, the focus mode, the system clipboard and opener, system thumbnails, text scale, wheel lines, raising the window | Services an OS crate covers better (each wraps one); an installer or updater | Experimental | `egui-pop-platform::{clipboard, crash, dirs, focus, instance, keys, logs, notify, open, text_scale, thumbnails, tray, wheel_lines, window}` | `docs/instance.md`, `docs/notifications.md`, `docs/tray.md`, `docs/global-shortcuts.md`, `docs/logs.md`, `docs/focus-mode.md`, `docs/dirs.md` |
| Test kit | Themed harnesses, mock hosts, scripted jobs, scenarios, waits, keyboard walks, several windows, a test tile source, the `critical` attribute | A test runner; pixel-perfect comparisons across GPUs | Supported | `egui-pop-test::{hosts, jobs, scenario, wait, walk, windows, viewport}`; `egui-pop-test-macros` | `docs/testing.md`, `docs/scenarios.md` |
| Facade | The paths users depend on and the prelude | Items of its own beyond re-exports and the mount list | Stable | `egui-pop::prelude` | ADR 0006; `docs/getting-started.md` |

## Consequences

- A pull request adding a public module adds it to a row (or a new row,
  with the bar above); `cargo xtask families --check` says which.
- Moving a module between families is an edit of this table, and of
  stability.md when the tier changes.
- The table is not the contract: each row's contract doc is. A row whose
  doc is missing is a gap below, not a promise.

## Known gaps

Found while writing the table (egui-pop `next`, 2026-11-15); each is left
to its own change.

- **Modules without a contract doc of their own**: placeholders,
  tooltips and hover cards (Feedback, hints and icons) have a tier in
  `docs/stability.md` but no page; they are documented in their
  module docs only.
- **Families split across crates in ways a reader may not expect**:
  thumbnails live in `egui-pop-core` and are re-exported by the
  viewport; the in-app viewer is in `egui-pop-views` but belongs to the
  links family.
