# Changelog

All notable changes to the egui-pop crates. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/); versions follow
[Semantic Versioning](https://semver.org/), with one egui-pop minor per
egui minor. Each crate has its own version
([API stability](docs/stability.md#versions-of-the-crates)): an entry
names the crate it changes, and a release lists the versions it
publishes.

## Unreleased

### Added

- A cache of compiled files (egui-pop-tokens, egui-pop-api,
  egui-pop-platform): `egui_pop_tokens::cache`, a general store keyed by
  a source's path, a hash of its contents and the compiler's version
  (`Cache`, `Compiler`, `Cache::get`, `Cache::put`, `CacheError`), in
  the platform's cache directory under the application's id
  (`Cache::for_app`, `cache::platform_dir`, `dirs::cache_dir`), or
  elsewhere (`Cache::at`), or off (`Cache::off`, the default until the
  application calls `cache::set_default`). User kits and themes read at
  run time compile their YAML through it, never next to it: a kit read
  twice parses its YAML once (`KitData::load_with`,
  `kit::discover_with`, `TokenFile::parse_cached`, `TokenFile::read`,
  the new `Themes::add_path` and `Themes::add_path_with`,
  `AppBuilder::cache` for the user's kits). An edited file or a new
  egui-pop compiles again; a cache that cannot be written reads the
  file uncached and says so (`kit::Problem::NotCached`,
  `Warning::NotCached`). A file that cannot be read is the new
  `Error::Io`. Closes #38.
- Spacing by intent and text by role (egui-pop-tokens, egui-pop-core):
  the system tokens `space.inline`, `space.group`, `space.section` and
  `space.page`, aliases of the spacing scale's steps (so densities scale
  them), and `text-role.{title, section, body, muted, meta}`, each a
  size on the size scale, a text colour and a weight, in every built-in
  theme and in the contract and contrast pairs. Typed as
  `Tokens::space.{inline, group, section, page}` and
  `Tokens::text_role` (`TextRoles::get(TextRole)`). Part of #35.
- A `card` kit component and button tones (egui-pop-tokens,
  egui-pop-core): `card` is an application's card (`Kit::card`, handler
  `card`), the raised surface at rest, by tone an edge in the status
  colour or (`muted`) a sunken well; `button.tone.*` gives a button in
  the accent (a primary action), a status colour as fill (`info`,
  `success`, `warning`, `danger`, `record`) or no box at rest (`muted`).
  Both are in the contract and the contrast pairs. Part of #35.
- Layout helpers named by role (egui-pop-core, egui-pop): the `UiPop`
  extension trait on egui's `Ui` (`egui_pop_core::ui`, in the prelude
  with `Space`, `TextRole` and `Tone`): `section_title`, `muted`,
  `meta` and `text_role` (labels in a text role; `ui::role_text` for
  other widgets), `space_role(Space)` (spacing by intent), `card(Tone,
  ..)` (the kit's card) and `button_tone(Tone, ..)` (egui's button in a
  tone of the kit's `button`). "Laying out by role" in the getting
  started guide. Part of #35.
- One writer per value (egui-pop-model): a path's descriptor says who
  may write it besides its owner (`Writers`: `Owner`, the default,
  `Also(...)`, `Any`; `Descriptor::writers`, `Descriptor::any_writer`).
  A write names the scope issuing it (`Set::by`, the new `Set::by`
  field), or takes the one `values::enter_scope` set on its thread
  (`current_scope`, `ScopeGuard`). A scope that is not the owner, not
  a writer named and not the framework (`OwnerId::FRAMEWORK`, acting
  for a person) is refused (`Refused::NotWriter`) with a
  `not-a-writer` diagnostic (`Store::diagnostics`), each path and scope
  once; `Store::set_writer_check(WriterCheck::Warn)` applies it with a
  warning instead. A write naming no scope is the application's own and
  is not checked; undo, redo, cancelling a gesture and restoring saved
  values are never refused. `Entry::writer` keeps the last write's
  scope, `Store::stray_writes` lists the writes from scopes that are not
  writers, `Store::may_write` asks. The `value.*` commands write as the
  framework and describe `writers` and `writer`. Part of #29.
  Migration: `Set` and `Entry` have a new public field, `Refused` a new
  variant; a `Set` written as a struct literal needs `by: None`.
- One writer per value in an application (egui-pop-api, egui-pop-core,
  egui-pop-views, egui-pop-i18n): what a plugin writes while it
  connects, in its command handlers (in the interface or through
  `Dispatch::run_local`; `Dispatch::owner`) and in its lifecycle hooks
  is written as that plugin, and `Ctx::writer` gives a `ValueWriter`
  that writes as it from anywhere (a panel, a block, a thread). The
  settings form, bound widgets, the debug bridge's `store.set`, the
  `value.*` commands and egui-pop's own helpers (zoom, theme
  preference, window place, dialogs, recent files, tips, arrangements,
  table sort and widths) write as the framework, so a setting stays
  writable by the settings form without opening it to every plugin;
  the language helpers write as the i18n plugin. `AppBuilder::writer_check`
  chooses refusing (the default) or warning; `AppHandle::diagnostics`
  includes the store's `not-a-writer` diagnostics. The `store` debug
  collector lists each path's owner, `writers` and last `writer`, the
  check, and the writes from scopes that are not writers (`stray`).
  Part of #29. Migration: a plugin writing a value another plugin
  declared, from its connect, a command or a lifecycle hook, is now
  refused; the owner names it (`Descriptor::writers`), or the
  application passes `writer_check(WriterCheck::Warn)` while it
  migrates. `Dispatch` has a new private field (it is built by
  egui-pop only).
- Declared writes in tests (egui-pop-test): `undeclared_writes` lists
  the writes from a plugin to a value it neither owns nor is named a
  writer of, `assert_writes_declared` fails on any, so a test of a
  plugin set asserts it writes only what it declared. Part of #29.
- Documentation: ADR 0016 (one writer per value), "Who writes a value"
  in the state-or-event guide, with a compiled example. Part of #29.
- egui's small widgets drawn through the kit (egui-pop-core,
  egui-pop-tokens): `widgets::Checkbox`, `RadioButton`, `SelectableRow`
  and `ToggleChip`, with `checkbox`, `radio`, `radio_value`,
  `selectable_label`, `selectable_value`, and `combo_box` /
  `combo_box_labelled` (egui's `ComboBox` with the kit's arrow,
  `paint_combo_arrow`). They keep egui's layout, clicks, keyboard and
  accessibility; the kit paints them through five new components,
  `checkbox` (based on `button`, `selected` while checked), `radio`
  (based on `checkbox`, handler `pill`), `combo-arrow`,
  `selectable-row` (based on `list-row`) and `toggle-chip` (based on
  `selectable-row`), and their marks are glyph roles (`check`,
  `indeterminate`, `radio`, and the new `dropdown`). The default kit
  looks like egui's own widgets; `check` and `indeterminate` must now
  draw different icons. Docs: [kit.md](docs/kit.md#eguis-small-widgets).
- Token file versions (egui-pop-tokens): a file says which format and
  contract it was written for with a stamp in its root `$extensions`,
  `"org.egui-pop.format": { "version": 1, "contract": 1 }`
  (`FORMAT_EXTENSION`, `FormatStamp`, `TokenFile::stamp`); a file
  without one is read as current. `CONTRACT_VERSION` is a number of its
  own, raised by a release that renames or removes a contract token.
  `RENAMED` (empty for now) maps a token's former name to its new one,
  keyed by the contract that dropped it: `TokenFile::upgrade` reads a
  file stamped with an older contract under the new names, aliases
  included, and says so in `Warning`s, as it does for a file stamped
  newer than this egui-pop knows. The theme editor's export writes the
  stamp. Part of #37.
- Light or dark (egui-pop-api, egui-pop-i18n): the
  `appearance.theme_preference` setting (`system`, the default, `light`,
  `dark`; Appearance section) chooses between a theme and its
  counterpart, applied before the next frame without installing again;
  a theme alone keeps its mode. The theme service exposes it
  (`ThemeService::preference`, `set_preference`), says which theme is
  shown (`shown`, `counterpart`, `is_dark`), picks a theme as a picker
  does (`pick`: the theme picked is shown, the preference following
  when it must) and switches (`toggle`); the
  `view.theme.toggle_light_dark` command (View menu, when the
  `themePaired` context key is true) does it. The theme editor edits
  the variant shown, and its first change makes it the theme, alone.
  Strings with their French. Migration: an application whose theme has
  a counterpart now follows the system's light or dark mode; set the
  preference (`set_preference`) or give `ThemeSpec::alone()` to keep
  one variant. The gallery starts in the mode of the theme it is given.
- The gallery's theme picker shows the theme drawn, its pair (marked
  in the list and named in the tooltip) and the light or dark choice
  (follow the system, always light, always dark) at the top of its
  list; picking a theme shows it.
- Theme counterparts (egui-pop-tokens): a theme names its other
  variant, light or dark, in its root `$extensions`
  (`"org.egui-pop.variant-of": "pop-dark"`, `VARIANT_OF_EXTENSION`; one
  side is enough), or a spec names it (`ThemeSpec::pair(dark, light)`,
  `ThemeSpec::counterpart`, `ThemeSpec::alone`, the new
  `ThemeSpec::counterpart` field, a `Counterpart`). `Themes::counterpart_of`,
  `Themes::counterpart` and `Themes::resolve_counterpart` (the spec's
  density, accent, reduced motion and overrides applied to it too),
  `ResolvedTokens::is_dark`. The built-in themes come in pairs: pop
  dark and light, and the GitHub-, Catppuccin-, One-, Solarized- and
  Gruvbox-inspired ones (GitHub's high-contrast ones together); pop high
  contrast stays alone.
- State slots (egui-pop-core): `state::Slot<T>` names a value kept in
  egui's memory, per context (`Slot::new`, `Slot::opaque` for values
  without a `Debug` form) or per instance (`slot.keyed(id)`), with
  `get`, `set`, `remove`, `get_or_insert_with` and `update`. A slot
  touched inside another slot's `update` panics with its name instead of
  deadlocking egui's context. The debug bridge's new `state` collector
  lists every slot written: name, type, scope, entries held and a
  summary of the first values; its `slot` action summarises every entry
  of one slot. egui-pop's own state goes through slots.
- A menu bar that overflows (egui-pop-api): in a narrow window, the
  main menu keeps the menus that fit and moves the rest into a "…"
  menu, or becomes one entry when none fits besides it (the `more` and
  `menu` icons, "More menus" and "Main menu"). Labels are measured once
  per language, kit and scale. Each menu has an access key (the first
  free letter of its label): Alt and that letter open it, inside "…"
  when it is hidden. `menus::bar_within` keeps room at the bar's end
  (the top bar's blocks), `menus::bar_layout` and the `menu_bar` debug
  collector say which menus are in the bar and which in "…". Strings
  with their French.
- Keys changed for the workspace (egui-pop-api): `AppHandle::rebind_in` and
  `reset_keys_in` change a command's keys in the user's or the
  workspace's layer (the `keys.rebind` step says which, `layer`),
  `rebound_in` says whether one did, `key_sources` gives each live
  binding with its layer. The shortcuts editor offers to change keys for
  the workspace once the application asks (`offer_workspace_keys`), and
  says where each key comes from (workspace, user, or the application's
  own name, `name_key_sources`).
- Zen mode (`egui-pop-api`): the `workbench.zen` command (View > Zen
  mode, no default key) shows the centre alone, without the title, menu
  and status bars, the activity bar, the side and bottom areas or the
  palettes; Escape leaves it (`workbench.leave_zen`). A step,
  `workbench.zen` (`{ "zen" }`), on the journal and open to vetoes;
  `AppHandle::set_zen` and `zen`, the `workbenchZen` context key,
  `zen` in the debug bridge's `workbench` collector. Not saved with the
  layout. Strings with their French.
- Full screen keeps the application's title bar (`egui-pop-core`,
  `egui-pop-api`): `FramePlugin` draws it with its `CAPTION_START` and
  `CAPTION` zones, so the menus and the application's items stay. Close
  stays; minimize goes, and a button leaving full screen takes
  maximize's place (`FrameRequest::ExitFullscreen`, the
  `frame.exit_fullscreen` step), published as client area so Windows
  shows no Snap Layouts; the caption no longer moves the window, a
  double click on it does nothing, and the hit table publishes no
  resize edge (`HitRegions::fullscreen`, `FrameLayout::fullscreen`;
  on Windows the caption answers as client area). `TitleBar::full_screen`
  draws a preview as in full screen. The window plugin's strip at the
  top edge shows only when no bar is drawn. Auxiliary windows follow.
  The `window.bar_in_fullscreen` setting (on by default) gives the
  former behaviour back. Strings with their French.
- A `window-exit-fullscreen` glyph role in the kit (`egui-pop-core`,
  `egui-pop-tokens`, `glyph.window-exit-fullscreen`, the
  `exit-fullscreen` icon by default): the mark of the title bar's
  button leaving full screen.

- Kit component `tab-preview` (`egui-pop-tokens`, `egui-pop-core`): the
  ghost of a dragged tab where it would land. Based on `tab` (its
  handler, sizes and radius), its fill is the selected tab's at 60 %
  alpha, its border the accent.
- Workbench (`egui-pop-api`): a tab dragged over another area it may go
  to, or a palette over an area it docks in, shows a ghost of its tab
  (`tab-preview`) after the last tab of that area's tab bar, where the
  drop puts it; none when the drop is refused. `DropHint::preview` and
  the `workbench` collector's `drop_hint.preview` give its area, index
  and rect.
- Micro-indicators (`egui-pop-core`, `core::indicator`), drawn by the
  kit like every component: dots and LEDs (off, on, busy, warning,
  error, each a shape of its own), count badges capped at `99+` (zero
  hidden unless asked) or a short text, activity pips (busy, a
  fraction, done, failed, delayed like `motion::Busy`), flags
  (modified, pinned, required, invalid, or a shape of the
  application's), colour swatches over egui's alpha checkerboard (or
  none, or mixed), and built-in icons by state. Passive by default: on
  a widget (`IndicatorExt::with_indicator`, `Indicator::on`) the
  meaning joins the widget's accessible name, inline it is read as an
  image. `interactive(action)` makes one a button (a 24-point target at
  least, focus ring, Space and Enter, tooltip, its own AccessKit node,
  a check box for a toggle, `disabled(reason)`, a `command` the `App`
  runs). Anchored on a corner (overhanging on a layer above the host,
  never clipped), an edge or the centre, with a direction for right to
  left. A pulse for busy LEDs and `attention`, still under reduced
  motion. Kit components `indicator-dot`, `indicator-badge`,
  `indicator-flag`, `indicator-swatch`, `indicator-outline` and
  `indicator-button`, system tokens `motion.pulse.period` and
  `motion.pulse.count`, contrast pairs in every built-in theme; strings
  with their French. Marks with an id are recorded each pass
  (`indicator::drawn`), shown by the debug bridge's new `indicators`
  collector and by the test kit's `indicators` and `assert_indicator`
  (`egui-pop-test`). `progress::paint_ring_on` and `paint_spinner_on`
  paint with a painter of their own. A gallery page (`indicators`).
  Guide: docs/indicators.md.
- A `font-size` term in the kit's vocabulary (`egui-pop-core`,
  `egui-pop-tokens`): the size of a component's text
  (`ComponentKit::font_size`), a kit sizing text with data; the badge
  of the micro-indicators reads it.
- The micro-indicators' LED and flag shapes are glyph roles of the kit
  (`egui-pop-core`, `egui-pop-tokens`): `glyph.led-off`, `led-on`,
  `led-busy`, `led-warning`, `led-error`, `flag-modified`,
  `flag-pinned`, `flag-required` and `flag-invalid`, drawn by default
  with new painted icons (`Icon::Circle`, `Disc`, `Square`, `Triangle`,
  `Asterisk`, `Corner`; `icon::paint_mark`). Two states a kit maps to
  the same icon are reported (`kit::DISTINCT_GLYPHS`,
  `kit::same_glyphs`, `Problem::SameGlyph`) and drawn with egui-pop's
  shapes, so colour never carries a state alone.
- Components based on another (`egui-pop-tokens`, `egui-pop-core`): a
  component's `based-on` term names its base, and when the theme is
  resolved the component receives every token of the base it does not
  give, as an alias of it: states, tones, component terms and the
  `shape`, its own tokens applying over them. A kit restyling the base,
  or an override of it, restyles every component based on it. Chains
  are followed; a cycle (`Error::BasedOnCycle`) or a base no token
  names (`Error::UnknownBase`) is reported, at install and by
  `kit::check`, and the component keeps its own tokens. A handler
  registered for the base under the inherited `shape` draws the
  component too. `BASED_ON` names the term. Guide: docs/kit.md.
- A side area's separator line that egui cannot draw is reported
  (`egui-pop-core`): a `side-area.rest.border-width` other than 0 or the
  button's, or a `border-color` other than `border.default`, is in
  `Kit::problems`, since egui draws the line with its own stroke.
  docs/kit.md lists the bridges to egui that remain and what each loses.
- A guide on how a plugin shares data, state or event
  (docs/state-or-event.md): which of the value store, commands, steps,
  the journal, the context stack, services and alter hooks to use for
  each need, the per-frame trap of immediate mode, and one example done
  both ways.

### Changed

