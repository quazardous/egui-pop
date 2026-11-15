# API stability

What a release promises, crate by crate and item by item. egui-pop
follows [Semantic Versioning](https://semver.org/) with one egui-pop
minor per egui minor; before 1.0 a minor may break the API, and does when
the result is cleaner (the changelog says how to move). Within a minor,
patch releases only add or fix.
How long a deprecated item keeps working, and what counts as breaking
for an extension (tokens, steps, commands, strings, the host contract),
is in [GOVERNANCE.md](../GOVERNANCE.md).
What each component family owns and refuses, with the tier this page
gives it, is in [ADR 0027](adr/0027-component-families.md).

## Versions of the crates

Each crate has its own version, and a release publishes only the
crates that changed: a fix in `egui-pop-views` is a new
`egui-pop-views` patch, and nothing else is published.

- **The facade** (`egui-pop`) takes one minor per egui minor; it is the
  version applications and extensions name.
- **A sub-crate** moves when its own code or its public dependencies
  do: a patch for a fix or an addition, a minor for a break. A sub-crate
  that does not depend on egui (`egui-pop-tokens`, `egui-pop-model`, the
  platform layers) may stay on its version across egui minors.
- **A dependency names what the crate needs.** The requirement in
  `[workspace.dependencies]` is the lowest version every user of it
  works with; a crate that starts using something newer raises it in
  the same change.
- **A break travels up.** A sub-crate's new minor changes the types
  the crates above re-export, so each of them takes a new minor too, up
  to the facade.

The versions are raised mechanically, from the changeset each pull
request adds, and checked by `cargo semver-checks` against crates.io
([Changesets and releases](releasing.md)).

Three tiers:

- **Stable**: kept within a minor; breaking it waits for the next minor
  and is announced in the changelog with the way to move.
- **Supported**: kept as far as possible within a minor; it may still
  change in a patch release when a defect forces it, said in the
  changelog.
- **Experimental**: may change in any release. Use it, tell us how it
  went, and expect to adapt.

## Stable

- `egui-pop-tokens`: the DTCG file format, the resolver, the built-in
  themes' token names.
- `egui-pop-model`: typed ids and their syntax, `when` clauses, commands
  (`CommandSpec`), the keymap (layers, resolution, the keymap document),
  menus, the value store (descriptors, `Set`, gestures, undo), versioned
  documents and their migrations.
- `egui-pop-core`: the theme (`ThemeSpec`, `Tokens`), focus scopes and the
  focus chain, the key dispatcher, overlays, persistence, value binding
  (the three modes).
- `egui-pop-api`: `Plugin`, `Ctx`, `AppBuilder`, `App`, `AppHandle`,
  zones and blocks, hooks, services, `DefaultPlugins` and
  `MinimalPlugins`; steps (`steps`, ADR 0011): `Ctx::step`, `before`,
  `replace`, `after`, `AppHandle::step`, and the names of the steps
  egui-pop declares.
- `egui-pop`: the facade's paths and its `prelude`.

## Supported

- The components: the command palette, dialogs, toasts, the settings
  window and shortcuts editor, the window's place, menus, the slider and
  the parameter widget, icons. Their behaviour and commands are kept;
  their drawing follows the theme and may be refined.
- The test kit (`egui-pop-test`): harnesses and checks, the helpers at
  several scales, the shared GPU device, the `critical` attribute
  (from `egui-pop-test-macros`).
- Translations (`egui-pop-i18n`): catalogues and their fallback chains,
  the rule that a plugin's catalogue carries its id, the language
  setting, locale formatting, the `i18n.resolve` step, the key
  extractor and checks.
- The window's own frame (`egui_pop_core::frame`, `egui_pop_api::frame`)
  and its published hit regions (`HitRegions`, `HitTable`): the shape
  of the regions is what a platform layer reads.
- Scale (`egui_pop_core::scale`) and the interface zoom
  (`egui_pop_api::zoom`).
- The keymap at run time: `AppHandle::declare`, `undeclare`,
  `load_keymap`, `export_keymap`, `KeymapDiff`.
- The debug bridge: collector names and snapshot shapes (agents read
  them).
- Placement of a floating element next to its anchor
  (`egui_pop_core::placement`): the geometry every popover, menu and
  tooltip uses since 0.1.
- Header bands (`egui_pop_core::band`), the handle movable dialogs and
  palettes are dragged by: their behaviour is kept, their drawing
  follows the kit.
- Fault boundaries around panels (`egui_pop_core::fault`, ADR 0003):
  a panel that panics is replaced by its fallback, the rest of the
  window goes on.
- Accessibility helpers (`egui_pop_core::a11y`): names for custom
  widgets and the announcer.
- The mount contract (`egui_pop_model::mount`, ADR 0013): what a
  component needs and a host offers, the `mounts` declarations of each
  crate and their check; the host behind it stays experimental.
- Placeholders (`egui_pop_core::placeholder`: skeletons and empty
  states), released since 0.8: their states and names are kept, their
  drawing follows the kit.
- F6 regions (`egui_pop_core::regions`, `egui_pop_core::focus::region`),
  released since 0.8: the order and where a hop lands are kept; the
  commands binding F6 follow the workbench's tier.
- The icon set (`egui_pop_core::icon_set`), with the icons: the names,
  their `owner:` prefix and the refusals are kept, as declarations and
  kits store them.
- The scroll-bar setting (`egui_pop_api::scroll_bars`): the
  `appearance.scroll_bars` key and its values (`auto`, `always`,
  `hover`), which users' settings files store.
- Busy state on command-bound controls (`egui_pop_api::busy`,
  `CommandButton`): greyed, a delayed spinner, the width and name kept;
  the host's later replies behind it stay experimental.
- The configuration folder (`egui_pop_api::dirs`): where users'
  settings are kept, per platform.

## Experimental

- The host contract (`egui_pop_api::host`): the split between declaring
  and running commands is decided; the trait's methods may still move
  while the first application with its own kernel adopts it.
- The focus mode and its system readers.
- The slider's gestures as data (`slider::Gestures`): profiles of input
  are not designed yet.
- The workbench (`egui_pop_api::workbench`): areas over egui_stack,
  placement rules, auxiliary windows; arrangements
  (`egui_pop_api::arrangements`); strips and indicators
  (`egui_pop_api::strips`); devtools (`egui_pop_api::devtools`). Their
  steps are kept; their API may move while the first applications
  adopt them.
- The shell's hooks for applications: `AppBuilder::wrap`,
  `main_edge_to_edge`, `frameless`, `layout`, `on_exit`,
  `run_native_with`, the `TOP_BAR_END` zone.
- Visual kits (`egui_pop_core::kit`, kit folders, `AppBuilder::kit`):
  a kit's boxes per state and tone, its handlers and its package format
  may still move; the tokens a kit reads follow the tokens' tier.
- The other components: the context stack, undo histories and the
  history panel, the clipboard, links, choosing files, jobs and
  progress, toolbars, the status bar, collapsible sections,
  micro-indicators, inputs and field states, tips, About, the theme
  editor. Their steps and commands are kept; their API may move
  while the first applications adopt them.
- The image viewport (`egui-pop-viewport`): its view model, mapping,
  signals and hooks may still move while the first application adopts it;
  so may the image tools on it (the histogram, curves, colour wheels,
  the zoom control and navigator, their steps in `egui-pop-imaging`),
  the node graph (`egui_pop::viewport::graph`) and its wire router
  (`ortho-router`).
- The hook kernel (`egui-pop-hooks`): new; its API may move while
  the first applications move their extension points onto it.
- The data views (`egui-pop-views`): the tree, the list, the data table,
  the in-app viewer and the photo library.
- The text and code editors and the terminal (`egui-pop-editors`).
- The platform services (`egui-pop-platform`): their steps are kept;
  their backends, services and the system scope's registration state
  (`egui_pop_api::system_keys`) may move while the first applications
  ship them.
- Compact controls for dense rows (`egui_pop_core::compact`,
  `Slider::scrub`): new, not yet released.
- Values worked out late (`egui_pop_core::late`, new, not yet released)
  and slow loads where the latest asked wins
  (`egui_pop_core::latest`): no application has adopted them yet.
- Layer ranks (`egui_pop_core::layers`, ADR 0026): the order between
  kinds of foreground layers may still gain ranks.
- Pointer gestures and the canvas (`egui_pop_model::pointer`,
  `egui_pop_core::pointer`, `egui_pop_core::canvas`,
  `egui_pop_api::pointer`): profiles of input are not designed yet,
  as for the slider's gestures.
- Application windows (`egui_pop_api::windows`) and the native runner
  (`egui_pop_api::runner`, the `create-native` feature, not yet
  released): the run loop they share may still move.
- The read kernel (`egui_pop_api::reads`) and waiting on the
  application (`egui_pop_api::wait`): their states are kept, their API
  may move while the first applications adopt them.
- The main window's paint guard (`egui_pop_api::paint_guard`): new,
  not yet released.
- Admin forms (`egui_pop_model::admin`, `egui_pop_api::admin`).
- Tooltips (`egui_pop_core::tooltip`) and hover cards
  (`egui_pop_core::hover_card`): their timing and contents are still
  being refined.
- The knob (`egui_pop_core::knob`), changed since 0.11 (both axes);
  rating stars (`egui_pop_core::rating`), with the photo library; the
  number field (`egui_pop_core::number`), new in 0.11.
- Thumbnails and their grid (`egui_pop_core::thumbnail`, re-exported
  by `egui_pop_viewport::thumbnail`), with the photo library.
- Anything marked "experimental" in its documentation.

## Not covered

- Pixel output: snapshots are refreshed when the drawing improves.
- The gallery and the application template: examples, not APIs.
- The operating system layers (`egui-pop-win32`, `egui-pop-wayland`,
  `egui-pop-winit`):
  they serve the window frame and the viewport, which carry the promise.
- Undocumented items and the `pub(crate)` internals.
