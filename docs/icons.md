# Icons: an inventory of where they serve

Status: **inventory, decided** (2026-10-22). Sections 1 and 2
describe what exists, read from the code of egui-pop and of Helios, an
application built on it. Sections 3 to 5 were proposed and are
**decided (2026-10-22)**, with the owner's answers in section 7.
Section 6 is the order of work that follows. The "Today" column of
section 2 follows the work: the registry of step 2 (section 6) is in.

egui-pop draws icons by role (`Icon`, in `egui_pop_core::icon`):
a name such as `search` or `warning`, never a codepoint, drawn from the
Phosphor font with the `phosphor` feature or painted otherwise, sized
by a token and coloured by a tone. An application adds its own SVG
icons by name (`icon_set::register`, feature `svg`). A kit remaps the
small marks widgets draw through **glyph roles** (`glyph.check`,
`glyph.disclosure-open`...; [kit.md](kit.md)). What is missing is a
rule for *which* icon a surface shows and *who* declares it: today each
widget has a field of its own, or picks its icon in its code.

## 1. Two families

An icon says one of two things, and they are declared in different
places.

- **Command icons** say an **action**: Save, Undo, Export, Toggle the
  grid. The command is declared once (`CommandSpec`), and so is its
  icon; every surface that runs the command (a menu, the palette, a
  toolbar, the shortcut editor, a status item) shows the same one. A
  command that toggles may have a second icon for its "on" state (eye
  and eye-off).
- **Object-kind icons** say **what a thing is**: a node type, a file
  type, a job kind, a mask element, a severity, a place or a mode, a
  panel. They are declared on the kind, not on a command: a row in a
  tree, a chip in the status bar or an entry in quick open shows the
  icon of the kind of thing it holds.

Two neighbours stay outside both families:

- **Glyph roles** (check, radio, disclosure, sort, submenu, close,
  window buttons, LED and flag shapes) are the marks widgets draw for
  their own mechanics. Their vocabulary is fixed and a kit remaps them
  with data. They are already one registry, and they stay as they are.
- **Application and document icons** in the system's places (the
  window's icon, the tray icon, desktop files, notifications) are
  pictures handed to the system, not icons of the set
  ([packaging.md](packaging.md)).

## 2. Where icons serve

Each table gives, for every surface:

- **Rule**: required, optional or not used, and why.
- **Family**: command, object kind, or glyph role when the mark is a
  widget's own.
- **Size and states**: the size token (`size.icon.inline` 16 points,
  `size.icon.toolbar` 20, `size.icon.panel` 24, each 4 points smaller in
  the compact density) and the states the icon follows.
- **Accessibility**: *decorative* when a visible text already names the
  thing (the icon adds nothing for AccessKit), or *only label* when the
  icon is all there is, and then which text names it.
- **Today**: the file and the field, or "hard-coded in the widget".
- **Gap**: what is missing for the rule.

Paths are relative to `crates/`. "Name lookup" means a string resolved
with `Icon::by_name` (egui-pop's names, then the application's
registered ones).

### 2.1 Shell: the window frame and the status bar