- egui-pop's own checkboxes, radio buttons, selectable labels and combo
  boxes are its kit-drawn widgets (`egui_pop_core::widgets`): the
  settings, dialogs, the file picker, contexts, devtools, the theme
  editor, parameters, the decorated input's list, the library's tag
  suggestions, the assist card's rows, the find bar's toggles (now
  `toggle-chip`s) and the gallery. The default look is egui's, with
  these deliberate changes: a chosen row under the pointer keeps its
  resting outline (egui drew the hover outline over the selection); a
  focused selectable row shows `list-row`'s focus ring inside it rather
  than egui's pressed look; a checkbox's colours follow its state in the
  same pass rather than the previous one; a checkbox in egui's menu bar
  (the gallery's top bar) keeps its box's border, which egui's menu bar
  drops from egui's own widgets. A `Choose` dialog's rows are
  `SelectableRow::steady`. A kit restyling `list-row` now restyles
  selectable rows too (they are based on it).
- One loading rule for theme files (egui-pop-tokens, egui-pop-api,
  egui-pop-i18n; breaking): `Themes::add_text`, the theme editor's
  import and the new `Themes::check_file` load a file over its base
  theme the same way. A token whose value does not load (unreadable, an
  alias to nothing or in a cycle, another type than the base's or the
  contract's) is left out, so the base theme's value stays rather than
  `pop-dark`'s; a token under a contract group the base does not have
  is kept and reported as a likely typo; renamed tokens are read under
  their new names; a file stamped newer than this egui-pop loads
  best-effort. Each is a `Warning` with the token's path and why; only
  a file that cannot be read is an error. `Themes::add_text` returns
  `Result<Vec<Warning>, Error>` (was `Result<(), Error>`);
  `ThemeDraft::import` and `ThemeEditor::import` return warnings instead
  of the paths left out, and no longer refuse a file from a newer
  editor. The `theme.import` command adds `warnings` next to `skipped`,
  and the editor's import message lists them (a string with its
  French). Migration: `themes.add_text(..)?` keeps compiling; to see
  what the resolver does with a broken token, add the parsed file with
  `Themes::add`. Part of #37.
- A theme with a counterpart is installed with it (egui-pop-core,
  breaking in behaviour): `theme::install` and `install_with` resolve
  the spec's counterpart and write each variant into its own egui style
  (`ctx.style_mut_of`), with its own kit; `Tokens::of` and `Kit::of`
  give the ones of `ctx.theme()` at draw time, so `ctx.set_theme` or the
  system's light or dark mode switches tokens, egui's style and the kit
  without installing again. The theme installed is still shown first
  (its mode is set, and is egui's fallback when the system says
  nothing). New: `theme::install_variants`, `theme::Variants`,
  `theme::variants`, `theme::is_paired`, `theme::set_preference` (a
  theme alone keeps its mode), `Tokens::of_theme`, `Tokens::egui_theme`,
  `kit::Kits`, `kit::install_variants`; the debug bridge's `theme`
  collector gives the counterpart and egui's preference. Migration:
  egui's other style was the same theme, it is now the counterpart's;
  install `ThemeSpec::new(..).alone()` to keep one theme in both. A
  cache keeping what it drew from the tokens or the kit across frames
  keys on the kit's address or the colours, not on the install.
- `Request::Rebind` carries the layer changed, and `Request::ResetKeys`
  is a struct with its command and layer (egui-pop-api).
- The window frame's types grow for full screen and zen mode
  (`egui-pop-core`, `egui-pop-api`, breaking): `frame::FrameRequest`
  gains `ExitFullscreen`, `frame::HitRegions` and `frame::FrameLayout`
  gain `fullscreen` (struct literals add it or `..Default::default()`),
  and `workbench::Change` gains `Zen`. A match on them adds the new
  arm. By default the title bar now stays in full screen; set
  `window.bar_in_fullscreen` off for the former behaviour.
- One tone for every mark (`egui-pop-core`, `egui-pop-api`,
  `egui-pop-views`, breaking): `kit::Tone` gains `Record` (recording,
  the danger colour unless a kit tells them apart through
  `component.tone.record.*`), serializes by its token name and reads one
  back (`Tone::from_name`). The status bar's `strips::Tone` and the
  library's badge `library::Tone` are now re-exports of it. To move:
  `Quiet` is `Neutral` (still drawn muted in the status bar), `Active`
  is `Accent`, `Error` is `Danger`; a status item's state serializes the
  new names. `status-indicator` gives the `record` tone. Icons keep
  their own `icon::Tone`, whose `Strong` and `OnAccent` are colours of
  content rather than tones (`IconWidget` maps it with `kit_tone`).
  `spinner`, `progress-ring` and the library's `pill` give the `record`
  tone as well.

- The default kit's components that copied another's values are based
  on it (`egui-pop-tokens`): `command-button` on `button`,
  `command-palette-row` on `menu-item`, `dialog-header` on
  `header-band`, `band-icon-button` and `find-bar-button` on
  `icon-button`, `thumb-overlay-badge` on `pill` and `thumbnail` on
  `thumb-cell`, each giving only what differs. A kit restyling the base
  now restyles them; `pop-high-contrast` no longer repeats
  `command-button.rest.border-color`. A kit that gave one of these
  components a value of its own keeps it. What the default kit draws
  changes in two states no snapshot shows: a band's and the find bar's
  icon buttons are muted when disabled, and the find bar's brighten
  under the pointer, as every icon button does.

- Each crate has its own version and is published only when it changes;
  the `egui-pop` facade keeps one minor per egui minor.
  `cargo xtask publish --dry-run` names the crates changed since the
  commit their version was published from, whose version is still to
  raise. Only the facade's minor releases are tagged.

### Deprecated

- `DRAFT_EXTENSION` and `DRAFT_VERSION` (egui-pop-tokens): the theme
  editor stamps its files with `FORMAT_EXTENSION` now
  (`FormatStamp::CURRENT`); the former stamp is still read on import.

### Removed

- The former token names are no longer read (`egui-pop-tokens`,
  breaking): `egui_pop_tokens::RENAMED`, the step moving their values to
  the kit's names and `Error::Renamed` are gone. They were kept through
  0.1 (GOVERNANCE.md's one-minor window). A theme still giving an old
  name now carries an ordinary token nothing reads, and `kit::check`
  reports it as outside the vocabulary when its component is a kit's.
  The system tokens `size.titlebar.height` and `size.titlebar.button`
  are renamed in the same move, after the components they size (the
  last two rows). To move, rename each token; where a row names several
  tokens, give each of them:

  | Former name | Name now |
  |---|---|
  | `field.background` | `field.rest.fill` |
  | `field.background-read-only` | `field.tone.muted.fill` |
  | `field.text` | `field.rest.text` |
  | `field.placeholder` | `field.tone.muted.text` |
  | `field.border` | `field.rest.border-color` |
  | `field.border-hover` | `field.hover.border-color` |
  | `field.modified` | `field.tone.accent.border-color` |
  | `field.required` | `field.tone.warning.border-color` |
  | `field.invalid` | `field.tone.danger.border-color` |
  | `field.pending` | `field.tone.info.border-color` |
  | `menu.item.background-hover` | `menu-item.hover.fill` |
  | `menu.item.text` | `menu-item.rest.text` |
  | `menu.item.text-muted` | `menu-item.tone.muted.text` |
  | `button.background` | `button.rest.fill` |
  | `button.background-hover` | `button.hover.fill`, `button.selected.fill` |
  | `button.background-active` | `button.active.fill` |
  | `button.text` | `button.rest.text` |
  | `button.border` | `button.rest.border-color` |
  | `button.radius` | `button.rest.radius` |
  | `titlebar.background` | `title-bar.rest.fill` |
  | `titlebar.text` | `title-bar.rest.text`, `window-button.hover.text` |
  | `titlebar.text-muted` | `window-button.rest.text`, `window-close.rest.text` |
  | `titlebar.button-hover` | `window-button.hover.fill` |
  | `titlebar.close-hover` | `window-close.hover.fill` |
  | `titlebar.close-text` | `window-close.hover.text` |
  | `titlebar.height` | `title-bar.height` |
  | `titlebar.button-width` | `window-button.width`, `window-close.width` |
  | `titlebar.grip` | `window.grip` |
  | `progress.track` | `progress-bar.rest.fill`, `progress-line.rest.fill`, `meter.rest.fill`, `progress-ring.rest.fill`, `spinner.rest.fill`, `status-indicator.rest.border-color` |
  | `progress.fill` | `progress-fill.rest.fill`, `progress-fill.tone.accent.fill`, `progress-ring.rest.indicator-color`, `progress-ring.tone.accent.indicator-color`, `spinner.rest.indicator-color`, `spinner.tone.accent.indicator-color` |
  | `progress.height` | `progress-bar.height`, `meter.height` |
  | `progress.line` | `progress-line.height` |
  | `progress.ring` | `progress-ring.width`, `spinner.width` |
  | `dialog.background` | `dialog.rest.fill` |
  | `dialog.border` | `dialog.rest.border-color` |
  | `dialog.radius` | `dialog.rest.radius` |
  | `dialog.padding` | `dialog.padding-x`, `dialog.padding-y` |
  | `dialog.backdrop` | `backdrop.opacity` |
  | `dialog.header` | `header-band.rest.fill` |
  | `dialog.header-text` | `dialog-header.rest.text` |
  | `palette.header` | `header-band.rest.fill` |
  | `palette.header-text` | `header-band.rest.text` |
  | `palette.background` | `palette-window.rest.fill` |
  | `palette.border` | `palette-window.rest.border-color` |
  | `palette.padding` | `palette-window.padding-x`, `palette-window.padding-y` |
  | `toast.background` | `toast.rest.fill` |
  | `toast.border` | `toast.rest.border-color` |
  | `toast.text` | `toast.rest.text` |
  | `toast.padding` | `toast.padding-x`, `toast.padding-y` |
  | `editor.background` | `editor-frame.rest.fill` |
  | `terminal.background` | `terminal-frame.rest.fill` |
  | `slider.track` | `slider.rest.fill`, `slider-toggle.rest.fill` |
  | `slider.fill` | `slider-fill.rest.fill`, `slider-toggle.selected.text` |
  | `slider.thumb` | `slider-thumb.rest.fill` |
  | `skeleton.fill` | `skeleton.rest.fill` |
  | `skeleton.shimmer` | `skeleton-shimmer.rest.fill` |
  | `thumbnail.background` | `thumbnail.rest.fill`, `thumb-cell.rest.fill`, `compare-pane.rest.fill` |
  | `thumbnail.hover` | `thumbnail.rest.border-color`, `thumb-cell.rest.border-color` |
  | `thumbnail.selected` | `thumbnail.selected.border-color`, `thumb-cell.selected.border-color` |
  | `library.selected-background` | `thumb-cell.selected.fill` |
  | `library.overlay` | `pill.rest.fill`, `thumb-overlay-badge.rest.fill` |
  | `library.overlay-text` | `pill.rest.text`, `thumb-overlay-badge.rest.text` |
  | `library.dim` | `thumb-layer.tone.muted.fill` |
  | `library.stack` | `thumb-layer.tone.accent.indicator-color` |
  | `library.best` | `compare-pane.selected.ring-color` |
  | `library.chip` | `tag-chip.rest.fill` |
  | `library.chip-text` | `tag-chip.rest.text` |
  | `font.size.small` | `font.text-style.small` |
  | `font.size.body` | `font.text-style.body`, `font.text-style.button` |
  | `font.size.heading` | `font.text-style.heading` |
  | `font.size.mono` | `font.text-style.monospace` |
  | `size.titlebar.height` | `size.title-bar.height` |
  | `size.titlebar.button` | `size.window-button.width` |

- `Tokens` keeps the system tier only (`egui-pop-core`, breaking): the
  per-widget structs `ButtonTokens`, `SliderTokens`, `FieldTokens`,
  `MenuItemTokens`, `DialogTokens`, `ToastTokens`, `TitlebarTokens` and
  `PaletteTokens`, and the `Tokens` fields holding them, are gone. They
  read kit tokens under names of their own. To move, read the kit
  component (`Kit::of(ctx)`): `tokens.button.background` is
  `kit.button.style(State::REST).fill` (`background_hover` the `HOVER`
  box, `background_active` the `ACTIVE` box, `text`, `border` and
  `radius` the `REST` box's `text`, `border_color` and `radius`);
  `tokens.field.background` is `kit.field.style(State::REST).fill`,
  `background_read_only` and `placeholder` are the `Tone::Muted` box's
  `fill` and `text` (`style_toned`), `border_hover` the `HOVER` box's
  `border_color`, and `modified`, `required`, `invalid` and `pending`
  the `border_color` of the tones `Accent`, `Warning`, `Danger` and
  `Info`; the slider's, menu item's, dialog's, toast's, title bar's and
  palette's are the boxes and terms of `slider`, `slider-fill`,
  `slider-thumb`, `menu-item`, `dialog`, `header-band`,
  `dialog-header`, `backdrop`, `toast`, `title-bar`, `window-button`,
  `window-close` and `palette-window`. A token outside the kit
  (`palette.magnet`, `window.grip`) is read with
  `tokens.resolved().dimension(path)`. `field::color` takes the kit
  instead of the tokens.

## 0.1.1 (2026-10-18)

### Added

- Every published sub-crate has a README on crates.io (what it holds,
  how the `egui-pop` facade re-exports it, links to the guides), and
  every crate links to the repository as its homepage.
- `cargo xtask publish` publishes the workspace's crates in the order of
  their dependencies, skipping versions already on crates.io and waiting
  out its rate limit (`--dry-run`, `--interval SECS`).

### Fixed

- The `egui-pop` page on crates.io shows its images and its links work:
  the README links absolutely, since crates.io resolves relative links
  from the crate's folder; a test keeps every published README so.

## 0.1.0 (2026-10-18)

The first release: the tokens and themes, the framework (plugins,
commands, keymaps, zones, the host), the components of the paved road
and the workbench; the visual kits (every themable component drawn
from data per state and tone) and YAML token files compiled at build
time. On egui 0.36.

### Added

- Devtools (`egui_pop_api::devtools::DevtoolsPlugin`, opt-in): the
  debug bridge's collectors in the application, in a bottom panel behind
  the `developer.devtools` setting (off by default) and `F12`: key
  resolutions, the journal, steps and their hooks, focus and overlays,
  each window's scale, and every collector as JSON with its actions,
  run through the `devtools.call` step a plugin may veto. A core
  `screen` collector gives each window's scale and rectangles.
- Application hooks around the shell: `AppBuilder::main_edge_to_edge`
  (the centre without margins), `AppBuilder::wrap` (draws around the
  whole application, `App::show_wrapped`), `AppBuilder::on_exit` and the
  `app.exit` step whose default saves (`App::exit`, run by eframe's
  `on_exit`), `AppBuilder::run_native_with` (the application's own
  `eframe::App` around egui-pop's), and `window::restore`, the window's
  kept place given to `NativeOptions`.
- The keymap at run time: `AppHandle::declare` and `undeclare` (a
  command declared or removed after build, owned by a named owner, its
  keys, menus and palette entry following with no plugin reconnected);
  `AppHandle::export_keymap(layer)` and `load_keymap(doc, layer)` (and
  `App::load_keymap`), a full keymap document with its scopes, `when`
  clauses and every key, with a `KeymapDiff` (added, removed, changed,
  skipped, conflicts). Steps `keys.rebind` and `keys.load`; the signal
  `keys.conflict` for each new conflict. The shortcuts editor shows
  conflicts by scope, searches by key and removes every key of a
  command (`No key`).
- Arrangements (`egui_pop_api::arrangements::ArrangementsPlugin`, in
  `DefaultPlugins`): named layouts of the workbench saved, switched and
  reset, kept in the host's store (`settings/workbench.arrangements`);
  templates offered by plugins (`Ctx::arrangement`), removed with them;
  a saved layout is repaired when switched to. Steps
  `arrangement.save`, `arrangement.switch`, `arrangement.reset`,
  `arrangement.delete`; commands under View. Gallery page:
  Arrangements.
- Menus: list providers (`Ctx::list_provider`) fill a menu's lists
  (`MenuItem::list`) when it opens, never on every frame, each entry
  (`ListItem`: label, icon, command, arguments, tick) running its
  command; the `lists` collector shows what each gave last.
  `menus::popup_at` opens a declared menu as a popup under a rectangle
  or at a point (a logo, `Alt+Space`), drawn until an item is chosen, a
  click lands outside or Escape. `menus::context_menu_with` merges
  arguments of the moment (the clicked row's path) into every item's
  command and into the `when` context as `args.<key>`. Opening either
  runs the `menu.open` step, which a plugin may veto or redirect.
- Auxiliary windows: a panel detached into a window of its own (its own
  scale, the application's frame when it draws one), docked back, or its
  window closed, through the steps `workbench.detach`,
  `workbench.attach` and `workbench.close_window` (whose default docks
  the panel back; a veto keeps the window open). Its place is kept in the
  layout and repaired (`WindowState::repair`) before the window opens;
  `scale.changed` names the window. `AppHandle::detach_panel`,
  `attach_panel`; the tab menu offers Detach.
- `AppHandle::overlay_rects()`: every overlay on screen with where it
  was drawn (dialogs by their id, other modals, popovers, menus, the
  toast stack, egui's popups), and the same list in the debug bridge's
  `overlay_rects` collector, so an application keeps its hit-testing
  and pictures right without egui-pop's internal ids.
- Slim strips (`egui_pop_api::strips::StripsPlugin`, opt-in): the
  activity bar (an icon per panel of the start side bar in the new
  `ACTIVITY_BAR` zone: it shows its panel, or folds the side bar to its
  strip; `Ctrl+B`), and status bar indicators fed by the host's journal
  (`Indicator`, a reducer over the events of a prefix, `Ctx::indicator`;
  text, tone, count, progress). Areas fold through the `workbench.fold`
  step; `Layout::folded` keeps them. `Icon::by_name`. Gallery page:
  Strips.
- Dialogs: a choice's options carry values of their own
  (`ChooseOption { label, value, hint }`, `Choose::with_options`,
  `value_key`), and the answer is the chosen value; a filter field above
  them narrows the list as one types (fuzzy, on label and hint), arrows
  move through it and Enter takes the highlighted one. An agent answers
  by value (`{"answer": "choose", "value": ...}`). A form's description
  adds its values as a plain map (`values`). The steps `dialog.show` and
  `dialog.answer` let a plugin stop a dialog, stop or change an answer,
  and the journal says `dialog.chosen`, `dialog.cancelled` and
  `dialog.filtered`.
- The workbench (`egui_pop_api::workbench`): panels in areas (centre,
  start and end side bars, bottom) as tabs and splits over `egui_dock`,
  projected from the versioned layout document and read back from it;
  placement rules from each panel's declaration (`only_in`, `min_size`,
  `default_size`, `closed`, `detachable`, `icon`); a tab dragged onto
  another area moves there when its rules allow it, the drop hint says
  why not, a refusal is reported (journal and toast). The layout changes
  through the steps `workbench.move`, `workbench.show`, `workbench.hide`
  and `workbench.focus` (`AppHandle::move_panel`, `focus_panel`,
  `set_panel_visible`, `set_layout`, `layout`); each panel gets
  `<id>.focus` and `<id>.toggle` commands; the layout is saved with the
  settings and repaired at the next start. The shell's side bar, main and
  inspector zones are panels of their areas (`app.side-bar`, `app.main`,
  `app.inspector`), drawn without a tab bar when alone. Gallery page:
  Workbench.
- Test kit: one wgpu device per test process (`SharedGpu::shared_wgpu`,
  `gpu_lock`, feature `wgpu`); the gallery's and the facade's rendering
  tests use it, as parallel device creation crashes the Vulkan loader
  1.4.341 (`docs/testing.md`).
- Scale per window (`egui_pop_core::scale`): `Scale::of` (native scale,
  interface zoom, pixels per point of the window being drawn), `snap`,
  `stroke` and `hairline` for lines that stay whole pixels at 1.25 or
  1.5, and `changed`, the window's scale change once per pass. The theme
  snaps its stroke widths for the scale and is installed again when it
  changes; `scale.changed` is published on the journal. The interface
  zoom (`egui_pop_api::zoom`): a setting in fixed steps (67 % to 200 %),
  commands bound to `Ctrl+Alt+=`, `Ctrl+Alt+-` and `Ctrl+Alt+0`, the
  `interface.zoom` step; egui's own zoom keys are turned off. Test kit:
  `SCALES`, `app_at`, `set_native_scale`, `at_scales`, `rects_of`.
  Gallery page: Scale and zoom, and an overview snapshot at 2x.
- Undo and redo through the host: `Host::undo`, `Host::redo`,
  `can_undo`, `can_redo`, `undo_label` and `redo_label` (by default the
  store's). The `edit.undo` and `edit.redo` commands (`Ctrl+Z`,
  `Ctrl+Shift+Z`, `Ctrl+Y`) run the steps `edit.undo` and `edit.redo`,
  which plugins can veto, redirect, replace or follow; their `when`
  checks the new `canUndo` and `canRedo` context keys, their menu items
  name the step ("Undo drag exposure"), and the journal says
  `edit.undone`, `edit.redone`, `edit.undo_failed` or
  `edit.redo_failed`. A `history` debug collector shows what they would
  do.
- The window's own frame: `egui_pop_core::frame` draws a title bar
  (caption to drag, double click to maximize, minimize, maximize and
  close buttons, room for leading and trailing items) and resize edges
  never thinner than 6 physical pixels, and publishes each window's hit
  regions in physical pixels (`HitTable`, `HitRegions::hit`) for a
  platform layer answering the system's hit tests (Windows Snap
  Layouts). `egui_pop_api::frame::FramePlugin` (opt-in) puts it in the
  new `TITLE` zone above the menu bar, with a system menu (`Alt+Space`,
  `MENU_SYSTEM`), the `CAPTION` zone for the application's items, a
  `window.frame` setting (own or system), the steps `frame.minimize`,
  `frame.maximize`, `frame.close` and `frame.system_menu`, and a `frame`
  debug collector. Titlebar tokens: `size.titlebar.{height, button}`,
  `size.frame.grip`, `titlebar.*`. Gallery page: Window frame.
- Steps (ADR 0011, `egui_pop_api::steps`): a feature's moments
  (`frame.close`, `workbench.detach`...) declared with `Ctx::step` and
  run with `AppHandle::step`; plugins observe them on the journal, veto
  or redirect them (`Ctx::before`), replace them (`Ctx::replace`) or
  follow them (`Ctx::after`). The debug bridge's `steps` collector shows
  each step's hooks and the last runs; hooks on unknown steps and two
  replacements of one step are reported.
- `egui-pop-viewport` (facade feature `viewport`), a generic image
  viewport, first its view model: the zoom counts physical screen pixels
  per image pixel, so 100 % is exactly `1.0` in every window whatever
  its scale; `Mapping` is the one image-to-screen mapping (origin snapped
  to whole physical pixels) that drawing, hit tests and the pixel readout
  share; the zoom ladder (whole ratios first), fit, and a wheel that
  stops at 100 % when it crosses it. Drag, Space-drag or middle-drag
  pans as one gesture, Escape puts the view back, a double click fits,
  keys walk the ladder; the gestures are data (`gestures::Gestures`).
  Every change is a `Signal` with its phase and cause; `before_view`,
  `on_double_click` and `after_paint` hooks let the application veto,
  redirect, replace or extend, and the `viewport` debug collector shows
  views, hook decisions and signals and moves a viewport. Component
  tokens `viewport.*`. A Viewport page in the gallery.
- Viewport: the display-transform hook. `display::DisplayTarget` says
  what a window's surface expects (primaries, transfer, SDR white,
  headroom, surface format, where the description comes from), per
  window (`display::set_target`); only sRGB is drawn faithfully for now,
  other targets are drawn as sRGB, and the type is ready for wide gamut
  and HDR. A `DisplayTransform` (the application's colour management,
  `LinearSrgb` or `Matrix` built in) turns a target into a `Stage`
  (matrix, clamp, transfer) the GPU path runs in its shader and the CPU
  path runs with the same arithmetic. A new target is a
  `Signal::Display`.
- Viewport: tiles from an asynchronous, live `source::TileSource` the
  application implements. The viewport asks for a proxy of the whole
  image (a power of two at or below fit) and, once the view has settled,
  the exact tiles of what is visible at the view's scale (one texel per
  physical pixel below 100 %); requests no longer wanted are cancelled
  through their token, late tiles dropped; the source delivers through a
  `TileSink` from any thread and announces changed regions, whose tiles
  stay on screen until replaced. A texture cache with a budget (never
  evicting what is drawn), tiles within the device's largest texture
  side. With `gpu::install` (feature `wgpu`, on by default) scene tiles
  are `Rgba16Float` textures (`Rgba32Float` where half floats cannot be
  sampled) drawn by a paint callback whose shader runs the display
  transform's stage; without it, the CPU path. `Signal::Tiles`, the
  counters in `Stats` and the debug collector show what moved.
  `demo::Procedural` (worker threads, `wait_idle`) feeds the gallery's
  Viewport page; `egui_pop_test::viewport::TestSource` (feature
  `viewport`) serves tests without threads or sleeps. Tests check that
  at 100 % every physical pixel shows its image pixel, on both paths.
- Viewport: overlays in image coordinates with one handle kit
  (`overlay::Overlay`, `Shape`: point, line, box, circle, path; handles
  move, point, corner, edge, size, rotate, feather, drawn small with a
  24-point hit target). A drag on a handle is one gesture (`Handle`
  signals: begin, update, end, or cancel on Escape) and never pans;
  Space-drag still pans over a handle; a double click on a handle asks
  for its reset and the wheel over it adjusts it, never the zoom; the
  `before_handle` hook vetoes or snaps a move; `Overlay::apply` follows
  the signals for the standard shapes. Before and after
  (`Viewport::before`, `Compare::Split` with a draggable divider,
  `Compare::Before`, and the gestures' `before` key held as a
  quasimode, through the `before_compare` hook). A pixel readout fed by
  the source (`TileSource::readout`, answered through the sink, in the
  output and as a `Readout` signal, drawn in a corner with
  `Viewport::readout`, its text replaceable with the `on_readout` hook).
- Image widgets in `egui-pop-viewport`: `thumbnail::Thumbnail` (an
  image supplied as a texture, fitted with its aspect, or a flat
  placeholder in its average colour until it is there; selected, hover
  and focus rings; named and selectable for screen readers) and
  `thumbnail_grid`, virtualised (only the rows in view are built);
  `histogram::Histogram`, filled curves from bins the application
  computed, linear or logarithmic. Component tokens `thumbnail.*` and
  `histogram.*`; an Image widgets page in the gallery.
- `egui-pop-viewport`'s `viewer` example: an application implementing
  `TileSource` (a Mandelbrot set computed on a thread, scene pixels,
  the GPU path on eframe's wgpu device); the README shows a source in a
  few lines, and a headless screenshot of the gallery's Viewport page.
- Twelve editor-inspired built-in themes (`github-light`, `github-dark`,
  `github-light-high-contrast`, `github-dark-high-contrast`,
  `catppuccin-latte`, `catppuccin-mocha`, `one-light`, `one-dark-pro`,
  `solarized-light`, `solarized-dark`, `gruvbox-light`, `gruvbox-dark`),
  extending the `pop-*` themes with the original editors' workbench
  colours, moved in OKLCH lightness only where a contrast pair required
  it; every one passes the contrast pairs in every density (7:1 for the
  high-contrast pair) and with every accent. A theme may bring hue scales
  of its own (Catppuccin's `purple`), offered as accents. `theme_label`
  gives people's names ("GitHub-inspired Dark"); `POP_THEMES` lists
  egui-pop's own. Sources and licences are in `NOTICE` (also shipped in
  `egui-pop-tokens`). The gallery's picker lists them by label.
- `docs/stability.md`: what a release promises, in three tiers (stable,
  supported, experimental); CI packages every published crate (a dry
  run of `cargo publish`). A workflow builds the gallery with Trunk and
  deploys it to GitHub Pages (by hand until the repository enables it).
- An application template for `cargo generate` (`template/`) and the
  `minimal` example it is made from (`cargo run -p egui-pop --example
  minimal`): one plugin with a panel, a setting and a command over
  `DefaultPlugins`; a test runs the example headless and keeps the
  template identical to it. `egui_pop::eframe` re-exports the eframe
  `run_native` uses.
- Test kit: `harness_for(plugin)` runs a plugin in an application with
  the default plugins; `keymap_problems` and `assert_keymap_clean` find
  keys two commands claim in one scope and bindings running a command
  nobody declares. The gallery's keymap is checked.
- The window's place as a plugin (`egui_pop_api::window::WindowPlugin`,
  in `DefaultPlugins`): size, position, maximized and full screen kept in
  the store (`settings/window.*`, saved with the settings, no undo step;
  `Set::untracked`) and restored by `AppBuilder::run_native` after a
  repair (`WindowState::repair`: back onto a screen, shrunk to it and
  under 8192 points). Full screen (`F11`, View menu) shows a strip with
  the title and "Exit full screen" when the pointer reaches the top edge.
- The settings window as a plugin
  (`egui_pop_api::settings_window::SettingsPlugin`, in `DefaultPlugins`;
  `Ctrl+,` or File > Settings): a search over a list of sections on one
  side, the form on the other (each setting's editor bound to the store,
  applied live, with its reset and description), and a shortcuts
  editor: every command with its keys, changed by pressing the new ones
  (the dispatcher stands aside meanwhile; Backspace: no key, Escape:
  cancel), the conflicts said, each change taken back by its reset and
  kept in the user's keymap. `Keymap::rebind`, `reset` and `rebound`;
  `AppHandle::rebind`, `reset_keys`, `key_conflicts`, `capture_keys`;
  `OverlaySpec::width` for a wider modal.
- A darktable-style slider (`egui_pop_core::slider::Slider`): the label
  and the value inside the bar, a soft range the bar spans and a hard
  range a typed value may reach, drag (Shift: finer), arrows when
  focused (Shift: coarse, Ctrl: finer), the wheel only when focused,
  Enter to type a value with its unit (a comma is a point), a double
  click to reset; built from a value descriptor with
  `Slider::from_descriptor`. A parameter widget
  (`egui_pop_core::param::param`) draws the editor a descriptor calls
  for (slider, check box, drop-down, colour swatch, text) in the three
  binding modes (`Mode::Plain`, `Bound`, `Watch`); `binding::Choice`
  and `Vec<f64>` bind choices and colours. A gallery page shows them.
- Toasts as a plugin (`egui_pop_api::toasts::ToastsPlugin`, in
  `DefaultPlugins`): notices in the bottom-right corner that never take
  the focus, by level (information and success go after 4 s, warnings
  after 8 s, errors stay; the pointer over one holds it), with an
  optional command action and a close button, four on screen and the
  rest waiting. Screen readers hear them through the live region. The
  queue and the history are data (`Toasts::shown`, `history`), shown and
  dismissed through the debug bridge's `toasts` collector; each one is a
  `toast.shown` event in the journal. A gallery page shows them.
- Dialogs as a plugin (`egui_pop_api::dialogs::DialogsPlugin`, in
  `DefaultPlugins`): `Confirm` (named by its verb; `danger` styles it,
  starts the focus on Cancel and keeps Enter from accepting), `Form`
  (fields from value descriptors), `Choose` and `Info`, queued and shown
  one at a time on egui's `Modal`. Accepting runs the dialog's command
  with its arguments, the form's values or the choice merged in. A path
  field has a Browse button: the system's chooser with the
  `native-dialogs` feature (rfd), else an in-app picker of folders and
  accepted files. The `dialogs` collector of the debug bridge shows the
  open dialog and answers it as an agent; answers go to the journal.
  `Ctx::collector` adds a plugin's own collector, removed with it. A
  gallery page opens each kind.
- A host contract (`egui_pop_api::host`, ADR 0010): the application's
  kernel runs the commands (`Host::dispatch`, a result now or later from
  any thread), keeps an event journal (sequence, source, cause; read
  since a sequence number, and by the debug bridge's `journal`
  collector), holds the store and wakes the interface from any thread.
  `LocalHost` is the default and runs in-process handlers as before;
  `AppBuilder::host` takes another, `Ctx::declare` declares a command it
  runs. A command waiting for its result greys its menu items; a failed
  one says so on hover. `AppHandle::running`, `events_since`,
  `publish`; `App::settle` takes the results that arrived.
- The command palette is a plugin (`egui_pop_api::palette::PalettePlugin`)
  with fuzzy ranking (`egui_pop_model::fuzzy`: word starts, runs,
  matched characters highlighted), the commands run lately first, and
  prompts for a command's arguments (from its `args` example, the
  example's value by default); Escape steps back from a prompt, then
  closes. Plugin groups `DefaultPlugins` (the palette, and the next
  components as they come) and `MinimalPlugins` go to
  `AppBuilder::plugins`; a new `app.overlays` zone is drawn after the
  shell for blocks showing overlays. `focus::escape_passes` lets Escape
  through a search field that is a whole popup.
- Keyboard focus that follows the system's focus mode
  (`egui_pop_core::focus_mode`): in hover mode the surface under the
  moving pointer gets the keys (a widget focused elsewhere lets go) and
  text fields that call `focus_mode::follow_pointer` take the keyboard;
  never while a button is held, a drag runs or an overlay is open. The
  mode is read once from the desktop by `run_native` (GNOME
  `focus-mode`, KDE `FocusPolicy`, Windows active window tracking;
  click elsewhere), `EGUI_POP_FOCUS_MODE` (`click`, `hover`) stands in
  for it, and the user overrides it with the `appearance.focus` setting
  (`system`, `click`, `hover`, in the Appearance section). The
  `FOCUS_MODE` service and the debug bridge's `focus` collector report
  the mode and where it was read; `AppBuilder::system_focus` sets it.
- Escape leaves a text input first: with the focus plugin
  (`egui_pop_core::focus::install`, installed by the application), the
  first Escape in a text field drops its focus and goes no further, and
  the focus stays in the field's surface, so the second Escape reaches
  that surface's own bindings or closes the overlay around it. The order
  of Escape handlers (text input, drag, top overlay, key bindings from
  the innermost surface out, widgets) is documented in `focus`.
- Key bindings have a scope: `global`, or `surface:<name>` (a focus
  scope of that name, which must hold the focus, shown with a focus ring
  or not). The binding of the innermost focused surface wins over an
  outer surface's, which wins over a global one. Each binding says
  whether a text input keeps its key (`Typing`: `auto`, `always`,
  `unless-typing`; `auto` leaves plain keys to the text input and
  fires shortcuts with Ctrl, Alt or Meta and function keys), and a modal
  blocks global bindings unless they go `through_modal`. Traces name the
  winning scope and say why a binding did not fire (surface not
  focused, left to typing, blocked by a modal). `CommandSpec::key_with`,
  `Ctx::keybinding_with` and `Keymap::bind_with` take the options;
  saved keymaps keep them.
- Menus drawn from their declarations with egui's own menus
  (`egui_pop_api::menus`): `bar` (the shell's menu bar), `show` (a
  menu's items in place) and `context_menu` (a declared menu on a
  secondary click, evaluated and run in the context of the moment it
  opened). Items show the command's title (or their own label), its
  first live key, a painted tick when their `checked` clause holds (and
  a check-box role for screen readers), and are greyed with the reason
  where the command does not apply; submenus open on hover with a
  painted chevron. `CommandSpec::checked` ticks a command's menu
  entries; `MenuItem::label` names an item running a command with
  arguments. A gallery page shows the menu bar and a context menu.
- Icons by role (`egui_pop_core::icon::Icon`: check, chevrons, close,
  search, settings, folder, warning, undo... 39 in all), never by
  codepoint. With the `phosphor` feature (on by default in `egui-pop`)
  they are drawn from the Phosphor icon font through `egui-phosphor`,
  installed with the theme in a font family of its own; without it they
  are painted with lines and fills. A gallery page shows them all, in a
  size and a tone, next to their painted fallbacks.
- `egui-pop-model` (no egui): typed ids serialized as strings and checked
  at compile time for literals; diagnostics that log and collect what
  registration ignored or changed; owner-tagged registries removable by
  owner; `when` clauses with focus depth; commands declared once (keys,
  menus, palette, agent surface) with a bounded run history and a ranked
  palette search; a layered keymap with chords, focus-depth precedence,
  command removal (`"-id"`), key unbinding (`null`) and a resolution
  trace; declared menus and panels; layouts and their repair; a value
  store (descriptors, soft and hard ranges, steps, gestures, `if_seq`,
  undo, change listeners, a journal, settings export and import with
  values kept for paths declared later) and its `value.*` commands;
  versioned documents with migrations (`layout` v2 renames the areas to
  start and end); plugin ordering with typed errors.
- `egui-pop-core` (egui): focus scopes and a focus chain rebuilt each
  pass, giving context keys their depth; a key dispatcher (chords with a
  timeout, keys consumed when they resolve, the last traces kept); an
  overlay stack (Escape closes the top only, outside clicks stop at a
  modal, closing closes what is above, focus restored) and overlays drawn
  as modals or placed popovers; placement with flip, shift and shrink;
  accessible names and an announcer with live regions; a `Strings`
  provider with English fallbacks; icons by role and tone (glyphs,
  images, painted shapes); a persistence driver (atomic writes, one
  backup, migrations on load); fault boundaries showing an error card
  with Retry; plain, bound and watch-only value binding.
- `egui-pop-api` (its own semver): plugins with requirements,
  priority, `enabled`, `connect` and `disconnect`, sorted at start-up
  with typed errors (missing, cycle, duplicate, failed connection), whose
  registrations are all removed when they disconnect (dependents first);
  one `Ctx` facade (`command`, `keybinding`, `menu_item`, `panel`,
  `zone`, `block`, `setting`, `service`, `on`, `on_change`, `hook`,
  `story`); a command declared once yields its key bindings, menu
  entries, palette entry and agent surface; zones with blocks added,
  replaced, extended or wrapped, resolved at build time with diagnostics
  for what cannot apply; alter hooks over menus, the palette and settings
  ordered by priority and `after`; lifecycle events; typed service
  tokens; gallery stories; an `AppBuilder` over eframe or headless, with
  a shell (menu bar, zones, panels in fault boundaries and focus scopes,
  a command palette), the theme as a service, a settings form applied
  live with per-field reset, settings and the user's keymap persisted.
- A debug bridge (`egui-pop-core::debug`): collectors describe
  themselves, snapshot as JSON and offer actions flagged `mutates`;
  every call returns a feedback block. Core collectors: focus, overlays,
  faults, announcements, theme, store; the app adds commands (with
  `run`), keys (the last resolution traces), zones, plugins (with
  `set_enabled`) and diagnostics.
- `egui-pop-test`: harnesses for an app or a themed `Ui`, a check that
  every interactive node has a name, an accessibility tree dump, and a
  double click that registers.
- The gallery is a plugin application: a shell plugin (page list, page,
  playground, theme switchers, next and previous page on Alt+Down and
  Alt+Up, next theme on Ctrl+Shift+T) and one plugin per page. New
  pages: zones and blocks (one plugin replacing, extending and wrapping
  another's blocks, switched with Ctrl+E), settings applied live with
  per-field reset, and the debug bridge (collectors, key traces, a
  popover, an announcement, a panel that panics). `--dump-debug` prints
  the bridge's snapshot when quitting. Every page is checked for unnamed
  interactive nodes.
- `consumer-smoke` (not published) runs the README's code as doctests
  against the `egui-pop` crate alone.
- `egui-pop` re-exports `model`, `core` and `api`, with a `prelude`.
- A layering test reading `cargo metadata`: dependencies between
  workspace crates point down the layers of ADR 0001.
- `egui-pop-tokens`: a DTCG 2025.10 token parser and resolver with no egui
  dependency: groups, inherited `$type`, `$description` and `$extensions`
  kept, whole-value aliases, `$extends` between theme files, colours in
  sRGB, OKLCH and OKLab. Alias and `$extends` cycles, unknown aliases,
  wrong types and missing contract tokens are reported and fall back to
  `pop-dark`; resolving never panics.
- Three tiers of tokens (palette, system roles, component tokens) and a
  contract of the system and component tokens every theme provides.
- Built-in themes `pop-dark`, `pop-light` and `pop-high-contrast`;
  densities `comfortable` and `compact`; accents `gold`, `blue`, `green`,
  `amber` and `red`; reduced motion; per-token overrides, composed by a
  `ThemeSpec`.
- WCAG 2.x contrast checks over an explicit list of token pairs, holding
  every built-in theme, density and accent to 4.5:1 for text and 3:1 for
  interface parts (7:1 for text in high contrast).
- `egui-pop`: `theme::install` turns resolved tokens into egui's `Visuals`
  and `Style` and stores a typed `Tokens` set in the context
  (`Tokens::of`); `egui` and `egui-pop-tokens` re-exported.
- `egui-pop-gallery` (not published): a catalogue application with theme,
  density, accent and motion switchers, pages for the theme overview, the
  token table and egui's built-in widgets, and options for scripted runs;
  it builds for the web too.
- Snapshot tests of the gallery per theme (egui_kittest on wgpu), a wbox
  workspace to run the gallery offscreen, and CI: format, clippy, tests on
  lavapipe, cargo-deny, the web build and semver checks.

- Kits: placeholders (`skeleton`, `skeleton-shimmer`, `empty-state`): a
  skeleton's shapes and the band crossing them are drawn by the kit; an
  empty state's box (none in the default kit), its `padding-y`, and its
  icon and sentence colour (`text`, the `danger` tone when something
  went wrong) come from it. The default look is unchanged.
- Kits: icon buttons (`icon-button`): an `IconWidget` made clickable is
  drawn by the kit, its icon in the box's `text` per state (hover,
  focus ring, disabled) and by tone (the icon's tone, except `Strong`
  and `OnAccent`, which keep their colour at rest).
  `IconWidget::selected` makes it a toggle, `selected` while on and
  announced as toggled. The focus ring follows the box's corners (one
  point rounder than before).
- Kits: sliders (`slider`, `slider-fill`, `slider-thumb`,
  `slider-value`, `slider-toggle`): the bar, its fill strip (`height`,
  15 % of the bar by default), its thumbs, its value (the `warning` tone
  past the soft range) and the square button at its end are drawn by the
  kit, per state (hover, active, focus; `selected` for a toggle that is
  on), range sliders included; the gradient tracks stay content. The
  default look is unchanged, except that a focused slider's fill and
  thumb now stay whole over its ring.
- The shipped token files are compiled from YAML to JSON when egui-pop is
  built: the built-in themes load in 3 ms instead of 24 (release build),
  and an unquoted `#colour` or `{alias}` in them fails the build. A
  component's token file is found by the build; `KIT_FILES` is no longer
  kept by hand.
- Kits: the application's chrome is drawn by the kit, with tokens per
  state: `header-band` (a movable dialog's and a palette's band, `hover`
  under the pointer and `active` while dragged, which a kit may now
  show), `dialog-header` (a dialog's title), `band-icon-button` (the
  band's buttons, `selected` for a pinned palette, with a focus ring),
  `find-bar-button`, `activity-bar-item` (now with a focus ring),
  `dock-preview` (`disabled` where a panel may not dock),
  `command-button` and `command-palette-row`. The close buttons draw
  the `close` glyph role. The gallery's `palettes-kit-band` snapshot
  restyles the bands with data only.
- Tokens: a colour token aliasing another may carry
  `$extensions: {org.egui-pop.alpha: 0.85}`, the aliased colour with its
  alpha multiplied (`docs/tokens.md`); a value outside 0 to 1, or on a
  token that is not a colour, is a reported problem.
- Kits: the photo library and the viewport's widgets are drawn by the kit
  (`thumb-cell`, `thumb-layer`, `thumb-overlay-badge`, `pill`, `tag-chip`,
  `mark-toggle`, `search-field`, `compare-pane`, `thumbnail`, `readout`,
  `viewport-focus`; a `pill` handler with round ends); the default look
  is unchanged. The library's search field shows a focus ring while its
  text has the keyboard, and a focused thumbnail's ring is no longer
  clipped away.
- Workbench: a side area may lie **over** the centre instead of beside
  it (`workbench.place`, `{ "area", "placement" }`, a step a plugin may
  veto; `AppHandle::place_area`, `area_placement`, `AppBuilder::over`):
  its folded strip keeps the room, so the centre keeps its width, and
  the area lies above the centre with the kit's `side-area` shadow; a
  click on the strip folds or unfolds it, its inner edge resizes it.
  The placement is saved with the layout (`Layout::over`,
  `layout::Placement`).

- Shadows from tokens: `shadow.popup`, `shadow.window` and `shadow.side`
  (`offset-x`, `offset-y`, `blur`, `spread`, `color`) become egui's
  popup and window shadows (menus, popovers, dialogs, palettes) with
  egui's values by default; kit components cast one through the
  `shadow-*` terms (`ComponentKit::shadow`), `side-area` aliasing
  `shadow.side`; `theme::shadow` reads a group as egui's `Shadow`.

- Typography: a size scale in tokens, `font.size.xs`, `s`, `m`, `l`
  and `xl`, that egui's five text styles sit on
  (`font.text-style.small`, `body`, `button`, `heading`, `monospace`,
  aliases of a step) and component tokens may alias; built-in themes
  keep their sizes. `fonts::font_at(tokens, role, weight, step)` gives
  the `FontId` of a step (`fonts::Step`), `Tokens::font.scale` its sizes.

- Kits: the window buttons' glyphs are glyph roles (`glyph.window-minimize`,
  `window-maximize`, `window-restore`, `window-close`), so a kit can draw
  them with its own icons, `indicator-size` across.
- Kits: `window.rest.border-width` and `border-color`, a line around a
  window the application draws, inside its edges (none in the default
  kit; not drawn when maximized or full screen).
- Token and kit files may be written in YAML, for comments
  (`docs/tokens.md`): `TokenFile::parse` reads JSON when the text starts
  with `{` and YAML otherwise. An unquoted `#colour` (a YAML comment) or
  `{alias}` (a YAML map) is refused with its line instead of being lost.
  JSON is still read, and is what egui-pop writes.
- Visual kits, first component (`egui_pop_core::kit`, `docs/kit.md`): the
  workbench's tabs and tab bars are drawn by the kit, from `tab.*` and
  `tab-bar.*` tokens per state (`rest`, `hover`, `active`, `focus`,
  `selected`, `disabled`) and a `shape` naming the handler that draws them
  (`box`, the default, or `underline`; applications register their own
  with `kit::register_tab_shape`). The kit is compiled when the theme is
  installed: drawing resolves nothing. egui_dock keeps the layout and
  docking. Tabs are prepared once (title translated, label laid out, width
  measured) and again only when the theme, the scale, the language or a
  panel's title changes: a frame of the workbench costs no more than with
  egui_dock's own tabs (measured, `docs/kit.md`).
- The title bar and window buttons are kit components (`title-bar`,
  `window-button`, `window-close`); `title-bar.align: start` puts the
  title after the bar's leading items instead of centring it.
- `AppBuilder::shell_panel_size` (the side bar's and inspector's
  starting size) and `AppBuilder::folded` (areas folded at the first
  start; a saved layout wins); `AppHandle::area_rect`.
- Kits: a `side-area` component (`border-width` 0: no separator line
  along the area; `padding-x` and `padding-y` between its edges and its
  panels) and a vocabulary term `offset` (how far a part sits
  from the edge it is drawn on: a side area's grip, off its edge).
- Side areas: a folded side area shows a strip (the kit's `area-strip`,
  12 px, the drag-handle glyph) whose click unfolds it, instead of
  vanishing; an open side area can show a resize grip at the middle of
  its edge (`area-grip`, invisible in the default kit). New glyph role
  `drag-handle` and icon `grip`.
- Window buttons per platform flavour: `title-bar.buttons-side`
  (`start`/`end`) puts them at the start of the title bar, close first,
  the title still centred, Snap Layouts following them; a
  `traffic-light` handler and `title-bar.buttons-hover: group`; window
  buttons carry tones (minimize `warning`, maximize `success`, close
  `danger`). An example data-only kit, `examples/kits/flavour-macos`.
- On Windows 11, a window drawn by the application gets the system's
  rounded corners, chosen by `window.corners` (`round`, `small`,
  `square`); the outcome is in the `frame` debug collector.
- Kits: `button` and `field`; egui's own widgets (every egui and
  third-party widget) take their states from the kit's `button` (rest,
  hover, active, selected for an open menu) and text fields their
  background from `field` (`theme::style_with`). What egui cannot draw
  (an indicator or a ring on a button, a field outline other than the
  button's) is reported in the kit's problems.
- Kits: menu items (`menu-item`): egui lays them out and draws their
  label, the kit paints their box under it (`Component::paint_under`);
  the shortcut is the muted tone. The look is unchanged.
- Kits: dialogs, popovers and menus, toasts, palettes, tip, fault and
  job-log cards, the full screen strip, the editor, terminal, find bar
  and code assist frames are kit components (`dialog.rest.fill`,
  `toast.padding-x`...), drawn by a `card` handler or, through
  `Component::frame`/`show`, as egui frames; a text or code editor shows
  a focus ring while it has the keyboard. New terms `padding-y` and
  `opacity`, new system token `scrim`.
- Kits: tones, a second axis beside the states
  (`component.tone.<tone>.<term>`: accent, info, success, warning,
  danger, muted), compiled ahead per state; a component without tones
  pays nothing. Progress bars, lines, meters, rings, spinners and the
  status bar's indicators are drawn by the kit by tone (`progress-bar`,
  `progress-line`, `progress-fill`, `progress-caption`, `meter`,
  `meter-tick`, `progress-ring`, `spinner`, `status-indicator`).
- Kits as packages: a kit folder (`kit.json` with a manifest, more token
  files, `fonts/`, `LICENSE`) read at run time
  (`egui_pop_tokens::kit::KitData::load`) or embedded (`kit!`), its
  problems reported and never fatal; `kit::check` for application tests
  and a `kit-check` example. `AppBuilder::kit` adds a kit (theme, faces,
  handlers) in one line, `KitSource` carries a kit's handlers,
  `AppBuilder::user_kits` reads `<persist>/kits` and `ThemeService::kits`
  lists them. The default kit (`themes/kit/`) is a kit folder read by
  the same loader; an example kit, `examples/kits/underline-tabs`.
- Tables: the sort direction is the kit's glyph after the column title.
- Kits: the rows of trees, lists and tables (`list-row`), a focused
  view's outline (`list-view`), a table's header cells
  (`table-header-cell`) and stripes (`table-stripe`) and the tree's drop
  marker (`tree-drop-marker`) are drawn by the kit, from the theme; the
  default look is unchanged. Two handlers: `box` (a free-standing box)
  and `outline` (a border only).
- Kits: glyphs by role (`kit::Glyph`: disclosure, sort, check, radio,
  bullet, submenu, chevrons, close, more, expand, collapse), each mapped
  by the kit to an icon (`glyph.<role>` tokens). The tree's disclosure,
  the menus' check and submenu marks and the find bar's toggle ask for a
  role. New painted icons `triangle-right`, `triangle-down`,
  `triangle-up`.
- An application's own icons (`egui_pop::core::icon_set`, feature
  `svg`): SVG files registered by name, single-colour (tinted) or
  full-colour, rasterised with resvg when the theme is installed, drawn
  as textures; usable as `Icon::Custom` and by a kit's glyphs.
