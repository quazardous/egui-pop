# 0013, appendix: the analysis behind the mount contract

The analysis that led to [ADR 0013](0013-mount-contract.md), kept for
the reasons it gives. Where it differs from the ADR, the ADR wins; the
open questions of section 6 are answered there. The
matrix it starts from, written by hand from the code, is
[hosts.md](../hosts.md); the codes used below (c1, D1, E1, U1...) are
that page's.

## 1. What egui-pop already has

Several studs exist already, each covering one host or one concern:

| Piece | What it declares | What it misses for a general contract |
|---|---|---|
| `PanelSpec` (`egui-pop-model::panels`) | Areas allowed (`only_in`), `min_size`, `default_size`, `detachable`, `floatable`, `palette`, `bare`, `open`; `refuses(area)` returns a reason ("X stays out of the bottom panel"), shown by the drop hint and reported (ADR 0003). | Only areas; nothing about keyboard, size bounds of other hosts, windows, display, instances. It describes the panel, not the component inside it. |
| The panel registry | `AppHandle::panels()` on the main branch; `AppHandle::declared_panels()` with each panel's owner on the entry-icons branch (not merged). | A list of panels, not of components or hosts. |
| Zones and blocks (ADR 0005) | Named places, blocks added, replaced, extended, wrapped, resolved at build time with reported failures. | A zone says nothing about what fits in it (the status bar and the main zone look alike to a block). |
| The overlay stack (`egui-pop-core::overlay`) | Modal, popover, menu; Escape closes the top one; a click outside closes popovers, never a modal; focus restored on close. | Not exposed as a host: the settings window, the command palette and the file picker use it directly. |
| Focus scopes and key resolution (ADR 0007) | Surfaces (`surface:<name>`), `through_modal`, the Escape order, traces with verdicts (`BlockedByModal`, `SurfaceNotFocused`). | Already the right vocabulary for the keyboard part of the contract; nothing ties a component's surface to the hosts it may be in. |
| Steps (ADR 0011) | `workbench.detach`, `palette.float`, `dialog.show`, each with veto and redirect. | Hooks can refuse a move for a plugin's own reasons; the contract gives the structural reasons before any hook runs. |
| Fault boundary | Panels, blocks and stories draw inside one. | Dialog bodies and raw modals do not. |
| The context stack's projection (ADR 0012, on its branch) | Per projection key: the layout (docked, floating and detached panels), the focused panel, named view values (`set_view_value`, `scroll_area`). | Dialogs, popovers and zones are outside it; a component's view state is kept only if it uses those names. |
| Stories | A component in its states, reused by tests as fixtures. | The fixture a mount test needs already exists for most components. |
| Threading (docs/threading.md) | Every producer posts to a queue and wakes the interface; the queue is drained when the component draws. | Whether a component's work must go on while another surface is modal, or while it is hidden, is nowhere declared. |

### Hosts, in the contract's vocabulary

| Host | Size | Resizable | Focus policy, keys it owns | Lifetime | Window | Keymap | Projection | Keeps | `Ui`, `AppHandle` |
|---|---|---|---|---|---|---|---|---|---|
| Docked panel | fills its area rect, `min_size` across | yes | shared (scope `panel:<id>`); none | persistent | main, or auxiliary when detached | yes | yes (branch) | place; view values (branch) | yes, yes |
| Floating palette | width `min_size`..`default_size`, content height | no (E2) | click-into; Escape gives the keyboard back | persistent | main | yes | yes (branch) | place, rolled, pinned | yes, yes |
| Auxiliary window | fills a window | yes | the OS window's; none | persistent | auxiliary | **no** (E3) | yes (branch) | window rect | yes, yes |
| Dialog (four kinds) | `dialog.width`, content height | no | modal-exclusive; Enter, Escape | transient, one at a time | main | through modal only | no | position (opt-in) | **no** (E1) |
| Raw modal | optional width, content height | no | modal-exclusive; Escape | transient | main | through modal only | no | nothing | yes, yes (from a block) |
| Popover | content | no | transient; Escape, a click outside | transient | main | yes | no | nothing | yes, yes |
| Line zone, status bar | one row | no | shared (zone scope); none | persistent | main | yes | no | nothing | yes, yes |
| Viewport overlay | the image | n/a | none | per frame | follows the viewport | n/a | no | the application's data | **no** (D2) |
| Gallery story | the gallery's main zone | yes | shared; none | persistent | main | yes | no | nothing | yes, yes |

