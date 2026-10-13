# Hosts and components: what can live where

Status: **first version, written by hand from the code** (2026-10-14).
[ADR 0013](adr/0013-mount-contract.md) (proposed) replaces this hand-written page with one generated
from the declarations of a mount contract: components declare what they
need, hosts what they offer, and each cell below becomes the verdict of
matching the two. Until then this page is the reference, and every cell
that is not "supported" says why.

A **host** is a place egui-pop draws content in (a docked panel, a
floating palette, a dialog). A **component** is something drawn there
(the image viewport, the tree, the terminal). The question each cell
answers: can this component be mounted in this host today, and if not,
why not?

## How to read a cell

| Code | Status | Meaning |
|---|---|---|
| `yes` | supported | Works today and is exercised: a gallery page or a test draws it there. |
| `yes*` | supported | Works by construction (the host draws any `Ui`, the component is a plain widget with no condition below), but nothing exercises it yet. |
| `if c…` | supported with conditions | Works, under the conditions named (see [Conditions](#conditions)). |
| `D…` | not supported, by design | It would break a named rule on purpose; the rule says what to do instead. |
| `E…` | not supported, egui-pop limit | egui-pop does not do it yet; an estimated cost is given. |
| `U…` | not supported, upstream limit | egui, eframe or another dependency cannot do it; linked to [upstream.md](upstream.md). |
| `?` | not tried | No known reason and no test: a gap, not a refusal. |

"Impossible" is only ever a `D` or a `U`. An `E` is work not done yet.

## The hosts

| Column | Host | Where in the code | What it offers today |
|---|---|---|---|
| **P** | Docked panel: a tab in an area (centre, start or end side bar, bottom), including the shell's zones shown as panels (`app.side-bar`, `app.main`, `app.inspector`) | `Ctx::panel`, `PanelSpec`, `workbench` (over egui_dock) | The area's whole rect, resized by the user, at least `min_size` across the area; a focus scope `panel:<id>` and a fault boundary around the content; an `AppHandle`; the place kept in the layout (and, on the context-stack branch, per projection key); one drawing per frame per panel id. |
| **F** | Floating palette (toolbox, navigator): a panel declared `palette()` or `floatable(true)` | `palettes` | An `Area` over the workbench, width between `min_size` and `default_size` (320 by default), **height from its content, not resizable by the user**; never modal, keyboard only when clicked into, Escape gives it back; hidden by Tab, rolled up, pinned; its place kept in the layout. Same focus scope, fault boundary and `AppHandle` as a panel. Main window only. |
| **W** | Auxiliary window: a detachable panel detached into an OS window | `workbench::windows`, `show_viewport_immediate` | A whole OS window (own scale, own title bar when the application draws its frame), on the interface thread; the panel's scope, boundary and `AppHandle`; its rect kept in the layout. **Keys pressed there do not reach the keymap**, and overlays are drawn in the main window (see E3, E4). |
| **D** | Dialog: `Confirm`, `Form`, `Choose`, `Info` | `dialogs` (`DialogsPlugin`) | Modal, one at a time (a queue); the primary button says what it does, Enter accepts, Escape cancels; `Dim` or `Clear` backdrop; movable, closable, a remembered position; steps `dialog.show`, `dialog.answer`, `dialog.move`; an agent sees and answers it (the `dialogs` collector). **No free content**: a dialog is one of the four kinds, and a form's fields come from value descriptors. |
| **M** | Raw modal overlay: `egui_pop_core::overlay::show` with `Kind::Modal`, drawn from a block of the `OVERLAYS` zone | `overlay`; used by the settings window, the command palette, the in-app file picker | Any `Ui` in a centred modal (`width` optional, height from the content), in the overlay stack (Escape closes the top overlay, focus restored on close); global bindings blocked unless `through_modal`, surface bindings still fire. **None of the dialog rules**: no primary button, no Enter, no steps, no collector, not movable. A block gets an `AppHandle` and a fault boundary, but no focus scope of its own. |
| **O** | Popover anchored to something (a status bar item, a button) | `overlay` (`Kind::Popover`) or `egui::Popup`; the jobs centre's popover | An area next to its anchor, sized by its content; closed by Escape or by a click outside; transient. |
| **L** | A block in a line zone: the top bar, the title bar's caption, the activity bar | `zones` (ADR 0005) | One row (or one column of icons), the zone's focus scope (`topBar`, `titleBar`, `activityBar`), a fault boundary, an `AppHandle`. |
| **S** | The status bar: a block in `STATUS_BAR`, or an indicator | `zones`, `strips` | One row, the `statusBar` scope; an indicator is data (a reducer over the journal), a block draws a `Ui`; a click may open a popover (host O). |
| **V** | An overlay on the image viewport | `egui_pop_viewport::overlay` | **Data, not a `Ui`**: shapes and handles in image coordinates, drawn by the viewport, their drags handed back as signals. |
| **G** | A gallery story | `Ctx::story`, `Story` | Any `Ui` with an `AppHandle`, drawn by the gallery in its main zone (so inside a docked panel), with a fault boundary; tests reuse it as a fixture. |

Hosts that take **no component at all**, by design, and so have no column:

- **Toast** (`toasts`): text, a level, actions and a job's progress;
  data only. A component goes in the jobs centre or a panel the toast's
  action opens.
- **Settings window page** (`settings_window`): pages are generated from
  declared settings; there is no custom page (a gap, E5 below). The
  theme editor cannot be a page of it today.
- **Menus and the command palette**: items, not content.
- **A palette outside the main window** (a native utility window): not
  built, and blocked upstream, U3.

## The components

| Row | Component | Crate | What it assumes today |
|---|---|---|---|
| Image viewport | `Viewport` (with its loupe and compare split) | viewport | Fills the space given (no minimum); keys when focused (zoom, arrows), Space and the loupe and compare keys when hovered, Escape cancels a drag; wheel zoom; tool cursors (bitmap images); tiles from the application's source on any thread, drained when it draws; GPU paint callback when `gpu::install` ran, CPU otherwise; wide gamut through a Wayland subsurface **in the main window only**; display target per egui viewport; state owned by the caller, several instances fine; nothing persisted; one AccessKit node "Image viewport". |
| Navigator | `Navigator` | viewport | 120 to 320 wide, 3:2; keys when focused; shares the viewport's state. |
| Node graph | `GraphEditor` | viewport (+ imaging for steps and agent commands) | Fills the space; egui `Scene`; keys when focused (Delete, Escape, arrows); **Space read globally** (no hover check); several instances need an `id_salt`; state owned by the caller, not persisted; AccessKit bounds corrected by hand. |
| Curves | curve editor | viewport | Square, 120 to 320; keys when focused; drags. |
| Wheels | colour wheel | viewport | Square, at least 48 (160 by default); keys when focused. |
| Histogram | `Histogram` | viewport | 120 to 600 wide, a third as high; read-only unless draggable. |
| Thumbnail | `Thumbnail` | viewport | Fits a texture the application gives; clicks only; 96 by 72 by default. |
| Library grid | grid | views | Fills the space; keys when focused (arrows, culling keys), rubber band selection; thumbnails from the application's `ThumbSource`; commands `library.*` act on the last focused library. |
| Filmstrip | grid, filmstrip view | views | As the grid, 120 high by default. |
| Compare view | compare | views | Fills the space, at least 80 high; no keys; drag pans, Ctrl+wheel zooms. |
| Text editor | editor | editors | Fills the space; the `editor` focus scope and its surface bindings (find, replace, go to line, save); popups as egui areas; read-only mode. |
| Code editor | editor, code mode | editors | As the text editor, with assist popups and providers answering from any thread. |
| Terminal | `Terminal` | editors | Fills the space; **every key while focused** except Shift+Escape (leaves); the `terminal` surface; the PTY's threads start on its first drawing and live with the source. |
| Tree | tree | views | Keys when focused; internal drag and drop (off by default); context menus; a source answering from any thread. |
| Table, list | `DataTable`, list | views | Fills the width; keys when focused; widths and sorting kept in the store under a prefix. |
| Sliders | `Slider` and its kinds | core | At least 120 wide; keys when focused; Escape cancels a drag. |
| Forms | value descriptor fields (`param`) | core, api | One row per field; a slider, a check box, a drop-down, a colour, a text, a path with Browse. |
| Progress | bar, spinner | core | Not interactive; fills the width. |
| Jobs centre | jobs list | api | Needs an `AppHandle`; one service per application (the panel, the popover and the toast show the same jobs). |
| Theme editor | `theme_editor::show` | api | Needs an `AppHandle`; one draft per application; a list (200 to 320) beside a form; colour pickers in popups. |
| Tips | tip of the day card, hint | api | Needs an `AppHandle`; dismissed tips kept in the settings. |
| Breadcrumb, mode chips | `breadcrumb`, `mode_chips` | api, **on the context-stack branch only** | Needs an `AppHandle`; one row; drawn wherever the application calls them. |

## Today's matrix

| Component | P panel | F palette | W aux window | D dialog | M raw modal | O popover | L line zone | S status bar | V viewport overlay | G story |
|---|---|---|---|---|---|---|---|---|---|---|
| Image viewport | `yes` | `if c1` | `if c2 c3` | `E1` | `if c1 c4` | `?` | `D1` | `D1` | `D2` | `yes` |
| Navigator | `yes` | `yes*` | `if c2` | `E1` | `yes*` | `yes*` | `D1` | `D1` | `D2` | `yes` |
| Node graph | `yes` | `if c1` | `if c2` | `E1` | `if c1 c4 c9` | `D3` | `D1` | `D1` | `D2` | `yes` |
| Curves | `yes` | `yes*` | `yes*` | `E1` | `yes*` | `yes*` | `D1` | `D1` | `D2` | `yes` |
| Wheels | `yes` | `yes*` | `yes*` | `E1` | `yes*` | `yes*` | `D1` | `D1` | `D2` | `yes` |
| Histogram | `yes` | `yes*` | `yes*` | `E1` | `yes*` | `yes*` | `D1` | `D1` | `D2` | `yes` |
| Thumbnail | `yes` | `yes*` | `yes*` | `E1` | `yes*` | `yes*` | `if c6` | `if c6` | `D2` | `yes` |
| Library grid | `yes` | `if c1` | `if c2` | `E1` | `if c1` | `?` | `D1` | `D1` | `D2` | `yes` |
| Filmstrip | `yes` | `if c1` | `if c2` | `E1` | `if c1` | `?` | `D1` | `D1` | `D2` | `yes` |
| Compare view | `yes` | `if c1` | `yes*` | `E1` | `if c1` | `?` | `D1` | `D1` | `D2` | `yes` |
| Text editor | `yes` | `if c1` | `if c2` | `E1` | `if c1 c4` | `D3` | `D1` | `D1` | `D2` | `yes` |
| Code editor | `yes` | `if c1` | `if c2` | `E1` | `if c1 c4 c5` | `D3` | `D1` | `D1` | `D2` | `yes` |
| Terminal | `yes` | `if c1 c10` | `if c2` | `E1` | `if c1 c4 c10` | `D3` | `D1` | `D1` | `D2` | `yes` |
| Tree | `yes` | `if c1` | `yes*` | `E1` | `if c1` | `if c5` | `D1` | `D1` | `D2` | `yes` |
| Table, list | `yes` | `if c1` | `yes*` | `E1` | `if c1` | `if c5` | `D1` | `D1` | `D2` | `yes` |
| Sliders | `yes` | `yes*` | `yes*` | `if c8` | `yes` | `yes*` | `if c6` | `if c6` | `D2` | `yes` |
| Forms | `yes` | `if c11` | `yes*` | `yes` | `yes` | `yes*` | `D1` | `D1` | `D2` | `yes` |
| Progress | `yes` | `yes*` | `yes*` | `E1` | `yes*` | `yes` | `if c6` | `if c6` | `D2` | `yes` |
| Jobs centre | `yes` | `if c1 c7` | `if c7` | `E1` | `if c7` | `yes` | `D1` | `D1` | `D2` | `yes` |
| Theme editor | `yes` | `if c1 c7` | `if c7` | `E1` | `if c1 c7` | `D3` | `D1` | `D1` | `D2` | `yes` |
| Tips | `yes` | `yes*` | `yes*` | `E1` | `yes*` | `yes*` | `D1` | `D1` | `D2` | `yes` |
| Breadcrumb, mode chips | `?` | `?` | `?` | `E1` | `?` | `?` | `?` | `?` | `D2` | `?` |

Evidence for `yes`: every gallery page draws its component inside the
gallery's main zone, itself a docked panel, so a page counts for both P
and G. The jobs centre has its own bottom panel and a status bar
popover; sliders are in the settings window (M) and in a form (D, as a
float or int field); the progress bar is in the jobs popover (the
status bar's jobs item paints a ring of its own, not the bar).

| What guarantees a `yes` | Where |
|---|---|
| Gallery pages (P and G), each with its snapshot test | `crates/egui-pop-gallery/src/pages/`: `viewport`, `navigator`, `node_graph`, `grading` (curves, wheels), `images` (histogram, thumbnails), `library` (grid, filmstrip, compare), `editors`, `terminal`, `tree`, `table`, `sliders`, `values`, `progress`, `jobs`, `theme_editor`, `tips`; tests in `crates/egui-pop-gallery/tests/gallery.rs` |
| Jobs centre in its panel and its popover | `crates/egui-pop-api/src/jobs.rs`, `jobs_centre.rs` and their tests |
| Sliders and forms in a modal | the settings window, `crates/egui-pop-api/src/settings_window.rs`; the `Form` dialog, `dialogs.rs` |
| A panel in an auxiliary window (a label only) | `crates/egui-pop-api/tests/auxiliary.rs` |

No test mounts a component in a palette, an auxiliary window, a
popover or a raw modal: those `yes*` and `if` cells are read from the
code.

220 cells (22 components, 10 hosts); the count by status and reason
kind is [at the end](#counts).

## Conditions

| Code | Condition |
|---|---|
| c1 | **Content-sized host.** The palette, the raw modal and the popover size themselves from their content; a component that fills the space it is given (viewport, graph, grid, editors, terminal, tree, table, theme editor) must be given an explicit size (`.size()`, a `ScrollArea` with a maximum height) or it grows to the window. The user cannot resize a palette (E2). |
| c2 | **Keymap in an auxiliary window.** The key dispatcher reads the main window's input only (`App::show` dispatches before drawing the auxiliary windows), so bindings of the keymap (global and surface: the editor's find and save, the terminal's clear, the library's commands) do not fire while the auxiliary window has the keyboard. Keys a widget reads itself (arrows, zoom keys, typing, culling keys) work. Gap E3. |
| c3 | **Display in an auxiliary window.** Wide gamut and HDR fall back to sRGB there, with the reason reported (U2); bitmap tool cursors fall back to their system cursor (U1); the application sets the display target for that window's egui viewport id. |
| c4 | **Keys under a modal.** Global bindings are blocked unless `through_modal`; surface bindings fire when their scope is in the focus chain. **Escape closes the modal before any component sees it** (ADR 0007's Escape order: a text input, a drag, then the top overlay, then the bindings and widgets), so a component's own Escape (the graph's deselect, a terminal program's Escape) never arrives; Shift+Escape still leaves the terminal. |
| c5 | **Popups inside a transient overlay.** The component opens egui areas of its own (code assist, context menus, drop-downs) that are not in egui-pop's overlay stack; untested whether a click in them counts as a click outside the popover (which closes it) or above the modal. |
| c6 | **One row.** A compact size given by the application (a slider at least 120 wide, a small thumbnail, a thin bar). |
| c7 | **One state per application.** The jobs centre and the theme editor draw a service's state: two mounts show the same jobs or the same draft, not two independent instances. |
| c8 | **As a descriptor field only.** In a dialog, a slider exists only as a `Form` field made from a float or int descriptor, not as a free widget. |
| c9 | **Space read globally.** The node graph reads Space without checking the hover: behind a modal, or with two graphs, Space pans every graph drawn. |
| c10 | **A process per mount.** The terminal starts its program on its first drawing; a transient host shown again starts another one unless the application keeps the `TerminalState`. Escape never reaches the program inside a modal (c4). |
| c11 | **Not a question.** A palette never asks for an answer (D4): a form there applies its values as they change, with no primary button. |

Conditions that apply to every cell of a column, not repeated in it:

- **Workers.** Tile, thumbnail, tree and terminal sources run on the
  application's threads and keep running whatever the host (a modal
  included); their queues are drained when the component is drawn. A
  component in a hidden tab, a palette hidden by Tab or rolled up, or a
  closed dialog whose source still delivers, keeps filling an unbounded
  queue ([threading.md](threading.md), section 2.2).
- **View state.** Zoom, pan, scroll, expansion and selection belong to
  the caller's state and are not kept by any host on the main branch.
  The context-stack branch keeps named values per projection key
  (`set_view_value`, `scroll_area`) for content of panels, palettes and
  auxiliary windows (all in the layout); dialogs, popovers and zones are
  not in the projection. Table widths and sorting are kept in the store.
- **Accessibility in an auxiliary window** is not tried: whether eframe
  gives an immediate viewport its own AccessKit tree has not been
  checked.
- **Tests across windows.** egui_kittest draws an immediate viewport
  inside the main one, so no headless test can prove the behaviour of
  W cells (keys, modals, focus) as a separate window would have it.

## Reasons

### By design

| Code | Rule | Do instead |
|---|---|---|
| D1 | **Line hosts hold one row.** The top bar, the caption, the activity bar and the status bar hold indicators and small controls; nothing there takes the keyboard for long or needs more than a row. | An item in the line that opens a popover (the jobs centre's status item) or focuses a panel (`<panel>.focus`). |
| D2 | **Viewport overlays are data.** Overlays on the image are shapes and handles in image coordinates, drawn by the viewport, their edits handed back as signals; there is no `Ui` to put a component in. | A palette floating over the canvas (a navigator, a toolbox), or a heads-up host over the viewport (not built: a gap, not a refusal). |
| D3 | **Popovers are transient.** A click outside or Escape closes them; an edit session (graph wiring, an editor with unsaved text, a running program, a theme draft) would end by accident. | A panel, a palette, or a dialog that asks before closing. |
| D4 | **Palettes are never modal** (decided 2026-10-13). | Anything that must be answered first is a dialog. |
| D5 | **One dialog at a time.** Dialogs queue; a dialog does not open another over it. | Draw the second choice inline in the dialog (as a form's path field draws the in-app file picker). |

D4 and D5 constrain how a component is used rather than whether it fits,
so they appear as conditions (c11) rather than as cells.

### egui-pop limits

| Code | Limit | Estimated cost |
|---|---|---|
| E1 | **No free-content dialog.** A dialog is `Confirm`, `Form`, `Choose` or `Info`; a viewport, a curve, a preview cannot go in one, although the `Clear` backdrop was made for a dialog previewing its effect on the image. The raw modal (M) takes any content but loses every dialog rule. | Medium: a `Custom` kind with a body, keeping the rules (ADR 0013, section 5); about two to three days with tests. |
| E2 | **Palettes are not resizable.** Width between `min_size` and `default_size`, height from the content. | Small: a resize grip on the palette's area, the size kept in `Floating`; a day. |
| E3 | **No keymap in auxiliary windows.** Keys pressed there are not dispatched. | Medium: run the dispatcher in each immediate viewport's pass, with that window's focus chain; two days, and a decision on which window's modal blocks what. |
| E4 | **Overlays live in the main window.** Dialogs, menus, toasts and the command palette are drawn in the main window, and egui's modal blocks its own window only: a dialog opened from an auxiliary window appears in the main one and does not block the auxiliary window. | Medium to large: an overlay stack per window, or a rule that a modal blocks every window; three to five days. |
| E5 | **No custom page in the settings window.** | Small: a page kind drawing a block; a day. |

### Upstream limits

| Code | Limit | Entry |
|---|---|---|
| U1 | No custom cursor in immediate viewports (bitmap cursors fall back to system cursors). | [upstream.md](upstream.md#eframe-no-custom-cursor-on-the-web-or-in-immediate-viewports) |
| U2 | Wide gamut only through the image's own Wayland surface, made from the main window's handles (eframe gives no surface colour space, winit no colour management); other windows draw sRGB. | [upstream.md](upstream.md#wayland-no-colour-management-through-winit), [upstream.md](upstream.md#egui-wgpu-no-surface-format-or-colour-space-to-choose) |
| U3 | No owner window for egui viewports: a palette in a native utility window would float above every application. | [workbench.md](workbench.md#palettes-and-toolboxes-never-modal); not yet in upstream.md |

U1 and U2 degrade a cell (conditions c3) rather than refuse it: no
component of today requires wide gamut. A component that did (a
mastering view) would be refused in W with U2.

### Not tried

Each `?` is a gap with no known reason:

- **Viewport, grid, filmstrip, compare in a popover**: content-sized
  (c1) and transient; a quick preview popover is plausible, nothing
  draws one.
- **Breadcrumb and mode chips everywhere**: they exist on the
  context-stack branch only (the gallery page there draws them in its
  panel); the main branch has no such component.

## Counts

| Status | Reason kind | Cells |
|---|---|---|
| supported | exercised (`yes`) | 47 |
| supported | by construction (`yes*`) | 34 |
| supported with conditions | `if` | 44 |
| not supported | by design (`D1` 36, `D2` 22, `D3` 5) | 63 |
| not supported | egui-pop limit (`E1`) | 20 |
| not supported | upstream | 0 |
| not supported | not tried (`?`) | 12 |
| | **total** | 220 |

No cell is refused for an upstream reason today: upstream limits only
add conditions (c3). The one egui-pop limit that refuses cells is E1,
the free-content dialog, which refuses 20 of the 22 rows of column D
(forms are a dialog kind, sliders a form field). E2, E3, E4 and E5
appear as conditions or as hosts that do not exist yet.

## Next

- The matrix becomes generated (ADR 0013): this page is then produced
  from declarations and checked for staleness by a test.
- One mount test per component against mock hosts turns the `yes*`
  cells into `yes`, and refusal tests prove the `D` cells (the refusal
  is reported with its rule).
- The free-content dialog lifts E1.
