# egui_stack: directions for egui-pop

Status: analysis, 2026-11-03. Nothing here is decided; the sections and
toolbars direction is accepted in [ADR 0022](adr/0022-sections-and-toolbars-in-egui-stack.md).
Since then egui-pop has moved to egui 0.37 and draws its docks with
egui_stack, which has the first changes listed at the end of this page;
the workarounds those changes allow egui-pop to drop are still in place,
to be removed one by one.

[egui_stack](https://github.com/quazardous/egui_stack) continues
[egui_dock](https://github.com/anhosh/egui_dock) (MIT, its full history
kept): it starts as egui_dock 0.21.1's API moved to egui 0.37, and it is
meant to grow into more of an application's panel management (stackable
sections, toolbars). egui-pop draws its workbench over egui_dock 0.21 on
egui 0.36 today. This page reads both codebases and says which
directions would make egui_stack fit egui-pop best, before egui-pop moves
onto it and before sections and toolbars move into it.

Two rules hold throughout:

- **egui_stack never depends on egui-pop.** It stays a plain egui crate,
  useful to any egui application; egui-pop wraps it. Whatever egui-pop
  needs from it is a hook, a trait or a value, never an egui-pop type.
- **Ideas, not code, from GPL projects**; egui_tiles (MIT OR Apache-2.0)
  may lend ideas and code, with credit.

References: egui-pop paths are from the repository's root; egui_stack
paths are from its own root and marked "egui_stack". egui_stack had no
functional change from egui_dock 0.21.1 when this was written (its
changelog lists the rename and egui 0.37 only), so what is said of one
holds for the other.

## Where egui-pop touches egui_dock

| Where | What |
|---|---|
| `crates/egui-pop-api/src/workbench.rs:127` | The one import: `DockArea`, `DockState`, `NodeIndex`, `Split`, `TabViewer`. |
| `workbench.rs:1528-1600` | `to_dock`, `fill`, `from_dock`, `read`: egui-pop's own layout tree converted to a `DockState` and read back. |
| `workbench.rs:798-830` | `layout`, `read_back`, `project`: the layout document is the truth, the docks a projection of it, rebuilt when it changed outside them. |
| `workbench.rs:1230-1243` | `take_dock`, `put_dock`: one `DockState` per area, taken out while drawn. |
| `workbench.rs:1258-1274` | `dragged_tab`: rebuilds egui_dock's private drag id (`show/leaf.rs`) to know which tab egui is dragging. |
| `workbench.rs:2571-2614` | `draw_dock`: one `DockArea` per area with close, add, close-all and collapse buttons off, then the kit paints over it. |
| `workbench.rs:2668-2694` | `dock_style`: egui_dock's tab painting made transparent. |
| `workbench.rs:2785-2850` | `paint_tabs`: the kit paints the bars and tabs over what egui_dock laid out. |
| `workbench.rs:2852-2970` | `TabViewer` for panels: `id`, `title`, `ui`, `on_tab_button`, `tab_style_override`, `context_menu`, `allowed_in_windows`. |
| `workbench.rs:2972-3093` | `follow_drag`, `drop_hint`: drops between areas, the veto, the hint and the ghost tab, done by egui-pop. |
| `crates/egui-pop-api/tests/theme_pairs.rs:183`, `crates/egui-pop-gallery/tests/gallery.rs:1766` | Tests that lean on egui_dock's drag timing. |

egui-pop builds egui_dock without features (`Cargo.toml:87`): it never
serializes a `DockState`.

## 1. Theming

### Today

