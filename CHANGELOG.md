# Changelog

All notable changes to the egui-pop crates. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/); versions follow
[Semantic Versioning](https://semver.org/), with one egui-pop minor per
egui minor.

## Unreleased

### Added

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

### Changed

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