### Components, in the contract's vocabulary

| Component | Keyboard, claims | Pointer | Workers | Surface, colour | Size (min, fill) | Instances | View state | Session | Needs `AppHandle` |
|---|---|---|---|---|---|---|---|---|---|
| Image viewport | hover (Space, loupe, compare) and focus (zoom, arrows); claims Escape during a drag | drag, wheel, bitmap cursors | tiles, drained when drawn | paint callback or textures; wide gamut wanted | none, fills | several | zoom, centre (caller) | no | no (steps optional) |
| Navigator | focus | drag, wheel | no | textures | 120..320, 3:2 | several | shares the viewport's | no | no |
| Node graph | focus; claims Escape, Delete; Space read globally (c9) | drag, wheel | no | egui `Scene` | fills | several (`id_salt`) | view rect, selection (caller) | yes (wiring) | for agent commands |
| Curves, wheels, histogram | focus | drag | no | painter | 120..320 square; 48+; 120..600 | several | caller | no | no |
| Thumbnail | none | click | no | texture | 96 by 72 | several | none | no | no |
| Library grid, filmstrip | focus (culling keys) | rubber band, wheel | thumbnails | textures | fills; filmstrip 120 high | several | selection, size, scroll (caller) | no | for commands |
| Compare view | none | drag, wheel | thumbnails | textures | at least 80 high, fills | several | pan, zoom (caller) | no | no |
| Text, code editor | focus; surface `editor`; claims Enter, Tab, Escape (popups) | drag-select | assist providers | text | fills | several | model, scroll | yes (unsaved text) | for steps and keymap |
| Terminal | **exclusive** while focused (except Shift+Escape); surface `terminal`; claims Enter, Tab, Escape | drag-select, Ctrl+click | PTY threads, started on first drawing | text | at least 4 cells, fills | several | scrollback | yes (a process) | for steps |
| Tree, table, list | focus; claims Enter, Space, F2 (tree) | drag inside (tree), resize columns | sources (tree) | text | fills | several | expansion, selection (caller); widths in the store (table) | no | no |
| Sliders, forms | focus; claims Enter (typed entry), Escape (drag) | drag | no | painter | at least 120 wide | several | value | no | no |
| Progress | none | none | no | painter | fills width | several | none | no | no |
| Jobs centre | focus | click | the host's jobs, folded from the journal | text | fills | **single** (service) | expanded jobs (service) | no | **yes** |
| Theme editor | focus | colour picker popups | no | text, swatches | about 520 wide | **single** (service) | the draft (service) | yes (a draft) | **yes** |
| Tips | focus | click | no | text | a card, or one line for a hint | several (one service) | dismissed tips (settings) | no | **yes** |
| Breadcrumb, chips (branch) | focus | click | no | text | one row | several | the stack (owner) | no | **yes** |

## 2. The contract

### Needs