The kit paints the workbench tabs, egui_dock only lays them out and
takes their input ([kit](kit.md#built-so-far)):

- **A style made invisible.** `dock_style` (`workbench.rs:2668`) starts
  from `egui_dock::Style::from_egui` and sets to transparent
  `tab_bar.bg_fill`, `tab_bar.hline_color`, and `bg_fill`,
  `outline_color`, `text_color` of all seven `TabInteractionStyle`s
  (`active`, `inactive`, `focused`, `hovered` and the three
  `*_with_kb_focus`); it sets `tab_bar.height` from the kit's `tab-bar`
  and `tab` heights. Nothing else is touched: **separators**
  (`SeparatorStyle`: width, colours idle, hovered, dragged), the
  **drop overlay** inside an area (`OverlayStyle`: selection colour,
  leaf highlight, the icon buttons), `TabBodyStyle` and the buttons keep
  egui's colours, not the theme's tokens.
- **Not copied every frame.** The style is kept per area in a
  `TabCache` (`workbench.rs:2714-2783`), rebuilt when egui's `Style` Arc
  changes, and the prepared tabs (title translated, label laid out,
  width measured) when the kit, the scale or the strings provider
  change. The first kit version did it every frame and cost 56% more
  frame time ([kit, cost](kit.md#cost)).
- **Widths through an override.** `tab_style_override`
  (`workbench.rs:2890`) gives each tab the kit's measured width as
  `minimum_width`; egui_dock still adds 8 points either side of the
  label, so a kit can make tabs wider, never narrower, nor overlap them
  ([kit](kit.md), known limits).
- **Painting after the fact.** `on_tab_button` (`workbench.rs:2871`)
  records each tab's rect, layer and state (`SeenTab`); after
  `show_inside`, `paint_tabs` paints the bars (`kit.tab_bar`) and tabs
  (`kit.tab`), clipping to the bar and following a dragged tab onto its
  floating layer. egui_dock lays the title out a second time for its
  own (invisible) text.
- **Hints drawn twice over.** Between areas, the kit's `dock-preview`
  and `tab-preview` are egui-pop's (`drop_hint`, `workbench.rs:3004`).
  Inside one area, egui_dock's own overlay still shows, in egui's
  colours.

In egui_stack, every part is painted from `Style` fields with no hook:
the tab in `tab_title` (egui_stack `src/widgets/dock_area/show/leaf.rs:1079-1211`),
the bar (`leaf.rs:225-234`), separators (`show/mod.rs:446-550`), the
drop overlay (`drag_and_drop.rs:78-143`). The only levers are
`tab_style_override`, `title` and `on_tab_button`, which gets a
`Response` but no painter.

### What egui_stack could expose

| Option | For | Against |
|---|---|---|
| More `Style` fields | No new concept | Every kit shape (box, underline, Chrome-like) needs fields; a kit's handlers, states and per-part data do not map onto colours |
| Per-part callbacks on `DockArea` (`.paint_tab(|p, part| ..)`) | Small, opt-in | Several closures to thread through; no shared measure step |
| **A painting trait** (`DockPainter`), one method per part, defaults painting from `Style` | One object a kit implements once; defaults keep today's look; measure and paint in one place | A public trait to keep stable |
| Painting in `TabViewer` (egui_tiles' `Behavior` style) | One trait for everything | Mixes "what a tab is" with "how the theme looks": egui-pop's viewer is per panel set, the kit per theme |

**Direction T: a `DockPainter` trait** passed to `DockArea::painter`,
its default the current painting from `Style`:

- `tab_width(&self, title: &Galley, tab: TabFlags) -> f32`: the
  measure, so a kit decides the width, padding included;
- `tab(&self, painter, TabPaint { rect, title, state, closeable, close_hovered })`;
- `tab_bar(&self, painter, rect, scrolled)`;
- `separator(&self, painter, rect, axis, state)`;
- `drop_preview(&self, painter, DropPaint { rect, destination, refusal })`;
- the state as flags (`selected`, `hovered`, `pressed`, `focused`,
  `dragged`), the same four egui-pop's kit `State` already reads.

What it removes from egui-pop: `dock_style`, `SeenTab` and its
recording, `paint_tabs`'s second pass, the transparent text laid out
twice, the 8-point floor, egui's colours on separators and in-area hints.
The kit implements the trait in `egui-pop-core` (L2) with `Kit`, as it
bridges egui's widgets today ([kit](kit.md#bridges-to-egui)).

- **Benefit:** high. Every visible part of the workbench follows the
  theme; tabs narrower or overlapping become possible; less code.
- **Cost:** medium. Painting is spread across `leaf.rs` and
  `drag_and_drop.rs`; each site is replaced by a call.
- **Risk:** low to medium. Defaults keep egui_dock's look; the trait is
  public API, so its argument structs are `#[non_exhaustive]`.

## 2. Behaviour egui-pop works around

### 2.1 The drop veto, by tab and destination

**Today.** egui_stack's only veto is `allowed_in_windows(tab)` (egui_stack
`src/widgets/tab_viewer.rs:93`, read at `show/mod.rs:222-230` and
`leaf.rs:1414-1423`) and the global `AllowedSplits`
([upstream](upstream.md#egui_dock-no-per-tab-per-destination-drop-veto)).
egui-pop panels declare the areas they may live in, so each area is a
dock of its own, windows are refused (`workbench.rs:2965`), and a drag
leaving an area is followed by egui-pop: it guesses the dragged tab by
rebuilding egui_dock's private id (`workbench.rs:1258-1274`), finds the
area under the pointer (`area_at`, `workbench.rs:1277`), moves the panel
through the `workbench.move` step on release (`follow_drag`,
`workbench.rs:2977`), and draws its own hint (`drop_hint`). Since
egui_dock follows a drag only inside its own `DockArea`, the ghost tab
always lands at the end of the first tab bar: no insertion index
between tabs, no split from another area ([workbench](workbench.md#panels-in-areas)).

**A small API.**

1. `TabViewer::drop_allowed(&self, tab: &Tab, to: &TabDestination) -> DropVerdict`,
   with `DropVerdict::{Allow, Refuse(Option<WidgetText>)}`, default
   `Allow`; consulted where `allowed_splits` and `allowed_in_window`
   are computed; a refused target draws the preview as refused (through
   `DockPainter::drop_preview`) with its reason.
2. Public drag state: `DockArea::dragged_tab(ctx, id) -> Option<(TabPath, Pos2)>`,
   instead of an id scheme callers must copy.
3. Later, **one dock for the whole workbench**: with (1), egui-pop's
   areas could be the main surface's top splits, and every drop between
   areas would get egui_stack's insertion index and splits. That needs
   "fixed" nodes (an area that never collapses or merges away) and
   egui-pop's step model still deciding the move: a
   `TabViewer::on_move(from, to) -> MoveResponse::{Apply, Cancel}` hook
   lets the `workbench.move` step veto or redirect.

Removes: `dragged_tab`, `follow_drag`'s guesswork, the ghost at the end,
the workaround entry in `docs/upstream.md`. (1) and (2) are small; (3)
is a redesign of the workbench's drawing and is worth it only after (1)
has proved itself.

### 2.2 Tab context menus and commands

**Today.** `context_menu` (`workbench.rs:2902-2963`) draws plain egui
buttons: Move to each area (greyed with the refusal as tooltip), Float,
Detach, Close. They call `change_workbench` directly; they are not
command-backed menu items, so they show no keys, are not in the command
palette, and break "declare once" (credo 1). egui_stack adds its own
Eject and Close items after the viewer's (`leaf.rs:512-525`); egui-pop
avoids them by refusing windows and turning close buttons off. The Close
item calls `on_close` twice (`leaf.rs:518`, then the removal loop at
`show/mod.rs:104`), a bug egui-pop never hits because it hides the
item.

**A small API.**

- `DockArea::show_builtin_tab_menu(bool)`: the caller's menu alone.
- `TabViewer::context_menu` already gets the `NodePath`; add the tab's
  `TabPath` so a caller can name the tab in a command's arguments.
- Public `DockState` operations for every built-in action (close,
  eject, focus, move to a `TabDestination`), so a command does what
  a built-in button does, from a palette or a key.
- Fix the double `on_close`.

On egui-pop's side, the tab menu becomes a menu id (as toolbars are,
[toolbars](toolbars.md)) whose items are commands with `args.panel`:
keys, palette entries and agent surfaces follow.

### 2.3 Close confirmation

**Today.** egui-pop hides close buttons (`workbench.rs:2600`) and closes
through the tab menu's Close, the `workbench.hide` step, which a plugin
may veto (`workbench.rs:256`, `:1007`). A detached window's close is the
`workbench.close_window` step (`workbench.rs:1456-1462`). egui_stack has
`on_close -> OnCloseResponse::{Close, Focus, Ignore}`, `is_closeable`
and `force_close` (`tab_viewer.rs:35-59`), enough for a synchronous
answer; a confirmation dialog answers later.

**A small API.** Nothing new is needed for an asynchronous confirmation
once the double call is fixed: `Ignore` now, then a public
`DockState::remove_tab(path)` when the dialog answers. Documenting that
pattern, and showing the close button as the kit's (through
`DockPainter::tab`, close hovered or not), lets egui-pop show close
buttons again, as the indicators design asks (the modified dot that
turns into a close button, [indicators](indicators.md)). Removes: the
"no close button" stance and its keyboard-survey note.

### 2.4 Focus and the keyboard

**Today.** Each area is wrapped in a focus region for F6
(`draw_area`, `workbench.rs:2564-2569`); the focused panel follows the
widget focus ([workbench](workbench.md#the-focused-panel)). The keyboard
survey found: every tab is a Tab stop; Enter or Space activates it
(egui_stack `leaf.rs:579-585`); separators take the focus and move 16
points with Ctrl or Shift and an arrow (`show/mod.rs:481-508`), a plain
arrow leaves them, nothing announces them; no key goes to the next tab
of an area ([keyboard survey](keyboard-survey.md), items 3 and 9).

**A small API.**

- **One Tab stop per tab bar** (roving focus): the active tab takes Tab,
  Left and Right move along the bar, Home and End, as WAI-ARIA's tab
  list; a `DockArea::tab_focus(TabFocus::Each | TabFocus::Roving)`
  option keeps today's behaviour available.
- **Ctrl+Tab** cycling between an area's tabs, as egui_tiles'
  `Behavior::cycle_tabs_with_keyboard`; off by default so a host's
  keymap may own the key (egui-pop's would bind a command instead).
- **Separators as splitters**: a plain arrow moves them once focused (a
  separator has no other use for arrows), Shift for a larger step,
  Enter resets (today a double click resets to 0.5, `show/mod.rs:546`),
  as egui-pop's sections do.
- **Escape** while dragging a tab cancels the drag (today only the
  release ends it).
- `DockArea::focus_tab(path)`: the focus to a tab button, for
  egui-pop's `<panel>.focus` when the panel has nothing focusable.

F6 regions stay egui-pop's: they span the whole window (menu, strips,
status bar), not a dock.

### 2.5 Accessibility names and roles

**Today.** egui_stack sets no `WidgetInfo` anywhere (no `widget_info`
nor accesskit call in `src/`): tabs, close, add, close-all, collapse
buttons and separators have no role or name. egui-pop names tabs after
the fact in `on_tab_button` as plain buttons (`workbench.rs:2871-2880`),
and separators stay role `Unknown` with no name.

**A small API**, built in, no hook needed:

- a tab: role `Tab`, its title as name, `selected` when active, inside
  a `TabList` node per bar (egui_tiles' default `tab_ui` sets
  `WidgetInfo::selected(Button, …)`, `behavior.rs:155-276`);
- the close, add, close-all, collapse and minimise buttons: role
  `Button`, names from a `DockArea::labels(Labels { close, add, … })`
  struct with English defaults, so a translated application passes its
  strings;
- a separator: role `Splitter`, its fraction as value, increment and
  decrement actions, named from a `TabViewer::separator_name(path)` or
  the labels' format.

Removes: the naming in `on_tab_button`, an accessibility gap in every
egui_stack application.

### 2.6 Drag and drop between windows

**Today.** egui-pop refuses egui_stack's floating windows
(`allowed_in_windows` false, `workbench.rs:2965`); a panel leaves the
main window only through Detach, one panel per OS window, and comes
back through the close step or Attach. A tab cannot be dragged into or
out of a detached window. egui_stack's drag state lives in the
`Context`'s temporary data keyed by the dock's id (egui_stack
`state.rs:16-28`), in one viewport's coordinates.

**Direction.** This waits on egui and eframe: dragging between
viewports needs a pointer known across windows, which eframe does not
give (egui_dock issue 202's last word: blocked). What egui_stack can do
first is cheap: hit-test drops in global coordinates (each viewport's
`inner_rect` known), and accept a drop released over another viewport's
`DockArea` sharing the same `DockState`. Not before section 3's work.

## 3. Detached windows

**What egui-pop adds on top of egui_dock's windows.** egui_stack's
windows are `egui::Window`s inside the same viewport, bounded by the
dock's `window_bounds` (egui_stack `src/dock_state/window_state.rs:121-140`,
`show/window_surface.rs:13-120`); `WindowState::screen_rect` and
`dragged` are never written. egui-pop uses none of them: its detached
panels are OS windows (`windows`, `workbench.rs:1371-1478`, through
`show_viewport_immediate` at `:1409`), each:

- titled "Panel — Application", with the application's own frame when it
  draws one (title bar, resize edges, full screen), its own scale and a
  `scale.changed` event;
- a window of the application in full: the key dispatcher runs in its
  pass, the `window` context key holds the panel, dialogs and the
  palette come up over the window in use and block the others, each
  window has its own overlay stack and `WINDOW_OVERLAYS` zone;
- closed through the `workbench.close_window` step, vetoable
  (`CancelClose`);
- kept in the layout (`Auxiliary { panel, screen, rect }`,
  `crates/egui-pop-model/src/layout.rs:109`), repaired against the screens
  before it opens (`crates/egui-pop-api/src/window.rs:95`);
- one panel each: no tabs, no splits in a detached window.

On top, egui-pop has its own floating **palettes** (`palettes.rs`):
groups, stacks, tabs one level deep, anchoring, magnetism, docking into
an area; none of it is egui_dock's.

**Pull request 203** (egui_dock, branch `multi-viewport`, open since
2023-11, last commit 2024-01, 4 files, +113 -52, against egui_dock 0.11
on egui 0.24): behind a `viewports` feature, `WindowState::create_window`
returns a `ViewportBuilder` (no decorations, resizable, position and size
from the state) and `show_window_surface` runs the window body inside
`show_viewport_immediate`. Its own checklist leaves open deferred
viewports, docking a second tab into an existing window, dropping a
window outside the main viewport, minimise and maximise, tiling window
managers; closing the OS window does not remove the surface.

**Verdict: take the idea, not the branch.** The diff targets code that
no longer exists (`create_window` returned `(Window, bool)`, leaves
were struct variants, egui moved 13 minors), so it is a rewrite. And
for egui-pop a viewport per surface is not enough: everything listed
above (frame, keys, overlays, dialogs, the close step, screen repair)
happens around the window body. If egui_stack grows OS windows, the
shape that fits egui-pop is a **host hook**: `TabViewer::window(&mut self,
surface, builder, body: &mut dyn FnMut(&mut Ui))` (or a `SurfaceHost`
trait) where the caller opens the viewport, wraps the body with its
frame and dispatcher, and reports close requests; egui_stack keeps the
surface's tree, its tabs and its drops. That lets egui-pop put several
tabs in a detached window, the step model untouched.

- **Benefit:** medium, later: tabbed detached windows.
- **Cost:** high (cross-viewport drag waits on eframe).
- **Risk:** medium: two window systems (egui_stack's in-viewport ones and
  OS ones) to keep apart.

## 4. State: save, restore and migration

**Today.** egui-pop never serializes egui_dock's state. Its `layout`
document (`crates/egui-pop-model/src/layout.rs:666`) is JSON through the
persisted documents' schema ([ADR 0008](adr/0008-persisted-documents.md)),
version 2 with one migration (areas renamed, `layout.rs:696-722`). It
holds per area a binary `Node` (`Tabs { panels, active }` or
`Split { vertical, ratio, a, b }`, `layout.rs:18`), plus what no dock
knows: detached windows with their screen, hidden panels, folded
areas, areas lying over the centre, flyout widths, floating palette
groups and docked groups. `Layout::repair` (`layout.rs:1071`) fixes it
against the declared panels and screens; named arrangements are layouts
in the host's store ([workbench](workbench.md#arrangements)).
Sections keep their own `Layout` (fold and heights set by hand, by key)
in egui's persisted memory, readable whole for an application
(`crates/egui-pop-core/src/sections.rs:104-180`).

egui_stack's serde (feature `serde`) covers `DockState`, `Surface`,
`WindowState`, `Tree`, `Node`, `LeafNode` with runtime rects, indices
and all styles; no version field. Its binary heap stores `Empty` nodes
and positions (`2n+1`, `2n+2`), so a saved tree is tied to its storage.

**Direction S: a small, plain, versioned layout value in egui_stack, not
a migration framework.**

- `egui_stack::LayoutTree<T>`: `Tabs { tabs: Vec<T>, active }`,
  `Split { axis, fraction, children }` (n-ary once section 6's
  direction lands), with `DockState::to_layout()` and
  `DockState::from_layout()`, serde behind the feature, a `version: u32`
  field and a documented rule: a field added gets a default, a change
  of meaning bumps the version, and egui_stack reads the versions it
  wrote (its own small upgrade, inside the crate).
- The application owns its document: what goes with the tree (egui-pop's
  areas, hidden, folded, palettes) and the migrations of its own
  meaning stay egui-pop's, in ADR 0008's schema.

Removes `to_dock`, `fill`, `read`, `leading_tabs` (`workbench.rs:1528-1600`)
and the 0.05-0.95 clamping guess; egui-pop's `Node` becomes egui_stack's
`LayoutTree<PanelId>` inside its own document (a migration, version 3,
if the field names change).

- **Benefit:** medium: one tree shape, any egui_stack user gets a stable
  format instead of serializing runtime state.
- **Cost:** low to medium.
- **Risk:** low; owning a full versioned model (with app migrations) in
  egui_stack would be the risk: it would duplicate ADR 0008.

## 5. Sections and toolbars

What exists in egui-pop:

- **Sections**: the arithmetic in `crates/egui-pop-model/src/sections.rs`
  (1041 lines, no egui: `fit` at `:302`, `plan_drag` at `:108`,
  `keep_title`, `pin_plan`, `rows_least`); the widget in
  `crates/egui-pop-core/src/sections.rs` (1425 lines). The widget reaches
  into egui-pop in few places: the kit (`section-band`,
  `section-disclosure`, `section-count`, focus rings, `:641-1060`), the
  theme's tokens and text roles, the strings provider for the boundary's
  name (`SECTION_RESIZE`, `:745`), the pointer gestures map for the pull's
  keys (`gesture.sections.pull`), the indicator pill for the count, and
  `Slot` state. Accessibility is its own: buttons named with their
  count and `expanded` (`:1062-1063`), splitters with value and
  increment and decrement actions (`:1163`, `:1318`). `laid_out`
  (`:304`) feeds the debug bridge's `state` collector.
- **Toolbars**: `crates/egui-pop-api/src/toolbars.rs` (901 lines),
  a menu id drawn as buttons. Most of it is egui-pop's command world
  (menus, `when` clauses, keys, arguments, list providers, busy runs,
  the icon registry, the `toolbars` collector); the plain part is the
  layout: orientation, labels mode, the overflow into More tools
  (`fit`, `:870`), separators that never end a row.

**Direction P: a plain-egui core in egui_stack, egui-pop wraps it**
(accepted in [ADR 0022](adr/0022-sections-and-toolbars-in-egui-stack.md)):

| In egui_stack | Stays in egui-pop |
|---|---|
| `sections::plan`: the arithmetic, as is (no egui types) | The kit's painter for bands, marks, counts and rings |
| `Sections`, `Section`, `Stack` with the three body kinds, fold, resize, pull, titles that drag or stay, empty room, windowed rows, the wheel contained | The strings (Fluent) behind the labels struct |
| The persisted `sections::Layout` (by key), `laid_out` | The pull's keys from the gesture map, rebindable |
| Roles, names (from a labels struct with English defaults), keyboard on bands and boundaries | The debug collector, the value store, agent actions |
| A `SectionsPainter` trait, defaults from egui's `Visuals` | Saving the layout with the workbench's |
| `ToolRow`: items (id, icon as an `egui::Image` or a paint closure, label, tooltip, enabled, checked, busy, separator, submenu), orientation, labels mode, overflow into a More button, roles and names | Toolbars declared as menus, commands, `when`, keys in tooltips, arguments, list providers, the icon registry, the busy spinner, the `toolbars` collector |

What a caller gets from egui_stack, sketched:

```rust,ignore
let out = egui_stack::sections::Sections::new("sessions")
    .painter(&my_painter)          // default: egui's visuals
    .labels(&my_labels)            // default: English
    .pull_modifiers(Modifiers::COMMAND)
    .show(ui, |stack| {
        stack.section(Section::new("live", "Live").count(3), |ui| { /* … */ });
    });

let tools = egui_stack::toolbar::ToolRow::new("format")
    .orientation(Orientation::Horizontal)
    .show(ui, &[Tool::new("bold", "Bold").icon(bold_image).checked(true), Tool::separator()]);
if let Some(id) = tools.clicked { /* the caller runs its command */ }
```

- **Benefit:** high for egui_stack's purpose (its stated growth),
  medium for egui-pop: the widget gets other users and testers, egui-pop
  keeps less code; sections can later become a container kind of the
  dock (section 6).
- **Cost:** medium: a painter trait and a labels struct to cut, the
  arithmetic moved (egui-pop-model loses a module; layering, ADR 0001),
  the gallery's snapshots must not move.
- **Risk:** medium: two repositories to change in step while sections
  still evolve (five versions in a week). Mitigation: move only when the
  behaviour has settled, and keep egui-pop's tests as the reference
  (they run against the wrapper unchanged).

## 6. egui_tiles

[egui_tiles](https://github.com/rerun-io/egui_crates/tree/main/crates/egui_tiles)
0.18 (MIT OR Apache-2.0, now in the `egui_crates` workspace) models a
layout as `Tiles` in a map by `TileId`, each a pane or a container:
`Tabs { children, active }`, `Linear { children, dir, shares }` with
relative weights (n-ary), `Grid { children (holes allowed), layout:
Auto | Columns(n), col_shares, row_shares }`. `SimplificationOptions`
prune empty and single-child containers, join nested linears, flatten
tabs in tabs. One `Behavior` trait holds the pane's UI, titles,
closing (`is_tab_closable`, `on_tab_close -> bool`), the whole tab
(`tab_ui`), tab bar slots (left, trailing, top right), colours and
strokes per part, `paint_drag_preview`, `paint_on_top_of_tile`,
`resize_stroke`, `retain_pane`, `cycle_tabs_with_keyboard`, `on_edit`.
It has no drop veto, no floating windows and no viewports.

Where its model fits egui-pop better than a binary tree:

- **n-ary splits with shares.** A side area holding three panels one
  above the other is two nested binary splits today, whose fractions
  interact (moving one sash rescales the other); egui-pop's sections
  already chose VS Code's model, a sash between neighbours that gives
  way in turn. A `Linear` with shares is the natural tree for it, and
  egui-pop's `Node::Split { ratio, a, b }` would become
  `Split { axis, shares, children }`.
- **Sections as a container kind.** A `Linear` vertical container whose
  children show title bands and fold is exactly a stack of sections:
  VS Code lets a view move from a side bar's stack to the panel's tabs.
  With an n-ary tree, egui_stack could offer `Stack` (sections) beside
  `Tabs` and `Linear`, and a panel dragged onto a section stack would
  become a section ([sections](sections.md) lists "moved out of the
  stack" as not done).
- **Grids** fit image work (compare views, a 2×2 of viewports) better
  than nested halves; useful later, not today.
- **Simplification** as explicit, named options replaces egui_dock's
  implicit collapse rules, and is what egui-pop's `read` does by hand
  (`(Some(n), None) => Some(n)`, `workbench.rs:1597`).
- **Stable ids** (`TileId`) instead of heap positions make paths stable
  across edits, so a drag, a focus or a hint can name a node from one
  frame to the next.

Where egui_dock's model fits better: windows (surfaces), built-in scroll
areas, close and add buttons, the drop overlay's split preview, and a
history of users who know it.

**Direction N: borrow the model, keep the API.** Grow egui_stack's tree
toward n-ary `Linear` nodes with shares and stable node ids, behind the
existing `DockState`/`TabViewer` surface, and add a sections container
kind; take `SimplificationOptions` as named options; credit egui_tiles
in NOTICE and the changelog for each idea or piece of code taken. Do
not adopt a single `Behavior` trait: keep `TabViewer` (what a tab is)
apart from `DockPainter` (how it looks), because egui-pop's kit and its
panels change for different reasons.

- **Benefit:** high, long term: sections, areas and docks in one tree.
- **Cost:** high: the binary heap (`NodeIndex` arithmetic, `Tree::split`
  moving subtrees level by level) runs through the whole crate.
- **Risk:** high: the largest divergence from egui_dock, and its
  serialized states change (direction S's versioned value absorbs that).

## Directions, ranked

Ranked by benefit for cost, risk last.

| # | Direction | Benefit | Cost | Risk |
|---|---|---|---|---|
| 1 | **A** Accessibility built in: roles, names, labels struct (2.5) | High: every egui_stack user; removes egui-pop's after-the-fact naming | Low | Low |
| 2 | **V** Drop veto by tab and destination, public drag state (2.1, items 1 and 2) | High: removes a documented workaround and the private-id guess | Low | Low |
| 3 | **K** Keyboard: roving tab focus, plain-arrow splitters, Escape cancels a drag, optional Ctrl+Tab (2.4) | Medium to high: closes keyboard survey items | Low to medium | Low (options keep today's behaviour) |
| 4 | **M** Tab menu and close: built-in items optional, public operations, the double `on_close` fixed (2.2, 2.3) | Medium: lets egui-pop's tab menu become commands and show close buttons | Low | Low |
| 5 | **T** `DockPainter` trait for tabs, bars, separators, drop previews (1) | High: the kit drives every part, less code, narrower and overlapping tabs | Medium | Low to medium |
| 6 | **S** A versioned plain `LayoutTree<T>` (4) | Medium | Low to medium | Low |
| 7 | **P** Sections and a toolbar row as plain-egui cores (5, ADR 0022) | High for egui_stack, medium for egui-pop | Medium | Medium |
| 8 | **N** n-ary linear nodes with shares, stable ids, a sections container kind (6) | High, long term | High | High |
| 9 | **D** One dock for the whole workbench, moves through a hook (2.1, item 3) | Medium: insertion and splits across areas | High | Medium |
| 10 | **W** OS windows through a host hook, then drops between windows (2.6, 3) | Medium, later | High, partly blocked upstream | Medium |

## Recommended order

1. In egui_stack, the small changes below (A, V, K, M): each lands alone,
   with its test, and is offered upstream to egui_dock where it applies.
2. egui-pop moves to egui 0.37 (also expected for the default fonts,
   [upstream](upstream.md)), then from egui_dock to egui_stack: a rename,
   then dropping the workarounds A and V remove.
3. T, the painter trait; egui-pop's kit implements it and `paint_tabs`
   goes.
4. P, sections first (their arithmetic is egui-free and tested), then the
   toolbar row, once sections' behaviour has settled.
5. S, the layout value, folded into egui-pop's `layout` document version 3.
6. N, then D, as one redesign of the tree, decided by its own ADR.
7. W when eframe can drag between viewports.

## First egui_stack changes, issue-sized

Each is one pull request with a test and a changelog entry.

1. **Context menu Close calls `on_close` twice.** `leaf.rs:518` asks the
   viewer, then the removal loop asks again (`show/mod.rs:104`). Push a
   forced removal once the viewer said `Close`.
2. **Tabs have no accessible role or name.** Role `Tab` with the title,
   `selected` when active, a `TabList` per bar.
3. **Buttons and separators have no accessible role or name.** Close,
   add, close-all, collapse, minimise named from a `Labels` struct with
   English defaults; separators as `Splitter` with value, increment and
   decrement.
4. **`TabViewer::drop_allowed(tab, destination) -> DropVerdict`**, default
   `Allow`, the refusal's reason given to the overlay.
5. **Public drag state**: `DockArea::dragged_tab` (path and pointer), so
   no caller copies the tab button's id scheme (`leaf.rs:403-407`).
6. **Escape cancels a tab drag.**
7. **Plain arrows move a focused separator**; Shift a larger step;
   Enter resets to the default fraction.
8. **`DockArea::show_builtin_tab_menu(false)`**: the viewer's menu alone,
   without Eject and Close.
9. **`TabViewer::context_menu` gets the `TabPath`**, and the built-in
   actions are public `DockState` methods (close, eject, focus, move).
10. **`DockArea::tab_focus(TabFocus::Roving)`**: one Tab stop per bar,
    arrows, Home and End along it.
11. **`WindowState::rect()` and `dragged()` are never written**
    (`screen_rect`, `dragged`): write them, or remove them.
12. **The width a tab gets**: a `DockPainter::tab_width` measure (or,
    before the trait, a `TabStyle::padding` field) so a tab can be
    narrower than its label plus 16 points.