| Surface | Rule | Family | Size and states | Accessibility | Today | Gap |
|---|---|---|---|---|---|---|
| Window menu button (title bar) | Required: no room for text in the bar; the menu icon is the convention | Command (opens the system menu) | `toolbar`; hover, pressed | Only label: "Window menu" | Hard-coded `Icon::Menu` in `egui-pop-api/src/frame.rs` (`menu_button`) | None needed; could read the opening command's icon once commands have one |
| Window buttons (minimise, maximise, restore, close) | Required: system convention | Glyph role (`window-*`) | Drawn on the pixel grid; hover, pressed, the close's danger hover | Only label: each button's name | Kit glyphs in `egui-pop-core/src/frame.rs` | None |
| Overflow and main menu entries ("More menus", "Main menu") | Required: the menu bar folded into one button | Command | `toolbar`; hover, open | Only label: "More menus", "Main menu" | `Icon::More` and `Icon::Menu` hard-coded in `bar_within` (`egui-pop-api/src/menus.rs`) | None needed |
| Caption items (the application's marks in the title bar) | Required: icon-only by place | Command (each opens something) | `toolbar`; a count badge, a ring, a busy state | Only label: the command's title; the state joins the name | Chosen by the application per item (Helios: `Icon::Notification` hard-coded) | Should come from the command the item runs |
| Breadcrumb crumbs | Optional: the text names the place; an icon on every place that declares one, hidden in the compact density (decided 2026-10-22) | Object kind (a place, `FrameSpec`) | `inline`; the current crumb strong | Decorative | `FrameSpec.icon` (`egui-pop-model/src/context.rs`), drawn before the crumb by `status_context` (`egui-pop-api/src/strips.rs`), not in the compact density; `contexts::crumb_icons` and the `strips` collector give the names; the separator is a literal `›` | The separator should be a glyph role (`chevron-end`) |
| Mode chips | Optional: a mode is named by its text; an icon makes an active mode visible at a glance | Object kind (a mode, `FrameSpec`) | `inline`; the mode's tone | Decorative | `FrameSpec.icon` by name lookup, copied to `Chip.icon` (`egui-pop-api/src/contexts.rs`), drawn by `strips.rs` as `Mark::icon`; an unknown name is an `unknown-icon` diagnostic | An unknown name still draws nothing on the chip |
| Status items (indicators) | Optional: most items are short texts; an item that is icon-only (a connection, a sync) needs one | Command when the item runs one, object kind otherwise (a connection) | `inline`; tone, busy spinner, error and warning counts | Decorative beside text; only label when alone: the item's label | `strips::Indicator` has no icon field; the error and warning counts are hard-coded `Icon::Error` and `Icon::Warning` (`strips.rs`) | No icon on an item; the counts' icons should be the severity kinds |
| Status item popovers and hover cards | Optional: content drawn by the application | Either | `inline` | As for the content | Drawn by the item's `PopoverFn` or `HoverFn` (`strips.rs`, `egui-pop-core/src/hover_card.rs`) | None in egui-pop |
| Busy slot of a command control | Required: a spinner replaces the icon while a run waits | Command | The control's icon size; busy after `motion.spinner.delay` | Decorative; AccessKit's busy flag carries the state | `busy::paint_slot(.., icon: Option<Icon>, ..)` in `egui-pop-api/src/busy.rs`, the icon given per call | The icon should be the command's |

### 2.2 Menus and commands

| Surface | Rule | Family | Size and states | Accessibility | Today | Gap |
|---|---|---|---|---|---|---|
| Menu bar titles | Not used: every platform shows text titles | — | — | — | Text | None |
| Menu items (bar, context and popup menus) | Optional, in an aligned gutter (section 5); following the `menus.icons` setting (section 3) | Command | `inline`; disabled (greyed), checked (a tick in the gutter, or a frame around the icon), busy (a spinner in the gutter), the "on" icon of a toggle | Decorative: the title is the name; a check item is announced as a check box | Every row already reserves an `inline` square on its left (`command_row` in `egui-pop-api/src/menus.rs`) holding the spinner, the tick or the row's icon, in that order; command rows pass `icon: None`, though `CommandSpec` (`egui-pop-model/src/commands.rs`) now declares `icon` and `icon_on` | Rows do not read the command's icons yet; the tick hides the icon instead of framing it; no radio items (only checkable ones); no setting |
| List-provider entries | Optional, as menu items; an entry naming a thing (a recent file, a profile) takes that thing's kind icon | Command, or object kind | As menu items | Decorative | `ListItem::icon`, by value or by name (`menus.rs`), serialised by name for the `lists` collector | Set per entry; an entry without one does not fall back to its command's |
| Submenu entries | Optional: Apple and Microsoft both allow an icon on the entry; its arrow is a glyph | Command group (no command of its own) | `inline`; open | Decorative | `Item::Submenu { menu, label }` (`egui-pop-model/src/menus.rs`) has no icon; the arrow is egui's | No icon on a submenu; the arrow should be the `submenu` glyph role, as the kit documents |
| Command palette | Optional: shown in a fixed column when the command declares one; text stays the label | Command | `inline`; disabled entries greyed | Decorative | Text only (`egui-pop-api/src/palette.rs`) | Reads nothing; needs the command's icon |
| Quick open (a list of things to go to) | Required when the list mixes kinds (a module, an image, a workspace): the icon says which kind a match is | Object kind | `inline` | Decorative; the kind joins the accessible name when the text does not say it | egui-pop has none; Helios builds it on the choice dialog, whose rows are text only (`choice_row` in `egui-pop-api/src/dialogs.rs`) | Choice rows take no icon |
| Toolbars | Required: a toolbar is icon-first, with a tooltip giving the title and the key | Command | `toolbar`; disabled, on (selected), busy | Only label: the command's title | No toolbar component; an application lays out `CommandButton`s (`busy.rs`, icon set per instance with `.icon()`) or clickable `IconWidget`s | The icon is given by each caller; no container (overflow, text or icon mode) |
| Command buttons with text | Optional: text is the label | Command | `inline`; busy | Decorative | `CommandButton::icon` (`busy.rs`) | Per instance |
| Split buttons | Optional on the main part, not used on the arrow (a glyph) | Command | `inline` | Decorative | `input::split_button(ui, text, items)` (`egui-pop-core/src/input.rs`): text only | No icon |
| Toggle chips | Optional: a chip may be an icon alone | Command or object kind | `inline`; on | Only label when alone: `ToggleChip::name` | `ToggleChip::glyph(Icon)` (`egui-pop-core/src/widgets.rs`) | Per instance |
| Shortcut editor rows | Optional: helps to scan a long list; the title stays the label | Command | `inline` | Decorative | Text only (the shortcuts section of `egui-pop-api/src/settings_window.rs`) | Reads nothing |

### 2.3 Panels and the workbench

| Surface | Rule | Family | Size and states | Accessibility | Today | Gap |
|---|---|---|---|---|---|---|
| Activity bar | Required: an icon-only strip by convention | Object kind (a panel) | `panel`; selected (the shown panel), hover, focus, an indicator | Only label: the panel's title | `PanelSpec::icon: String` (`egui-pop-model/src/panels.rs`) by name lookup in `activity_bar` (`strips.rs`); empty or unknown: the title's first letter, an unknown name being an `unknown-icon` diagnostic | None |
| Folded strips and flyouts | Required: same as the activity bar | Object kind (a panel) | `panel`; selected, an indicator's dot or count | Only label: the title and its key | Built on the `wip/folded-strip-icons` branch from `PanelSpec::icon`, not yet on main | The same name lookup |
| Panel tabs | Optional, behind a setting off by default (decided 2026-10-22): the title is the label and tab bars are narrow | Object kind (a panel) | `inline`; active | Decorative | Text only (the tab bar in `egui-pop-api/src/workbench.rs`) | None now; when wanted, the same `PanelSpec` icon |
| Floating palette bands (close, roll up, pin) | Required: a band has no room for text | Glyph role (close, roll), command (pin) | The band's side; pin is `selected` while on | Only label: Close, Roll up / Roll down, Pin / Unpin | `band::button(ui, side, icon, name, on)` (`egui-pop-core/src/band.rs`); `Icon::ChevronDown`/`ChevronUp` and `Icon::Lock`/`Unlock` hard-coded in `egui-pop-api/src/palettes.rs` | Pin uses the lock icons; roll should be a glyph role |
| Settings categories (the window's tree) | Optional: categories are text; icons help on a long tree (as macOS and GNOME settings do) | Object kind (a settings section or page) | `inline` | Decorative | Text only (`settings_window.rs`, `settings_pages.rs`; `SettingsPage` has no icon) | No icon on a section or page |
| Theme editor token groups | Not used: groups are named by role and few; colour swatches already mark the rows | — | — | — | Text; undo and redo are text buttons (`egui-pop-api/src/theme_editor.rs`) | None; undo and redo could take the commands' icons if they become commands |
| Gallery page list | Optional: a long list of pages; a kind icon per page family would help scanning | Object kind (a page family) | `inline` | Decorative | Text only (`egui-pop-gallery/src/shell.rs`) | None needed for the gallery's purpose |
| Empty states | Required: the icon says what is empty; the title says why | Object kind | Drawn large in the `empty-state` component; danger tone on failure | Decorative: the title is the name | `EmptyState::new(icon, title)` (`egui-pop-core/src/placeholder.rs`) | Per instance |

### 2.4 Views

| Surface | Rule | Family | Size and states | Accessibility | Today | Gap |
|---|---|---|---|---|---|---|
| Tree rows | Optional per tree, required where rows mix kinds (files and folders, mask elements) | Object kind | `inline`, in the row's text colour; selected | Decorative | `TreeView::icon(Fn(&Node) -> Option<Source>)` (`egui-pop-views/src/tree/mod.rs`), a name given with `Source::named`; each row's icon name reported to agents (the `icons` collector's `drawn`) | A closure per tree, so the same kind may look different in two trees (kinds: step 6) |
| Tree disclosure | Required | Glyph role (`disclosure-*`) | `inline`; open, closed | Decorative (the row's expanded state is announced) | Kit glyphs | None |
| Row actions (buttons on a row) | Required: icon-only on hover or focus, by space | Command | `inline`; on/off for toggles | Only label: the command's title | Each application draws its own (Helios: eye and lock in the masks tree) | No row-action helper reading commands |
| Table and list headers | Not used, except the sort mark | Glyph role (`sort-*`) | `inline` | Decorative (the sort is announced) | Kit glyphs in `egui-pop-views/src/table.rs` | None |
| Table cells | Optional: drawn by the application | Object kind | `inline` | Decorative | Application code | None in egui-pop |
| File chooser (in-app picker): places | Optional: home, recent, bookmarks and mounted volumes are told apart by an icon on every platform | Object kind (a place) | `inline` | Decorative | Text only (`egui-pop-api/src/files.rs`) | No icons; the built-in `home`, `history`, `folder` exist unused there |
| File chooser: entries | Optional: a folder or a file type | Object kind (file type) | `inline` | Decorative; the type joins the name when hidden | Text only; folders end with a literal `/` | Folders and files look the same but for the slash |
| Library marks (stars, pick and reject flags, colour labels) | Required: the mark is the information | Glyph role | The component's size; on, off, hover | Only label: the rating or flag as text | Painted shapes in `egui-pop-views/src/library/marks.rs` (`paint::flag`, `paint::cross`), outside the icon set | A kit cannot remap them; they should become glyph roles |
| Library grid and filmstrip badges | Required where they mark a state (a failed thumbnail's warning, a stack's chevron) | Glyph role or severity kind | Scaled with the thumbnail, at most `panel` | Decorative beside text, else the state's name | `Icon::Warning`, `ChevronDown`/`ChevronRight` hard-coded in `library/grid.rs`; `Icon::Warning` in `library/compare.rs`; `Icon::Search` in `library/filter.rs`; `Icon::Close` on tag chips in `library/tags.rs` | Hard-coded; chevrons should be the disclosure role, the warning the severity kind |
| Editors' gutter | Required: a diagnostic's severity, a fold | Object kind (severity), glyph role (fold) | Line height; error, warning, info | Decorative: the diagnostic's text is reachable | `Icon::Error`/`Warning`/`Info` and `ChevronRight`/`ChevronDown` hard-coded in `egui-pop-editors/src/view.rs` | Hard-coded |
| Find bar | Required: compact buttons | Command (next, previous), glyph role (close, toggles) | `inline` | Only label: each button's name | `Icon::ChevronUp`/`ChevronDown` hard-coded, close from the kit, in `egui-pop-editors/src/find_bar.rs` | Next and previous are not commands with icons |

### 2.5 Imaging

| Surface | Rule | Family | Size and states | Accessibility | Today | Gap |
|---|---|---|---|---|---|---|
| Node graph: node kinds | Optional on a node's title, required in the "add a node" menu when it is long | Object kind (node type) | `inline` in the title; selected, bypassed, in error | Decorative: the title names it | `graph::Kind { id, title }` (`egui-pop-viewport/src/graph/model.rs`): no icon | No icon on a kind |
| Node graph: port types | Required as a **shape**: a type is told by colour today, and egui-pop never says a state by colour alone | Object kind (port type) | The port's size; connected or not | The type joins the port's name | `PortType { name, color }`: a colour token only | Add a shape (a glyph) per type |
| Node graph: group fold | Required | Glyph role (`disclosure-*`) | `inline` | Decorative | `Icon::ChevronRight`/`ChevronDown` hard-coded in `graph/mod.rs` | Should be the disclosure role |
| Navigator and zoom control | Required: compact buttons | Command (zoom in, zoom out) | `inline` | Only label: the command's title | `Icon::Plus`/`Minus` hard-coded in `egui-pop-viewport/src/navigator.rs` | Should read the zoom commands' icons |
| Slider quad button | Optional: an eyedropper, a toggle beside a slider | Command | `inline`; on for a toggle | Only label: the given label | `Slider::quad(icon, label)`, `quad_toggle` (`egui-pop-core/src/slider.rs`) | Per instance |
| Viewport tools | Required in a tool palette (icon-first) | Command | `toolbar`; the active tool selected | Only label: the command's title | No tool palette in egui-pop; tools are the application's | None in egui-pop yet |
| Tool cursors | Not icons: cursors, drawn in the icon set's line style | — | The cursor size | — | `egui-pop-viewport/src/cursor.rs` ([cursors.md](cursors.md)) | None; out of this inventory |
| Viewport overlays (handles) | Not used: handles are shapes | — | — | — | Overlay kit | None |

### 2.6 Feedback, dialogs and forms

| Surface | Rule | Family | Size and states | Accessibility | Today | Gap |
|---|---|---|---|---|---|---|
| Toasts | Required: the severity icon, with its tone | Object kind (severity) | `inline`; info, success, warning, error | Only label for the level: the level's name is the icon's label | `Level::icon()` in `egui-pop-api/src/toasts.rs`, hard-coded; close is `Icon::Close` | Hard-coded; severities should be kinds a kit or an application can restyle |
| Notifications inside the window (toast actions) | Not used on action buttons: text, the verb matters | — | — | — | Text | None |
| Jobs centre: job rows | Optional: a job kind's icon (import, export); the state is text and a ring | Object kind (job kind), severity for a failure | `inline`; running ring, failed | Decorative: the state is text | Text and a ring; the log toggle is `Icon::ChevronDown`/`ChevronRight` hard-coded (`egui-pop-api/src/jobs_centre.rs`) | No job kind icon; the log toggle should be the disclosure role |
| Jobs status item and panel | Required in the status item (a ring), optional on the panel | Object kind (the Jobs panel) | `inline` | The item's text names it | A ring painted in `jobs.rs`; the panel declares `.icon("history")` | None |
| Tips | Optional: the info icon marks a tip card | Object kind (severity: info) | `inline` | Decorative: the heading names it | `Icon::Info` hard-coded in `egui-pop-api/src/tips.rs`; `Tip` has no icon | A plugin's tip cannot have its own |
| Micro-indicators (icon kind) | Required: the icon is the mark | Either: an on/off pair is a command's, a state mark is a kind's | `inline`; on/off, tone | Only label: the mark's meaning | `Indicator::icon`, `Indicator::toggle(on, when_on, when_off)` (`egui-pop-core/src/indicator.rs`), each icon by value or by name | No `Indicator::command(id)` taking a command's icon pair yet ([design/indicators.md](design/indicators.md)) |
| Field state marks | Required: a state is never said by colour alone | Glyph role | `inline`; disabled, invalid, pending, modified, read-only | Decorative: the state joins the field's name | `field::icon(Mark)` in `egui-pop-core/src/field.rs` | Hard-coded mapping; could be glyph roles |
| Inputs: leading and trailing icons | Optional: a search field's magnifier, a path field's folder | Object kind (what the field holds) | `inline` | Decorative | `Input::leading_icon`, `trailing_icon` (`egui-pop-core/src/input.rs`), by value or by name, reported to agents (the `icons` collector's `drawn`); search fields add `Icon::Search` | Per instance; no kind (`file/folder`) yet |
| Inputs: attached buttons | Optional: an icon button when the action is standard (clear, browse) | Command | `inline` | Only label: `Attached::icon(icon, label)` | `Attached::icon` in `input.rs`, by value or by name | Per instance |
| Dialog titles | Not used: the title and the buttons say it; a danger dialog is marked by its buttons | — | — | — | None | None |
| Dialog buttons | Not used: the verb matters | — | — | — | Text (`egui-pop-api/src/dialogs.rs`) | None |
| Dialog close | Required | Glyph role (`close`) | Band button | Only label: "Close" | Kit glyph on a `band-icon-button` (`dialogs.rs`) | None |

### 2.7 Platform

| Surface | Rule | Family | Size and states | Accessibility | Today | Gap |
|---|---|---|---|---|---|---|
| OS notifications | Optional: the application's icon by default; a themed icon name on Linux | Application icon | The system's | The system's | `Notification::icon: Option<String>`, a themed icon's name or a file path (`egui-pop-platform/src/notify.rs`) | A system name space, not the registry's; keep it apart |
| Tray icon | Required: it is the icon | Application icon | The system's | Tooltip text | `tray::Icon` RGBA pixels (`egui-pop-platform/src/tray.rs`), a disc by default | None |
| Tray menu entries | Optional, following the platform: the system draws that menu | Command | The system's | The system's | `Entry` has no icon; entries become `muda` items without icons | No icon; `menus.icons` should apply when the system allows |
| Desktop file actions and jump lists | Not used: the desktop draws them, most without icons | — | — | — | Packaging ([packaging.md](packaging.md)) | None |

### 2.8 Helios

Helios is read at release 0.24.0. It declares about a hundred commands,
fourteen panels, three places and three modes; **none declares an icon**,
since commands cannot and the panel and frame fields are left empty.

| Surface | Rule | Family | Size and states | Accessibility | Today | Gap |
|---|---|---|---|---|---|---|
| Commands (open, export, undo and redo, the masks tools, culling marks, view modes) | Optional in menus and the palette, required in toolbars and caption items | Command | As the surface | As the surface | None: `CommandSpec` has no icon | Declare once the registry exists |
| Panels (Image, Graph, Vault, Modules, Masks, Navigator, Parameters, Scopes, Filmstrip, Queue, Log, Compare, Stacks, Duplicates) | Required in the activity bar and strips | Object kind (panel) | `panel` | Only label: the title | `helios_panels()` in `helios-gui/src/lib.rs` sets no `PanelSpec::icon`: strips show letters | Fourteen icons to choose |
| Places and modes (Library, Darkroom, Network; Culling, Mask editing, Compare) | Optional on crumbs and chips | Object kind (place, mode) | `inline`; the mode's tone | Decorative | `helios-template/src/contexts.rs` sets no `FrameSpec::icon` | Six icons to choose |
| Modules list and pipeline nodes | Optional on rows and node titles, required if module groups become icon-only tabs | Object kind (node type) | `inline` | Decorative, or only label for icon tabs | None; the graph's kinds carry no icon | Waits for kind icons in the graph |
| Masks tree: element kinds (shape, parametric, luminance, raster, AI object, group, refinement) | Required: rows mix kinds | Object kind (mask element) | `inline` | Decorative | `Item::icon()` in `helios-gui/src/mask_tree.rs`: Unicode glyphs and emoji (`○`, `◎`, `✏`, `🖼`, `✨`...), drawn with `Source::glyph` | Emoji depend on the emoji font and ignore the theme; move to kind icons |
| Masks tree: visibility and lock | Required: row actions | Command (toggle, with an "on" icon) | `inline`; on, off | Only label: the hint text | `Icon::Visible`/`Hidden`, `Icon::Lock`/`Unlock` hard-coded in `mask_tree.rs` | Should be the commands' icon pairs |
| Library marks (rating, flags, colour labels), stacks, compare, duplicates | Required for marks; optional for the panels | Glyph role (marks), object kind (panels) | As in 2.4 | As in 2.4 | egui-pop's library views (2.4); Helios gives data | Follows egui-pop |
| Templates and workspaces | Optional: an icon per workspace in pickers and the recent list | Object kind (workspace) | `inline` | Decorative | `meta.icon: Option<String>` in `helios-template/src/workspace.rs`, offered to agents as "a short generic icon name"; stored, edited, never drawn | Not checked against any name list; not shown |
| Quick open (modules, images, workspaces) | Required: one list of three kinds | Object kind | `inline` | Decorative | `helios-gui/src/quick_open.rs` on the choice dialog: text only | Waits for choice rows with icons |
| Status items (images, zoom, history, culled) | Not used: short texts | — | — | — | `helios-gui/src/status_items.rs`: text | None |
| Caption items (jobs, decisions, profile) | Required: icon-only in the title bar | Command | `toolbar`; a count badge, a ring | Only label: the action's text | Ring painted; the decisions bell is `Icon::Notification` hard-coded in `helios-gui/src/caption.rs` | Should come from the command it runs |
| Jobs (import, export, batch, calibration) | Optional: a job kind's icon in the jobs centre | Object kind (job kind) | `inline` | Decorative | Text | Waits for job kinds in egui-pop |
| Node and AI connection indicators | Required when built: icon-only status items with a state | Object kind (a connection), with an LED | `inline`; off, on, busy, error | Only label: the connection's name and state | Not built (a connected node's jobs are relayed, with no indicator) | Waits for status items with icons |
| Tray and desktop menus | Optional, following the platform | Command | The system's | The system's | Not used by Helios | None now |
| Viewport tools and overlays | Required for a tool palette; not used on overlays | Command | `toolbar` | Only label | The picker is a slider's quad button (`Icon::Picker`, `helios-gui/src/params.rs`); mask handles are overlays | No tool palette yet |
| Parameters sections | Required | Glyph role (`disclosure-*`) | `inline` | Decorative | `Icon::ChevronDown`/`ChevronRight` hard-coded in `params.rs` | Should be the disclosure role |

### 2.9 Count and main gaps

Counting each row once (glyph-role rows included, as they are surfaces
too; a conditional rule counts by its first word):

| | Required | Optional | Not used | Rows |
|---|---|---|---|---|
| egui-pop (2.1 to 2.7) | 27 | 27 | 9 | 63 |
| Helios (2.8) | 9 | 6 | 1 | 16 |

The gaps that matter most:

1. **Commands have no icon.** `CommandSpec` declares none, so menus,
   the palette, toolbars, the shortcut editor and status items cannot
   show one, and every command control takes its icon from its caller.
   *Declared since step 2 (`CommandSpec::icon`, `icon_on`); the surfaces
   read them from step 3.*
2. **Icon names are not checked.** `PanelSpec::icon`, `FrameSpec.icon`
   and a workspace's icon are free strings; an unknown name becomes a
   letter or nothing, with no diagnostic (against
   [ADR 0003](adr/0003-no-silent-no-op.md)). *Closed by step 2 for
   egui-pop's declarations (the `unknown-icon` diagnostic, the `icons`
   collector); a workspace's icon is Helios's.*
3. **Icons are chosen inside widgets**: severities in toasts, the
   editors' gutter and the status bar's counts; chevrons in the jobs
   centre, the library grid, the graph and Helios's parameters (which
   should be glyph roles); the palettes' pin; the library's painted
   marks, which a kit cannot remap.
4. **Agents see only part of it.** Panel and frame icons and the
   indicators' icon names are reported; `ListItem::icon` is skipped,
   tree and input icons are closures or fields nobody describes.
   *Closed by step 2: list entries, commands, tree rows and inputs are
   reported by name.*
5. **Port types are told by colour alone** in the node graph.
6. **The breadcrumb ignores `FrameSpec.icon`**, which says it serves
   the crumbs. *Closed by step 2.*
7. **The vocabulary is small.** About 57 built-in names cover
   egui-pop's own needs; an application's commands (Export, Crop,
   Import) need its own SVGs or a larger set (section 4.5).
8. **Helios declares nothing**: no panel, place, mode or command icon,
   and its mask kinds are emoji.

## 3. Platform conventions for menu icons

What each platform says, with its source:

- **Windows.** The Win32 guidelines say: "Consider providing menu item
  icons for the most commonly used menu items, menu items whose icon is
  standard and well known, menu items whose icon well illustrates what
  the command does", and "If you use icons, don't feel obligated to
  provide them for all menu items"
  ([Menus, Icons](https://learn.microsoft.com/en-us/windows/win32/uxguide/cmd-menus)).
  The system menu's visual style has a gutter of its own: the theme
  parts `MENU_POPUPGUTTER`, `MENU_POPUPCHECK` (`MC_CHECKMARKNORMAL`,
  `MC_BULLETNORMAL`) and `MENU_POPUPCHECKBACKGROUND`, whose
  `MCB_BITMAP` state is the box drawn behind a checked item's bitmap
  ([Parts and States, MENU](https://learn.microsoft.com/en-us/windows/win32/controls/parts-and-states)).
  WinUI gives the same advice for `MenuFlyoutItem.Icon`, at 16 by 16
  pixels ([Menu flyout and menu bar, Icons](https://learn.microsoft.com/en-us/windows/apps/develop/ui/controls/menus)).
  Office (Word) follows this layout: icons in the left gutter, ticks
  in the same column, a checked item with an icon shown as the icon in
  a highlighted frame. That last point is observed in Word, not
  written in a guideline; `MCB_BITMAP` is the system's part for it.
- **macOS 26 (Tahoe).** The Human Interface Guidelines gained a section
  on menu item icons on 2025-07-28, updated on 2026-06-08: "Use menu item
  icons sparingly and with purpose. Icons allow people to find menu
  items more quickly, and help clarify what selecting an item does",
  and "provide icons for all menu items in a group, or none of them"
  ([Menus, Icons](https://developer.apple.com/design/human-interface-guidelines/menus)).
  The same page asks to "represent common actions consistently" with
  the system's standard icons.
- **GNOME and GTK 4.** The GNOME guidelines for menus mention check
  boxes and radio buttons and no item icons
  ([Menus](https://developer.gnome.org/hig/patterns/controls/menus.html)).
  GTK 4 accepts an `icon` attribute on a menu item, "names of icons
  that may be displayed"
  ([GtkPopoverMenu](https://docs.gtk.org/gtk4/class.PopoverMenu.html)),
  but an item that has a label does not show it: in
  `gtk/gtkmodelbutton.c`, `update_visibility()` shows the image only
  when `has_icon && (self->iconic || !has_text)`, and only the sections
  drawn as rows of buttons are iconic (`gtk/gtkmenusectionbox.c`)
  ([source at 49eeaf8a](https://gitlab.gnome.org/GNOME/gtk/-/blob/49eeaf8a8c5988051796564f741edc96ae66f81d/gtk/gtkmodelbutton.c)).
  So, by default, no icons in GNOME menus.
- **KDE Plasma.** Icons are shown by default and a user setting turns
  them off: Plasma's Qt platform theme reads `ShowIconsInMenuItems`
  from the `[KDE]` group, default `true`, and sets Qt's
  `AA_DontShowIconsInMenus` from it (`updateShowIconsInMenuItems` in
  `qt6/src/platformtheme/khintssettings.cpp` of plasma-integration,
  [source at 46eddde9](https://invent.kde.org/plasma/plasma-integration/-/blob/46eddde9d38cf907786e567dd4a942140bb1f797/qt6/src/platformtheme/khintssettings.cpp)).

All four agree on two things: an icon is never the only label of a
menu item, and icons are for the common, well-known actions, not for
every item.

### The `menus.icons` setting (decided, 2026-10-22)

A setting under Appearance, `menus.icons`, with three values:

- **`on`** (default): the Word-like gutter (section 5) in every menu,
  with the icons the commands declare.
- **`off`**: no icons in menus. Ticks and radio dots keep their column,
  reserved only in a menu that holds a check or radio item.
- **`platform`**: `on` on Windows, macOS and KDE Plasma; `off` on
  GNOME (read from `XDG_CURRENT_DESKTOP`); on Plasma, follow
  `ShowIconsInMenuItems` when it can be read.

The default is `on`, Word's look, as the owner asked (an established
application's habit) rather than the platform; `off` and `platform` are
the other choices, `platform` being there for GNOME users. The setting changes menus only: the palette,
toolbars and other surfaces keep their own rule. Tray menus follow it
when the system lets an application draw icons there.

## 4. The icon registry (decided, 2026-10-22)

One place where commands and object kinds declare their icon **once**;
every surface reads it, no widget picks an icon for a command or a
kind, and agents read the names.

### 4.1 Names, not values

An icon is referred to by **name** wherever it is declared: the model
crate has no egui (ADR 0001), and `PanelSpec::icon` already works this
way. A name is egui-pop's (`settings`, `history`, the kebab-case
`Icon::name`), a Phosphor glyph's (section 4.5) or one a plugin
registered (`plugin-id:name`, section 4.5). Every name is checked
against that vocabulary; resolution happens in `egui-pop-core`,
through `Icon::by_name`.

An unknown name is reported (a diagnostic naming the declarer, the
surface and the name, ADR 0003) when the plugin connects and again
when the theme is installed, and drawn as a neutral placeholder (the
title's first letter, as the activity bar does today), never silently
dropped.

In code: `egui_pop_core::icon::registry` holds the vocabulary
(`lookup`, which keeps each answer; `resolve`, which also reports;
`unknown`; `search`), `IconRef` lets a widget take an `Icon` or a name,
and `Source::named` gives a tree row's icon by name. egui-pop-api checks
every command's, panel's and frame's icon when it resolves the
declarations (the `unknown-icon` diagnostic) and its `icons` collector
shows the whole of it to agents.

### 4.2 Command icons

`CommandSpec` gains two optional fields:

```text
CommandSpec::new(id, "Show the grid", "View")
    .icon("grid")          // the action
    .icon_on("grid-off")   // optional: shown while `checked` holds
```

Every surface that runs a command reads them: menu items, the palette,
quick open entries that run commands, `CommandButton`, toolbars,
caption items, status items with a command, the shortcut editor, and
indicators made from a command (`Indicator::toggle` from `icon` and
`icon_on`). A caller may still pass an icon for one control (a
different size of the same idea), but the default is the command's.

### 4.3 Object-kind icons

A kind is a namespaced id (`<domain>/<kind>`) mapped to an icon name,
declared by the plugin that owns the domain (ADR 0004) and listed in
the registry:

| Domain | Examples | Declared by |
|---|---|---|
| `panel/<id>` | `panel/egui_pop.jobs` | `PanelSpec::icon`, unchanged |
| `frame/<id>` | `frame/darkroom` | `FrameSpec::icon`, unchanged |
| `severity/<level>` | `severity/info`, `severity/warning`, `severity/error`, `severity/success` | egui-pop |
| `file/<type>` | `file/folder`, `file/image`, `file/raw` | egui-pop (folder, file), the application (its types) |
| `place/<id>` | `place/home`, `place/recent`, `place/bookmark` | egui-pop's file chooser |
| `job/<kind>` | `job/import`, `job/export` | the application |
| `node/<kind>` | `node/blur` | the application, on its graph kinds |
| `port/<type>` | `port/image`, `port/mask` | the application, as a shape beside the colour |
| `settings/<section>` | `settings/appearance` | the plugin owning the section |

### 4.4 The existing fields read from it

| Field | Becomes |
|---|---|
| `ListItem::icon: Option<Icon>` | `ListItem::icon(name)` or `ListItem::kind(kind)`; empty: the command's icon; serialised for agents |
| `FrameSpec.icon` | Unchanged as a declaration; the chips and the crumbs both read it |
| `PanelSpec::icon` | Unchanged as a declaration; the activity bar, the strips and (when wanted) tabs read it, with the diagnostic of 4.1 |
| `Input::leading_icon`, `Attached::icon` | Also accept a name or a kind (`file/folder` for a path field) |
| `TreeView::icon` closure | Keeps returning a `Source`, with a new `Source::kind(kind)` so a tree names kinds instead of icons |
| `Indicator::icon`, `Indicator::toggle` | Accept names; `Indicator::command(id)` takes the command's icon pair |
| Hard-coded icons in widgets | Severities become `severity/*` kinds; chevrons used as disclosures become the `disclosure-*` glyph roles; the library's painted marks become glyph roles |

### 4.5 Plugins and the vocabulary

A plugin registers its SVGs by name at connect time and declares its
kinds; both are owned by the plugin and removed with it. A plugin's
own names are prefixed by its id, `plugin-id:name` (`helios:mask-circle`),
so two plugins never clash; egui-pop's names stay bare.

The built-in vocabulary is about 57 names. With the `phosphor` feature
on (the default), every Phosphor glyph is in the font already, and they
are exposed by name, under the same prefixed form (`phosphor:export`),
which gives applications a large set with no SVG to ship. Without the
feature, a Phosphor name does not resolve and is reported as unknown.

### 4.6 What `describe` reports

- The `commands` collector gives each command's `icon` and `icon_on`
  names, and whether each resolves.
- A new `icons` collector lists the vocabulary (egui-pop's names, each
  registered name with its owner and its paint, single-colour or
  full-colour), the kinds with their icon and owner, and the
  unresolved names with who declared them.
- Menus, the palette, list entries, tree rows, status items and
  indicators report the icon name each entry shows, so an agent reads
  what is drawn without pixels.

## 5. The menu gutter (decided, 2026-10-22)

The rows of a menu already reserve an `inline` square on their left
(section 2.2). The gutter makes it a column with rules.

- **Width.** A new datum of the `menu-item` component, `gutter`,
  defaulting to `{size.icon.inline}` plus `{space.s}` on each side: 24
  points in the comfortable density, 16 in the compact one, since the
  density overlays move both tokens. Every row of a menu, with or
  without an icon, starts its label after the gutter, so labels align.
  With `menus.icons` off and no check or radio item, the gutter is not
  reserved.
- **Without an icon.** A checked item draws the `check` glyph role in
  the gutter; the chosen item of a radio group draws the `radio` glyph
  role (a radio group is new: an item kind whose `checked` clause picks
  one of a group). A mixed state draws nothing, as Windows advises.
- **With an icon, "on".** The icon stays and is framed, as Word does:
  a box the size of the gutter behind the icon, in a new `checked`
  state of `menu-item`'s gutter (fill `{surface.active}`, a 1-point
  border in `{accent.default}`), so the state does not rest on the fill
  alone. A command with an `icon_on` shows that icon instead, framed
  the same way.
- **Busy.** A running command shows the spinner in the gutter, as
  today, over whatever it holds.
- **Columns on the right.** The shortcut keeps its own column, right
  aligned, in the `muted` tone; the submenu arrow (the `submenu` glyph
  role) takes the last column. A menu's width is the widest label, plus
  the widest shortcut, plus the arrow, plus the gutter.
- **Disabled.** The icon and the tick take the item's disabled colour,
  as the label; the reason stays in the tooltip.
- **High contrast.** Icons are drawn in the item's text colour, never
  in a tone; full-colour application icons are drawn as they are only
  when the theme is not a high-contrast one, else as their single-colour
  mask. The "on" frame keeps its border.
- **Accessibility.** The gutter is decorative: AccessKit reads the
  title, and a check or radio item is announced as checked or not.
- **Tests.** Labels of rows with and without icons start at the same x;
  a ticked row and a framed row; both densities; high contrast.

## 6. Delivery order

1. **This inventory**, reviewed by the owner (decisions in section 7,
   2026-10-22).
2. **The registry, model side**: `CommandSpec::icon` and `icon_on`,
   kind declarations, names checked with diagnostics, the `icons`
   collector and the names in `describe`; `ListItem` by name and
   serialised. *Done, but for the kind declarations (`<domain>/<kind>`,
   section 4.3), left to step 6 with the surfaces that need them; the
   breadcrumb's icons (from step 6) came with it.*
3. **Menus**: the gutter of section 5, radio items, the `menus.icons`
   setting (default `on`); the gallery's menus page with icons, ticks,
   radios, framed and disabled entries; snapshots and alignment tests; the extension
   template declares an icon on its command.
4. **Command surfaces**: the palette's icon (when the command declares
   one), `CommandButton`
   and caption items from the command, the shortcut editor; choice rows
   with icons (for quick open).
5. **Icons out of widgets**: severities as kinds (toasts, the editors'
   gutter, the status bar's counts, tips), disclosure chevrons as glyph
   roles (jobs centre, library, graph), the palette's pin, the
   library's marks as glyph roles.
6. **Object kinds where the table says required**: the file chooser's
   places and entries, the breadcrumb's icons (hidden in the compact
   density), panel tabs' icons behind a setting (off by default), the
   graph's node kinds and port shapes, job kinds, settings sections.
7. **Toolbars**: a toolbar component in egui-pop (overflow, icon or
   text mode).
8. **Helios**: icons for its panels, places and modes, commands (open,
   export, undo and redo, the masks tools, culling marks, view modes),
   mask kinds instead of emoji, job kinds, a workspace's icon checked
   and shown, the caption's bell from its command.

## 7. Decisions (2026-10-22)

The owner accepted the plan on 2026-10-22 and settled the open
questions:

1. **`menus.icons` defaults to `on`** (the Word-like gutter); `off` and
   `platform` are the other choices.
2. **A checked command with an icon shows the icon framed**, as Windows
   and Word do. Without an icon, the tick goes in the gutter.
3. **Phosphor glyphs are exposed by name**, and every name is checked
   against the vocabulary.
4. **A plugin's icon names are prefixed by its id**: `plugin-id:name`.
5. **The breadcrumb shows an icon on every place that declares one**,
   hidden in the compact density.
6. **Icons in panel tabs sit behind a setting, off by default.** Folded
   strips (and the activity bar) always show icons.
7. **The command palette shows an icon when the command declares one.**
8. **egui-pop gets a toolbar component.**
