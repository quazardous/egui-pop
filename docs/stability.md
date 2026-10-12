# API stability

What a release promises, crate by crate and item by item. egui-pop
follows [Semantic Versioning](https://semver.org/) with one egui-pop
minor per egui minor; before 1.0 a minor may break the API, and does when
the result is cleaner (the changelog says how to move). Within a minor,
patch releases only add or fix.

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
  `MinimalPlugins`.
- `egui-pop`: the facade's paths and its `prelude`.

## Supported

- The components: the command palette, dialogs, toasts, the settings
  window and shortcuts editor, the window's place, menus, the slider and
  the parameter widget, icons. Their behaviour and commands are kept;
  their drawing follows the theme and may be refined.
- The test kit (`egui-pop-test`): harnesses and checks.
- The debug bridge: collector names and snapshot shapes (agents read
  them).

## Experimental

- The host contract (`egui_pop_api::host`): the split between declaring
  and running commands is decided; the trait's methods may still move
  while the first application with its own kernel adopts it.
- The focus mode and its system readers.
- The slider's gestures as data (`slider::Gestures`): profiles of input
  are not designed yet.
- Anything marked "experimental" in its documentation.

## Not covered

- Pixel output: snapshots are refreshed when the drawing improves.
- The gallery and the application template: examples, not APIs.
- Undocumented items and the `pub(crate)` internals.
