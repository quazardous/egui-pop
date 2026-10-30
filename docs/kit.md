# Visual kits

Status: design **not decided**, first component built (2026-10-18).
Discussion and plan: the "Visual kits" issue on egui-pop's tracker.

A kit is what egui-pop's components look like: data from the theme and
drawing handlers in code. A theme today gives colours, sizes and fonts; a
kit also gives **shapes**: underlined or boxed tabs, a title at the start
of the title bar, a field drawn as a line.

## Principles

These are the maintainer's (2026-10-18).

1. **No overhead.** A kit is set when the application starts, or in a
   dedicated step when the user picks or customises one, never in the
   normal life of the application. Drawing never resolves anything.
2. **One homogeneous vocabulary**, for maintenance: every component is
   described with the same words, and every drawing handler has the same
   shape.
3. **The default kit is a kit.** egui-pop's own look goes through the same
   path as any other kit. Nothing is drawn outside a kit.
4. **A kit is easy to add to an application.**
5. **A kit is not only data**: it can provide drawing handlers in code.

## Behaviour and appearance

| | egui-pop (the widget) | the kit |
|---|---|---|
| Interaction, focus, keyboard, accessibility | ✓ | |
| State (`rest`, `hover`, `active`, `focus`, `selected`, `disabled`) | computes it | receives it |
| Layout | asks the kit for sizes | measures |
| Drawing | | paints |
| A hit area that is not a rectangle | uses it | provides it (optional) |

This is Qt's QStyle split. egui-pop exposes **primitives** (one per part
it draws through a kit); the kit fills them.

## Vocabulary

A token is always `component.state.term`, or `component.term` for what
does not change with the state.