| Need | Values | Why it matters |
|---|---|---|
| `keyboard` | `None`, `Hover`, `Focus`, `Exclusive` | Line hosts give no lasting focus; a modal owns Escape and Enter; an exclusive component (a terminal) conflicts with any host owning a key. |
| `claims` | a set of keys: `Enter`, `Escape`, `Space`, `Tab`, arrows | A dialog's Enter accepts and Escape cancels; a component that uses them must be asked first (section 5). |
| `surface` | a keymap surface name, and whether its commands are bound there | Surface bindings fire through a modal but not in an auxiliary window today (c2, c4). |
| `pointer` | `Click`, `Drag`, `DragOut`, `Wheel`, `CursorImages` | Drag out of a transient host closes it; bitmap cursors fall back in an auxiliary window (U1). No component drags out today. |
| `workers` | `None`, `DrainedWhenDrawn`, plus `keep_under_modal` | Workers are the application's threads and keep running whatever the host; what matters is whether the queue grows while the component is hidden. |
| `display` | `Srgb`, `WideWanted`, `WideRequired`; `PaintCallback`, `ImageSurface` | Wide gamut exists only in the main window (U2): wanted degrades, required refuses. |
| `size` | `min`, `preferred`, `max`, `fills`, `aspect` | A content-sized host gives a filling component nothing to fill (c1); a line host gives one row (D1). |
| `instances` | `Several` (caller's state), `Single` (a service) | Two mounts of a single component show one state (c7). |
| `view_state` | a key, or none | Kept only by hosts in the projection, or by the store. |
| `app` | whether it needs an `AppHandle`, and which services | A host without one (the viewport overlay) cannot take it. |
| `role` | an AccessKit role and name | Checked by the test kit ("every interactive node has a name"); the host adds its own (a dialog is named by its title). |
| `asks` | whether the component is a question that must be answered | Only a modal host takes it (D4). |
| `session` | whether ending it by accident loses work | A transient host closed by a click outside refuses it (D3). |
| `preview` | whether it can be drawn non-interactive | A thumbnail-sized, read-only mount (a story's index, a template's preview) fits more hosts. |

A plain widget declares `Mountable::plain()` (focus keyboard, no
claims, several instances, no workers, sRGB, any size); most of today's
`yes*` cells are exactly that.

### Offers

| Offer | Values |
|---|---|
| `size` | `Fill(rect)`, `Bounded { width, height: content }`, `Row`, `NoUi` |
| `resizable` | by the user or not |
| `focus` | `Shared`, `ClickInto`, `ModalExclusive`, `None` |
| `owns_keys` | the keys the host takes before the content (Enter, Escape, Tab) |
| `dismissed_by` | `Answer`, `Escape`, `OutsideClick` |
| `lifetime` | `Persistent`, `Transient` |
| `window` | `Main`, `Auxiliary`, `FollowsPanel` |
| `keymap` | whether the key dispatcher reaches it |
| `projection` | whether it is part of the context stack's projection |
| `keeps` | place, size, view values |
| `gives` | a `Ui`, an `AppHandle`, a focus scope, a fault boundary |
| `display` | `Srgb`, `Wide` (main window with an image surface) |

Offers compose: a panel detached into an auxiliary window offers the
panel's focus scope and the window's display and keymap reach.

### Matching rules

Each rule looks at one need against one offer and gives a verdict and,
when it does not fit, the kind of reason. The worst verdict wins; all
conditions are kept.

| Rule | Need, offer | Verdict |
|---|---|---|
| R1 size | `fills` against `Bounded` | fits under condition: an explicit size (c1) |
| | `min` larger than a `Row` | no, by design: D1 |
| | `fills` and `resizable` wanted, palette not resizable | fits under condition (E2 named as the gap) |
| R2 keyboard | `Focus` or `Exclusive` against a `Row` host | no, by design: D1 |
| | a claimed key the host owns (Escape in a modal) | fits under condition: the host takes it first (c4), until the free-content dialog lets a focused component claim it |
| | a keymap surface where the keymap does not reach (auxiliary window) | fits under condition: c2 (E3 named) |
| R3 session | `session` against `dismissed_by: OutsideClick` | no, by design: D3 |
| R4 question | `asks` against a non-modal host | no, by design: D4 |
| R5 display | `WideWanted` against `Srgb` | fits under condition: sRGB with the reason (c3, U2) |
| | `WideRequired` against `Srgb` | no, upstream: U2 |
| | `CursorImages` in an auxiliary window | fits under condition: system cursors (U1) |
| R6 instances | `Single` mounted twice | fits under condition: one shared state (c7) |
| R7 application | `app` against a host that gives none | no, by design: D2 (data-only host) |
| R8 `Ui` | any component against `NoUi` | no, by design: D2 |
| R9 workers | `DrainedWhenDrawn` in a host that can hide its content | fits under condition: the queue grows while hidden |
| R10 view state | a key against a host outside the projection | fits under condition: not kept across places and restarts |
| R11 dialog content | any component against the four-kind dialog | no, egui-pop limit: E1 (until section 5) |

A rule refers to a reason by code; the code's text (the rule, what to
do instead, the gap and its cost, the upstream entry) lives once, with
the rule, and is what the generated page and the reported refusal show.

### Worked examples

- **Image viewport in a dialog**: today R11 refuses (E1). With the
  free-content dialog: R1 gives c1 (an explicit size), R2 gives "Escape
  claimed during a drag" (handled, section 5), R9 nothing (the dialog
  draws it every frame): fits under conditions.