- Kit vocabulary: the focus ring as data (`ring-color`, `ring-width`,
  `ring-offset`, drawn by every shipped handler) and
  `kit::State::from_response`, the state egui sees on a response.
- Renamed tokens: a theme giving a former name still works, its value
  moved to the new name, with a reported problem (`RENAMED`). New system
  token `transparent`.

- Fonts from the theme (`egui_pop_core::fonts`): the families named by
  `font.family.ui` and `font.family.mono` become egui's fonts, at four
  weights each (`fonts::family(role, weight)`, `fonts::font_id`), egui's
  `Proportional` and `Monospace` being the regular ones. Names are looked
  for among faces the application registers (`fonts::register`), egui's
  bundled faces, and the system's fonts (feature `system-fonts`, through
  fontdb); a name found nowhere or a missing weight is in
  `fonts::report`, and egui's bundled fonts stay last in every chain.
  A variable face draws every weight its `wght` axis covers, instead of
  its one registered weight.
- `AppBuilder::menu_bar(false)`: an application without a menu bar
  (its menus stay in the palette, under their keys and in menus it
  opens itself); the top bar's zone is then drawn only when it has
  blocks.
- A preview dialog (`imaging::preview`): an image viewport in a custom
  dialog over a `Clear` backdrop, movable and kept in place, sRGB
  first; Enter runs its command with the view it was left at, Escape
  cancels it except during a drag of its view. The interface behind,
  its own viewport included, keeps receiving and drawing tiles while
  the dialog is modal. Gallery page: `preview`.