| Term | Per state | Meaning |
|---|---|---|
| `fill` | ✓ | the box's fill |
| `border-color`, `border-width` | ✓ | its border (widths snap to whole physical pixels) |
| `radius` | ✓ | its corner radius (the handler decides which corners) |
| `text` | ✓ | its label's colour |
| `indicator-color`, `indicator-size` | ✓ | a bar on one side of the box (none at size 0) |
| `shape` | | the name of the handler that draws the component |
| `based-on` | | the component this one is based on: it receives every token of it it does not give ([below](#components-based-on-another)) |
| `padding-x` | | between the label and the sides |
| `height` | | the component's height, when it has a fixed one |
| `indicator-side` | | `top`, `bottom`, `left` or `right` |
| `ring-color`, `ring-width`, `ring-offset` | ✓ | the focus ring, usually given by `focus` only: an outline `ring-offset` points outside the box (inside when negative), following its corners; none at width 0 |
| `padding-y` | | between the content and the part's top and bottom edges |
| `opacity` | | how much the part hides what is behind it, 0 to 1 (a backdrop; the `box` handler's fill) |
| `buttons-side` | | on `title-bar`: `end` or `start`, where the window buttons go, close outermost |
| `buttons-hover` | | on `title-bar`: `own` or `group`, whether the three buttons are hovered together |
| `corners` | | on `window`: `round`, `small` or `square`, the corners asked of the system (Windows 11 only) |
| `offset` | | how far the part sits from the edge it is drawn on, towards the outside (a side area's grip) |
| `align` | | where the label goes: `start`, `centre` (on the part's centre, moved into its room when it does not fit) or `end` |
| `width` | | the component's width, when it has a fixed one |
| `font-size` | | the size of its text, in points (an alias of a `font.*` size); the widget's own size when not given |
| `margin` | | on `scroll-bar`: between the bar and its scroll area's edge |
| `placement` | | on `scroll-bar`: `over` the content (floating, the content runs under it) or `beside` it (taking room) |
| `visibility` | | on `scroll-bar`: `always` (at rest too, at full width), at `rest` too but thin and quiet, on `hover` of the scroll area, or while it `scroll`s |
| `width-rest`, `opacity-rest`, `room`, `min-length` | | on `scroll-bar`: the bar at rest under `rest`, the room kept for it, the thumb at its shortest |
| `track-fill` | | on `scroll-bar`: the track behind the thumb (transparent: none) |
| `shadow-offset-x`, `shadow-offset-y`, `shadow-blur`, `shadow-spread`, `shadow-color` | | the shadow the part casts (egui's `Shadow`; none when not given): usually aliases of a `shadow.*` group; on `side-area`, cast while it lies over the centre, `shadow-offset-x` towards the centre |

States are always the same six. `rest` gives every term (the contract
requires it); each other state gives only what differs. A part in several
states (a selected tab under the pointer) takes `rest`, then `selected`,
`hover`, `focus`, `active`, `disabled`, in that order, a later one
winning.

## Components based on another

A component that is a kind of another names it: `based-on`, a string.

```yaml
command-button:
  based-on: {$type: "string", $value: "button"}
  focus:
    border-color: {$type: "color", $value: "{focus.ring}"}
```

When the theme is resolved (at install, never while drawing), the
component receives every token of its base it does not give itself, as
an alias of the base's (`command-button.hover.fill` is
`"{button.hover.fill}"`): every state, every tone, the component terms
and the `shape`. Its own tokens then apply over them, as `focus` above.
A kit restyling the base restyles every component based on it, through
any term, unless that component gives the term itself; so does an
override of the base.

- **One base, chains allowed**: `find-bar-button` may be based on
  `icon-button`, itself based on another. A base is completed before
  what is based on it, whatever the order of the files.
- **A cycle is a problem** (`Error::BasedOnCycle`, reported at install
  and by `kit::check`): each component of the circle keeps its own tokens
  only. So is a base no token names (`Error::UnknownBase`).
- **The handler follows**: the `shape` is inherited like any term, and
  a handler registered for the base under that name draws the component
  too, unless one is registered for the component itself. A component
  giving its own `shape` keeps it.
- **Tones follow**: the base's tones are inherited; a tone the component
  gives replaces the base's term by term.
- **Only `based-on` crosses the component tier.** Any other component
  token aliases a system token (a test checks the default kit): an alias
  of one term of another component would cascade one term only.
- The contract still lists every token a based component must end with;
  the theme giving the base gives them.

## Handlers

Every component's handler has one shape (`egui_pop_core::kit::Primitive`):

```rust,ignore
trait Primitive<D>: Send + Sync {
    fn measure(&self, data: &D, label: Vec2) -> Vec2;
    fn paint(&self, cx: &KitCx<'_>, data: &D, part: &Part<'_>);
    fn hit(&self, data: &D, rect: Rect, pos: Pos2) -> bool { rect.contains(pos) }
}
```

`Part` is the rectangle, the room the label may take (less than the
rectangle when the widget puts things on it: a title bar's items), the
state and the label (laid out without a colour; the handler gives the
state's `text`). A part is painted box first (`paint`), then the widget's
own content, then the label (`paint_label`, which by default places it
with `align`, `kit::paint_label`). `KitCx` is the painter and
the pixels per point. A shape a theme names is a handler registered under
that name: egui-pop ships some, an application adds its own with
`kit::register_tab_shape` before installing a theme that names them.

## Tones

Some components show a **tone**, which is not an interaction state:
`accent`, `info`, `success`, `warning`, `danger`, `muted`, `record`
(`kit::Tone`, the same names everywhere; `Neutral` is the component's
own box). It is egui-pop's one tone: progress, status bar items
(`strips::Tone`), library badges (`library::Tone`) and micro-indicators
take it. `record` (recording) aliases `danger` by default, so that a kit
can tell a recording from an error; the default kit gives it wherever a
component gives `danger` (status indicators, spinners, progress rings,
pills, micro-indicators). A
token is `component.tone.<tone>.<term>` (every state) or
`component.tone.<tone>.<state>.<term>`, giving only what differs. The
defaults alias the status tokens. A box is merged as `rest`, the tone,
then each state followed by the tone's own patch for that state: a state
wins over a tone for what it gives, unless the tone gives that state
too. A tone a component does not give draws its own box. At install, a
component whose tokens give tones gets a table of 7 × 32 boxes; others
get none. The tone travels in `State` (`State::toned`), so handlers and
`Part` are unchanged; `Component::style_toned(tone, state)` reads one
box. Colours-only components (rings, spinners, status indicators) keep
their geometry in code and read their colours from the kit.

## Frames

`Component::frame(state, ppp)` gives a state's box as an egui `Frame`
(fill, border snapped to whole physical pixels, radius,
`padding-x`/`padding-y` as inner margin), and
`Component::show(ui, state, shadow, add)` lays content out in it, then
paints the state's ring. With the shipped `card` handler egui draws the
box (no extra cost); another handler paints once the content is laid
out, its shapes moved under the content (one allocation per part and
frame). Shadows are egui's (`Visuals`), whose `window_shadow` and
`popup_shadow` come from the `shadow.window` and `shadow.popup` tokens
(egui's own values by default): the dialog, popover, palette and assist
frames, and egui's menus, take them from there.

## Bridges to egui

Where egui still draws a widget, the kit reaches it through one of these
adapters. Each is the one bridge for what it covers, and what it cannot
carry is reported in `Kit::problems` (logged at install) rather than
dropped in silence. Replacing one means an egui-pop-drawn widget.

- **`theme::visuals_with`.** The kit becomes egui's `Visuals` at
  install: `button`'s rest, hover, active and selected are egui's
  inactive, hovered, active and open widget states, for every egui
  widget; `field`'s rest fill is text fields' background. Lost, and
  reported: an indicator or a ring on `button`, a `field` outline or
  text colour differing from the button's (egui shares one set of
  states).
- **egui's own checkbox, radio, selectable label and combo box.** An
  application calling egui's directly still gets them through `Visuals`
  only (they follow `button`); egui-pop's [own
  widgets](#eguis-small-widgets) draw them through the kit. A combo
  box's box stays egui's drawing: it follows `button`, or `combo-box`
  in a scope ([below](#eguis-widgets-in-a-scope)); its arrow is the
  kit's.
- **A scope (`Kit::scope`, `UiPop::scoped`).** For a few of egui's
  widgets, one kit component's states become egui's visuals inside a
  `ui.scope`, compiled with the kit: a combo box's box, a button shown
  on or off, a button in a tone, a text field in a tone
  ([below](#eguis-widgets-in-a-scope)).
- **`menu-item` through `Component::paint_under`.** egui lays the item
  out, takes its clicks and draws its label with a transparent frame; the
  kit paints the box into a slot reserved under it. `command-button`,
  `command-palette-row` and `empty-state` are painted the same way.
- **`scroll-bar` through egui's `ScrollStyle`.** Its `width`,
  `width-rest`, `opacity-rest`, `room`, `min-length`, `margin`,
  `placement`, `visibility` and whether it has a track are
  written into egui's style with the theme, so every scroll area
  follows them, an application's own included; its thumb's colours
  and corners, which egui takes from the widget states, are the kit's
  in a `ScrollBar` scope ([below](#scroll-bars)).
- **`side-area`.** egui draws the separator line, with its
  non-interactive stroke: `border-width` 0 hides it; another width than
  the button's, or a colour other than `border.default`, is reported.

Two parts of the token tree are outside the grid on purpose:

- **`window`** is read by `WindowKit`, not by `components!`: the window
  is not a part egui-pop paints but a request to the system (its
  `corners`), its resize edges (`window.grip`) and the line around it
  (`window.rest.border-*`).
- **The component tokens outside kits** (`viewport.*`, `histogram.*`,
  `curve.*`, `wheel.*`, `navigator.*`, `cursor.*`, `loupe.*`,
  `graph.*`, `editor.*`, `syntax.*`, `terminal.*`, and `palette.*`: the
  magnet, the group gap, the resize limits and grip) are colours and
  sizes of content drawn in code,
  not boxes in states: they keep their flat names.

## Kits as packages

A kit ships as a folder: `kit.yaml` (a DTCG token file with a manifest
under `$extensions."org.egui-pop.kit"`, YAML or JSON), more `*.tokens.yaml`, faces
under `fonts/` and a `LICENSE`. It is read at run time
(`KitData::load`) or embedded (`kit!`), checked by `kit::check` (in an
application's tests) or the `kit-check` example, and added with one
line, `AppBuilder::kit`. A kit with code is a crate exposing
`pub fn kit() -> KitSource`, its handlers registered at install. The
user's kits are found in `<persist>/kits`. egui-pop's default kit is
the folder `themes/kit/`, read by the same code. Details in
[Kits as packages](kit-packages.md). A term added to the vocabulary is
also added to `egui_pop_tokens::kit::COMPONENT_TERMS` or `STATE_TERMS`,
which the checks read.

## Adding a component

Each step touches its own file, plus one line in a list kept in
alphabetical order, so changes adding different components can be made
in parallel (the lanes of the umbrella issue):

1. **Its tokens** in `crates/egui-pop-tokens/themes/kit/<component>.tokens.yaml`
   (aliases of system tokens, `rest` complete, other states only what
   differs). `build.rs` finds it and compiles it in: no list to edit.
2. **Its contract** in `crates/egui-pop-tokens/src/contract/kit/<component>.rs`
   (`shape`, the component terms and every `rest` term), plus one line in
   `GROUPS` (`contract/kit/mod.rs`).
3. **The component** in the kit: one line in `components!`
   (`crates/egui-pop-core/src/kit/mod.rs`): its field, token name and
   default handler.
4. **Its handlers**, if `plain` is not enough: a module under
   `kit/shapes/` and one line each in `shapes::built_in`.
5. **The widget**: computes its state with `State::from_response` (plus
   what only it knows, `SELECTED`), paints with `Kit::of(ctx).<component>`
   (`paint`, its own content, `paint_label`), and no longer reads colours
   from `Tokens` for that part.
6. **Its snapshots**: only the gallery snapshots of that component change.

## Compiled once

`theme::install_tokens` compiles the kit (`kit::install`): it reads the
component tokens, merges every combination of states ahead (32 boxes per
component) and picks the handlers. Drawing reads `Kit::of(ctx)`'s fields
and calls one handler per part: no lookup by name, no inheritance walk,
no fallback. What could not be compiled (an unknown shape, a bad side) is
in `Kit::problems`, logged, and drawn with the default.

## Cost

Measured with `crates/egui-pop-api/tests/bench_tabs.rs` (72 tabs in three
areas, the interface built and tessellated without a GPU, the pointer
sweeping the window; release build, versions run in alternation, five
runs each, median of medians), 2026-10-18:

| Version | Median frame | Instructions (callgrind) |
|---|---|---|
| Before kits (egui_dock's own tabs) | 360 us | 5.29 G |
| Kit, first version | 562 us | 5.96 G |
| Kit with prepared tabs | 355 us | 4.97 G |

The first version did per frame what only changes with the theme, the
scale, the language or a panel's title: titles translated and laid out
three times per tab, egui_dock's style rebuilt, and the box tab's outline
built as a path with arcs (an allocation and trigonometry per tab). Now
each area keeps its tabs prepared (title, label laid out, width measured)
and its egui_dock style, keyed on the kit, the pixels per point and the
strings provider; a tab is prepared again only when its declared title
changes. Handlers draw with egui's rectangles, not paths.

Rule for handlers: nothing allocated or computed per frame that depends
only on the kit. What a handler needs ahead goes in the component's data,
compiled at install.

## Built so far

- **Tabs of the workbench** (`tab`, `tab-bar`, `tab-preview`). egui_dock keeps the
  layout, docking, drag and drop and the tabs' input; its own tab painting
  is made invisible, each tab is as wide as the kit measures it
  (`TabViewer::tab_style_override`), and the kit paints the bars and tabs
  over what egui_dock laid out. Shapes shipped: `box` (joined to its
  panel, the default, close to egui_dock's own look), `underline`, and
  `plain` for the bar. The gallery's `workbench-kit-underline` snapshot
  is a theme that switches to `underline` with data only. A tab dragged
  over another area leaves a ghost where it would land, the
  `tab-preview` component at rest, [based on](#components-based-on-another)
  `tab` (its handler, sizes and radius, so a kit restyling its tabs
  restyles their ghost), its fill the selected tab's (`surface.overlay`)
  at 60 % alpha, its border `accent.default`. The
  vocabulary has no dashed border: a kit wanting one names a handler of
  its own.
- **The title bar and window buttons** (`title-bar`, `window-button`,
  `window-close`). The title goes where `title-bar.align` says: `centre`
  (the default) or `start`, after the leading items (the gallery's
  `frame-kit-title-start` snapshot). The buttons' glyphs are the
  `window-minimize`, `window-maximize`, `window-restore` and
  `window-close` glyph roles (egui-pop's painted marks by default, any
  icon of the set otherwise), in the box's `text`, `indicator-size`
  across (10 by default). In full screen, the bar keeps close and puts
  a `window-button` leaving full screen where maximize was, its glyph
  the `window-exit-fullscreen` role (`exit-fullscreen` by default).
  Buttons and items are centred above the bar's line
  (`title-bar.rest.border-width`).
- **Data views** (`list-row`, `list-view`, `table-header-cell`,
  `table-stripe`, `tree-drop-marker`). One function paints the rows of
  the tree, the list and the table. A row is `selected`, `hover` (never
  while selected, so the selection stays under the pointer) and `focus`
  for the cursor, whose ring sits inside the row (`ring-offset` -2). The
  view's outline is `list-view`'s focus ring, painted over the rows, so
  its fill stays transparent. A header is `hover` when its column sorts
  and `selected` when the table is sorted by it. A refused drop is the
  marker's `disabled` state. Two handlers: `box` (a free-standing box)
  and `outline` (a border only; a part with no height is a line between
  rows). The kit is fetched once per view and pass. The gallery's
  `table-kit-rows` snapshot restyles rows and headers with data only.
- **Window buttons per platform flavour.** The buttons go where
  `title-bar.buttons-side` says; each carries a tone (minimize
  `warning`, maximize `success`, close `danger`) that a kit may give;
  `traffic-light` draws them as circles, and with `buttons-hover: group`
  their glyphs show together. `examples/kits/flavour-macos` is the macOS
  flavour, data only (`frame-kit-macos`). `window.corners` is asked of
  Windows 11 when the application draws the frame; elsewhere there is no
  portable request. `window.rest.border-width` and `border-color` draw a
  line around the window, inside its edges (the shell is laid out within
  it), except when maximized or full screen.
- **Resizable dialogs and splitters** (`dialog-grip`, `splitter`). A
  modal declared resizable takes the pointer `dialog-grip.width` on
  either side of its edges, and draws three lines in `text`
  (`border-width` wide) in a square `dialog-grip.height` across at its
  bottom right corner, the handle that takes the keyboard (`focus`). A
  splitter (`widgets::Splitter`) is a line `splitter.width` thick in the
  middle of the room it takes the pointer on, in `rest`, `hover`,
  `active` and `focus`.
- **Side areas** (`area-strip`, `area-grip`). A folded side area is a
  strip (`area-strip.width`) with the `drag-handle` glyph and a line on
  the centre's side; a click unfolds it (`workbench.fold`). An open side
  area's grip is painted at the middle of its edge (`width` thick,
  `height` long), above the panels; egui still takes the drag and draws
  the separator line (it reserves room for it in the panel's layout).
  The default kit's grip is transparent. An open side area
  (`side-area`) has `padding-x` and `padding-y` between its edges and its
  panels (egui's 8 and 2 by default), and no separator line when its
  `border-width` is 0 (egui draws the line: see Bridges to egui). A side area may lie over the centre
  (`workbench.place`): its strip keeps the room (`selected` while the
  area is open; a click folds it) and the area is drawn above the
  centre in `side-area`'s box, casting its `shadow-*` towards the centre
  (`shadow.side` by default). The gallery's `workbench-over-pop-dark`
  snapshot shows it.
- **egui's own widgets** (`button`, `field`). egui's style is built from
  the kit when a theme is installed (`theme::style_with`): `button`'s
  rest, hover, active and selected are egui's inactive, hovered, active
  and open widget states, `field`'s rest fill is text fields'
  background. egui shares one set of states for every widget, so a
  field's outline and text follow the button's; what egui cannot draw
  (an indicator or a ring on a button, a field outline of its own) is
  listed in the kit's problems.
  `button` has tones, for an egui button drawn in one
  (`UiPop::button_tone`, a `Button` scope ([egui's widgets in a
  scope](#eguis-widgets-in-a-scope)), so the button keeps its layout,
  icon and shortcut text):
  `accent` a primary action on the accent fill, `info`, `success`,
  `warning`, `danger` and `record` their status colour as fill with the
  panel's colour as text (legible both ways, like a badge), `muted` no
  box at rest. Components based on `button` inherit them.
- **Menu items** (`menu-item`). egui lays each item out, takes its clicks
  and draws its label, its own frame transparent; the kit paints the box
  under it (`Component::paint_under`: a slot reserved before egui draws,
  filled once the rectangle and state are known), in the state egui's
  button draws with, `selected` while a submenu is open; the shortcut is
  the `muted` tone. `menus-kit-bar` restyles items with data only.
  Its gutter (`menu-item-gutter`, [icons.md](icons.md), section 5) is
  the column on the left holding the command's icon, the tick, the
  radio dot or the spinner: its `width` (else the inline icon size plus
  `space.s` on each side, so the density moves it), and its `selected`
  state the frame around the icon of a checked command (`surface.active`
  with a 1 point `accent.default` border, so the state does not rest on
  the fill alone).
- **Frames** (`card`, `dialog`, `backdrop`, `popover`, `fault-card`, `toast`,
  `palette-window`, `tip-card`, `job-log`, `fullscreen-exit` (the
  strip at the top of a full screen window showing no title bar),
  `editor-frame`, `terminal-frame`, `find-bar`, `assist-card`), handler
  `card`; the `dialogs-kit-square` snapshot restyles a dialog with data
  only. Their borders are snapped to whole physical pixels like the
  kit's own painting (`Component::frame` takes the pixels per point).
  `card` is the application's own card (`UiPop::card`): the raised
  surface at rest, an edge in the status colour by tone, `muted` a
  sunken well; its padding is `space.group`.
- **The application's chrome** (`header-band`, `dialog-header`,
  `band-icon-button`, `find-bar-button`, `activity-bar-item`,
  `dock-preview`, `command-button`, `command-palette-row`,
  `toolbar-button`). The band at
  the top of a movable dialog and of a palette is drawn by the `band`
  handler (its top corners follow the frame's `radius` inside its
  outline, its border is the line below it), `hover` under the pointer
  and `active` while dragged; the dialog's title is `dialog-header`'s
  `text`, the room below it its `padding-y`. The band's buttons (a palette's
  close, roll and pin, `selected` while pinned; a dialog's close) and
  the find bar's are boxes with an icon in their `text`, the close
  marks the `close` glyph role, a focus ring as data. An activity bar
  item is `selected` while its panel is shown (never `hover` then). The
  dock preview is at rest where a panel may dock and `disabled` where
  it may not, its label saying why, its fill at `opacity` (the `box`
  handler fades its fill by the component's `opacity`). A command
  button and a palette row are laid out by egui, which takes their
  clicks; the kit paints their box under them (`paint_under`), in the
  state of the last pass as egui's own frame was; a palette row's
  matches are the `accent` tone, its keys the `muted` tone. The
  gallery's `palettes-kit-band` snapshot restyles the bands with data
  only. A command button is based on `button`, a palette row on
  `menu-item`, the dialog's title on `header-band` and the band's and
  find bar's buttons on `icon-button` ([based on
  another](#components-based-on-another)). A toolbar's button
  ([toolbars](toolbars.md)) is based on `icon-button` too: a fill under
  the pointer and while pressed, its icon and label in `text`, and
  `selected` while its command is on, framed (`surface.active` with a 1
  point `accent.default` border) as a checked menu icon is.
- **Progress and status by tone** (`progress-bar`, `progress-line`,
  `progress-fill`, `progress-caption`, `meter`, `meter-tick`,
  `progress-ring`, `spinner`, `status-indicator`), handlers `track`,
  `span` and `tick`.
- **Micro-indicators** (`indicator-dot`, `indicator-badge`,
  `indicator-flag`, `indicator-swatch`, `indicator-outline`,
  `indicator-button`; [indicators](indicators.md)). Dots and flags are
  colours only; the shapes that tell states apart are glyph roles, kept
  distinct (below). A badge is a `pill` by tone, a
  swatch a `box` over its colour, an interactive mark's target an
  `indicator-button` in its state; pips are `spinner` and
  `progress-ring`. The `indicators` snapshots.
- **Collapsible sections** (`section-band`, `section-disclosure`,
  `section-count`; [sections](sections.md)). The band is `hover`,
  `active` while pressed and `focus` (a ring inside it); the `section`
  handler draws its fill and its border as a line along its top and its
  bottom edges, so two stacked bands share one line. Its mark is the
  `disclosure-*` glyph role in `section-disclosure`'s `text`, `width`
  across; its count a `section-count`, based on `indicator-badge`. The
  `sections` snapshots.
- **Sliders** (`slider`, `slider-fill`, `slider-thumb`, `slider-value`,
  `slider-toggle`). The bar is a box with its label at the start
  (`align`), `hover`, `active` while dragged and `focus` (a ring inside
  it in the default kit); a range slider's bar is in the states of both
  thumbs, each thumb in its own. The fill is a strip along the bottom
  (`slider-fill.height`, else 15 % of the bar, handler `track`); a
  gradient track's colours are content and stay code. A thumb is a line
  (`tick`, `width` thick). The value is a part of its own at the end, in
  the `warning` tone past the soft range. The square button at the end
  is `selected` while its toggle is on, its icon in `text`. The
  `sliders-kit-flat` snapshot restyles them with data only.
- **Icon buttons** (`icon-button`). An `IconWidget` made clickable is a
  box (transparent in the default kit) with its icon in `text`: `hover`,
  `focus` (a ring one point outside), `disabled`, and `selected` for a
  toggle (`IconWidget::selected`). The icon's tone is the part's tone;
  `Strong` and `OnAccent`, which are not tones, keep their colour at
  rest. An icon that is not clickable is content and keeps its colour.
- **Placeholders** (`skeleton`, `skeleton-shimmer`, `empty-state`). Each
  shape of a skeleton is a `skeleton` part, and the band crossing them a
  `skeleton-shimmer` part where it meets a shape; their geometry (lines,
  blocks, gaps) stays code. An empty state is laid out by egui as before
  (`padding-y` above and below), its box painted under it once its size
  is known (`Component::paint_under`; transparent in the default kit);
  its icon and sentence are in `text`, the icon in the `danger` tone
  when something went wrong.
- **The photo library** (`thumb-cell`, `thumb-layer`,
  `thumb-overlay-badge`, `pill`, `tag-chip`, `mark-toggle`,
  `search-field`, `compare-pane`). A cell is `selected`, `hover` (never
  while selected) and `focus` for the cursor of a focused grid; its
  caption is its `text`. What it lays over its image is `thumb-layer` by
  tone: `muted` a rejected image's veil, `accent` the bar beside an
  expanded stack's member (its indicator, the cell's outline painted again
  over it); a failed image's icon is the layer's `text`. A pending image
  stays a skeleton (`skeleton.fill`). The flag, labels and stars sit on
  `thumb-overlay-badge`s. Badges and a stack's count are `pill`s, badges
  by tone (`info`, `success`, `warning`, `danger`); the `pill` handler
  caps the radius at half the height, so a large radius gives round ends.
  The neutral pill's fill is the overlay surface at 85 %, an alias with
  the alpha modifier (`docs/tokens.md`). Tag and filter chips are
  `tag-chip`s (`pill` too), the pick, reject and colour label buttons
  `mark-toggle`s (`selected` while on), the search field a `card` whose
  `focus` ring shows while its text has the keyboard. A comparison's pane
  is `compare-pane`, `selected` for the best image, its ring painted
  again over the image; the best star is that ring's colour. Stars, flags
  and colour labels are content and keep their `library.*` colours. The
  `library-kit` snapshot restyles them with data only.
- **The viewport's widgets** (`thumbnail`, `readout`, `viewport-focus`):
  a thumbnail is `selected`, `hover` and `focus` (its ring, clipped away
  before, now shows); the pixel readout is a `card` with its text as
  label; the viewport's focus outline is `viewport-focus`'s ring, inside
  its edges (`ring-offset` -3).
- **Glyphs by role** (`kit::Glyph`, `glyph.<role>` tokens): the small
  marks widgets draw, named by what they mean and mapped by the kit to
  an icon of its set, compiled into a table read by index. The vocabulary
  is fixed: disclosure open and closed, sort ascending and descending,
  check, radio, indeterminate, bullet, submenu, chevron start and end,
  dropdown (a combo box's arrow),
  close, more, expand, collapse, the window buttons', and the shapes of
  the micro-indicators' LEDs (`led-off`, `led-on`, `led-busy`,
  `led-warning`, `led-error`) and named flags (`flag-modified`,
  `flag-pinned`, `flag-required`, `flag-invalid`). Those must keep
  states apart: two roles of `egui_pop_tokens::kit::DISTINCT_GLYPHS`
  naming the same icon are a problem (`kit::check`, and at install, where
  both go back to their default shapes), so colour never carries a state
  alone. The marks they draw by default (`circle`, `disc`, `square`,
  `triangle`, `diamond`, `asterisk`, `corner`) are icons of the set. The tree, the menus and the find bar
  ask for roles, and so do the table's sort arrows.
- **egui's small widgets** (`checkbox`, `radio`, `combo-arrow`,
  `selectable-row`, `toggle-chip`), [below](#eguis-small-widgets).
- **egui's widgets in a scope** (`combo-box`, `toggle-button`, and
  `button` and `field` by tone), handler `scoped`,
  [below](#eguis-widgets-in-a-scope).
- **Scroll bars** (`scroll-bar`), handler `scoped`: egui's
  `ScrollStyle` for every scroll area, the thumb's colours in a scope,
  [below](#scroll-bars).
- **Application icons** (`icon_set`, feature `svg`): SVG files
  registered by name, single-colour (a white mask tinted at draw) or
  full-colour, rasterised at install for the theme's icon sizes and the
  window's scale, and at any other size the first time it is drawn (a
  scaled texture blurs); a kit's glyph may name them.
- The default kit is these components' tokens in `pop-dark`, which every
  built-in theme extends. A `transparent` system token says "paint
  nothing" (window buttons at rest).

## egui's small widgets

egui-pop draws egui's checkbox, radio button and selectable label
itself (`egui_pop_core::widgets`, `egui_pop::core::widgets` from the
facade), keeping egui's layout (its atoms, icon width, spacing,
padding and interaction height), its clicks and keyboard, and what it
tells assistive technology; the kit paints them. egui-pop's own code
uses them everywhere, and an application switches by replacing the
call:

| egui | egui-pop |
|---|---|
| `ui.checkbox(&mut on, text)` | `widgets::checkbox(ui, &mut on, text)`, or `ui.add(widgets::Checkbox::new(..))` |
| `ui.radio(on, text)`, `ui.radio_value(..)` | `widgets::radio(ui, on, text)`, `widgets::radio_value(ui, ..)` |
| `ui.selectable_label(on, text)`, `ui.selectable_value(..)` | `widgets::selectable_label(ui, on, text)`, `widgets::selectable_value(ui, ..)` |
| `egui::ComboBox::from_id_salt(id)`, `ComboBox::new(id, label)` | `widgets::combo_box(id)`, `widgets::combo_box_labelled(id, label)` |
| a selectable label used as an on/off option | `ui.add(widgets::ToggleChip::new(&mut on, "Aa").name("Match case"))` |

```rust
use egui_pop::core::widgets;

fn options(ui: &mut egui_pop::egui::Ui, wrap: &mut bool, mode: &mut usize) {
    widgets::checkbox(ui, wrap, "Wrap lines");
    widgets::combo_box_labelled("mode", "Mode")
        .selected_text(["Fast", "Exact"][*mode])
        .show_ui(ui, |ui| {
            for (i, name) in ["Fast", "Exact"].iter().enumerate() {
                widgets::selectable_value(ui, mode, i, *name);
            }
        });
}
```

| Component | Part | States | Based on |
|---|---|---|---|
| `checkbox` | the box; its mark the `check` glyph, or `indeterminate` | `selected` while checked; `focus` like `active` (egui's look) | `button` |
| `radio` | the circle (handler `pill`, radius 999); its mark the `radio` glyph | `selected` while chosen | `checkbox` |
| `combo-arrow` | the box under a combo box's arrow (transparent); the `dropdown` glyph in its `text` | `selected` while the list is open | |
| `selectable-row` | a row picked from a short list: a combo box's choices, the assist card's completions and actions, small tab strips | `selected` for the chosen one, never `hover` then | `list-row` |
| `toggle-chip` | a small on/off option showing a text or a glyph: the find bar's case, whole word, regular expression and replace toggles | `selected` while on | `selectable-row` |

- A checkbox's and a radio button's mark is in the box's `text`, and
  its label in the `text` of the same state without `selected` (a kit
  filling a checked box keeps the label readable); `width` is the box's side (egui's icon width when not
  given). A radio's circle grows by egui's `expansion` under the
  pointer, as egui's does, and so does a selectable row's box.
- A mark is the role's icon (`glyph.check`, `glyph.radio`,
  `glyph.indeterminate`, `glyph.dropdown`). The default icons are drawn
  with egui's own geometry (a one-point check line, a dash, a dot of a
  third of the inner box, a triangle as wide as egui's), so the default
  kit looks like egui; a kit naming another icon gets that icon, 80 %
  of the box wide (the arrow: the icon's square). `check` and
  `indeterminate` must differ (`DISTINCT_GLYPHS`).
- A toggle chip is a checkbox for assistive technology, named by
  `ToggleChip::name`; `ToggleChip::glyph` draws an icon on it.
- The combo box's box, and its list's frame, are egui's: they follow
  `button` and egui's popup through `Visuals`, and the box follows
  `combo-box` when shown with `UiPop::combo_box` (a scope,
  [below](#eguis-widgets-in-a-scope)). Drawing them would mean
  re-implementing egui's combo box. egui's own `ComboBox` (and the
  other egui widgets) keep egui's arrow.
- The gallery's `builtins` page shows them next to egui's own, and its
  `builtins-kit-toggles` snapshot restyles them with data only.

## egui's widgets in a scope

Some of egui's widgets are not worth drawing again: egui-pop would
re-implement their layout and input for a look that egui's `Visuals`
can already carry. For those, a kit component's states become egui's
visuals in a `ui.scope`, and egui keeps drawing the widget, with its
layout, icons and shortcut text. This is the `scoped` handler: the
component's boxes are mapped onto egui's fields when the kit is
compiled, per tone (`ScopedKit`, one `ScopedVisuals` per kind and
tone), and a scope only copies them (`Kit::scope`); nothing is resolved
while drawing. Outside the scope, egui's visuals are unchanged.

```rust
use egui_pop::core::ui::{Stock, Tone, UiPop};
use egui_pop::core::widgets;

fn view(ui: &mut egui_pop::egui::Ui, fit: &mut bool, mode: &mut usize, name: &mut String) {
    ui.scoped(Stock::ToggleButton, Tone::Neutral, |ui| {
        if ui.add(egui_pop::egui::Button::new("Fit").selected(*fit)).clicked() {
            *fit = !*fit;
        }
    });
    let combo = widgets::combo_box("mode").selected_text(["Fast", "Exact"][*mode]);
    ui.combo_box(combo, |ui| {
        widgets::selectable_value(ui, mode, 0, "Fast");
        widgets::selectable_value(ui, mode, 1, "Exact");
    });
    ui.scoped(Stock::Field, Tone::Danger, |ui| ui.text_edit_singleline(name));
}
```

| `Stock` | Component | egui's widgets | Mapping |
|---|---|---|---|
| `Button` | `button`, by tone | `Button`, and what egui draws as one (`DragValue`, a menu button, a `CollapsingHeader`'s header) | `rest`, `hover`, `active`, `selected` are egui's inactive, hovered, active and open states; `UiPop::button_tone` is this scope |
| `ComboBox` | `combo-box` (based on `button`) | `ComboBox`'s box (`UiPop::combo_box`) | the same; `selected` while its list is open |
| `ToggleButton` | `toggle-button` (based on `button`) | `Button::selected`, egui's `selectable_label` | the same, and `selected`'s `fill` and `text` are egui's selection colours while on |
| `ScrollBar` | `scroll-bar` | `ScrollArea`'s bars (`UiPop::scroll_area`) | the thumb's `rest`, `hover` and `active` fills are egui's inactive, hovered and active foreground colours, its `radius` their corners, `track-fill` egui's `extreme_bg_color`; the content keeps the style outside the scope ([below](#scroll-bars)) |
| `Field` | `field`, by tone | `TextEdit` | `rest`'s fill is the background, `rest` and `hover` the outline, `focus` the outline with the keyboard (egui's selection stroke); the tones (`accent` changed, `warning` required, `danger` refused, `info` being checked, `muted` read-only) colour the edge |

- Every egui widget already follows `button` (and text fields' background
  `field`) without a scope, through the style installed with the theme
  (`theme::visuals_with`, the same mapping as a `Button` scope). A
  scope is for a widget that should differ from the plain button: in a
  tone, on or off, a combo box.
- egui reads one set of states: `focus` is egui's `active` for buttons
  and combo boxes; a disabled scope draws every state in the
  component's `disabled` box when the kit gives one, which egui still
  fades (its `disabled_alpha`). egui's expansion under the pointer is
  kept.
- What egui cannot draw is in the kit's problems: an indicator or a ring
  on `combo-box` or `toggle-button` (beyond what they inherit from
  `button`, reported there), a `toggle-button.selected` outline or radius
  of its own (egui keeps the state's outline on a selected button), a
  ring or a fill on `field.hover` or `field.focus` other than its
  resting one, a `shape` other than `scoped` on `combo-box`,
  `toggle-button` or `scroll-bar`.
- egui-pop's own combo boxes (settings, forms, the file chooser's
  filter, the theme editor, parameters) are shown with
  `UiPop::combo_box`, and the navigator's zoom buttons (fit, 100 %)
  with a `ToggleButton` scope.
- Not given a component of their own: `Slider` (egui-pop's own slider
  is drawn by the kit), `CollapsingHeader`, `DragValue`, egui's menus. They follow `button` globally, or any
  scope they are put in (a `DragValue` in a `Field` scope does not
  become a field: egui draws it as a button).
- The gallery's `scoped` page shows them; its `scoped-kit-restyled`
  snapshot restyles them with data only.

### Scroll bars

egui draws every scroll area's bars. The kit's `scroll-bar` reaches
them two ways:

- **With the theme, in every scroll area.** `width`, `width-rest`,
  `opacity-rest`, `room`, `min-length`, `margin`, `placement` and
  `visibility`, and whether there is a track, become
  egui's `ScrollStyle` when the theme is installed
  (`ScrollBarKit::style`, `Kit::scroll`), and in a scoped theme. An
  application's own `egui::ScrollArea` follows them with nothing to
  change.
- **In a `ScrollBar` scope, the colours.** egui draws the thumb in the
  widget states' foreground colours and corners, which every button
  shares, and the track in `extreme_bg_color`, which text fields and
  canvases share. `UiPop::scroll_area` shows a scroll area in a scope
  where they are the thumb's `rest`, `hover` (under the pointer) and
  `active` (dragged) fills, its `radius` and the `track-fill`; the
  content is drawn in the style outside the scope.

| Token | Meaning | Default |
|---|---|---|
| `scroll-bar.width` | the bar at its widest, and its hit area | 10 |
| `scroll-bar.width-rest` | the bar at rest, under `rest` | 4 |
| `scroll-bar.opacity-rest` | the thumb's opacity at rest, 0 to 1, under `rest`: quiet, but 3:1 against the surfaces (a test holds every built-in theme to it, through egui's blending) | 0.45, 0.6 in `pop-light`, more where a theme's text is close to its surfaces |
| `scroll-bar.room` | the content's width kept for the bar while it shows, under `rest` | 6 |
| `scroll-bar.min-length` | the thumb at its shortest | 24 |
| `scroll-bar.margin` | between the bar and the scroll area's edge | 0 |
| `scroll-bar.placement` | `over`: floats over the content, which runs to the edge under it; `beside`: takes room, the content is narrower by the bar, its margin and egui's 4 point gap | `over` |
| `scroll-bar.visibility` | `always`: at rest too, at full width; `rest`: at rest too, thin and quiet with no track, whenever the content overflows, wide over the bar; `hover`: while the pointer is over the scroll area, thin until it is over the bar (egui's own); `scroll`: while scrolling | `rest` |
| `scroll-bar.track-fill` | the track; transparent for none, in every scroll area | `{surface.sunken}` |
| `scroll-bar.rest.fill`, `hover.fill`, `active.fill` | the thumb at rest, under the pointer, dragged | the button's text colours |
| `scroll-bar.rest.radius` | the thumb's and track's corners; half the width or more rounds them fully | the button's |

The defaults are the survey's quiet bar with the button's colours, and the
high contrast themes take `always`, `beside` the content and 14 wide, which
covers nothing. A theme decides with data only: `visibility: hover` and a
`min-length` of 12 are egui's `ScrollStyle::floating()`; tvty's bar (a 6 point thumb, fully rounded, 4 points from the
edge, no track, over the content, shown at rest):

```yaml
$extends: "pop-dark"
scroll-bar:
  width: {$type: "dimension", $value: {value: 6, unit: "px"}}
  margin: {$type: "dimension", $value: {value: 4, unit: "px"}}
  placement: {$type: "string", $value: "over"}
  visibility: {$type: "string", $value: "always"}
  track-fill: {$type: "color", $value: "{transparent}"}
  rest:
    fill: {$type: "color", $value: "#4b4b4b"}
    radius: {$type: "dimension", $value: {value: 999, unit: "px"}}
  hover:
    fill: {$type: "color", $value: "#5a5a5a"}
  active:
    fill: {$type: "color", $value: "#6a6a6a"}
```

```rust
use egui_pop::core::ui::UiPop;

fn tickets(ui: &mut egui_pop::egui::Ui, rows: &[String]) {
    let area = egui_pop::egui::ScrollArea::vertical().auto_shrink(false);
    ui.scroll_area(area, |ui| {
        for row in rows {
            ui.label(row);
        }
    });
}
```

`rest` is the survey's quiet bar ([Scrolling survey](scrolling-survey.md)).
All of it is `ScrollStyle`: `floating_width`, `dormant_handle_opacity`,
`floating_allocated_width` and `handle_min_length` take its tokens, the track
opacities leave only the one over the bar. What `ScrollStyle` cannot say:

- The hit area is the whole `width`, always: 4 points of the content beside
  a 6 point room lose their clicks to the bar, even with the bar thin. A view
  with something clickable at its right edge keeps `width` as its room (the
  gallery's pages do).
- The opacity is one number: egui has none per surface, so the ratio of
  3:1 holds on the surface it was chosen for, and differs on others.
- A bar `beside` the content is opaque and at full width, so `rest` needs
  `over`.
- A bar forced on by a scroll area (`ScrollBarVisibility`) and a bar shown
  because the content overflows look the same; a view that wants a bar a
  `hover` theme hides takes `ScrollBarKit::seen_at_rest`, as the sections'
  stack does.

What egui cannot draw is in the kit's problems: a thumb whose colours
or radius differ from the button's (egui's scroll areas outside a
`ScrollBar` scope draw the button's), a track in another colour than
`surface.sunken` outside a scope, `visibility: scroll` (egui 0.36 has no
while-scrolling state: drawn as `hover`), a bar `beside` the content
not `always` visible (egui shows such a bar whenever the content
overflows, and opaque under `rest`), a `width-rest` wider than `width`, and
an `opacity-rest` outside 0 to 1. The gallery's `scoped` page shows one, restyled in its
`scoped-kit-restyled` snapshot.

Known limits of this first step:

- egui_dock adds 8 points either side of a label and never makes a tab
  narrower than that; a kit can make tabs wider, not narrower, and cannot
  overlap them (Chrome-like tabs) until egui-pop lays the tab bar out
  itself.
- Corner radius, border width and indicator are uniform; per-side borders
  and per-corner radii are a later step.

## Next

- One component per change: the title bar, fields, buttons, menus.
- Kits as folders (data, fonts, icons), `kit!` embedding and validating
  them at compile time, user kits found in the config directory, a
  `check` command, the gallery previewing a kit.
- egui's combo box's list frame (egui's popup) from a kit component;
  more of egui's widgets in a scope (`Slider`, `CollapsingHeader`)
  when a kit needs them apart from `button`.
