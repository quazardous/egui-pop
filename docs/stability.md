# API stability

What a release promises, crate by crate and item by item. egui-pop
follows [Semantic Versioning](https://semver.org/) with one egui-pop
minor per egui minor; before 1.0 a minor may break the API, and does when
the result is cleaner (the changelog says how to move). Within a minor,
patch releases only add or fix.
How long a deprecated item keeps working, and what counts as breaking
for an extension (tokens, steps, commands, strings, the host contract),
is in [GOVERNANCE.md](../GOVERNANCE.md).

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
  several scales, the shared GPU device.
- The window's own frame (`egui_pop_core::frame`, `egui_pop_api::frame`)
  and its published hit regions (`HitRegions`, `HitTable`): the shape
  of the regions is what a platform layer reads.
- Scale (`egui_pop_core::scale`) and the interface zoom
  (`egui_pop_api::zoom`).
- The keymap at run time: `AppHandle::declare`, `undeclare`,
  `load_keymap`, `export_keymap`, `KeymapDiff`.
- The debug bridge: collector names and snapshot shapes (agents read
  them).

## Experimental

- The host contract (`egui_pop_api::host`): the split between declaring
  and running commands is decided; the trait's methods may still move
  while the first application with its own kernel adopts it.
- The focus mode and its system readers.
- The slider's gestures as data (`slider::Gestures`): profiles of input
  are not designed yet.
- The workbench (`egui_pop_api::workbench`): areas over egui_dock,
  placement rules, auxiliary windows; arrangements
  (`egui_pop_api::arrangements`); strips and indicators
  (`egui_pop_api::strips`); devtools (`egui_pop_api::devtools`). Their
  steps are kept; their API may move while the first applications
  adopt them.
- The shell's hooks for applications: `AppBuilder::wrap`,
  `main_edge_to_edge`, `on_exit`, `run_native_with`.
- The image viewport (`egui-pop-viewport`): its view model, mapping,
  signals and hooks may still move while the first application adopts it.
- The platform services (`egui-pop-platform`): their steps are kept;
  their backends, services and the system scope's registration state
  (`egui_pop_api::system_keys`) may move while the first applications
  ship them.
- Anything marked "experimental" in its documentation.

## Not covered

- Pixel output: snapshots are refreshed when the drawing improves.
- The gallery and the application template: examples, not APIs.
- Undocumented items and the `pub(crate)` internals.