- **Terminal in a popover**: R3 refuses, by design (D3): a running
  program would end on a click outside. Instead: a panel, or a dialog
  that asks before closing.
- **Viewport in an auxiliary window**: R2 gives c2 (its keymap
  commands, if any, do not fire), R5 gives c3 (sRGB, system cursors):
  fits under conditions.
- **A mastering view requiring wide gamut, detached**: R5 refuses,
  upstream (U2).

## 3. Generating the matrix and testing it

- **Declarations live with the code.** Each component crate exports
  its `Mountable`s (a function per component, next to the widget); each
  host's offer is declared where the host is built (`egui-pop-api`
  for panels, palettes, windows, dialogs, overlays, zones and stories;
  `egui-pop-viewport` for its overlays). The types and `check` live in
  `egui-pop-model` (no egui), so every layer can use them (ADR 0001).
- **The page.** An xtask (`cargo xtask hosts`) collects every
  declaration through a registry the facade fills, runs `check` for
  every pair and writes `docs/hosts.md`: the hosts and their offers,
  the components and their needs, the matrix, the conditions and the
  reasons, and the counts. A test compares the committed page with a
  fresh generation (as `gallery_doc.rs` does for snapshots), so it
  cannot drift.
- **What stays hand-written**: the evidence (which test or page
  exercises a cell) comes from the mount tests themselves, which tag
  the cells they draw; "not tried" is any accepted cell no test tags.