- A custom dialog (`Dialog::Custom`, opened with
  `Dialogs::open_custom`): a body of its own, drawn by a closure kept
  on the application's side, in a size of its own, a `dialog` focus
  scope and a fault boundary, every frame. It keeps the dialog rules
  (a primary button saying what it does, Cancel, Enter and Escape,
  steps, the debug bridge, movable, closable, a kept place, the
  backdrop); keys its body claims go to it first (Ctrl+Enter accepts
  when it claims Enter; Escape goes to it while it has something to
  cancel). A body declaring its needs is checked against the custom
  dialog's offer (`dialogs::custom_host`) and recorded as a mount.
  Guide: docs/widgets.md, section 6.

- Every component declares what it needs (`mounts` in
  `egui-pop-viewport`, `egui-pop-views` and `egui-pop-editors`: the
  image viewport, navigator, node graph, curves, wheels, histogram,
  thumbnail, library grid, filmstrip, compare view, tree, table, text
  and code editors, terminal), and the viewport's overlays declare
  their offer (data, no `Ui`). The facade gathers them
  (`egui_pop::mounts::components`, `hosts`, `page`), and
  `egui_pop::mounts::MountsPlugin` declares them to an application.

- The mount contract in the application (`api::mounts`): the offers
  of the hosts `egui-pop-api` builds (docked panel, floating palette,
  auxiliary window, the four-kind dialog, raw modal, popover, line
  zone, status bar, gallery story) and the needs of its components and
  `egui-pop-core`'s (sliders, forms, progress, the jobs centre, the
  theme editor, tips). `AppHandle::where_can` gives every host's
  verdict for a component, `AppHandle::mount` records a mount with its
  conditions or refuses it with its rule (reported as `mount-refused`),
  `Ctx::mountable` and `Ctx::host_offer` declare a plugin's own, and
  the debug bridge's `mounts` collector shows them and answers
  `where_can` and `check`.

