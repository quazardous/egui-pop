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
| `padding-x` | | between the label and the sides |
| `height` | | the component's height, when it has a fixed one |
| `indicator-side` | | `top`, `bottom`, `left` or `right` |
| `align` | | where the label goes: `start`, `centre` (on the part's centre, moved into its room when it does not fit) or `end` |
| `width` | | the component's width, when it has a fixed one |

States are always the same six. `rest` gives every term (the contract
requires it); each other state gives only what differs. A part in several
states (a selected tab under the pointer) takes `rest`, then `selected`,
`hover`, `focus`, `active`, `disabled`, in that order, a later one
winning.

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

- **Tabs of the workbench** (`tab`, `tab-bar`). egui_dock keeps the
  layout, docking, drag and drop and the tabs' input; its own tab painting
  is made invisible, each tab is as wide as the kit measures it
  (`TabViewer::tab_style_override`), and the kit paints the bars and tabs
  over what egui_dock laid out. Shapes shipped: `box` (joined to its
  panel, the default, close to egui_dock's own look), `underline`, and
  `plain` for the bar. The gallery's `workbench-kit-underline` snapshot
  is a theme that switches to `underline` with data only.
- **The title bar and window buttons** (`title-bar`, `window-button`,
  `window-close`). The title goes where `title-bar.align` says: `centre`
  (the default) or `start`, after the leading items (the gallery's
  `frame-kit-title-start` snapshot). The glyphs are still drawn by
  egui-pop, in the box's `text`.
- The default kit is these components' tokens in `pop-dark`, which every
  built-in theme extends. The former `titlebar.*` tokens were renamed into
  the grid (`titlebar.background` is `title-bar.rest.fill`,
  `titlebar.button-hover` is `window-button.hover.fill`...): a theme still
  giving an old name works, with a reported problem
  (`egui_pop_tokens::RENAMED`). A `transparent` system token says "paint
  nothing" (window buttons at rest).

Known limits of this first step:

- egui_dock adds 8 points either side of a label and never makes a tab
  narrower than that; a kit can make tabs wider, not narrower, and cannot
  overlap them (Chrome-like tabs) until egui-pop lays the tab bar out
  itself.
- Corner radius, border width and indicator are uniform; per-side borders
  and per-corner radii are a later step.

## Next

- `docs/kit.md` and the vocabulary settled; today's component tokens
  renamed to the grid (`button.background-hover` becomes
  `button.hover.fill`), with aliases for one version.
- One component per change: the title bar, fields, buttons, menus.
- Kits as folders (data, fonts, icons), `kit!` embedding and validating
  them at compile time, user kits found in the config directory, a
  `check` command, the gallery previewing a kit.
- egui's own widgets: the kit translated to `Visuals` at install, with
  what is lost reported.
