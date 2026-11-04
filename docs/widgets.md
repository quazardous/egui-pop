# Advanced widgets: a survey, and what egui-pop should have

Status: **survey** (2026-10-12, crates refreshed 2026-11-03). It
answers which advanced widgets egui-pop should have, progress bars and
spinners first, and whether each one is egui's built-in, a crate to
adopt or wrap, or our own. Each crate's latest version, its date, the
egui version it requires and its licence were read from crates.io on
2026-11-03. egui-pop is on egui 0.36; egui 0.37.0 was released on
2026-10-15, and what it brings and what holds the move back are in
sections 2.3 and 2.4.

The first application is a photo editor: a darkroom (image viewport,
parameter sliders, curves, scopes), a lighttable (tens of thousands of
thumbnails, ratings, labels, tags, filters), a graph of modules, and long
background jobs (renders, exports, imports, GPU work).

**What decides**, as for the rest of egui-pop:

- **Values live outside the widget.** A widget reads a value from the
  store or the host and writes through a command (ADR 0009, ADR 0010).
  Crates that keep their own model (a string queue, their own graph,
  their own tree) are, at most, drawing helpers.
- **Progress comes from the host as events.** egui-pop never polls a
  job. The host publishes events, egui-pop builds a model from them and
  draws it (ADR 0010).
- **Everything can be inspected and driven.** Each widget is named for
  AccessKit, shows up in the debug bridge, and gives the same result in
  a headless test on every run.
- **Theme tokens only.** Colours, sizes and motion come from tokens;
  reduced motion is a theme setting already (`motion.duration` is 0).
- **No silent no-op** (ADR 0003): a widget that cannot show what it was
  given says so.

## 1. Catalogues of other libraries

Each line lists only what matters here. The names are the libraries' own.