- The mount contract (ADR 0013) in `egui-pop-model` (`model::mount`):
  a component declares what it needs (`Mountable`: keyboard and claimed
  keys, pointer, workers, display, size, instances, view state, the
  application, a question, a session, popups), a host what it offers
  (`HostOffer`: size, focus policy and owned keys, what dismisses it,
  lifetime, window, keymap reach, projection, what it keeps and gives,
  display), and `check` matches them with eleven named rules: it fits,
  fits under named conditions, or does not fit with a reason by design,
  an egui-pop limit or an upstream limit, each written once (`CODES`).
  `mount::page::render` writes `docs/hosts.md` from the declarations.

- The context stack (ADR 0012, guide: docs/contexts.md): places
  (drill-downs, shown as a breadcrumb) and modes (global states, one
  per group, set or pushed, shown as chips) on one stack owned by the
  application's kernel. `egui_pop_model::context`: frames
  (`FrameSpec`, guards, transient frames), the state with its
  revision, the pure `apply` and `restore`, the `context` and `view`
  documents. `egui_pop_api::contexts`: the `Contexts` contract
  (`Host::contexts`), `LocalContexts` by default (the stack written
  inside each transition), `ContextsPlugin`; every transition is the
  `context.request` step, the owner publishes `context.changed`,
  `context.entered`, `context.left`, `context.refused`,
  `context.restored`, `context.held`, `context.released`. The
  projection keeps a layout, the focus and view values per place and
  mode (`view_value`, `scroll_area`), and sets the context keys
  `place`, `placeDepth`, `modePushed` and `mode.<group>`;
  `Ctx::frame` and `Ctx::frame_keybinding` (a frame's own keys). The
  breadcrumb, mode chips, a mode switch and a toggle; the commands
  `nav.back` (Alt+Left, Escape while a mode is pushed), `nav.up`
  (Alt+Shift+Up), `nav.enter`, `context.read`, `context.request`
  (`if_rev`, `dry_run`), `context.go`, `context.set_mode`,
  `context.hold`, `context.release`; the `contexts` collector; an
  undo made out of sight says where, with "Go there". Strings with
  their French. Gallery page: `contexts`.

- Context stack frame titles are translated like other declarations,
  under the key derived from the frame's id (`frame-<id>.title`,
  `egui_pop_core::strings::frame_title`): the breadcrumb, the mode
  chips and switches, the screen reader's "Now in" and the toast of
  frames not restored read the catalogue; the extractor lists them.
  The gallery's frames with their French.

- The current arrangement belongs to where the user is in the context
  stack: the last active mode declaring an arrangement (a mode may now
  declare one, `FrameSpec::arrangement`, shown while it is on), else
  the top place, else the root. Switching or saving in a place keeps
  the choice for that place (`settings/workbench.arrangement.frames`),
  and the next entry of the place starts from it; the root's stays in
  `settings/workbench.arrangement`, so existing settings files read the
  same. `arrangements::arranged_frame`, `by_frame` and `layout_of`.

- A plugin setting the context stack's own keys by hand (`place`,
  `placeDepth`, `modePushed`, `mode.<group>`, through
  `AppHandle::set_context`) is reported: a diagnostic the first time
  per key (`context-key-by-hand`) and the `contexts` collector's
  `warnings` (`ContextService::hand_writes`: the value, how many
  times, the command running and its plugin). `contexts::is_stack_key`.

- Two frames of the context stack that can be on together (a place and
  a mode, modes of different groups) binding the same keys are
  reported instead of the later one winning silently:
  `contexts::frame_clashes`, a diagnostic at start
  (`context-frame-key-clash`), the `contexts` collector, and the keymap
  check of `egui-pop-test` (`keymap_problems`), which names the frames.

- The context stack's `context` and `view` documents are kept beside
  the layout's document as the host names it, so a host with a layout
  per workspace has a stack per workspace (`contexts::documents`);
  `contexts::reload` reads them again when the host opens another
  workspace in the same window.

- A debounced document writer (`egui_pop_core::persist::Writer`,
  about a second after the last change, at most five): the settings,
  the user's keymap and the layout (through `layout.save`) are written
  while the application runs, not only at exit;
  `AppHandle::save_later`, `flush_documents`, `pending_documents` and
  `persist` for a plugin's own documents.

- Field states (`model::field::FieldState`, drawn by `core::field`): one
  model for every input, modified (against the saved or the default
  value, with "reset to default"), required, invalid with a message,
  disabled with a reason, read-only, pending. Each state has an edge
  colour from the new `field.*` tokens and a marker icon, never colour
  alone; assistive technologies get required, invalid, disabled with
  its reason, read-only and busy; agents read every field's state and
  disabled reason in the debug bridge's `fields` collector. Strings with
  their French. Guide: docs/inputs.md.

- Sliders and range sliders take a field state (`.state(..)`): the
  edge and markers on the bar; a read-only slider keeps the focus and
  refuses every change, a disabled one is greyed, skipped by Tab and
  says its reason on hover. Without a state they are unchanged.

- Tab order per focus scope (`core::tab`, `model::tab`): reading order
  (rows, then left to right) whatever the drawing order, explicit
  indices and groups, disabled fields skipped and read-only ones kept;
  Tab cycles inside a modal scope, F6 and Shift+F6 move between panel
  scopes, back to the stop focused there last.

- The decorated input (`core::input::Input`, for text, numbers, paths
  and search): a placeholder, leading and trailing icons, prefix,
  suffix and unit, a clear button reachable by Tab (one undo step when
  bound to a store), a reset arrow, attached buttons, split buttons
  (also on their own: `input::split_button`) and a dropdown of choices;
  a number field reads expressions as a slider's typed entry. The
  gallery's new "Inputs" page shows every decorator, every state and
  the Tab order (a form, two panels, a modal). The survey behind it is
  in docs/widgets.md.

- A theme editor (`api::theme_editor`, `ThemeEditorPlugin`, drawn by
  `theme_editor::show` in any `Ui`): the application's theme edited
  live, its tokens grouped by role (surfaces, text, accent, states,
  borders, shape and space, components, the palette), egui's colour
  picker and a value field taking a value or an alias, the contrast of
  every pair a token takes part in (AA, AAA, and 7:1 text in high
  contrast), starting from any built-in theme, undo and redo, export
  and import as a `.tokens.json` file that `$extends` its base. Each
  change runs the `theme.token` step (undo and redo too), a start or an
  import `theme.load`; agents read and set tokens through `theme.read`,
  `theme.set`, `theme.undo`, `theme.redo`, `theme.start`,
  `theme.export` and `theme.import`. Strings with their French. Guide:
  docs/theme-editor.md.

- Tips (`api::tips`, `TipsPlugin`): plugins register tips, shown as a
  tip of the day (`tip_of_the_day`: previous, next, "Don't show
  again", close) or as a hint where the application asks
  (`hint`: "Got it"). A dismissed tip is remembered in the settings
  (`tips.dismissed`, `tips.show` turns them off) through the
  `tips.dismiss` step; `tips.reset` shows them all again. Titles and
  texts are translated by keys derived from the tip's id. Guide:
  docs/tips.md.

- Theme drafts in `egui-pop-tokens` (`ThemeDraft`, `TokenEdit`): a base
  theme, the tokens changed over it and their history (undo, redo),
  values checked against the token's type and aliases against loops,
  exported as a token file stamped with the editor's version and
  imported back. `TokenFile::to_json` writes a token file out as DTCG
  JSON.

- The gallery's `theme-editor` and `tips` pages.

- `AppHandle::declared_panels`: every declared panel with the plugin
  that declared it (`Panels::owner`, `Panels::owned` in the model), so a
  host's layout or template check accepts the panels egui-pop's plugins
  declare, the jobs centre among them.

- A node graph editor (`viewport::graph`) over a graph the application
  keeps (`GraphModel`: nodes, typed ports coloured from tokens, wires,
  groups, parameters, kinds to add), drawn on egui's `Scene`: pan and
  zoom, a minimap sharing its state (`GraphMinimap`), nodes with a slot
  in the title bar and a collapsed state, groups framed or folded into
  stacks, bezier wires checked against the ports' types while they are
  drawn, a rubber band, moves snapped to a grid, delete and duplicate.
  The zoom picks the level: an overview of compact nodes, the ports, and
  a detail view where parameters are egui-pop sliders edited inline.
  Every edit comes back as a request (`Edit`) through an `on_edit` hook
  (allow, veto, redirect); the editor never changes the graph. A
  keyboard path goes through nodes and ports and connects them; nodes,
  ports, groups and stacks are AccessKit nodes with roles and values,
  and wires begun, made or refused are announced. New tokens `graph.*`
  and strings with their French. Guide: docs/node-graph.md.

- The node graph editor in an application (`imaging::show_graph`): each
  edit runs a step (`graph.connect`, `graph.disconnect`, `graph.move`,
  `graph.delete`, `graph.add`), the selection, the level and refused
  wires go on the journal, and agents read and edit a graph through the
  `graph.read`, `graph.edit`, `graph.select` and `graph.view` commands.

- The gallery's node graph page: fifty nodes from six sources to one
  output, with groups, masks and blends, the minimap, and a plugin
  refusing to delete the output.

- The viewport's tile intake off the interface thread, opt-in:
  `ViewportState::set_prepare_on_worker` converts each tile on the
  thread that delivers it (the display transform on the CPU path, the
  half-float texels on the GPU path), and
  `ViewportState::set_upload_budget` (`tiles::UploadBudget`, tiles or
  bytes a frame) uploads at most that much per frame, the visible
  area's tiles first; what is left waits for the next frames and is
  kept when the view moves. `ViewportState::upload_report` says what
  the last frame uploaded and what waits. See docs/threading.md.

- A prototype of tiles rendered on the GPU by the source:
  `TileSink::deliver_texture` (feature `wgpu`) hands the viewport a
  scene-referred texture the source's worker rendered on the GPU path's
  device; nothing is copied, the interface thread only binds it. The
  viewport crate's `gpu_tiles` example times the tile intakes (on the
  workers, on the interface thread, rendered on the GPU) on a real
  device, offscreen; docs/threading.md has the numbers.

- A photo library's widgets (`egui_pop::views::library`): a virtualised
  thumbnail grid and a filmstrip sharing its selection, over images the
  application lists (`LibraryItems`) and thumbnails it supplies
  asynchronously (`ThumbSource`, placeholders until they arrive), with
  keyboard navigation, the thumbnails' size by zoom, multi-selection
  (Shift, Ctrl, a rubber band), overlays (rating, colour labels, pick or
  reject flag, badges) and stacks collapsed or expanded; Lightroom's
  culling keys with auto-advance (`CullKeymap`); rating stars, colour
  label and flag toggles; a comparison of two to four images with zoom
  and pan synced and a "best" marker; tag chips and a tag input with
  completion; a search field with filter chips giving a structured
  `Filter`. Every change runs a step (`library.select`, `.rate`,
  `.label`, `.flag`, `.tag`, `.stack`, `.filter`) and agents have
  commands for them, declared by `LibraryPlugin`. Guide:
  docs/library.md; gallery page `library`.

- The image on a surface of its own on Wayland (`image_surface`, feature
  `wayland` of `egui-pop-viewport`, on by default): after
  `image_surface::install` (eframe's creation context, a transparent main
  window), the main window's viewports draw their image into a subsurface
  below the interface, tagged through `wp_color_management_v1` with the
  wanted encoding (BT.2020 with a 2.2 power, Display P3, PQ; KWin, sway,
  Weston), or presented in a colour space the driver declares where the
  driver holds the surface's colour itself (Display P3 on GNOME and
  Hyprland); the interface stays sRGB and is drawn over the image. The
  negotiation (`image_surface::negotiate`) is plain code tested with
  fake compositors; without the protocol, on X11 or another system the
  image stays sRGB and `display::Resolution::reason` says why
  (`Fallback::Unavailable`). The `viewport` collector reports it, with
  the compositor's preferred image description for the window; the
  `EGUI_POP_IMAGE_SURFACE` variable turns it `off` or `no-tag`. The
  Wayland side is a new crate, `egui-pop-wayland`. Guides:
  docs/viewport.md, docs/wayland-colour.md.

- The gallery's `--wide-gamut` option: a transparent window, image
  surfaces installed and BT.2020 with a 2.2 power wanted; the display
  page offers that wish and shows where the image goes and why, what the
  compositor announces and its preferred description.

- Profiling scopes (the `profiling` crate egui and wgpu already use) on
  the interface thread's heavy moments and on the locks a worker thread
  shares with it: the app's logic, settle and requests, the viewport's
  tile intake, uploads and GPU preparation, the terminal's painting,
  output and model lock, the editor's text area, highlighting and model
  lock. They compile to nothing unless a profiler backend is enabled
  (`profiling/profile-with-tracing`, `-with-puffin`...); the gallery's
  `threading` example measures them (`docs/threading.md`).
- Skeletons and empty states (`egui_pop_core::placeholder`): `Skeleton`
  stands in for content that loads, in its layout (a paragraph, list
  rows), with a shimmer that stops under reduced motion and in tests,
  its region busy for assistive technologies; `EmptyState` gives an
  icon, a title, a sentence and an action. New component tokens
  `skeleton.{fill, shimmer}`. Gallery page `placeholders`.

- The jobs centre (`egui_pop_api::jobs_centre`): every job as a panel
  (`Jobs`, closed until asked for) and as a popover from the status
  bar's jobs item, the same list in both: the estimated percentage as a
  bar (failed and skipped items stacked), "n of m", the step, the item,
  the time left, errors and refusals, Pause, Resume, Cancel, Retry and
  Dismiss, a batch's children folded under it, and the history. A
  chevron on each job opens its log console, collapsed by default and
  remembered per job everywhere. The console draws plain lines in the
  application layer; `egui-pop-editors`' `JobConsolesPlugin` makes it a
  pseudo-terminal (`Terminal` over a `Console` per job) through the new
  `JOB_LOG_VIEW` service, the layering keeping the editors above.
  `Toast::job` carries a job's progress and its Cancel or its first
  action, staying while the job runs. Translated in French. The
  gallery's jobs page shows the centre, its snapshots with a console
  open and with the popover.