- **Mock hosts** (`egui-pop-test::hosts`): one per offer, each a
  harness that reproduces the host's constraints: a `Fill` rect, a
  content-sized area of bounded width, a modal overlay (with the
  dispatcher, so `modalOpen` and Escape behave), a row, an immediate
  viewport for the auxiliary window (with kittest's limit noted).
- **One mount test per component**: for each host its verdict
  accepts, mount the component's fixture (its story, interactive and
  in preview), check that it stays within the host's size, that every
  interactive node is named, and the checkable conditions (a global
  binding refused under the modal with `BlockedByModal`; the
  component's own keys still received). For each refused host, check
  that `AppHandle::mount` reports the refusal with its rule and does
  nothing else (ADR 0003).
- **At run time**: `AppHandle::where_can(component)` lists the hosts
  and verdicts, and a `mounts` collector on the debug bridge shows
  every mounted component with its host and conditions, so an agent can
  ask "where can this go?" before moving anything.

## 4. Survey

| System | What a part declares | What the container declares | Take |
|---|---|---|---|
| **egui_dock** (MIT) | A `TabViewer` per tab type: `title`, `ui`, `is_closeable`, `allowed_in_windows`, `clear_background`, `scroll_bars`, `on_close`, `context_menu`. | The dock style; `AllowedSplits` globally. | Already used; the per-tab flags are the right granularity, but there is no per-destination refusal (its issue 351, our upstream entry). Take: capabilities per content, answered per destination by egui-pop. |
| **egui_tiles** (MIT OR Apache-2.0) | A `Behavior`: `pane_ui`, `tab_title_for_pane`, `is_tab_closable`, simplification options. | Container kinds (tabs, linear, grid). | Same shape as egui_dock; nothing about keyboard or windows. Nothing new to take. |
| **egui `Modal`, `Window`, `Popup`** (MIT OR Apache-2.0) | Free content in each. | Modal per viewport; popups closed by a click outside (`PopupCloseBehavior`). | The free-content dialog builds on `Modal` as dialogs already do; `PopupCloseBehavior` is the popover's `dismissed_by`. |
| **VS Code** views and contribution points | `contributes.views`: id, name, `type` (tree or webview), `when`, `initialSize`, `visibility`; `viewsContainers` per location (activity bar, panel); custom editors with a `selector` and `priority`; webviews with `retainContextWhenHidden`; `supportsMultipleEditorsPerDocument`. | Locations (side bar, panel, secondary side bar, editor area); views movable between view containers. | Declarative manifests read before any code runs; `when` clauses on placement; **`retainContextWhenHidden`** is exactly our `workers`/hidden question, made explicit and costly on purpose; multi-instance as a declared capability. Take all four. |
| **Qt** docks and widgets | `QWidget::sizePolicy`, `sizeHint`, `minimumSizeHint`; `focusPolicy` (`NoFocus`, `TabFocus`, `ClickFocus`, `StrongFocus`, `WheelFocus`); `QShortcut` context (`WidgetShortcut`, `WidgetWithChildrenShortcut`, `WindowShortcut`, `ApplicationShortcut`); `QAccessible` roles; `objectName` as the key `saveState` uses. | `QDockWidget::setAllowedAreas`, `DockWidgetFeatures` (closable, movable, floatable); `Qt::WindowModality`; `QMainWindow::saveState`. | The size triple and the focus policy enum map directly to our `size` and `keyboard`; the shortcut contexts are our global and surface scopes (and say "window" explicitly, which is what c2 is missing); `objectName` as the persistence key is our `view_state` key. Take the vocabulary. |
| **Blender** areas and editors | Panels declare `bl_space_type`, `bl_region_type`, `bl_context` and a `poll()` that decides at run time; operators have `poll()` too; keymaps per space and region ("Window", "Screen", "Image"); popups and `invoke_props_dialog` draw any layout in a dialog. | Any area can become any editor (space); regions (header, tool bar, side bar, window, HUD) are fixed per space. | The extreme case: every editor fits every area, which works because each editor fills its area and owns its keymap layer; placement rules sit in regions, not areas. `poll()` is a dynamic need; our needs stay static, and dynamic refusals stay in step hooks (ADR 0011). Free-content dialogs are ordinary there, with the operator keeping the confirm button. Take: keymap per host kind, the HUD as a region over the editor (our missing heads-up host over the viewport, D2's "do instead"). |
| **Other egui crates** | egui-modal (MIT) gives free-content modals with buttons; egui_extras has no placement model. | | Nothing to adopt: egui-pop's dialog already does more. |

**Adopt, wrap or own**: own. No crate declares placement needs and
offers across hosts; the vocabulary is taken from Qt (size, focus,
shortcut context), VS Code (declarative contributions, hidden-state
retention, multi-instance) and Blender (keymaps per host kind, a HUD
region). The matching is a pure function of a few hundred lines in
`egui-pop-model`.

## 5. The free-content dialog as a host

A new kind, `Dialog::Custom`, with a title, a primary verb and command
(or none, for a dialog that only closes), and a body: a draw function
returning the values the command gets (as a form's values are merged
in) and whether the primary action is possible now.

**What it offers**: modal and transient; bounded size, given by the
spec (`size(width, height)`, a fraction of the window by default for a
preview) rather than the content, so a filling component has something
to fill; optionally resizable by its corner (kept with the remembered
position); keyboard exclusive, global bindings blocked unless
`through_modal`, surface bindings of the body's components firing; a
focus scope `dialog` and a fault boundary around the body; main window
only; not in the projection (a modal tool that should come back after a
restart is a transient frame of the context stack instead, as ADR 0012
suggests).

**What it keeps from the dialog rules**:

- **A primary button that says what it does** ("Export 3 images"), a
  danger style, Cancel; never "OK".
- **Enter accepts, Escape cancels**, with claims: when the focus is in a
  component that claims Enter (an editor, a tree's rename) the key goes
  to it, and Ctrl+Enter accepts from anywhere; Escape goes first to a
  focused component that claims it **and has something to cancel**
  (a drag, a selection, a popup), then cancels the dialog. This needs
  one small API on the focus side: a component says, during its pass,
  that it will take the next Escape (as text inputs already do).
- **Steps** `dialog.show`, `dialog.answer`, `dialog.move` unchanged;
  the body's values are in the answer's arguments.
- **Agents**: the `dialogs` collector describes a custom dialog by its
  title, verb and the body's `describe` (a JSON value the body gives,
  such as the preview's view and settings), and answers it with
  `accept` (values merged) or `cancel`, as for a form.
- **Movable, closable, remembered position, backdrop** as today; the
  `Clear` backdrop is the default for a preview.
- **One dialog at a time** (D5): a body never opens a second dialog; a
  choice inside it is drawn inline.

**Implementation notes**: the queue clones the shown dialog every frame
and lives in a `Send` service; a body capturing the application's state
is neither `Clone` nor `Send`, so bodies are kept on the `AppHandle`
side, by dialog id, and the queue holds only the id. The dialog draws
the body every frame, so the components' queues are drained while it is
open.

**First use: a preview dialog with an image viewport.** An export or
crop preview: `Custom` with a 70 % window size, `Clear` backdrop,
movable, remembered under `export.preview`, primary verb "Export". The
viewport draws from its own tile source; the main viewport behind keeps
receiving and drawing tiles (egui still draws the interface behind a
modal). Its mount test: tiles delivered from another thread appear in
both viewports while the dialog is open; a global binding is refused
with `BlockedByModal`; the viewport's zoom keys work when focused;
Escape during a handle drag cancels the drag and not the dialog; Enter
exports. Not tried and to check first: two Wayland image surfaces in
the main window (the main viewport's and the dialog's) overlapping;
until then the dialog's viewport may ask for sRGB.

## 6. Steps, risks and open questions

### Steps

1. Accept this ADR (or amend it with the answers below).
2. `egui-pop-model::mount`: needs, offers, rules, verdicts, reason
   codes; unit tests per rule. No egui.
3. Offers for today's hosts in `egui-pop-api` and
   `egui-pop-viewport`; `PanelSpec` gains its content's `Mountable`,
   `refuses(area)` becomes `refuses(host)`; refusals reported (ADR
   0003) when a detach, a float or a mount is refused.
4. Needs for today's components (`Mountable::plain()` for most).
5. The generator and its staleness test; `docs/hosts.md` regenerated,
   this hand-written version retired.
6. Mock hosts and one mount test per component; refusal tests.
7. The free-content dialog and the preview dialog in the gallery.
8. `where_can` and the `mounts` collector.
9. A ticket per egui-pop limit: E1 (closed by step 7), E2 palettes
   resizable, E3 keymap in auxiliary windows, E4 overlays per window,
   E5 custom settings pages; and the upstream entry for U3 and for
   kittest's embedded viewports.

### Risks

- **Declarations that lie.** A component declared `Focus` that reads
  Space globally (the node graph today) shows "fits" where it does not.
  The mount tests are the guard; they must probe keys, not only draw.
- **Too fine a vocabulary.** Every need added is a column of
  declarations to keep. The list above stops at what today's matrix
  needed; anything else waits for a cell that needs it.
- **Dynamic needs.** A viewport with or without wide gamut, a graph
  read-only or editable: needs depend on options. Declarations take the
  options (`Mountable` built from the widget's settings), and preview
  and interactive are two declarations.
- **Multi-window cannot be tested headless.** kittest embeds immediate
  viewports in the main one; W cells stay "read from the code" until a
  wbox run covers them.
- **egui changes under the rules.** egui's modal is per viewport today;
  a change there moves E4. The rules name the egui behaviour they rely
  on, checked at each egui bump with the upstream register.
- **Cost for third-party components.** One declaration and one test
  each; the extension template gets them, so the cost is visible from
  the start.

### Open questions for the owner

1. **The free-content dialog's shape**: a new kind (`Dialog::Custom`,
   proposed) or a body that any kind may carry?
2. **Claims of Enter and Escape** inside a dialog: may a focused
   component take them (proposed: yes, when declared, with Ctrl+Enter
   always accepting), or does the dialog always win?
3. **Auxiliary windows**: fix the keymap and the overlays per window
   (E3, E4), or declare by design that an auxiliary window shows
   content and the commands stay in the main window?
4. **Palettes**: resizable (E2), or content-sized by design, refusing
   filling components?
5. **Several instances of a panel** (two viewports or two graphs as
   tabs): add a declared instance count to `PanelSpec`, or keep one
   panel per id and let the application declare several ids?
6. **Popovers holding a session** (D3): confirm as by design, or allow
   a popover that asks before closing?
7. **Wide gamut in the preview dialog**: required for its first use,
   or sRGB first and two image surfaces later?
8. **The generated page**: committed and checked for staleness
   (proposed), or generated only at documentation build?
9. **Dialogs and the projection**: stay outside (proposed), with
   transient frames for modal tools that must come back?
10. **A heads-up host over the viewport** (D2's "do instead"): wanted
    now, or wait for a component that needs it?