| Library | Progress and waiting | Other advanced widgets worth noting |
|---|---|---|
| **shadcn/ui** (on Radix primitives) | `Progress`, `Spinner`, `Skeleton`, `Sonner` (toasts with a promise state: loading, then success or error) | `Command` (palette), `Empty`, `Kbd`, `Item`, `ButtonGroup`, `InputGroup`, `Resizable`, `Toggle Group`, `Hover Card`, `Data Table` (TanStack), `Sidebar`, `Field` |
| **MUI** (Material UI, MUI X) | `LinearProgress` (determinate, indeterminate, **buffer**, query), `CircularProgress`, `Skeleton`, `Backdrop`, the `loading` prop of `Button`, `Snackbar` | `Rating`, `Chip`, `Badge`, `Stepper`, `ToggleButtonGroup`, `Slider` (range, marks, vertical), `Transfer List`; MUI X has `DataGrid`, `TreeView`, date pickers and charts |
| **Mantine** | `Progress` (**sections**, striped, animated), `RingProgress`, `SemiCircleProgress`, `Loader`, `LoadingOverlay`, `Skeleton`, `loading` prop on `Button`, notifications that update from loading to done | `SegmentedControl`, `Switch`, `Chip`, `Badge`, `Pill`, `TagsInput`, `Rating`, `ColorPicker`, `AngleSlider`, `RangeSlider`, `Tree`, `Stepper`, `Timeline`, `Indicator`, `Spoiler`, `Kbd` |
| **Ant Design** | `Progress` (line, circle, dashboard, **steps**, a success segment), `Spin` with a **`delay`** (shows only after a delay, so there is no flicker), `Skeleton`, `Button loading`, `message.loading` | `Segmented`, `Rate`, `Tag`, `Badge`, `Empty`, `Result`, `Steps`, `Statistic` (countdown), `Splitter`, `Tree`, `TreeSelect`, `Cascader`, `Transfer`, `Tour`, `Descriptions`, `Upload` (per-file progress) |
| **Blueprint** (data-dense, desktop-like) | `ProgressBar` (intent, stripes), `Spinner` (also **determinate**, with a value), progress inside a `Toast` (the documented upload example) | `NonIdealState` (empty or error state), `EditableText`, `TagInput`, `Breadcrumbs` (collapse into an overflow menu), `OverflowList`, `Tree`, `Table` (virtualized, frozen columns), `Omnibar`, `HotkeysDialog`, `SegmentedControl`, `MultiSlider`, `NumericInput`, `PanelStack2`, `MultistepDialog`, `EntityTitle` |
| **Fluent UI v9** | `ProgressBar` (indeterminate when it has no value, thickness, validation state), `Spinner` (label and its position), `Skeleton`, `MessageBar`, `Toast` | `Tag`, `TagPicker`, `InteractionTag`, `Badge`, `CounterBadge`, `Rating`, `SwatchPicker`, `Tree`, `FlatTree`, `DataGrid`, `Toolbar`, `SearchBox`, `InfoLabel`, `TeachingPopover`, `Drawer`, `Nav`, `Field` |
| **Carbon** (IBM) | `ProgressBar` (status: active, finished, error; helper text), **`InlineLoading`** (inactive, active, finished, error, with a description: for buttons and rows), `Loading` (overlay or small), `SkeletonText`, `SkeletonPlaceholder`; a written [loading pattern](https://carbondesignsystem.com/patterns/loading-pattern/) | `ProgressIndicator` (a stepper), `ContentSwitcher` (segmented), `Toggle`, `Tag` (dismissible, operational), `Notification` (inline, toast, actionable), `DataTable` (batch actions, expandable rows, toolbar search), `TreeView`, `FileUploader` |
| **libadwaita / GTK4** | `GtkProgressBar` (fraction, or `pulse()` for indeterminate), `GtkLevelBar` (a meter with offsets), `AdwSpinner` (libadwaita 1.6), `AdwToast` with a button, `AdwBanner` | `AdwStatusPage` (empty state), `AdwToggleGroup` (segmented, 1.7), `AdwSplitButton`, rows (`AdwSwitchRow`, `AdwSpinRow`, `AdwEntryRow`, `AdwExpanderRow`), `AdwPreferencesDialog` with search, `AdwNavigationSplitView`, `AdwTabView`, `AdwTabOverview`, `AdwBottomSheet`, `AdwWrapBox`, `AdwShortcutsDialog`; GTK's `GtkListView`, `GtkGridView` and `GtkColumnView` recycle row widgets (virtualized), and `GtkScale` has marks and a fill level |
| **Qt Widgets / QML** | `QProgressBar` (busy when min = max = 0), **`QProgressDialog`** (only shows when the operation will last longer than `minimumDuration`, 4 s by default), QML `BusyIndicator`, `ProgressBar` (indeterminate), **`DelayButton`** (fills while held); KDE's `KJob` and its job tracker gather the jobs of every application in one notification area | `QSplitter`, `QDockWidget`, model/view `QTreeView` and `QTableView` (virtualized), `QDial`, `QKeySequenceEdit`, `QUndoView`, `QCompleter`, `QColorDialog`; QML `RangeSlider`, `Switch`, `Tumbler`, `SplitView`, `TreeView`, `TableView` |
| **Dear ImGui** | `ProgressBar` (a negative fraction animates it as indeterminate, since 1.90), no spinner built in; [imspinner](https://github.com/dalerank/imspinner) has dozens | Extensions: ImPlot, ImGuizmo (gizmo, `ImSequencer`, `ImCurveEdit`, `GraphEditor`), imnodes, thedmd's imgui-node-editor, imgui-knobs, imgui_toggle, ImGuiFileDialog, ImGuiColorTextEdit, imgui_markdown, the curve and gradient editor gists; Tracy's timeline as the reference for dense timelines |
| **JetBrains IDEs** | Background tasks in the status bar: a small progress bar and a counter; a click opens the list of tasks, each with cancel (and pause for indexing). A modal task has a **"Background"** button that moves it to the status bar. A thin indeterminate line under a tool window's title | Tool windows (dock, float, split), Search Everywhere, the Notifications tool window (history), breadcrumbs, inline hints |
| **VS Code** | `window.withProgress` locations: **Notification** (a toast with a bar and Cancel), **Window** (a status bar spinner), **SourceControl**, and a view's progress (a thin moving line under the view title, and a badge on its activity bar icon); the notification centre keeps a history | The status bar, the quick pick and palette, breadcrumbs, the minimap, peek views, tree views with inline actions, the activity bar with badges |
| **Photo applications** (the domain's own references) | **Lightroom Classic**: an activity area at the top left stacks one bar per running task, each with a cancel cross. **Capture One**: an Activities window lists the process queue with its history. **darktable**: a "background jobs" module, one bar per job with cancel (its `dt_control_progress` API) | Filmstrip, navigator, histogram, curves, colour wheels, star ratings and colour labels on thumbnails, darktable's bauhaus sliders |

Patterns that come back across libraries:

- **Three ways to wait:** a spinner (indeterminate), a bar (determinate),
  a skeleton (the content's shape). Carbon and Fluent write the rule
  down: indeterminate when the length is unknown, determinate as soon as
  it is known.
- **Flicker control:** Ant's `Spin delay`, Qt's
  `QProgressDialog::minimumDuration`, VS Code's internal progress
  `delay`, and the web's "spin delay" idea (a delay before showing, then
  a minimum time on screen). The thresholds come from
  [Nielsen Norman Group](https://www.nngroup.com/articles/progress-indicators/):
  nothing under about 1 s, a looped indicator from 2 to 10 s, a
  percent-done bar beyond that.
- **Busy controls:** the button or row that started the work shows it
  (MUI, Mantine and Ant `loading`, Carbon `InlineLoading`). A modal can
  be sent to the background (JetBrains).
- **Jobs gathered in one place:** JetBrains background tasks, KDE's job
  tracker, Lightroom's activity area, Capture One's Activities,
  darktable's background jobs, and the VS Code and Fluent notification
  centres for the history.
- **Data-dense desktop libraries** (Blueprint, Qt, GTK, JetBrains)
  matter more for us than marketing-site kits: tag input, editable text,
  non-ideal state, virtualized trees and tables, multi-handle sliders,
  overflow lists.

## 2. The egui ecosystem

### 2.1 What egui 0.36 has

egui-pop uses egui 0.36.2 (2026-09-08). The latest release is egui
0.37.0, with eframe, egui_extras and egui_kittest 0.37.0 (2026-10-15);
section 2.3 says what it changes for the gaps below.

| Built in | What it does | Gaps for us |
|---|---|---|
| `ProgressBar` | A fraction, text or percentage, fill colour, corner radius. `animate(true)` pulses the fill and draws a spinning arc at its end. AccessKit role `ProgressIndicator`, with the percentage as its numeric value | **No indeterminate mode**: `animate` decorates a determinate bar. No tone (success, warning, error), no buffer or sections, no reduced motion. It asks for a repaint every frame while animated, and it reads `input.time`, so a snapshot depends on the clock |
| `Spinner` | An arc spinning on `input.time`, size and colour, role `ProgressIndicator` | No label (so an unnamed node in our accessibility check), no delay, no reduced motion. It repaints every frame while visible |
| `Slider`, `DragValue` | Logarithmic scale, clamping modes, step, trailing fill, a value shown with its prefix and suffix | No soft and hard range, no fine adjustment, no reset, no gradient track, no range (two handles). **Own** (2026-10-13): egui-pop's slider family covers them all (see "Sliders" below) |
| `color_picker`, `color_edit_button_*` | HSV picker, alpha | Not colour-managed; no wheels |
| `Modal`, `Popup`, tooltips, menus, `ComboBox` | Already the base of egui-pop's overlays | (egui-pop's overlay stack) |
| `ScrollArea::show_rows`, `show_viewport` | Virtualized scrolling | A grid and its selection are ours |
| `Scene` | A container you can pan and zoom | A starting point for the graph view and the minimap |
| `Image` | Shows a spinner while its loader works | A spinner per thumbnail is noise; we want a placeholder |
| `selectable_value`, `toggle_value`, `Checkbox` | The building blocks of a segmented control | A toggle switch exists only as a demo (`egui_demo_lib`, `toggle_switch.rs`) |
| egui_extras 0.36.2 | `TableBuilder`, `StripBuilder`, `DatePickerButton` (feature), image loaders, syntax highlighting | |

### 2.2 Crates

Version and date: the latest release on crates.io; egui: the version it
requires. A verdict changed on 2026-11-03 says why.

| Crate | Version (date) | egui | Licence | What | Verdict |
|---|---|---|---|---|---|
| egui_plot | 0.38.0 (2026-10-19) | 0.37 | MIT OR Apache-2.0 | Lines, bars, boxes | Debug panels only; our scopes are images fed by the engine |
| egui_dock | 0.21.1 (2026-08-06) | 0.36 | MIT | Docking, tabs, splits | Adopted, then **replaced** by [egui_stack](https://github.com/quazardous/egui_stack), its continuation on egui 0.37 (changed 2026-11-03: egui_dock has no release for 0.37; [#145](https://github.com/quazardous/egui-pop/issues/145)). egui-pop's areas are still docks of their own (see `upstream.md`) |
| egui_tiles | 0.18.0 (2026-10-19) | 0.37 | MIT OR Apache-2.0 | Rerun's tiling, now in the rerun-io/egui_crates repository | Not needed next to egui_stack; a source of ideas and code for it, with credit ([directions](egui-stack-directions.md)) |
| egui-notify | 0.24.0 (2026-10-30) | 0.37 | MIT | Toasts | Ideas only, now taken: the slide, the corners and the toast's states are in egui-pop's toasts, with credit (`crates/egui-pop-api/src/toasts.rs`, `NOTICE`). Not a dependency: it keeps a string queue, with no commands and no state to inspect, and 0.24 pulls egui-phosphor from git |
| egui-toast | 0.22.0 (2026-08-08) | 0.36 | MIT | Toasts | Same as egui-notify (ideas only) |
| egui-snarl | 0.12.0 (2026-08-26) | 0.36 | MIT OR Apache-2.0 | Node graph that owns `Snarl<T>` | Reference for wire and pin interaction; the graph view is **our own** |
| egui_node_editor | 0.11.0 (2026-08-21) | 0.36 | MIT | Node editor | Too young (few downloads); reference at most |
| egui_graph_edit | 0.9.1 (2026-07-13) | 0.35 | MIT | Node graphs | Same |
| egui_graphs | 0.32.0 (2026-08-31) | 0.36 | MIT | A petgraph viewer | Not an editor |
| egui_ltreeview | 0.10.0 (2026-10-25) | 0.37 | MIT | Tree view: multi-select, keyboard, drag and drop, context menu; the caller keeps the data | **Own** (decided 2026-10-13, see "Data views" below); the drop zones (before, inside, after) follow its idea |
| egui_table | 0.11.0 (2026-10-19) | 0.37 | MIT OR Apache-2.0 | Rerun's virtualized table (sticky header, resizable columns), now in rerun-io/egui_crates | **Wrapped** (decided 2026-10-13): `egui-pop-views`' data table |
| egui_extras `TableBuilder` | 0.37.0 (2026-10-15) | 0.37 | MIT OR Apache-2.0 | egui's own table: rows in view, resizable columns, striped | **Skip** (2026-10-13): widths only in egui's memory, no sticky columns |
| egui_virtual_list | 0.13.0 (2026-10-19) | 0.37 | MIT | Virtual list with rows of varying height, measured and cached | **Own** the thumbnail grid (decided 2026-10-13, see docs/library.md): cells of one size need no measuring, and End must reach the last of 100 000 images at once. Kept as the candidate for lists of rows of varying height (history, logs) |
| egui_infinite_scroll | 0.13.0 (2026-10-19) | 0.37 | MIT | Pages loaded on scroll | Ideas only: our fetches are windowed by the host |
| egui_dnd | 0.18.0 (2026-10-19) | 0.37 | MIT | Reorder by drag and drop, animated | **Adopt** for reorderable lists (export targets, module order in a stack); the order is written back through a command. Not a dependency yet |
| egui-modal | 0.6.0 (2024-12-21) | 0.30 | MIT | Modals | **Dead**: egui's `Modal` replaces it |
| catppuccin-egui | 5.7.0 (2025-11-15) | 0.26 to 0.33 | MIT | Themes | Stale; our tokens replace it |
| egui_double_slider | 4.0.0 (2026-08-19) | 0.36 | **Apache-2.0** only | A range slider | **Skip** (2026-10-13): ideas only for the range slider (see "Sliders" below) |
| egui_fader | 0.1.2 (2025-06-12) | 0.31 | MIT | An audio fader | **Skip** (2026-10-13): a vertical audio idiom, no soft range or typed entry |
| egui_knob | 0.6.3 (2026-09-01) | 0.36 | MIT | Knob | Skip (knobs fit audio, not photos) |
| egui_colors | 0.14.0 (2026-10-16) | 0.37 | MIT | 12-step palettes | Theme work only |
| egui_colorgradient | 0.2.3 (2025-07-25) | 0.32 | MIT | Gradient editor | Reference only |
| egui-keyframe | 0.1.0 (2026-02-13) | 0.33 | MIT OR Apache-2.0 | Keyframes and curve editing | Reference for curve handles; too young |
| egui_timeline | 0.5.0 (2026-10-26) | 0.36 | MIT OR Apache-2.0 | Timeline for media (nannou) | Reference only |
| egui_chip | 0.3.1 (2025-11-16) | 0.33 | MIT OR Apache-2.0 | Chips | Too small to depend on; ideas only |
| egui-dropdown / egui_autocomplete | 0.16.0 (2026-09-04) / 15.0.0 (2026-09-06) | 0.36 | MIT | Autocomplete text fields | Ideas for the tag input; our palette's fuzzy matcher is enough |
| egui-multiselect | 0.8.0 (2026-04-01) | 0.34 (through eframe) | MIT | Multiselect | Skip |
| egui_suspense | 0.14.0 (2026-10-19) | 0.37 | MIT | Loading and error UIs for async data | Ideas only (the loading, error and retry triad); progress comes from the host |
| egui_inbox | 0.14.0 (2026-10-19) | 0.37 | MIT | A channel that wakes egui | The host's `Waker` already does this |
| egui-async | 0.8.0 (2026-10-19) | 0.37 | MIT OR Apache-2.0 | Async helpers | Skip: the host owns execution |
| egui_animation | 0.14.0 (2026-10-19) | 0.37 | MIT | Easing and transitions | Optional; motion must follow tokens and reduced motion |
| egui_flex, egui_taffy | 0.9.0 (2026-10-19), 0.16.0 (2026-10-22) | 0.37 | MIT | Flex or CSS layout | Skip for now (wrapping tag lists may want flex) |
| egui-file-dialog | 0.15.0 (2026-08-08) | 0.36 | MIT | File dialog drawn in egui | **Skip** (changed 2026-11-03: it was to be wrapped as the fallback behind rfd; that fallback is egui-pop's own in-app picker, [Choosing files](files.md)) |
| egui_json_tree | 0.17.0 (2026-08-15) | 0.36 | MIT OR Apache-2.0 | JSON tree with search | **Adopt** for the debug bridge and the journal viewer. Not a dependency yet |
| egui_commonmark | 0.26.0 (2026-10-15) | 0.37 | MIT OR Apache-2.0 | Markdown | **Adopted** (changed 2026-11-03: it was "later"; the viewer's `markdown` feature in `egui-pop-views` uses 0.25, [links](links.md)) |
| egui_logger, egui_tracing | 0.13.0 (2026-10-16), 0.3.0 (2026-04-10) | 0.37, 0.34 | MIT, Unlicense | Log views | Later; the journal viewer is ours |
| egui-phosphor | 0.14.0 (2026-09-10) | 0.36 | MIT OR Apache-2.0 | Icons | **Adopted**; on egui 0.37 through the `egui-pop-phosphor` fork until its pull request for 0.37 is released ([#146](https://github.com/quazardous/egui-pop/issues/146), section 2.4) |
| egui_circular_progress_bar | 2.0.0 (2026-08-19) | 0.36 | **GPL-3.0** | Ring progress | **Skip: GPL-3.0** cannot go into an MIT library. A ring is a few lines of painting anyway |
| egui-data-table | 0.11.0 (2026-08-01) | 0.35 | non-standard | Editable data table | Skip: non-standard licence, little use |
| re_ui | 0.41.0 (2026-10-28) | 0.37 | MIT OR Apache-2.0 (fonts OFL-1.1, Bitstream Vera) | Rerun's UI kit: list items, design tokens | Ideas only (too coupled to Rerun) |

**What we learn:** egui has the primitives (painting, `Popup`, `Modal`,
virtual scrolling, `Scene`, AccessKit) and no advanced widgets that are
maintained. The crates that are maintained and have no model of their
own are egui_dock (now continued as egui_stack), egui_table,
egui_ltreeview, egui_dnd, egui_json_tree and egui-file-dialog. Many
crates followed egui 0.37 within two weeks (egui_table, egui_ltreeview,
egui_dnd); egui_dock, egui-phosphor, egui_json_tree and
egui-file-dialog had not on 2026-11-03. Everything about progress is ours, and is built
([Jobs and progress](progress.md)).

### 2.3 egui 0.37: what it brings us

Read from egui's and egui_kittest's 0.37.0 changelogs (2026-11-03).
Fonts, colour emoji and the fixes egui-pop works around have their own
pages: [Fonts](fonts.md), [Upstream](upstream.md) (each entry says
whether 0.37 removes it), and [Scrolling](scrolling-survey.md). Two
changes are work the bump itself must do, not options: `WidgetType` is
replaced by `egui::Role` (accesskit's `Role`), and `Id::new` is
deprecated (with `Id::unique`, `IdSaltSet` and `IdSaltMap` as the new
ways), while the workspace denies warnings.

| In 0.37 | What it could replace or improve in egui-pop | Follow-up |
|---|---|---|
| `egui::Role` replacing `WidgetType` | `a11y::name` (`crates/egui-pop-core/src/a11y.rs`) takes a `WidgetType`; about 180 uses across the crates and tests. With the role itself, custom widgets can say `Tab`, `Splitter`, `Status` or `Dialog`, which `WidgetType` could not, and the `accesskit_node_builder` calls that set a role by hand (`overlay.rs`, the splitter handle) become ordinary | **Part of the bump** (required) |
| `Response::accessible_name`, `Ui::label_inputs_by` / `Ui::name_inputs` | `egui-pop-test`'s `unnamed_interactive` and `assert_all_named` (`crates/egui-pop-test/src/lib.rs`) read the label or `labelled_by` from the tree; the gallery pairs labels and values with `labelled_by` by hand (`crates/egui-pop-gallery/src/shell.rs`, `pages/builtins.rs`). `label_inputs_by` would name a form's inputs from their row label in one call. egui_kittest 0.37 also fails a test when an input has no name, which overlaps our check (ours covers every interactive role, buttons included, so it stays) | **Yes, small**: use `label_inputs_by` in forms and the settings pages; fix what kittest's new check finds during the bump |
| `Area::role`, named `Ui`, `Area`, `Popup` and `Tooltip`, better popup, menu, modal and panel trees | Dialogs are an `Area` of kind `Modal` (`crates/egui-pop-core/src/overlay.rs`, the modal area) with no `Dialog` role; popovers and hover cards (`overlay.rs`, `hover_card.rs`) and the toast area (`crates/egui-pop-api/src/toasts.rs`) neither. A screen reader would announce a dialog with its title | **Yes**: `Dialog` with the title for dialogs, `Group` or `Tooltip` for popovers, after the bump |
| `StyleProvider` (feature `experimental`): per-widget styles chosen by class | The kits (`crates/egui-pop-core/src/kit/`, [kit.md](kit.md)) paint egui-pop's own widgets from tokens, while egui's built-in widgets only get the global `Visuals` from `theme::visuals_with` (`crates/egui-pop-core/src/theme.rs`). A provider could give `Button`, `Checkbox`, `TextEdit` and `DragValue` their kit parts by class | **Watch**: experimental, behind a feature, its API may change; read it again at 0.38 |
| `RangeSlider` | egui-pop owns its range slider (`crates/egui-pop-core/src/slider/range.rs`): soft and hard ranges, typed entry, fine adjustment, steps and hooks, none of which egui's has | **No**: keep ours; compare its AccessKit output for ideas |
| `CompletionPopup` (completion under a `TextEdit`) | The tag input's completion (`crates/egui-pop-views/src/library/tags.rs`) and the palette (`crates/egui-pop-api/src/palette.rs`) use egui-pop's fuzzy matcher and overlay stack; the code editors' assist (`crates/egui-pop-editors/src/assist.rs`) does not use `TextEdit` | **No** for now: ours is placed, inspected and driven like every overlay. Ideas only |
| kittest videos (feature `video`, `KITTEST_RECORD=1`, ffmpeg) | Interaction tests are read from their code and snapshots ([testing](testing.md)); a video of a failing keyboard walk (`crates/egui-pop-gallery/tests/keyboard_walk.rs`) or of a drag would say more | **Yes, small**: document it in testing.md; never on in CI by default |
| Kinetic scrolling with a rubber band (`KineticScrollStyle`) | Already planned at the bump: off under reduced motion, no bounce on panels of sliders ([Scrolling, item 14](scrolling-survey.md)); `theme.rs` already turns egui's scroll animation off for reduced motion | **Part of the bump** (planned) |
| `InputState::scroll_source` (`MouseWheelSource`: wheel, trackpad, momentum) | `pointer::wheel_pointer` (`crates/egui-pop-core/src/pointer.rs`) guesses a mouse from a scroll in lines and a trackpad from one in points, with a `WheelDevice` setting to override. egui knows the source only on macOS (and winit's pan gestures); elsewhere it says `Unknown`, so the guess stays. Sliders and the viewport could ignore `Momentum`, so a flick does not keep changing a value after the fingers lift | **Yes, small**: use the source when known, ignore momentum on value widgets |
| `Id::unique`, `IdSaltSet`, `IdSaltMap`; `Id::new` deprecated | About 200 `Id::new` calls in 85 files, most for state slots and areas. `Id::unique` makes an id that cannot collide, which suits the overlay stack and the debug bridge's handles | **Part of the bump** (required) |
| Galley cache shared across viewports | Panels floated into OS windows (`crates/egui-pop-api/src/workbench.rs`, `show_viewport_immediate`) no longer evict each other's laid-out text | Nothing to do: free with the bump |

Also of note: `Event::PasteImage` for the clipboard ([clipboard](clipboard.md)),
`KeyboardShortcut::conflicts_with_text_editing` next to the focus rules of
the key dispatcher, `Context::set_repaint_observer`, which could tell the debug
bridge who asked for a frame, and `HarnessBuilder::with_context_setup` for the test
kit's themed harness.

### 2.4 What blocks the move to egui 0.37

Tracked in [#24](https://github.com/quazardous/egui-pop/issues/24).

- **egui_dock** 0.21.1 requires egui 0.36 and has no release for 0.37.
  egui-pop moves to [egui_stack](https://github.com/quazardous/egui_stack)
  instead: egui_dock's continuation (MIT, its history kept), on egui
  0.37, released on crates.io as 0.22
  ([directions](egui-stack-directions.md)).
- **egui-phosphor** 0.14.0 requires egui 0.36. Its upstream pull request
  ([amPerl/egui-phosphor#49](https://github.com/amPerl/egui-phosphor/pull/49))
  moves it to 0.37; until it is released, egui-pop depends on
  `egui-pop-phosphor` 0.15, a fork carrying that pull request,
  published on crates.io under its own name with its library still
  named `egui_phosphor`, so no code changes
  ([#146](https://github.com/quazardous/egui-pop/issues/146)). It goes
  back to egui-phosphor once upstream releases.
- Nothing else in the tree holds it back: egui_table 0.11 and
  egui_commonmark 0.26 already require egui 0.37, so both move with the
  bump.

### Data views: what was checked (2026-10-13)

The tree, the list and the table were checked against what egui-pop's
views must do: branches loaded on demand and asynchronously, edits from
the application at any time, keyboard navigation with type-ahead, drag
and drop gated by a step, rename in place, AccessKit names, 100 000
rows, and every moment a step.

- **egui_ltreeview 0.9 (own).** Its builder is fed every open node on
  every pass (fast, but the caller walks the whole open tree each
  frame), it has no loading state, no rename, no Home/End or
  type-ahead, reads its arrow keys directly from the input, and gives
  its rows no AccessKit names. Wrapping it would leave most of the work
  outside it; the tree is egui-pop's own, a cache flattened once per
  change and drawn from a scroll area that builds only the rows in
  view.
- **egui_table 0.10 (wrap).** Rows in view only, a sticky header,
  resizable columns, horizontal scrolling and prefetch hints; it leaves
  selection, sorting and keys to its delegate, and keeps the widths in
  egui's memory, where the wrapper writes the store's widths and reads
  back the result of a resize. Two things are compensated: the
  resize handles get names, and a row is drawn twice when a sticky
  region is empty (the empty one is skipped).
- **egui_extras `TableBuilder` (skip).** Virtualised and resizable, but
  its widths can only be reset, not set, and it has no sticky columns:
  egui_table does more for the same weight.

### Sliders: what was checked (2026-10-13)

The slider family was checked against darktable's bauhaus slider, the
reference: the label and the value inside one compact row, a soft range
for dragging and a hard range for typed values, a typed value or
expression, fine and coarse modifiers, a double click back to the
default, a quad button (a colour picker or a toggle), a wheel that
scrolls the panel unless the slider has the focus, and a gradient track
for hue and temperature; plus a range (two thumbs) and a logarithmic
bar, AccessKit roles and values, and every finished change a step.

- **egui `Slider` and `DragValue` (own).** `Slider` has a logarithmic
  scale, steps, clamping and AccessKit (role, range, step, increment and
  decrement); `DragValue` has typed entry and drag. Neither has a soft
  and a hard range, a label inside the bar, fine adjustment by modifier,
  a reset, a quad button, a gradient, two thumbs, a wheel that leaves
  the panel alone, or a hook to veto or redirect a change. Wrapping
  them would mean replacing their input handling and their painting:
  egui-pop's slider is its own, and keeps egui's AccessKit properties
  (range, step, increment, decrement, set value).
- **egui_double_slider 4.0 (skip).** A range slider with two handles
  and a filled span, Apache-2.0 only, with no soft range, typed entry or
  hook. The span drawn between the thumbs is the idea kept; none of its
  code is used.
- **egui_fader 0.1 (skip).** A vertical audio fader.
- **Expression evaluators (own).** A typed value takes `+ - * / ^`,
  parentheses and `x` (the current value), in either decimal separator.
  evalexpr is AGPL-3.0 in its current releases and does far more;
  meval (0.2, 2017) is unmaintained. A small parser of about 150 lines is egui-pop's own.

### Inputs and forms: what was checked (2026-10-14)

The inputs were checked against what desktop and web toolkits put
around a field (shadcn/ui's `InputGroup` and `Field`, Fluent's
`SearchBox` and `Field`, Blueprint's `InputGroup` and `NumericInput`,
Carbon's text input states, GTK's entry icons, Qt's `QLineEdit` clear
button): a placeholder; leading and trailing icons, prefix, suffix and
unit; a clear button; attached buttons, a dropdown and a split button;
one state model for every field (modified, required, invalid with a
message, disabled with a reason, read-only, pending) drawn from tokens
with an icon per state, given to AccessKit and to agents; Tab order by
reading order, explicit indices and groups, scopes cycling in a modal,
F6 between panels. Licences are MIT or Apache-2.0 unless said; the
versions, their dates and the egui each requires were refreshed on
2026-11-03, and no decision changed.

| Crate | Version (date), licence, egui | What it has | Decision |
|---|---|---|---|
| egui's `TextEdit` | 0.36 (0.37.0 released 2026-10-15), MIT OR Apache-2.0 | The field: editing, selection, undo, IME, a hint text (AccessKit placeholder), its own prefix and suffix, a frame it can drop | **Wrap**: it is the field inside the decorated input, without its frame; the slots, buttons and states are drawn around it |
| egui's focus | 0.36 | Tab in drawing order; a modal layer keeps the focus inside it; `scroll_to_me` for a widget to call, reaching the innermost scroll area only | **Own** the order on top: reading order, indices and groups per scope, F6; egui's order stays for widgets outside a scope. **Own** the focus coming into view and keeping its place, in the focus plugin ([Tab order](inputs.md#the-focus-keeps-its-place)) |
| egui_form | 0.11.0 (2026-10-19), MIT, egui 0.37 | Validation through `garde` or `validator`: a field wrapper turning red with its message after it loses the focus, and the focus sent to the first invalid field on submit | **Ideas** for the formal form layout (errors after leaving a field, focus to the first error); its validation reports stay the application's, and it has no required or disabled state, no tokens, no Tab order |
| egui-dropdown | 0.16.0 (2026-09-04), MIT, egui 0.36 | A text box with a list of matches under it | **Skip**: the dropdown chevron opens the input's choices; matching as one types is the tag input's and the palette's (the fuzzy matcher) |
| egui_autocomplete | 15.0.0 (2026-09-06), MIT, egui 0.36 | Completion under a text box | **Skip**, for the same reason |
| egui-typed-input | 0.1.3 (2026-03-11), MIT OR Apache-2.0, egui 0.32 to 0.33 | Parsed text inputs that refuse what does not parse | **Ideas** only (an older egui); a number field reads what a slider's typed entry reads |
| egui-probe, egui_struct, egui_field_editor | 0.13.0 (2026-08-26), MIT OR Apache-2.0, egui 0.36; 0.4.2 (2024-07-09), MIT, egui 0.21; 0.7.0 (2026-10-20), MIT, egui 0.37 | Forms derived from structs | **Skip**: forms come from value descriptors (as the `Form` dialog) |
| kylig-ui, functora-egui, egui-antd | 0.9.0 (2026-11-03), 0.2.0 (2026-10-19), 0.1.2 (2026-05-05); MIT; egui 0.36, 0.36, 0.34 | Kits with inputs, selects and buttons in their own style | **Skip**: young, styled outside the tokens, without field states or AccessKit for them |
| egui-modal | 0.6.0 (2024-12-21), MIT, egui 0.30 | Modals | **Dead** (as in section 2.2): egui's `Modal` keeps the focus in its layer |
| egui_path_picker | 0.2.0 (2025-09-02), **GPL-3.0-only**, egui 0.32 | A path field with a browse button | **Skip: GPL** cannot go into an MIT library; the path input with an attached Browse button covers it |

**Decision: own** the decorated input, the field states and the Tab
order, around egui's `TextEdit`; no dependency is added. No code was
taken from any of them.

### Image tools: what was checked (2026-10-13)

The image tools were checked against what photo editors have: a
histogram drawn from the application's bins with clipping markers and
a drag handed back (darktable drags the exposure on it); a curve editor
whose spline is the application's, with points added, moved and
deleted, keys, and the targeted adjustment (a value picked in the
image); lift, gamma and gain wheels and a hue and saturation picker
with a luminance slider; a zoom control and a navigator on the
viewport's own state; AccessKit roles and values, and every finished
change a step. Versions, dates and egui requirements refreshed on
2026-11-03; no decision changed.

| Crate | Version (date), licence, egui | What it has | Decision |
|---|---|---|---|
| egui_plot | 0.38.0 (2026-10-19), MIT OR Apache-2.0, egui 0.37 | Plots: lines, bar charts, its own axes, grid, legend, pan and zoom, box zoom | **Own** the histogram: a plot widget brings axes and its own pan and zoom, while the histogram is a compact strip whose drag belongs to the application; filling the bins takes a mesh of a few dozen lines (kept from the first histogram) |
| egui's colour picker | egui 0.36, MIT OR Apache-2.0 | A saturation and value square with a hue bar (`color_picker_hsva_2d`), colour buttons | **Own** the wheels: no wheel, no relative drag around a neutral centre, no hook; the swatch button stays egui's |
| egui_oklch_color_picker | 0.1.0 (2026-07-28), MIT, egui 0.33 | An OKLCH picker | Skip: one release, an older egui, no grading wheels |
| egui-keyframe | 0.1.0 (2026-02-13), MIT OR Apache-2.0, egui 0.33 | Keyframes and animation curves on a timeline | Skip: curves of time, not tone curves; no application spline, no pick |
| egui-colorgradient | 0.2.3 (2025-07-25), MIT, egui 0.32 | A gradient editor (stops along a bar) | Skip: the slider's gradient track covers what is needed |
| egui_minimap | 0.4.0 (2026-07-25), MIT OR Apache-2.0, egui 0.35 | A scrolling minimap of rows (text, hex bytes) | Skip: not an image navigator; the navigator draws the application's proxy and shares the viewport's state |
| rsplot | 0.6.1 (2026-10-17), MIT OR Apache-2.0, egui 0.36 | Scientific plots on wgpu | Skip: far more than a histogram (it has caught up with egui 0.36, so its egui is no longer a reason) |

No code was taken from any of them. The curve's default spline, a
monotone cubic, follows Fritsch and Carlson's published method.

## 3. Progress and waiting: design notes

### 3.1 Where progress comes from

egui-pop does not run jobs. The host publishes events on its journal
(ADR 0010), and an L1 model with no egui (`egui-pop-model::jobs`) folds
them into job states. Proposed events, which a host may map from its
own bus:

| Event | Data |
|---|---|
| `job.started` | `id`, `title`, `kind` (icon role), `source`, `parent` (for a batch), `total` (optional: no total means indeterminate), `cancellable`, `pausable`, `cause` (the command that started it) |
| `job.progress` | `id`, `done` (and a new `total` if it changed), `item` (the current item's name), `phase` (decode, process, encode, write), `buffered` (optional, MUI's buffer) |
| `job.item_failed` | `id`, `item`, `error` (the job goes on; it counts the failures) |
| `job.paused`, `job.resumed`, `job.cancelling` | `id` |
| `job.finished` | `id`, `outcome` (done, failed, cancelled), `error`, `actions` (commands such as "Show in folder", "Open in lighttable") |

- **The model, not the widget, holds the state.** Each widget is a view
  of a `JobId`. The model coalesces progress per frame: a thousand
  `job.progress` events in a frame are one repaint. It reads
  `Host::since(seq)` after each `Waker` call and never polls the host on
  a timer.
- **Cancel, pause and retry are commands** (`job.cancel {id}`,
  `job.pause`, `job.retry`) sent to the host. A command that waits for
  its result greys out like any other, and the job shows "cancelling"
  until the host confirms.
- **ETA and rate are computed in the model** with an exponential moving
  average over the `done` timestamps. They are not shown before about
  3 s, and not while the rate is unstable ("about 2 min", not
  "1 min 57 s"). The clock is injected, so tests can fix it.
- **"Wait until idle"** (a Helios rule): the model answers "are there
  running jobs, which ones", and the debug bridge gets a `jobs`
  collector, so an agent can wait on it and see what is running.
- A job with no events for a long time (a timeout the host declares) is
  shown as "not responding", never as frozen at 40%.

### 3.2 Determinate and indeterminate

- Indeterminate while `total` is unknown; determinate as soon as it is
  known, and **never back** to indeterminate (Carbon's rule). If the
  total grows (the import found more files), the bar may go back, and
  the text says why ("312 of 1,204, still scanning").
- A real indeterminate bar: a segment travelling along the track (MUI,
  Fluent, GTK `pulse`), not egui's pulsing fill.
- **Tones:** running (accent), paused (muted), done (success, then
  fades out), failed (danger, stays), done with failures (warning:
  "340 exported, 3 failed").
- **Sizes:** a thin line (2 px, under a panel title or at the top of the
  viewport, as JetBrains and VS Code do), a standard bar, a bar with a
  label and text (title, n of m, ETA).

### 3.3 Where progress shows

1. **In the control that started it** (Carbon `InlineLoading`, MUI
   `loading`). A button bound to a command shows the command's running
   state: after the delay, a small spinner replaces its icon, its width
   does not change, and the label stays readable (a screen reader keeps
   the name). The failure shows on the button, as menus do already.
2. **In a row** (the queue panel, export targets, a thumbnail): a small
   ring or a thin bar inside the row, with the state as text.
3. **In the status bar:** one summary for all jobs ("3 jobs, 42%", with
   a thin bar for the overall progress). A click opens the jobs centre
   as a popover, as JetBrains does.
4. **In a toast:** a job started by the user that takes longer than a
   few seconds can become a toast with a bar and Cancel (VS Code's
   Notification location). It is still the same job in the model, and
   closing the toast does not cancel it.
5. **On the viewport:** a delayed "working" overlay that never blocks
   (darktable's "working..." in the centre, Mantine's `LoadingOverlay`),
   or better a thin line along the top while the image recomputes. The
   old image stays visible until the new one arrives.
6. **Later, outside the window:** taskbar or dock progress (Windows
   `ITaskbarList3`, the macOS dock, Unity's `LauncherEntry` on Linux
   docks) and a system notification when a long job ends while the window
   is not focused. winit offers neither, so this is platform code.

### 3.4 The jobs centre

A panel (dockable, like any other) and the same content in the status
bar's popover:

- **Running:** title, icon by kind, a bar (stacked for a batch), "n of m",
  the current item, the phase, the ETA, buttons for pause, cancel and
  "show details". A batch's children fold under it.
- **Queued:** with the position in the queue, and reordering if the host
  allows it.
- **History:** finished jobs with their outcome, their duration and their
  result actions (commands); errors stay until dismissed; "clear
  finished". It is kept for the session, and the host may keep more.
- **Errors per item:** "3 failed" opens the list (item, error, retry this
  one).
- **Filters:** by kind (export, import, render, GPU) and by source (the
  user, an agent, a schedule). An agent's job carries its origin like any
  other write.
- References: JetBrains' background tasks, KDE's job tracker,
  Lightroom's activity area, Capture One's Activities, darktable's
  background jobs module.

### 3.5 Batches: stacked progress

- One bar for the batch (n of m) with **sections** for done, failed and
  skipped (Mantine `Progress` sections, Ant's success segment). An
  optional second, thinner bar shows the current item when items are
  long (a 100 MP render). The text gives the current item's name.
- Phases (decode, process, encode, write) are text, not more bars.
  Nested bars beyond two levels confuse users.
- Concurrency: with 4 items at once, "n of m" counts finished items, and
  the details list the items in progress.

### 3.6 Delayed spinner, minimum duration

- **Show nothing before 300 ms** (configurable token,
  `motion.progress.delay`). Most commands return before that, and a
  flash of spinner reads as a glitch.
- **Once shown, keep it at least 500 ms** (`motion.progress.min`), so it
  does not flash off.
- A **determinate bar after about 1 s** if a total is known; a modal
  operation offers "Run in background" after about 2 s (JetBrains).
- The delay starts from the job's or command's start time in the model,
  not from when the widget was first drawn, so a panel opened midway
  shows the right state at once.
- The delay needs no timer thread: the widget asks for
  `request_repaint_after(remaining)` once, then draws on the next frame.

### 3.7 Skeletons and placeholders

- **Skeleton** (grey shapes in the content's layout) for panels that fill
  progressively: metadata, the queue panel's rows, a module's parameters
  while its manifest loads. This follows pitfall 20 (a panel opens in
  one frame and fills progressively).
- **Not for thumbnails:** use what is already known (the raw's embedded
  preview, a thumbhash, the last cached thumbnail, or a flat tile in
  the image's average colour), then the real thumbnail. A grid of
  shimmering tiles is noise.
- Skeletons do not shimmer when motion is reduced, and they do not
  shimmer in tests.

### 3.8 Accessibility

- Role `ProgressIndicator`, with a **numeric value and a range** (0 to
  `total`, or 0 to 100) for a determinate bar, and **no value** for an
  indeterminate one. egui's `ProgressBar` sets the percentage but no
  range; our widget sets both.
- A name: "Exporting 340 images". The value text gives "12 of 340,
  about 2 minutes left".
- The region being refreshed gets AccessKit's **busy** flag while a
  skeleton or overlay stands in for it.
- **Announcements, sparingly**, through egui-pop's announcer (polite
  live region): when a job starts after the delay, when it finishes or
  fails, and never for each percent. At most one progress announcement
  every 10 s, and only on request.
- Cancel buttons are named with the job ("Cancel export of 340
  images"). The jobs centre is a list with keyboard navigation.
- Colour is never the only signal: the state is in the text too
  ("failed", "paused").

### 3.9 Reduced motion and repaint cost

- Reduced motion (`motion.duration` is 0): spinners become a static
  "busy" icon (Phosphor `hourglass` or `circle-notch`) or a slow 2 Hz
  step, indeterminate bars become a striped static track, skeletons do
  not shimmer, and toasts do not slide.
- An animated widget repaints the whole frame. Limit it: an animation
  runs only while the indicator is visible and the window is focused;
  otherwise `request_repaint_after(250 ms)`. An event-driven bar repaints
  only when the host wakes the interface.

### 3.10 Deterministic headless tests

- Animations read an egui-pop **motion clock** (a service), not
  `input.time` directly. Tests freeze it, step it, or set "no motion",
  so snapshots are the same on every run. In a kittest harness,
  `input.time` moves with `step_dt`, which is deterministic but couples
  snapshots to the number of frames.
- The delay and minimum duration use the same clock, so a test can say
  "advance 299 ms: no spinner; 301 ms: spinner".
- A fake host (`egui-pop-test`) publishes scripted `job.*` events: a
  test drives a job from start to failure without threads.
- The `jobs` debug collector lets a test or an agent assert on the
  model, not on pixels.

## 4. Prioritized list

**Must** = Helios needs it soon (the shell, the lighttable, the first
darkroom, jobs). **Useful** = next, when the surface that needs it comes.
**Later** = a real need, but not soon. **Skip** = not worth it.

"Own" = written in egui-pop on egui primitives; "wrap" = a crate behind an
egui-pop widget bound to values and commands; "adopt" = used as it is;
"built-in" = egui as it is, styled by tokens.

### Must

| Widget | What it is | Others | egui status | Helios use | Notes |
|---|---|---|---|---|---|
| **Progress bar** | Determinate, indeterminate (a moving segment), tones, sizes (line, bar, labelled), sections for done, failed and skipped | MUI `LinearProgress`, Mantine `Progress`, Ant `Progress`, Blueprint `ProgressBar`, Fluent `ProgressBar`, Carbon `ProgressBar`, `GtkProgressBar`, `QProgressBar` | Built-in lacks indeterminate, tones, sections, a range for AccessKit and reduced motion: **own** (about 200 lines) | Exports, imports, renders, the status bar, rows | Reads a `JobId` or a plain fraction; motion clock; tokens `progress.*` |
| **Spinner / busy indicator** | Indeterminate, small, with a label, a delay and a minimum duration | shadcn `Spinner`, MUI `CircularProgress`, Mantine `Loader`, Ant `Spin delay`, Fluent `Spinner`, `AdwSpinner`, QML `BusyIndicator`, imspinner | Built-in has no label, no delay, no reduced motion: **own**, painting like egui's | Buttons, rows, the viewport, panels | 300 ms delay, 500 ms minimum (tokens); static under reduced motion |
| **Jobs model and jobs centre** | Jobs folded from host events; a status bar summary, a popover, a panel and the history | JetBrains background tasks, VS Code notifications and progress, KDE job tracker, Lightroom activity area, Capture One Activities, darktable background jobs | None: **own** (model in L1, views in L3, a `jobs` debug collector) | Every background job is named, cancellable and visible (Helios rule); "wait until idle" | The centre of this survey; section 3 |
| **Busy state of a command-bound control** | A button or row showing that its command runs, then its outcome | Carbon `InlineLoading`, MUI/Mantine/Ant `loading`, QML `DelayButton` | **Own**, on the command running state egui-pop already tracks | "Export", "Import", "Apply style" buttons | Width stable, name kept, failure on hover |
| **Toast with progress and actions** | The existing toast with a bar, Cancel and result actions | VS Code notifications with progress, Sonner promise toasts, Blueprint toast progress, `AdwToast` | **Own** (toasts exist in Helios; move them to egui-pop) | "Export finished: Show in folder" | A view of a job; closing it does not cancel the job |
| **Status bar items** | Declared items (text, icon, progress, a command on click) placed left or right | VS Code status bar, JetBrains widgets | **Own** (a zone with declared items) | Job summary, profile, zoom, selection count | Needed by the jobs centre |
| **Empty state** | An icon, a sentence, an action (a command) | Blueprint `NonIdealState`, `AdwStatusPage`, Ant `Empty` and `Result`, shadcn `Empty` | **Own** (small) | Empty queue panel, empty collection, no module selected | Also covers error with Retry (the fault boundary card already exists) |
| **Slider family** | darktable-style slider (label in the bar, soft and hard range, fine adjustment, reset, typed entry, gradient track), range slider (two handles), logarithmic | darktable bauhaus, Blueprint `MultiSlider`, MUI and Mantine range `Slider`, `GtkScale` marks, egui_double_slider | egui `Slider` has logarithmic only: **own** (decided 2026-10-13: `egui_pop_core::slider`) | All module parameters; range filters (ISO, date, focal length) | Wheel only when focused or with a modifier (pitfall 7); a gradient track for hue and temperature |
| **Virtualized thumbnail grid** | A grid of tens of thousands of cells, selection (click, Shift, Ctrl, rubber band), keyboard, placeholders, cell overlays | `GtkGridView`, Qt `QListView` icon mode, MUI X `DataGrid` virtualization | **Own** (done: `egui-pop-views` `library`, docs/library.md) | The lighttable, the filmstrip | Placeholders, not spinners; the host fetches windows |
| **Rating stars and colour labels** | 0 to 5 stars, reject; five colour labels, toggled | MUI and Mantine `Rating`, Ant `Rate`, Fluent `Rating`; Lightroom and darktable thumbnails | **Own** (done: docs/library.md) | The lighttable, the darkroom | Keys 0 to 5 and F1 to F5 come from the keymap; labels need a shape or text besides colour |
| **Tag chip and tag input** | Chips (dismissible, selectable) and a field that adds tags with completion | Blueprint `TagInput`, Mantine `TagsInput` and `Pill`, Fluent `TagPicker`, Carbon `Tag`, Ant `Tag` | **Own** (done: docs/library.md); the completion uses egui-pop's fuzzy matcher | Keywords, filters, collections | Wraps onto several lines; hierarchical tags (`place|France|Paris`) |
| **Search field with filters** | A text field with chips for active filters, a clear button and a menu of filters | VS Code search and filter boxes, Fluent `SearchBox`, Carbon table toolbar search | **Own** (done: docs/library.md), on `TextEdit` | Lighttable filters, the panel and setting search | Its keys go through the focus rules (no shortcut fired while typing) |
| **Segmented control and toggle switch** | Mutually exclusive options in one row; an on/off switch | Mantine `SegmentedControl`, Ant `Segmented`, Blueprint `SegmentedControl`, `AdwToggleGroup`, Carbon `ContentSwitcher`; MUI and Mantine `Switch`, `AdwSwitchRow` | **Own**, on `selectable_value` and the egui demo's switch | View modes, module blend modes, settings | Role radio group for the segmented control, switch for the toggle |
| **Tree view** | Expand and collapse, multi-select, keyboard, drag and drop, context menu, lazy children | Blueprint `Tree`, Fluent `Tree`, Carbon `TreeView`, MUI X `TreeView`, `QTreeView` | **Own** (done: `egui-pop-views`, docs/tree.md) | Folders, collections, tag hierarchy, the graph's outline | Lazy children come from the host |
| **Data table** | Virtualized, sortable, resizable columns, sticky header | Blueprint `Table`, MUI X `DataGrid`, Carbon `DataTable`, Fluent `DataGrid` | **Wrap egui_table** (done: `egui-pop-views`, docs/table.md) | Metadata, the queue panel, the history of jobs | Sorting and widths are values; selection by key |
| **Dock, split panes and tabs** | Dockable panels, splits, editor tabs | JetBrains tool windows, VS Code, `QDockWidget`, `AdwTabView`, shadcn `Resizable`, Ant `Splitter` | **Adopt egui_dock** (decided), then its continuation egui_stack with egui 0.37 ([#145](https://github.com/quazardous/egui-pop/issues/145)) | The whole shell | Drop veto worked around (see `upstream.md`) |
| **Histogram** | Draws supplied bins (per channel, log or linear), clipping markers | Lightroom, darktable, Capture One | **Own**, done (`viewport::histogram`, clipping and drag 2026-10-13) | The darkroom, the import preview | Data from the host; egui-pop only draws |
| **Node graph editor** | Nodes with headers, pins, wires, semantic zoom, stacks | imgui-node-editor, imnodes, egui-snarl | **Own** (decided), snarl as reference | The graph view | A large item; the header shows the job state of each node (states and badges above) |

### Useful

| Widget | What it is | Others | egui status | Helios use | Notes |
|---|---|---|---|---|---|
| **Skeleton** | Grey shapes standing in for content | MUI, Mantine, Ant, Fluent `Skeleton`; Carbon `SkeletonText` | **Own** (small) | Metadata and queue panels, module parameters while loading | Not for thumbnails (section 3.7); no shimmer in tests |
| **Badge and counter** | A small count or dot on an icon, tab or node | MUI and Mantine `Badge`, Mantine `Indicator`, Fluent `CounterBadge`, the VS Code activity bar | **Own** (small) | A stack's `+3`, queue counts, the jobs count on the status bar | Never colour only |
| **Ring progress** | A small circular determinate progress | MUI `CircularProgress`, Mantine `RingProgress`, Ant `Progress` circle, Blueprint determinate `Spinner` | **Own** (do not use the GPL crate) | Per-thumbnail export or render state, node headers | 12 to 16 px |
| **Meter / level bar** | A value against thresholds (not progress) | `GtkLevelBar`, Mantine `Progress` sections | **Own**, sharing the bar's painting | Disk space, GPU memory, cache use | Role meter, not progress |
| **Viewport busy overlay** | A delayed, non-blocking "working" mark over a surface | Mantine `LoadingOverlay`, Carbon `Loading`, darktable's "working..." | **Own** | The darkroom while the pipeline recomputes | Old image stays; a thin line rather than a veil |
| **Popover, hover card, rich tooltip** | Anchored floating content, with or without interaction | Radix `Popover` and `HoverCard`, Blueprint `Popover`, Fluent `TeachingPopover` | Built-in `Popup` and tooltips; **own** layer on egui-pop's overlay stack (exists) | Pixel readout, node details, a job's details | Delay and placement tokens |
| **Inline editable label** | Text that becomes a field on double click or F2 | Blueprint `EditableText` | **Own** (small) | Rename presets, nodes, tabs, collections | Enter commits, Escape cancels, through a command |
| **Breadcrumbs** | A path with an overflow menu | Blueprint `Breadcrumbs`, Ant `Breadcrumb`, VS Code breadcrumbs | **Own** | Folder paths, macro nesting in the graph | Collapse in the middle when too long (Blueprint `OverflowList`) |
| **Zoom control** | Fit, fill, 100%, steps, a typed percentage | Every image and graph editor | **Own** | Viewport, graph | "100%" is one image pixel per physical pixel (pitfall 10) |
| **Navigator / minimap** | A small view of the whole with the visible rectangle | Lightroom navigator, darktable navigation, VS Code minimap, node editor minimaps | **Own**, done for images (`viewport::navigator`, 2026-10-13); the graph's later | Darkroom, graph | |
| **Filmstrip** | A one-row strip of thumbnails | Lightroom, darktable, Capture One | **Own** (done: the thumbnail grid with one row, docs/library.md) | Darkroom | Shares selection with the lighttable |
| **Curve editor** | Control points, spline, per channel, a histogram behind | darktable tone curve and rgb curve, ImGuizmo `ImCurveEdit` | **Own**, done (`viewport::curve`, 2026-10-13) | Tone and RGB curves | Each point is a value; one drag is one undo step |
| **Colour wheels and picker** | Hue-chroma wheels with a lightness slider; a swatch picker | darktable colour balance rgb, Lightroom colour grading, Fluent `SwatchPicker`, Mantine `ColorPicker` | Built-in picker for swatches; **own** wheels, done (`viewport::wheel`, 2026-10-13) | Colour grading; labels and theme work | Colour-managed display through the host's display transform |
| **Reorderable list** | Drag to reorder with keyboard alternatives | Mantine and Ant sortable lists, egui_dnd | **Adopt egui_dnd** | Export targets, a module stack, presets | The new order goes out as a command; Alt+Up and Alt+Down as keys |
| **Split button** | A main action and a menu of variants | `AdwSplitButton`, Fluent and Ant dropdown buttons | **Own** on egui menus | "Export" with its presets | Entries are commands |
| **Key display** | Keys drawn as caps | shadcn and Mantine `Kbd`, Blueprint `HotkeysDialog` | **Own** (small; menus and the palette show keys already) | Menus, the palette, tips, a shortcuts sheet | Text from the keymap, never typed by hand |
| **JSON tree** | A collapsible view of structured data | egui_json_tree | **Adopt egui_json_tree** | The debug bridge, the event journal, `inspect` output | |
| **Virtual list (variable rows)** | A long list whose rows differ in height | `GtkListView`, egui_virtual_list | **Adopt egui_virtual_list**, or our own if it gets in the way | History stack, log, journal | |

### Later

| Widget | What it is | Others | egui status | Helios use |
|---|---|---|---|---|
| Stepper / wizard | Numbered steps of a long form | MUI `Stepper`, Ant `Steps`, Carbon `ProgressIndicator`, Blueprint `MultistepDialog` | Own, on the normalized Form dialog | Import with options, first run, export setup |
| Date picker and range | Calendar, range of dates | MUI X date pickers, egui_extras `DatePickerButton` | Wrap egui_extras | Date filters |
| Timeline | Images or events over time, zoomable | darktable lighttable timeline, Tracy, egui_timeline | Own | Browse by capture date; job history over time |
| Gradient editor | Stops on a track | ImGui gradient gists, egui_colorgradient | Own | Gradient masks, theme work |
| Guided tour, teaching popover | Steps that point at parts of the interface | Ant `Tour`, Fluent `TeachingPopover` | Own on overlays | Tips (a Helios surface) |
| Markdown view | Rich text for help | egui_commonmark | Adopted (the viewer's `markdown` feature, [links](links.md)) | Help, tips, release notes |
| Log view | Filterable log lines | egui_logger, egui_tracing | Own on the virtual list | Diagnostics |
| Taskbar and dock progress, system notifications | Outside the window | Windows `ITaskbarList3`, the macOS dock, Unity `LauncherEntry` | Platform code (winit has none) | Long exports while the window is hidden |
| Shortcut editor | Captures a key chord | `QKeySequenceEdit`, VS Code keybindings editor | Own, on the keymap model | Keymap settings |
| Transform gizmo, handles | Handles on image overlays | ImGuizmo | Own, with the viewport's overlays (decided) | Crop, masks |

### Skip

| Widget | Why |
|---|---|
| Knob / dial (egui_knob, `QDial`, imgui-knobs) | Audio idiom; a slider is faster and more precise for photos |
| Carousel, speed dial, floating action button, watermark, QR code, transfer list, cascader, statistic and countdown | Web and mobile patterns with no use here |
| Pull to refresh, infinite scroll by pages | The host windows the data; there is no "refresh" gesture on desktop |
| egui_circular_progress_bar | GPL-3.0, not usable in an MIT library |
| egui-modal, catppuccin-egui, egui-aesthetix | Dead or stuck on old egui |
| Third-party toast crates as dependencies | String queues with no commands or inspectable state (ideas only) |
| A generic form-from-struct crate (egui-probe, egui_struct) | Forms come from value descriptors |

## 5. Suggested order for progress work

1. `egui-pop-model::jobs`: the job events, the fold, ETA with an injected
   clock, a fake host in `egui-pop-test`. No egui.
2. The motion clock service and the tokens `motion.progress.delay` and
   `motion.progress.min`.
3. Progress bar, spinner (delayed), ring and meter, sharing one painting
   module; gallery page "Progress" with every state, every theme and
   reduced motion.
4. Busy state on command-bound buttons; status bar items with the job
   summary.
5. The jobs centre (popover and panel) and the toast with progress.
6. Skeleton and empty state, used by the queue panel.

Each step: AccessKit names and ranges checked by `egui-pop-test`, a
`jobs` debug collector, snapshots per theme with motion frozen.

Done (2026-10-13), as described in [Jobs and progress](progress.md):
the tokens are named `motion.spinner.delay` and `motion.spinner.min`;
every job carries an estimated overall percentage (steps and child jobs
rolled up, refined by the `jobs.estimate` step); each job of the jobs
centre opens a log console, a pseudo-terminal when `egui-pop-editors`
provides it. Not yet: queued jobs, "not responding", taskbar progress,
the viewport's busy overlay, announcements of a job's start and end.

## 6. Dialogs

Decided on 2026-10-13, built in `egui_pop::api::dialogs`.

**The default: no header band and no ✕.** A dialog is its title, its
content and its footer. The way out is the Cancel button and Escape;
an acknowledgement (`Info`) has its own Close button.

**Why.**

- **The desktop convention.** Desktop dialogs that ask something end
  with their buttons: in the classic Mac OS guidelines, modal dialog
  boxes have no close box (even movable ones), the user answers
  through the buttons, and Cancel revokes what was done; WinUI's
  `ContentDialog` dismisses through its `CloseButtonText` button (and
  Escape), not a title-bar ✕. The ✕ in a header is the web's
  convention (the ARIA practices' modal dialog example has one).
- **No duplicate controls.** A ✕ next to Cancel is two controls for one
  action: one more thing to read, to reach by Tab, and to name for a
  screen reader.
- **One consistent rule.** Every dialog gets out the same way, so the
  user learns it once. Asked on the WCAG working group's list whether a
  modal should have a ✕, Cancel or both, the answer pointed to SC 3.2.4
  (consistent identification): choose one approach and use it
  consistently throughout the application; Escape acts as Cancel.

**The option: `DialogSpec::new(dialog).closable(true)`**, or the
`dialogs.closable` setting for every dialog. It puts a ✕ at the right
end of the title row, still with no band behind it. The ✕ is Escape and
nothing else: the same cancel, through the same `dialog.answer` step and
hooks, refused exactly when Escape is (a hook vetoing the cancel keeps
the dialog open either way). It is named "Close" (translated), is a
target of at least 24 points (WCAG 2.5.8) and is never focused when the
dialog opens: the focus goes where it would without it. Use it for a
notice of the application's own that has no Cancel button, or in an
application that follows the web's convention throughout (then turn
the setting on, rather than mixing dialogs with and without it).

**Movable dialogs: `DialogSpec::new(dialog).movable(true)`**, or the
`dialogs.movable` setting for every dialog. A dialog that hides what it
acts on (levels over an image, a find dialog over a text) can be moved
out of the way.

- **The header band.** Only a movable dialog has one: its title row on
  the kit's `header-band` (the title in `dialog-header.rest.text`), a
  line below it, the move cursor over it.
  It carries the ✕ when the dialog is closable. A dialog that is not
  movable keeps its plain title row: no band means "this does not
  move".
- **Dragging.** Dragging the band moves the dialog, kept inside the
  window, and kept inside again when the window is resized. The title
  is not selectable there, so a press on it drags too. A double click
  on the band centres the dialog again.
- **The keyboard.** The `dialogs.move` command (Alt+F7, the move key of
  several desktops, rebindable, live while a modal is open) starts a
  move: the arrow keys move the dialog by `space.m`, Shift+arrow by
  eight times that, Enter keeps the place, Escape puts it back where
  the move started (and only then, a second Escape cancels the dialog
  as usual). The band itself is not in the Tab order.
- **Where it comes up.** Centred, every time, unless it was opened with
  `.remember_position(key)`: then where it was last left, kept per key
  in the host's store (`ui/dialogs/<key>`, an offset from the window's
  centre, written without an undo step) and clamped to the window it
  opens in.
- **Hooks (ADR 0011).** Each finished move (a drop, an arrow key, a
  return to the centre) runs the `dialog.move` step with
  `{id, title, key, how, from, to}`: a before-hook may veto it (the
  dialog goes back) or redirect it (its own `to`); a run-hook may place
  it its own way. The journal then says `dialog.moved`. The positions
  in between, while the pointer drags, are not steps: the dialog
  follows the pointer, and the drop decides.

**The backdrop: `DialogSpec::backdrop(Backdrop::Dim | Backdrop::Clear)`.**
A dialog is modal either way: the interface behind it takes no input,
and a click on the backdrop never closes the dialog (an answer comes
from its buttons, Enter or Escape; a stray click must not lose what
was typed).

- `Dim`, the default, darkens the interface by the `backdrop.opacity`
  token (an alias of the system `opacity.scrim`, a number from 0 to 1).
  While a movable dialog is dragged the dim fades to about a third, so
  the user sees where to put it; it comes back on the drop.
- `Clear` leaves the interface untouched, for a dialog previewing its
  effect (a curve applied to the image behind it).

**A body of its own: `Dialog::Custom`** (decided 2026-10-14, ADR 0013),
opened with `Dialogs::open_custom(app, spec, body)`. The four kinds are
drawn from data; a custom dialog draws a closure (a viewport, a curve,
a preview), kept on the application's side so the queue stays `Clone`
and `Send`. It keeps every rule above: a primary button that says what
it does (or a Close button only), Cancel, steps, the debug bridge
(its body says what agents see with `CustomPass::describe`, and its
values are merged into the command's arguments), movable, closable, a
kept place, the backdrop. Its body has a size of the dialog's own (a
share of the window by default), a `dialog` focus scope and a fault
boundary, and is drawn every frame.

- **Keys the body claims** (`Custom::claims`, or its component's
  `Mountable` through `Custom::needs`) go to it first. A body claiming
  Enter keeps it, and Ctrl+Enter accepts; a body claiming Escape gets
  it while it has something to cancel (`CustomPass::claim_escape`, a
  drag of a viewport), and the next Escape cancels the dialog.
- **What it holds is checked.** A body declaring its needs is checked
  against the custom dialog's offer when it opens (a body requiring
  wide gamut is refused: sRGB first), and recorded as a mount while
  the dialog is open.
- **One dialog at a time** still: a body draws a second choice inline.

`egui_pop::imaging::preview` is the first use: an export preview, an
image viewport over a `Clear` backdrop, movable and kept in place; the
interface behind, its own viewport included, keeps drawing while the
dialog is modal.

Sources:

- WAI-ARIA Authoring Practices, modal dialog example:
  <https://w3.org/WAI/ARIA/apg/patterns/dialog-modal/examples/dialog>
- WinUI `ContentDialog.CloseButtonText`:
  <https://learn.microsoft.com/windows/windows-app-sdk/api/winrt/microsoft.ui.xaml.controls.contentdialog.closebuttontext>
- WCAG working group's list, ✕ or Cancel in a modal (SC 3.2.4):
  <https://lists.w3.org/Archives/Public/w3c-wai-gl/2017OctDec/0897.html>
- Inside Macintosh, Macintosh Toolbox Essentials, kinds of dialog
  boxes (movable modal dialog boxes have a title bar to drag them by):
  <https://developer.apple.com/library/archive/documentation/mac/Toolbox/Toolbox-373.html>

## Sources

- shadcn/ui: <https://ui.shadcn.com/docs/components>; Radix:
  <https://www.radix-ui.com/primitives>
- MUI: <https://mui.com/material-ui/>, MUI X: <https://mui.com/x/>
- Mantine: <https://mantine.dev/>
- Ant Design: <https://ant.design/components/overview>, `Spin` delay:
  <https://ant.design/components/spin>
- Blueprint: <https://blueprintjs.com/docs/>
- Fluent UI v9: <https://react.fluentui.dev/>
- Carbon: <https://carbondesignsystem.com/components/overview/>, loading
  pattern: <https://carbondesignsystem.com/patterns/loading-pattern/>
- libadwaita: <https://gnome.pages.gitlab.gnome.org/libadwaita/doc/main/>;
  GTK4: <https://docs.gtk.org/gtk4/>
- Qt: `QProgressDialog` <https://doc.qt.io/qt-6/qprogressdialog.html>,
  Qt Quick Controls <https://doc.qt.io/qt-6/qtquickcontrols-index.html>;
  KDE KJob: <https://api.kde.org/frameworks/kcoreaddons/html/classKJob.html>
- Dear ImGui: <https://github.com/ocornut/imgui>, its list of extensions:
  <https://github.com/ocornut/imgui/wiki/Useful-Extensions>
- VS Code progress API:
  <https://code.visualstudio.com/api/references/vscode-api#window.withProgress>;
  UX guidelines: <https://code.visualstudio.com/api/ux-guidelines/overview>
- JetBrains UI guidelines, progress:
  <https://plugins.jetbrains.com/docs/intellij/progress-indicators.html>
- Nielsen Norman Group, progress indicators:
  <https://www.nngroup.com/articles/progress-indicators/>
- egui 0.36.2 source (`widgets/progress_bar.rs`, `widgets/spinner.rs`):
  <https://github.com/emilk/egui>
- egui 0.37.0 changelog:
  <https://github.com/emilk/egui/blob/main/CHANGELOG.md>; egui_kittest's:
  <https://github.com/emilk/egui/blob/main/crates/egui_kittest/CHANGELOG.md>
- crates.io's API for each crate named in section 2 (latest version,
  its date, licence and dependencies), read on 2026-11-03.
- darktable background jobs (`src/control/progress.c`) and bauhaus
  sliders (`src/bauhaus/bauhaus.c`): <https://github.com/darktable-org/darktable>