- Busy state of command-bound controls: `busy::CommandButton` runs a
  command and, while the host has not answered, greys, shows a spinner
  in its icon's place after the delay (keeping its width and its name,
  AccessKit's `busy` flag set) and says why a failed run failed; menu
  items do the same. `AppHandle::running_since` (the motion clock's time
  a waiting run started). Status bar indicators show a delayed spinner
  while busy (`IndicatorState::busy`), their progress line on the
  `progress.*` tokens; `JobsPlugin` adds the jobs item (a ring with the
  overall estimate, how many jobs go on, how many failed). Translated in
  French. The gallery's host answers `gallery.slow.*` commands when the
  progress page says so.

- Progress widgets (`egui_pop_core::progress`): `ProgressBar`
  (determinate or indeterminate, a travelling segment that stands still
  as stripes under reduced motion; tones; stacked sections for failed
  and skipped items; a thin line; a caption), `Ring`, `Meter` (a value
  against thresholds, AccessKit role `Meter`) and `Spinner` (with a
  label; `delayed` waits `motion.spinner.delay` and stays
  `motion.spinner.min`). Each is a named AccessKit progress indicator,
  with a numeric value and its range when determinate and none when
  indeterminate. New component tokens `progress.{track, fill, height,
  line, ring}` with their contrast pair. Gallery page `progress`, its
  snapshots in two themes and under reduced motion.

- The motion clock (`egui_pop_core::motion`): spinners, travelling
  bars, shimmer and busy delays read it instead of the input's time; a
  test freezes and steps it (`freeze`, `set_clock`), reduced motion
  gives still frames (`phase`), and animations ask for their next frame
  without ever repainting at once (`animate`). `Busy`: nothing before
  `motion.spinner.delay` (300 ms), then shown at least
  `motion.spinner.min` (500 ms), counted from when the work started.
  New system tokens `motion.spinner.{delay, min, turn}`,
  `motion.progress.travel` and `motion.skeleton.shimmer`
  (`Tokens::motion`). The jobs model reads the same clock. Gallery
  page `motion`; the gallery's tests freeze it.

- Jobs: `JobsPlugin` (opt-in) folds the host's `job.*` events
  (`started`, `progress`, `item_failed`, `paused`, `resumed`,
  `cancelling`, `failed`, `finished`, `cancelled`, `log`) into the
  `egui_pop_model::jobs::Jobs` model behind the `JOBS` service, from the
  journal and never by polling. Every job carries an estimated overall
  percentage: declared steps and child jobs roll up weighted by their
  estimates, a job that cannot estimate is indeterminate until it can,
  and a hook on the new `jobs.estimate` step refines it. Time left from
  an injected clock (`egui_pop_model::clock`). Cancel, pause, resume and
  retry are commands the host runs (`job.cancel`...), or a runner the
  application gives; a job shows what it asked until the host answers.
  The `jobs` debug collector, and `egui_pop_test::jobs::FakeHost` to
  script jobs in tests. Gallery page `jobs`. Guide: `docs/progress.md`.

- `egui-pop-imaging`: the `cursor.resolve` step. `imaging::cursor`
  gives a viewport's `on_cursor` hook that asks the step each time the
  situation under the pointer changes; plugins give another cursor
  with a run-hook (or a redirect of `default`), a veto keeps the
  default.
- A cursors and loupe guide (`docs/cursors.md`): the cursor model, the
  tools' defaults, the `cursor.resolve` step, the brush outline,
  settings and accessibility, the loupe; its survey (winit's custom
  cursors, eframe 0.36, egui's cursor icons, GIMP, Krita and darktable,
  crates.io) and why the cursors and the loupe are egui-pop's own.
- Gallery: a Cursors and loupe page (the tool cursors, a viewport whose
  tool, brush, loupe and cursor settings are picked in the playground;
  the page's plugin shows a not-allowed cursor for the brushes outside
  the image through `cursor.resolve`).
- `egui-pop-imaging`: the `cursor.resolve` step. `imaging::cursor`
  gives a viewport's `on_cursor` hook that asks the step each time the
  situation under the pointer changes; plugins give another cursor
  with a run-hook (or a redirect of `default`), a veto keeps the
  default.
