# Changelog

All notable changes to the egui-pop crates. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/); versions follow
[Semantic Versioning](https://semver.org/), with one egui-pop minor per
egui minor.

## Unreleased

### Added

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

### Changed

- `Keymap::rebind` keeps the scope of the binding it replaces (the
  highest layer's below): the user's new keys for a surface or system
  shortcut stay in that scope instead of becoming global.
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

## 0.1.0 (prepared, not published)

The first release: the tokens and themes, the framework (plugins,
commands, keymaps, zones, the host), the components of the paved road
and the workbench. Publishing is the owner's call.

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