- Cursors over the viewport (`viewport::cursor`): `Viewport::tool`
  picks a tool (pan, zoom, picker, crosshair, move, rotate, brush,
  eraser, mask, disabled, or one of the application's) and the viewport
  shows its cursor, resolved again when the keys held, a drag or a
  handle under the pointer change it (Alt zooms out, a hand closes
  while dragging); `Viewport::on_cursor` gives another. Thirteen tool
  cursors drawn as bitmaps at the cursor size and the window's scale,
  in the theme's colours (`cursor.fill`, `cursor.outline`), handed to
  the system through egui's custom cursor image with a system cursor
  as the fallback; an application's bitmaps by name, at several sizes,
  with a dark variant and frames (still when motion is reduced); a
  brush outline sized in image pixels, with its hardness ring, the
  system cursor hidden, a crosshair when too small. `CursorSettings`
  turns painted cursors off and sets the size, which follows
  `XCURSOR_SIZE` by default.
- A loupe in the viewport (`viewport::loupe`): the displayed pixels
  under the pointer at 4 to 16 points each, with a grid, a marker on
  the centre pixel and a readout (RGB and Lab as displayed, or the
  source's values), beside the pointer and flipped at the edges. It
  draws the viewport's own tiles again: a second paint callback on the
  GPU path, with no read back, and the same egui textures on the CPU
  path. On from `ViewportState::loupe_mut` or a key
  (`Signal::Loupe`); tokens `loupe.*`.
- An image tools guide (`docs/image-tools.md`): the histogram, the curve
  editor and the targeted adjustment, the colour wheels, the zoom
  control and the navigator, their hooks and steps, accessibility and
  tokens; the widget survey records what was checked (egui_plot,
  egui's colour picker, colour, curve and minimap crates) and why the
  tools are egui-pop's own.
- Gallery: a Curves and wheels page (a tone curve over a histogram,
  with the targeted adjustment on a grey ramp, the lift, gamma and gain
  wheels and a picker; the page's plugin caps the gain's chroma through
  `wheel.change`) and a Zoom and navigator page (a viewport, its zoom
  control and its navigator on one state; the page's plugin keeps the
  zoom at 400 % at most through `zoom.change`). The Image widgets
  page's histogram shows its clipping and its drag adjusts an exposure.
- `egui-pop-imaging` (the facade's `imaging` feature,
  `egui_pop::imaging`): the image tools' changes as steps of an
  application. `histogram`, `curve`, `wheel` and `zoom` give a tool's
  hook that runs each finished change as `histogram.drag`,
  `curve.change`, `wheel.change` or `zoom.change`, where plugins veto,
  redirect, replace or follow it; `ImagingPlugin` declares them.
- A zoom control and a navigator (`viewport::navigator`), sharing a
  viewport's state: `ZoomControl` steps the zoom ladder, fits and shows
  100 %; `Navigator` shows the whole image (the application's proxy)
  with a frame around the part in view, which a drag pans, a press
  elsewhere centres, the wheel zooms and a double click fits, with keys
  when focused. Their changes go through their hook, then the
  viewport's `before_view`, and come out as the viewport's signals.
- Colour wheels (`viewport::wheel::ColorWheel`): `offset` for the lift,
  gamma and gain wheels of colour grading (neutral at the centre,
  relative drags, a signed luminance) and `picker` for hue and
  saturation (a press picks, a luminance from 0 to 1), each with a
  luminance slider from egui-pop's slider family. The value
  (`WheelValue`: hue, chroma, luminance) is the application's; every
  change, of the puck or of the luminance, goes through the wheel's
  hook. Keys move the puck, a double click resets; the disc is an
  AccessKit slider on the chroma.
- Palettes (`egui_pop_api::palettes`): toolboxes and palettes floating
  over the workbench, never modal. A panel declared `palette()` opens
  floating; dragged by the movable dialogs' header band, clamped inside
  the workbench and snapping to its edges and to the other palettes
  (the `palette.magnet` token, from the new `size.magnet`), it docks
  when dropped on a side bar or the bottom panel, with the drop hint,
  and floats again from its tab menu (Float). Its band has Pin, Roll up
  and Close. It takes the keyboard only when clicked into, and Escape
  gives it back. `palettes.toggle` (Tab, while some panel floats and no
  widget has the keyboard) hides and shows the unpinned palettes;
  `palettes.reset` puts them back; `set_stroke` fades them during a
  stroke with `palettes.fade_on_stroke`; `palettes.idle_opacity` makes
  them translucent away from the pointer. Steps `palette.open`,
  `palette.float`, `palette.dock`, `palette.move`, `palette.roll` and
  `palette.pin`, each published once done (`palette.moved`...); the
  workbench's `Change` gains the matching variants. Tokens
  `palette.*`.

- Floating panels in the layout: `PanelSpec::palette()` declares a
  panel that floats over the interface when it opens (its area is where
  it docks), `floatable(true)` lets any panel float. `Layout::floating`
  keeps each floating panel's place, rolled-up and pinned state
  (`Floating`), saved with the layout and repaired with it (a panel
  that may no longer float docks back); `Layout::float`,
  `floating_of`, `floating_mut`.

- Movable dialogs: `DialogSpec::movable(true)`, or the `dialogs.movable`
  setting for every dialog, gives a dialog a header band (its own tone,
  a separator, the move cursor, the ✕ when closable) to drag it by,
  kept inside the window and again when the window is resized; a
  double click on the band centres it again. The `dialogs.move` command
  (Alt+F7) moves it with the arrow keys (Shift for bigger steps), Enter
  keeping the place and Escape putting it back. A dialog comes up
  centred unless `remember_position(key)` keeps its place, in the
  host's store under `ui/dialogs/<key>`. `DialogSpec::backdrop` takes
  `Backdrop::Dim` (the default, from the `dialog.backdrop` token,
  fading while the dialog is dragged) or `Backdrop::Clear` (modal still,
  the interface untouched, for a dialog previewing on the image); a
  click on the backdrop never closes a dialog. The `dialog.move` step
  (veto or redirect a drop, a key move or a return to the centre) and
  the `dialog.moved` event; `Dialogs::offset`; the bridge's `dialogs`
  collector shows `movable`, `backdrop`, `offset` and `moving`.

- Modal overlays can be moved and keep their backdrop to a token:
  `OverlaySpec::offset` places a modal away from the window's centre,
  `OverlaySpec::dim` sets how much of the backdrop it shows (0: none,
  the interface still blocked), `overlay::rect` says where an overlay
  was drawn, and `OverlayStack::update` changes an open overlay's
  declaration in place (its Escape rule). New tokens: the system
  `opacity.scrim` (a number, egui's own backdrop by default) and the
  components `dialog.backdrop`, `dialog.header` and
  `dialog.header-text` (with its contrast pair).

- A curve editor (`viewport::curve::CurveEditor`): points added on the
  curve, dragged between their neighbours, deleted by a double or right
  click, and the whole curve reset by a double click elsewhere; the
  spline is the application's (`Spline`, with `MonotoneCubic` and
  `Linear` supplied); a histogram behind; the keys (next and previous
  point, arrows, Insert, Delete, R, remappable as `CurveKeys`); and the
  targeted adjustment, where the value picked under the pointer in the
  image selects or adds the point the application's drag then moves.
  Every change goes through its hook; it is an AccessKit slider on the
  selected point's output.
- Histogram: clipping markers (`Histogram::clipping`, lit in a bottom
  corner when shadows or highlights are clipped, and said to screen
  readers) and a drag handed back to the application
  (`Histogram::draggable`, `on_change`, `show` returning the changes):
  the histogram becomes an AccessKit slider whose drag, arrows and
  increment and decrement actions move a value the application owns,
  such as the exposure. `viewport::change` holds what the image tools
  share: `Change`, the `Cause`, the hook's `Verdict`, and `StepValue`,
  a value's form in a step.
- Component tokens for the image tools: `histogram.clip-shadows` and
  `histogram.clip-highlights` (the clipping markers), `curve.*` (the
  curve editor: background, grid, identity, line, point, selected point,
  pick), `wheel.*` (colour wheels: ring, puck, puck outline) and
  `navigator.*` (background, frame, veil). The curve and its points are
  contrast-checked against the editor's background.
- A sliders guide (`docs/sliders.md`): the gestures, typed expressions,
  the variants, hooks and the `slider.change` step, accessibility; the
  widget survey records why the family is egui-pop's own.

- The gallery's sliders page: the compact row (soft and hard range,
  signed values, a quad toggle and a picker), a logarithmic bar, hue and
  temperature tracks, range sliders, and the `slider.change` step, which
  the page's plugin hooks to snap one slider to halves.

- The `slider.change` step (`egui_pop_api::sliders`): a slider's
  finished change (a drag's release, a key, a wheel notch, a typed
  value, a reset) runs through the application when the slider's hook is
  `sliders::hook(app, name)`. A before-hook vetoes it or redirects it
  (snap, clamp), a run-hook gives the value, and the journal gets
  `slider.change`; a drag's frames are not steps.

- The slider family, after darktable's bauhaus sliders
  (`egui_pop_core::slider`):
  - typed entries take an expression (`x * 2`, `(1 + 2) / 4`, `^`,
    `x` the current value) with either decimal separator and grouped
    thousands (`slider::expr::eval`);
  - the value text is written in the interface's language
    (`strings::number`), with an optional sign (`signed`: `+1.50 EV`);
  - a quad button at the bar's end: `quad(icon, label)` (a picker, its
    click in `SliderOutput::quad_clicked`) or `quad_toggle(icon, label,
    &mut on)`;
  - `logarithmic()` bars, gradient tracks (`gradient(Gradient::hue())`,
    `Gradient::temperature(from, to)`, any `Gradient::new`), and
    `RangeSlider`, two thumbs on one bar that never cross, each its own
    focusable, named AccessKit slider ("ISO, minimum");
  - `on_change` sees every change before it is written (thumb, from, to,
    cause, whether the gesture is done) and answers go, veto or
    redirect; `show(ui)` returns the changes written in the frame;
  - Page Up and Page Down take coarse steps; the AccessKit node carries
    the range, the step, the value text and the increment, decrement and
    set-value actions.

- Numbers in the interface's language for widgets: `Strings::number`
  (a provider writes a value with a fixed number of decimals, its
  thousands grouped) and `strings::number(ctx, value, decimals)`, which
  falls back to Rust's formatting. The i18n plugin's provider writes
  them with the locale's separators (`1 234,50` in French).

- `Icon::Picker`, an eyedropper (Phosphor's, or painted as a tube, a
  collar and a bulb): a slider's colour picker needs it.

- A close button on dialogs, off by default (Cancel and Escape are the
  way out): `DialogSpec::new(dialog).closable(true)`, or the
  `dialogs.closable` setting for every dialog, puts a ✕ at the right end
  of the title row, with no header band. It is Escape: the same cancel
  through `dialog.answer` and its hooks, refused when Escape is. Named
  "Close" (translated), a target of at least 24 points, never focused
  when the dialog opens. `Dialogs::open` takes a `DialogSpec`, which
  every dialog converts into.

- Text and code editors, a new crate `egui-pop-editors` (L4, feature
  `editors` of the facade): `TextSurface` and `CodeEditor` over a shared
  `EditorState` (a line buffer, the selection, the editor's own
  history, marks, folds, search), virtualised (100 000 lines cost a
  screenful), text inputs for the focus rules (Escape leaves), their
  shortcuts commands bound on the `editor` surface. Search and replace
  (case, whole word, regular expressions), go to line. The code editor
  adds line numbers, a gutter of diagnostics, breakpoints and diff
  marks the application pushes, the cursor's line, matching brackets,
  indentation guides, folding by indentation, and highlighting behind
  the `Highlighter` trait: syntect by default (feature `syntect`),
  tree-sitter behind `editors-tree-sitter`. Code assist through
  `AssistProvider` (completion, hover, signature help, code actions, go
  to definition, inline suggestions; asynchronous, with request ids and
  cancellation), the completion list, the hover and signature cards
  and ghost text drawn from the theme; `WordCompletion` by default.
  Steps `edit.change`, `edit.save`, `code.diagnostics`,
  `code.insert_completion` and `code.accept_suggestion`, journal events
  `code.cursor_moved`, `code.selection_changed`,
  `code.completion_requested` and `code.completion_accepted`,
  collectors `edit_changes`, `edit_saves` and `code_diagnostics`, and
  commands an agent reads and writes the buffer with (`editor.read`,
  `editor.write`, ...). Translated in French. Guide: `docs/editors.md`.

- A terminal in `egui-pop-editors`: `Terminal` draws a `TerminalState`
  (vt100's emulator: ANSI colours from the theme's `terminal.*` tokens,
  the 256-colour cube and RGB, bold, dim, italics, underline, inverse,
  the cursor) fed by a `TerminalSource` the application picks: a
  program in a pseudo-terminal (`PtySource`, portable-pty, ConPTY on
  Windows; feature `pty`, `editors-pty` on the facade) or a read-only
  `Console` it writes logs and agent output to. Scrollback (wheel,
  Shift+Page Up and Down), selection and copy, links (Ctrl+click), the
  program told its size. While focused every key goes to the program,
  Escape and Tab included; Shift+Escape leaves, Ctrl+Shift chords stay
  the application's. Steps `terminal.spawn`, `terminal.input` and
  `terminal.exit` (veto, redirect, replace), collectors
  `terminal_sessions` and `terminal_io`, and commands for agents
  (`terminal.read`, `terminal.send`, `terminal.clear`,
  `terminal.copy`, `terminal.close`, `terminal.restart`) from
  `TerminalPlugin`. Translated in French.

- Theme tokens for the terminal: `terminal.*` (background, text, bold
  text, cursor, selection, links) and the 16 ANSI colours by number
  (`terminal.ansi.0` to `terminal.ansi.15`), each an alias of a system
  token, with their contrast pairs.

- Theme tokens for editors: `editor.*` (background, gutter, line
  numbers, the cursor's line, selection, cursor, guides, brackets,
  search matches, folds, breakpoints, diagnostics, diff) and `syntax.*`
  (one per kind of code), each an alias of a system token, with their
  contrast pairs.

- Gallery: a Terminal page (a coloured build log on a console source,
  an echoing demo program whose input a redirect upper-cases and whose
  close a veto refuses, and on the desktop a shell in a
  pseudo-terminal, on demand).

- Gallery: Text editor and Code editor pages (a veto on the title line;
  diagnostics, breakpoints, diff marks, 100 000 lines on demand, and a
  demo assist provider answering from a thread).

- Tree drops refused by the source (`TreeSource::can_drop`, allowing
  everything by default) and at the call site (`TreeView::can_drop`),
  besides `tree.drop`'s hooks: asked while dragging, where a refused
  target is marked in the danger colour under a "not allowed" pointer,
  and by `tree.drop`'s default, whose refusal comes back as
  `Signal::Vetoed` with the reason. The gallery's tree page refuses
  drops into its offline drive and moves of locked nodes.

- `TreeState::set_selection` queues the nodes the tree has not loaded
  yet: the selection takes each as its branch loads, then brings the
  cursor's row into view; the user selecting drops what is queued, and
  `TreeState::pending_selection` tells what still waits. Before, such
  nodes left the selection at the next edit. The gallery's tree page
  selects a photo before its folder loads.

- A tree row's context menu, for the row right-clicked whatever is
  selected: `TreeView::context_menu(app, menu)` opens a declared menu
  whose commands get `{"tree", "key"}` merged into their arguments,
  `TreeView::context_ui` one the application draws, given the node.
  The new `tree.context` step runs first (a veto keeps the menu
  closed), and `Signal::Context` names the node. The gallery's tree
  page hides, locks or renames a row from its menu.

- A tree node's plain name apart from its label (`Node::with_name`,
  `Node::name`), for labels with decorations: a rename starts from the
  name, type-ahead searches names, and `tree.rename` and
  `Signal::Rename` carry names. The gallery's tree page decorates its
  documents with their size.

- Tree rows carry an icon and widgets of the application's:
  `TreeView::icon` gives each node's icon (`icon::Source`), drawn
  between the triangle and the label; `TreeView::row_widgets` adds
  widgets at the row's right end (a visibility toggle, a lock) that act
  on their row without selecting it, the label cut before them.
  `TreeView` now names the key's type (`TreeView<'a, K>`). The
  gallery's tree page shows an eye and a lock on every row.

- Icons: `Icon::Lock` and `Icon::Unlock` (a padlock closed and open),
  from the Phosphor font or painted.

- Viewport overlays, for editing masks: a style per overlay
  (`overlay::Style`: `Emphasis::Selected` or `Faint`, a dashed line;
  `Overlay::selected`, `faint`, `dashed`), an ellipse shape
  (`Shape::Ellipse`, its radii and rotation on handles), handles of an
  overlay's own (`Overlay::handles`) with a Bézier control kind
  (`HandleKind::Control`), a click on a handle (`Signal::HandleClick`)
  and on an outline away from the handles (`Signal::EdgeClick`, with
  `overlay::hit_edge`'s segment and the nearest point), and the
  `under_overlays` paint hook between the image and the overlays
  (`after_paint` stays above the handles). The gallery's viewport page
  shows a dashed ellipse and selects the shape touched.

- The bottom panel's alignment, as VS Code's: the
  `workbench.panel_alignment` setting (`justify`, the whole width, by
  default; `center`, `start`, `end`), `workbench::PanelAlignment`, and
  `AppBuilder::panel_alignment` for another default. The gallery's
  workbench page sets it. Translated in French.

- The layout's document comes from the host: `Host::layout_document`
  (`layout` by default, a workspace's own such as
  `workspaces/photos/layout`, or `None` when the host keeps the layout
  itself; `LocalHost::with_layout_document`). `App::load_layout` loads
  it again (switching workspaces), `App::save_layout` saves it; both run
  the new `layout.load` and `layout.save` steps (`{ "document" }`), which
  a plugin may redirect, veto or replace. `Persist` makes the folders of
  a document name holding `/`.

- `AppBuilder::name` gives the application's name apart from its main
  window's title (`AppHandle::name`): a detached panel's window is titled
  `{ $panel } — { $application }` with it, no longer with the whole main
  title (which may name a document). `run_native` takes its `app_name`
  when none is given, and `FramePlugin` shows it when it has no name of
  its own.

- The workbench's drop hint is published: `AppHandle::drop_hint` (the
  dragged panel, the area, the rect, the refusal), `overlay_rects`
  (`OverlayKind::DropHint`, named `drop:<panel>:<area>`), and a new
  `workbench` collector on the debug bridge (the layout, each area's
  rect, the focused panel, the dragged tab, the drop hint).

- The start of the window's own title bar is a zone,
  `zones::CAPTION_START`, declared by `FramePlugin`: its default block
  `frame::BLOCK_MENU` (`egui_pop.frame.menu`) is the system menu button
  and the application's name. An application puts its logo there with
  `Block::replace(BLOCK_MENU, …)` and keeps the menu with
  `frame::menu_button`, while the bar (`frame::BLOCK_TITLE`) still
  follows the `window.frame` setting. The gallery shows its logo there.

- Choosing files (`egui_pop_api::files`, `FilesPlugin` in
  `DefaultPlugins`): `Files::choose` returns a `Pending` and never holds
  a frame. The system's chooser (feature `native-dialogs` of the facade,
  through `rfd`; the xdg desktop portal on Linux) is made on the
  interface thread and awaited on its own; without it, without a session
  bus, or with the `files.native` setting off, an in-app picker answers:
  keyboard navigation, a typed path with `Tab` completion, the choice
  dialog's fuzzy filter, the request's filters, hidden files, recent
  places and bookmarks kept in the host store. Steps `file.choose` (a
  run-hook replaces the chooser) and `file.chosen` (veto or redirect);
  the `files` collector shows and answers choices. `FilePicker` works on
  its own in any `Ui`. A form's path field browses through it. The
  gallery's `files` page opens each kind of choice. Guide: docs/files.md.

- Windows hit testing for the window's own frame: `egui-pop-win32` (no
  egui dependency, the one crate allowed `unsafe`) subclasses a window
  and answers `WM_NCHITTEST` from the frame's hit regions; `FramePlugin`
  installs it on the main window when the application draws the frame
  (step `frame.hit_test`), so the maximize button shows Snap Layouts and
  the caption moves the window with Aero Snap, while button clicks and a
  double or right click on the caption still run the `frame.*` steps.
  `frame::set_native_window` for applications starting eframe
  themselves; the `frame` collector shows the layer's state. Guide:
  docs/workbench.md.

- Wide gamut and HDR in the viewport's display stage: Display P3 and
  BT.2020 primaries (matrices from their chromaticities,
  `Primaries::conversion`), the extended sRGB transfer, SMPTE ST 2084
  (PQ) and BT.2100 HLG, on the CPU (`Stage::encode`,
  `DisplayTransform::apply_f32`) and in the GPU shader; a `Linear`
  transform for scene pixels in other primaries. An application wants a
  target per window (`display::want`); the viewport draws for what the
  surface carries (`display::resolve`, `ViewportState::display_resolution`)
  and says why when that is less (`Fallback`), and the `before_display`
  hook may veto or redirect it. `DisplayTarget::is_supported` and
  `effective` now say whether a surface could be configured that way and
  declared so. `gpu::install` records what eframe configured;
  `gpu::install_for_surface` and `gpu::choose_surface` are for a host
  configuring its own surface (formats and colour spaces from wgpu). The
  gallery's `display` page shows it. Guide: docs/viewport.md.

- Data views (`egui-pop-views`, feature `views` of the facade,
  `egui_pop::views`), virtualised, each taking the keyboard focus as a
  whole (arrows, Page Up/Down, Home/End, Space, Ctrl+A, type-ahead),
  with a selection by key (single or multiple) that survives the rows
  changing. `TreeView` shows a tree the application implements
  (`TreeSource`): branches load on demand, from any thread, with a
  loading row and a retry on failure; a `TreeHandle` inserts, removes,
  moves, renames or reloads nodes at any time, keeping the selection
  and the expansion; drag and drop before, after or inside a node;
  rename in place (F2); 100 000 nodes cost a screenful. `ListView`
  shows rows the application draws. `DataTable` wraps `egui_table`
  (sticky header, resizable columns) with sorting from the header,
  the sort and the widths kept in the value store (`table::declare`),
  cells drawn by the application. Every moment is a step declared by
  `ViewsPlugin` (`tree.load`, `tree.expand`, `tree.select`,
  `tree.drop`, `tree.rename`, `tree.activate`, `list.select`,
  `table.select`, `table.sort`, `table.resize`) and a signal in the
  view's output. Their strings are in egui-pop's catalogues.
- Gallery: a Tree page (a simulated drive answered from a thread,
  edits while branches are open, a veto on drops into one folder, a
  branch of 100 000 files) and a Lists and tables page (10 000 photos
  sorted and resized through the value store, stars drawn by the page,
  a veto on one sort, a list of keywords).
- Guides for the tree (docs/tree.md) and for lists and tables
  (docs/table.md), their code run as doctests; the widgets survey
  records the decisions: the tree is egui-pop's own, the table wraps
  egui_table, egui_extras' `TableBuilder` is not used.
- Translations (`egui-pop-i18n`, feature `i18n` of the facade,
  `egui_pop::i18n`): `I18nPlugin` loads Fluent catalogues per locale
  (egui-pop's own, the application's, an override directory read at
  start-up), negotiates a fallback chain (the locale, its parents, a
  regional catalogue of its language, English, then the code's English,
  then the key), and installs the interface's strings provider. The
  `appearance.language` setting (`system` or a locale, each shown by its
  own name) switches the language at run time and publishes
  `locale.changed`; lookups are the `i18n.resolve` step, which `ShowKeys`
  replaces to show keys (`developer.show_keys`); the `i18n` collector
  lists the keys missing per locale. `t`, `text` and `tr!` look text up;
  numbers, dates (`DATETIME`) and plurals follow the locale. For tools,
  `extract` finds the keys of Rust sources and of a built application's
  declarations and writes or updates the English catalogue, and `check`
  reports coverage, missing, orphan and changed keys per locale. Guide:
  docs/i18n.md.
- egui-pop's own catalogues (`egui-pop-i18n/locales`): the English one
  generated from the interface's strings and the declarations of every
  egui-pop plugin (a test keeps it current), and a complete French one.
  They cover the platform services too: their settings, commands,
  plugin titles and the crash report's toast
  (`egui_pop_platform::strings`).
- Gallery: a Translations page (the language setting, egui-pop's strings
  and declared titles following it, plurals, numbers and dates, keys
  shown instead of text, the keys missing) and a `--lang` option
  (English by default, so pictures do not depend on the machine).
- Gallery: a Platform services page (a second launch's link showing a
  page, notifications with an action, the tray's declared menu, the
  global shortcut Ctrl+Alt+G, the log's tail, a crash in a past session
  offered, the focus mode), on a simulated desktop unless `--platform`
  is given; with it, the gallery is a single instance answering
  `gallery://page/<name>` links and its window's app id is
  `org.egui_pop.Gallery`.
- Gallery: a `--size <W>x<H>` option, the window's inner size in points
  (1100x720 by default; `GALLERY_SIZE` through wbox's `app.sh`).
- `On::Pass` (`Lifecycle::Pass`): a handler run at every pass of the
  application's logic, also while the window is hidden or minimized
  when something asked for a repaint, so what a background thread hands
  over (a second launch, a tray click) is taken without a frame.
- Key bindings in the system scope (`BindOptions::system()`,
  `KeyScope::System`, `system` in keymap files): global shortcuts that
  also run their command while the application is not focused, once a
  plugin registers them with the desktop (`Keymap::system_bindings`,
  `AppHandle::system_bindings`). Inside the window they fire as global
  bindings do. The `system_keys` module's `SYSTEM_KEYS` service says
  whether the desktop takes global shortcuts and how each registration
  went; the shortcuts editor shows it next to each command, including
  when nothing registers them.
- Platform services (`egui-pop-platform`, feature `platform` of the
  facade, `egui_pop::platform`; one feature and one opt-in plugin per
  service). A single instance and deep links: `instance::single`, at the
  top of `main`, makes the first launch listen on a local socket and a
  later one hand over its arguments, working directory and activation
  token, then exit. `InstancePlugin` turns them into the
  `instance.second_launch` step (the window comes to the front, links
  in the plugin's schemes run `deeplink.open`) and `deeplink.open`
  (runs the plugin's links command with `{ "url" }`), also for the
  first launch's own links. `dirs` gives the state, log and crash
  directories of each platform. Guide: docs/instance.md.
- Notifications (`platform::notify`, feature `platform-notify`):
  `NotifyPlugin` shows a `Notification` (title, body, level, actions
  running commands, a command for a click on its body) on the desktop
  through notify-rust, or as a toast when the `notifications.desktop`
  setting says so (`always`, `unfocused`, `never`), when the desktop has
  no notification server, or when showing it fails (`notify.failed`).
  Steps `notify.show` (the result says where it went) and
  `notify.action` (runs the action's command; the toast's button goes
  through it too). `notify::show` from the interface thread,
  `Notifier::post` from any thread; `Simulated` records them for tests.
  `Desktop::app_id` names the application to each desktop (the
  `desktop-entry` hint, the AppUserModelID, the bundle identifier).
  Guide: docs/notifications.md.
- A tray icon (`platform::tray`, feature `platform-tray`): `TrayPlugin`
  shows an icon through tray-icon (StatusNotifierItem through ksni on
  Linux, without GTK; Windows; macOS) whose menu is the declared menu
  `MENU_TRAY`, kept current at each pass (titles, ticks, greying,
  submenus, lists); a pick runs the command, a click runs the
  `tray.activate` step (by default the window comes to the front).
  `Simulated` records the menu and clicks for tests. Guide:
  docs/tray.md.
- Global shortcuts (`platform::keys`, feature `platform-keys`):
  `SystemKeysPlugin` registers the bindings of the system scope with the
  desktop at start and after each change (a rebinding included), through
  global-hotkey on Windows, macOS and X11 and the XDG GlobalShortcuts
  portal on Wayland, and fills the `SYSTEM_KEYS` service the shortcuts
  editor shows (registered, waiting, refused and why, or only in the
  window and why). A press runs the `keys.system` step, whose default
  runs the command. A sequence of chords is refused and said so.
  Guide: docs/global-shortcuts.md.
- Logs (`platform::logs`, feature `platform-logs`): `LogsPlugin` writes
  the `log` facade to `<log dir>/<name>.log` (the platform's log
  directory), rotated by size into numbered files, keeps a tail in
  memory, and declares the `developer.log_level` setting, the
  `logs.reveal` command and the `logs` debug collector; `Logs::write`
  feeds lines from another logger. Guide: docs/logs.md.
- Crash reports (`platform::crash`, feature `platform-crash`):
  `CrashPlugin` marks the running session and installs a chained panic
  hook writing the version, OS, thread, message, location, backtrace,
  last log lines and journal tail; a clean exit leaves nothing. On the
  next start a session that is gone becomes a report (its panics, or
  that it ended without closing) offered through the `crash.report`
  step: a before-hook adds data, an after-hook may send it, the default
  writes it and offers it in a toast. Nothing is sent anywhere.
  `crash.reveal`, `crash.dismiss`, the `CRASHES` service.
- The focus mode read in-process (`platform::focus`, feature
  `platform-focus`): `focus::read` asks GNOME through the Settings
  portal, reads KDE's `kwinrc` as a file and Windows' active window
  tracking bit through the registry API, instead of running `gsettings`,
  `kreadconfig` or `reg`; `FocusModePlugin` reads it again from a thread
  each time the window gets the focus back. Guide: docs/focus-mode.md.
- A packaging guide (docs/packaging.md): the desktop file, AppStream,
  icons and Flatpak's permissions on Linux, the manifest, icon,
  installers, AppUserModelID, URL protocol and code signing on Windows,
  the bundle and notarisation on macOS, and what each platform service
  needs from the package.
- An extension template (`templates/extension`): a minimal third-party
  crate with a plugin, a component drawn from component tokens with
  system tokens as fallbacks, its strings in English and French, a
  gallery story, a step, a command for agents and headless kittest
  tests. A workspace member, so every commit builds it; start from it
  with `cargo generate --git https://github.com/quazardous/egui-pop
  templates/extension`.
- A registry of extensions (docs/extensions.md): official, community
  and featured tiers with a promotion path, the entry format, how to get
  listed and the checks an entry passes (licence, versions, plugin
  rules, token contract, accessibility, strings), and the applications
  built with egui-pop.
- Governance (GOVERNANCE.md): who decides, decisions recorded as ADRs
  with a request for comments for the extension contract and
  `egui-pop-api`, what counts as breaking for an extension (the host
  contract, tokens, steps, commands, strings, the debug bridge), and
  deprecation windows of one minor (one egui cycle). CONTRIBUTING.md has
  a section for extension authors; docs/upstream.md says how a limit of
  egui is handled, from the workaround to its removal.

### Changed

- Removing every key of a command needs no placeholder key: a removal
  (`"-id"`) has empty keys; the keymap document is version 2 and older
  ones migrate.
- The README is short: a goal, a quickstart, four snippets, a gallery of
  screenshots. The rest moved to guides in `docs/` (getting started,
  themes, plugins and steps, the viewport, debugging, the gallery), listed
  in `docs/README.md`; their Rust blocks are doctests in `consumer-smoke`.
- A toast's text wraps in the width its close button and action leave,
  no longer running under them (gallery: a long warning in English and
  a long error in French).
- `Choose::options` holds `ChooseOption`s, no longer strings, and a
  choice's command gets the option's value (`value`) besides `choice`
  and `label`.
- A host's journal is its own business: `Host::journal` and
  `journal_mut` are optional (`Option`, `None` by default), and
  `publish`, `since` and the new `last_seq` answer `None` when the host
  keeps no journal, egui-pop then keeping one in memory as `LocalHost`
  does. A host journaling on its own bus answers them from it, and
  `AppHandle::last_event` reads the host's last sequence number instead
  of an empty journal.
- The application template's manifest is `template/Cargo.toml.liquid`
  (cargo-generate renders it to `Cargo.toml`): a workspace depending on
  egui-pop by git no longer reads a package named `{{project-name}}` and
  says "invalid character `{` in package name" on every cargo run. A
  test keeps placeholders out of the repository's manifests.
- The shell's side bar and inspector are resizable workbench areas; a
  panel's area is its default one, not where it must stay.
- Control heights and icon sizes are multiples of 4 points, so they
  are whole pixels at every quarter scale: controls 24 (compact 20),
  icons 16, 20 and 24 (compact 12, 16 and 20); a token test keeps it.
- Edit > Undo and Redo are now `edit.undo` and `edit.redo` and undo the
  application's history through its host, no longer the interface's
  store directly. The settings window has its own Undo and Redo buttons
  for the settings, running the store's `value.undo` and `value.redo`,
  now hidden from the palette.
- A broken token is reported once by the resolver, not again for each
  token aliasing it.
- `zones::SHELL` holds the new `TITLE` zone, drawn above the menu bar
  when it has blocks.
- The slider follows the shared interaction grammar: Ctrl (Cmd) is
  finer and Shift coarser for drags, arrows and the wheel alike; the
  wheel scrolls the panel and steps a slider only when it has the focus
  or Alt is held over it; Escape during a drag puts the value back with
  no undo step (`Store::cancel`, `binding::cancel_gesture`). The keys
  are data (`slider::Gestures`, `set_gestures`) a profile may remap.
- `App::run` and `App::run_in` return a `host::Reply` (`Reply::now()`
  for a result that is already there), as commands may answer later.
- The palette is no longer built into `App`: add `DefaultPlugins` (or
  `PalettePlugin`) to keep it; `AppHandle::palette_open` became
  `palette::is_open(app)`, and `CMD_PALETTE` moved to `palette`
  (re-exported from `app`).
- `egui_pop_core::icon`: the former `Icon` (a glyph, an image or a
  painted shape) is now `Source`, its size `Role` is now `Size`, and the
  `Shape` enum is folded into the new `Icon` roles.
- The gallery's combo boxes, text fields and sliders are labelled for
  screen readers.
- `theme` moved to `egui-pop-core`; `egui-pop` is now the facade and
  re-exports it at the same path, with `model` and `core`.

- The repository is a Cargo workspace; the `egui-pop` crate moved to
  `crates/egui-pop`.

- The input fields' tokens are the kit's `field` component: `field.rest.fill`,
  `field.rest.text`, `field.rest.border-color`, `field.hover.border-color`,
  and its tones for the states every field shows (`muted`: read-only fill
  and placeholder; `accent`: modified; `warning`: required; `danger`:
  invalid; `info`: pending). The flat names (`field.background`...) still
  work, each reported.
- The `dialog.header`, `dialog.header-text`, `palette.header` and
  `palette.header-text` tokens are renamed into the kit
  (`header-band.rest.fill`, `dialog-header.rest.text`,
  `header-band.rest.text`); a theme giving them still works, with a
  reported problem. `band::BandStyle` keeps only the band's place
  (`padding`, `gap`): the kit draws it, and `band::begin` tells the row
  the band's state. A refused dock preview is outlined in the danger
  colour over a dimmed area instead of a red wash; the band's buttons'
  focus ring is two whole pixels instead of a blurred 1.5 point line.
- `LibraryTokens` keeps the marks' colours only (stars, flags, labels,
  the rubber band) and `ThumbnailTokens` is gone: the boxes are the
  kit's. The `thumbnail.background`, `hover` and `selected` tokens and
  the optional `library.overlay`, `overlay-text`, `selected-background`,
  `dim`, `stack`, `best`, `chip` and `chip-text` were renamed into the
  kit (`egui_pop_tokens::RENAMED`): a theme giving them still works,
  with a reported problem.
- Tokens: `font.size.small`, `body`, `heading` and `mono` became
  `font.text-style.small`, `body` (and `button`), `heading` and
  `monospace`; a theme still giving an old name works, with a reported
  problem (`RENAMED`).
- The title bar's line is the kit's (`title-bar.rest.border-width`, 1 in
  the default kit): egui's panel line is no longer drawn under it, nor
  room kept for it when the title zone draws nothing.
- The shipped themes, densities and default kit are `.tokens.yaml`
  files; a kit folder's file is `kit.yaml` (`kit.yml` and `kit.json`
  are read too). `Themes::add_json` is now `Themes::add_text` and
  `KitData::from_json` is `KitData::from_text`: both read YAML or JSON.
- Renamed into the kit's grid: `button.{background, background-hover,
  background-active, text, border, radius}` (`button.rest.fill`...),
  `menu.item.*` (`menu-item.hover.fill`, `menu-item.rest.text`,
  `menu-item.tone.muted.text`). The high-contrast themes outline text
  fields as well as buttons.
- Renamed into the kit's grid, the former names still working with a
  reported problem: `progress.*` (`progress.track` is
  `progress-bar.rest.fill`...), `dialog.{background, border, radius,
  padding, backdrop}`, `palette.{background, border, padding}`,
  `toast.{background, border, text, padding}`, `editor.background`,
  `terminal.background`. `progress::Tone` is `kit::Tone` and gains
  `Neutral`.
- Renamed into the kit's grid, the former names still working with a
  reported problem: `slider.track` (`slider.rest.fill`), `slider.fill`
  (`slider-fill.rest.fill`), `slider.thumb` (`slider-thumb.rest.fill`),
  `skeleton.fill` (`skeleton.rest.fill`), `skeleton.shimmer`
  (`skeleton-shimmer.rest.fill`).
- `egui_pop_core::kit::paint_ring`: a ring inside its box follows its
  corners.
- `egui_pop_tokens::CONTRACT` is replaced by `contract()`: the default
  kit's components keep their contract and their tokens in one file each
  (`contract/kit/`, `themes/kit/`), so components can be added
  independently (`docs/kit.md`, "Adding a component").
- The `titlebar.*` tokens are renamed into the kit's grid:
  `title-bar.rest.fill`, `title-bar.rest.text`, `title-bar.height`,
  `window-button.{rest,hover}.{fill,text}`, `window-close.*`,
  `window-button.width`, `window.grip`.
- The viewport takes tiles in off the interface thread by default:
  `ViewportState` converts each tile on the thread that delivers it
  (`set_prepare_on_worker` is on) and uploads at most
  `UploadBudget::DEFAULT` (8 MiB) a frame, the visible area first; the
  rest goes up in the next frames. `UploadBudget::default()` is that
  budget. Opting out: `set_prepare_on_worker(false)` and
  `set_upload_budget(UploadBudget::UNLIMITED)`. A source delivering a
  whole view at once may now take a few more frames to show it; tests
  stepping a fixed number of frames may need more. The gallery's
  viewport page shows the tiles waiting for their upload, and the
  `threading` example measures the old intake as `tiles-ui`,
  `tiles-views-ui` and `all-ui`.

- `PanelSpec::refuses` takes a host (`PanelHost`: an area, floating
  or a window, with the host's offer) instead of an area, and a panel
  may declare its content's needs (`PanelSpec::content`): moving,
  floating or detaching a panel is refused, with the rule, when its
  content does not fit the host (ADR 0013).

- A display target declared `TargetSource::Tagged` needs no wgpu colour
  space: any sRGB, Display P3 or BT.2020 primaries with a transfer the
  Wayland colour-management protocol names (BT.2020 with a 2.2 power,
  for one), on a format that carries it, is drawn faithfully
  (`DisplayTarget::untaggable` says why not).

- The gallery page (`docs/gallery.md`) shows every snapshot the gallery's
  tests keep, page by page; a test keeps it complete.

- An application's catalogue defining a key egui-pop's own catalogue has
  overrides it with a warning (`I18n::overridden`, an `i18n.overridden`
  event) instead of failing the plugin; egui-pop's catalogues are base
  catalogues (`Catalogue::as_base`, `Catalog::add_base`). A key the
  application defines twice is still refused.
- Two bindings of one key whose `when` clauses can never both hold
  (`view == 'masks'` and `view == 'lighttable'`, `x` and `!x`) are no
  longer a conflict (`Keymap::conflicts`, the shortcuts editor);
  `When::excludes` tells, conservatively.
- `viewport::HandleMove` (what `before_handle` gets) carries where the
  handle was when the gesture began (`start`) and the keys held
  (`modifiers`).
- Viewport overlays: shape lines (1.5 points over a 3-point outline),
  handle outlines, the comparison's divider and the focus ring are
  snapped to whole physical pixels with `scale::stroke`, so they no
  longer blur at 125 % and 150 % (at 100 % the line is one pixel).
  `overlay::LINE_WIDTH`, `UNDER_WIDTH` and `OUTLINE_WIDTH` name the
  widths before snapping.
- The slider no longer lets the panel drift after the wheel stepped it:
  while it holds the wheel (focused, or Alt held over it) it eats what
  is left of a notch's smoothed scroll every frame, not only the frame
  of the notch.
- `AppBuilder::run_native_with` takes a fallible `make`: it returns
  `Result<Box<dyn eframe::App>, MakeError>`. An error is logged,
  published on the journal as `app.make_failed` and returned as eframe's
  `Error::AppCreation`. An infallible `make` wraps its result in `Ok`.
- A panel's commands (`<panel>.focus`, `<panel>.toggle`) no longer carry
  the English panel title in their translated texts: they read the
  shared `egui-pop-panel-focus-title`, `-focus-description`,
  `-toggle-title` and `-toggle-description` texts, whose `{ $panel }` is
  the panel's title in the interface's language
  (`strings::PanelCommand`). The extractor leaves their own keys out;
  a translation under them still wins. The English and French
  catalogues have the shared texts.
- The window's own title bar is not drawn in full screen (`F11`), in the
  main window and in a detached panel's; it publishes no caption then,
  so the system's hit test sees only the client area.
- The built-in themes' screenshots (docs/themes.md) are full size, 1282
  px wide at one pixel per point, each shown at the page's width and
  linked to its image; they were reduced to 400 px. docs/tokens.md
  points to them instead of three squeezed overviews.
- `Keymap::rebind` keeps the scope of the binding it replaces (the
  highest layer's below): the user's new keys for a surface or system
  shortcut stay in that scope instead of becoming global.
- A form's Browse button no longer opens the system's chooser on the
  interface thread (it held the frame): it goes through the file
  chooser, its in-app picker drawn inside the dialog.
- `display::Matrix` maps the scene to linear sRGB, then to the target's
  primaries (the same as before for sRGB targets); `Stage` has a
  `white_nits` field (SDR white for PQ) and `Transfer` the `ExtSrgb`
  variant; the GPU path's uniform grew by one vector.
- Strings (`egui_pop_core::strings`): keys are Fluent message ids
  (`egui-pop-retry`, was `egui-pop.retry`) and placeholders are written
  `{ $name }`, so a provider reads `.ftl` catalogues as they are.
  `Strings::get_args` lets a provider format arguments itself (plurals,
  numbers); `lookup` and `lookup_args` take any key, falling back to the
  English, then to the key. What plugins declare is translated through
  keys derived from stable ids (`command-<id>.title`, `panel-<id>.title`,
  `menu-<id>.label`, `setting-<id>.label`, `plugin-<id>.title`,
  `category-<english>`), with `command_title`, `panel_title` and their
  siblings.
- Core widgets show their text through `egui_pop_core::strings`: the
  window frame's buttons (`MINIMIZE`, `MAXIMIZE`, `RESTORE`, `CLOSE`) and
  a bound widget's missing value (`NO_VALUE`).
  `param::param_shown` draws a parameter with its choices shown through
  a function (a translation), the value kept raw.
- Viewport: its default accessible names and the histogram's value are
  read through `egui_pop_core::strings`; `egui_pop_viewport::strings::ALL`
  lists them.
- The application interface's own text (the palette's prompt, the
  settings window and the shortcuts editor, dialogs' and the file
  picker's buttons, toasts, devtools, the workbench's tab menu, menus'
  reasons, the frame's window menu, the way out of full screen) is read
  through `egui_pop_core::strings`, by the keys of the new
  `egui_pop_api::strings` module, whose `ALL` lists them for catalogues.
  English is unchanged.
- What plugins declare is shown in the interface's language where it is
  drawn, through the derived keys of `egui_pop_core::strings`: command
  titles, descriptions and categories (menus, context menus, the palette,
  the shortcuts editor, undo and redo labels through
  `AppHandle::command_title`), submenu labels (the menu bar), panel titles
  (tabs, the activity bar, detached windows, fault cards), settings'
  labels, descriptions and choice values, and their sections (a plugin's
  title). The palette ranks the label it shows (`palette::rank_with`,
  `palette::shown`). Specs, palette entries, settings entries, stored
  values, the journal and the debug bridge stay English.

### Fixed

- Kits: a frame egui lays out (`Component::frame`, now given the pixels
  per point, and `Component::show`) has its border snapped to whole
  physical pixels, as the kit's own painting does, instead of a blurred
  fraction at scales like 1.25; a box painted under a fading or disabled
  `ui` (`Component::paint_under`) fades once, not twice.
- An application icon drawn at a size the theme does not list (15 points,
  a kit's `indicator-size`) is rasterised at that size instead of scaled
  from the next one, which blurred it.
- The title bar's buttons and items are centred above its line, not on
  the line's row as well.
- The `Choose` dialog's rows keep their size as the pointer moves over
  them (the dialog no longer changes height by a few pixels), and their
  text is left-aligned and wraps, a single-line row included.

- Menus, context menus, command buttons, the tray and global shortcuts
  show a command's title without the trailing hint of its arguments
  (``Open a workspace (`path`)`` reads `Open a workspace`), in every
  language: the hint is dropped after translation, so an application no
  longer passes an English label to hide it. The palette keeps it. New
  `egui_pop_core::strings::without_argument_hint` and
  `AppHandle::command_menu_title`.

- A vetoed exit no longer saves: `App::save` ran before the `app.exit`
  step, whatever its hooks decided. The step's default now writes what
  is still waiting, and what changed was written a second after it
  changed, so saving no longer depends on the exit.

- The space between a slider's value and its unit (`EV`, `%`, `px`) is
  the interface language's: French puts a narrow no-break space
  (U+202F), English keeps a plain space. New `Strings::quantity` (a
  provider's say, a plain space by default) and
  `egui_pop_core::strings::quantity`; `egui_pop_i18n::format::quantity`
  and `Separators::unit` give the locale's.

- A slider's typed entry takes the keyboard only when it opens: a click
  on another widget (or Tab) now commits the value typed, closes the
  entry and leaves the keyboard there, where the entry used to take it
  back on the next frame. Escape still closes it with nothing.

- A plugin's own title key (`plugin-<id>`) counts as owned by the plugin,
  so `add_owned` accepts it.

- A slider's typed entry owns Escape and Enter while it is open:
  Escape cancels it and Enter commits it even in a host whose key
  dispatcher reads the keys before the widgets are drawn, and no key
  binding runs. With the focus plugin, Escape used to take the focus
  from the entry, which then committed what was typed. New
  `focus::claim_keyboard`, `focus::text_input_owns_keyboard` (a text
  field focused, or an open input's claim) and `focus::escape_left`;
  egui-pop's key dispatcher leaves Escape and Enter (without Ctrl or
  Alt) to a text input that owns the keyboard, and a host with its own
  dispatcher asks `text_input_owns_keyboard` the same way.

- A slider's typed entry commits only on a real blur: a click elsewhere,
  Tab, or the focus taken by another widget. A frame where nobody holds
  the focus (a panel redrawn for another writer's change, the field not
  drawn for a pass) no longer commits it: the entry takes the focus
  back and waits a frame, and commits if the focus is still gone.

- Icon strokes are whole physical pixels at the window's scale: the
  painted shapes (`icon::paint_fallback`, and `Icon::paint` without the
  icon font) snap their stroke width (`max(side / 11, 1.25)` points) with
  `scale::snap`, and so does a focused icon's ring, so lines stay crisp
  at 1x, 1.5x and 2x.

- The viewport draws "before" alone when the application passes
  `Compare::Before`; only the held `before` key did, and the "after"
  image was drawn instead (`egui-pop-viewport`).
