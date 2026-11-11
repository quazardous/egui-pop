# 0026: Layers have ranks, pinned under base layers at the bottom of the foreground

Date: 2026-11-11. Status: accepted (2026-11-11), implemented (issue #352).

## Context

egui has no z-index. Inside an `Order`, a layer goes to the top when it
is first shown, shown again, clicked or dragged (`Area` calls
`Areas::move_to_top`), and egui keeps the rest in the order they had.
egui_stack draws a zone lying over its neighbour and a flyout as their
own `Area`s in `Order::Foreground` (`layout/fold.rs`). egui-pop's
overlays (toasts, the tips card, hover cards, popovers, the zen way
out) and an application's own (tvty's "Did you know?" card) share that
order, so a click on a panel lying over the centre puts it over the
card: the order follows the clicks, not what each layer is (issue
#352).

What egui 0.37 offers, read in its source:

- `Order` (`Background`, `Middle`, `Foreground`, `Tooltip`, `Debug`):
  fixed, but only five, and windows, popups and the shell's areas each
  already sit in one.
- `move_to_top` adds a layer to a set; at the end of the pass the set
  goes to the top **in the order its layers already had**. Calling it
  in rank order every pass therefore does not sort them, and a layer
  shown for the first time still lands over the others for a pass.
- `Areas::order()` is not public; nothing moves a layer down.
- `set_sublayer(parent, child)` (`Context`, `Areas`): at the end of the
  pass every child is moved **directly above its parent**, after the
  `move_to_top` sort, whatever was clicked. Both must share an `Order`.
  It lasts one pass (called again each pass), and is one level deep:
  nested sublayers are unspecified. `Areas::child_layers` reads them.
  egui-pop already uses it for the canvas overlays (#337) and the
  indicator's overhang: a layer kept right above another.
- Layers are never dropped from egui's order, and each viewport has its
  own `Areas`.

## Decision

- **Ranks, bottom to top**, in each viewport:
  1. `Docked`: the shell's docked panels and the central panel
     (`Background`, egui panels); nothing to enforce.
  2. `Over`: zones lying over their neighbour and flyouts.
  3. `Overlay`: egui-pop's and the application's overlays: cards,
     popovers, toasts, the zen way out.
  4. `Popup`: menus, combo lists, context menus, dialogs' backdrops and
     any foreground layer nobody ranked.
  5. `Tooltip`: egui's `Order::Tooltip`; nothing to enforce.

  Floating windows (`Order::Middle`) stay between 1 and 2, as today.
- **Enforcement: one base layer per pinned rank, sublayers above it.**
  egui-pop owns two empty foreground layers, `base(Over)` and
  `base(Overlay)`. At the start of each pass `layers::begin_pass` calls
  `set_sublayer(base, placeholder)` for both, in rank order: on the
  first pass this puts them into egui's order before any other
  foreground layer, and since nothing ever raises them they stay at the
  bottom of the foreground, `Over` under `Overlay`. Every ranked layer
  is made a sublayer of its rank's base each pass, so egui moves it
  right above that base at the end of the pass, whatever was clicked:
  `base(Over)`, lying-over zones and flyouts, `base(Overlay)`,
  overlays, then every layer nobody ranked. Rank 4 needs no base: an
  unranked layer (egui's menus and popups, a dialog, an application
  area not yet ranked) is above both bases and their sublayers.
- **Two levels made one.** A ranked layer may itself be a parent (a
  `Scene` or a canvas overlay inside a lying-over panel). At the end of
  egui-pop's pass, `layers::end_pass` also makes each ranked layer's
  own children (`Areas::child_layers`) sublayers of the base, so they
  stay together whatever order egui walks the parents in.
- **egui_stack exposes a hook**, not ids: `LayoutArea::sublayer_of(parent)`
  makes the lying-over zones' and the flyout's layers sublayers of
  `parent` each pass. The shell passes `layers::base(ctx, Rank::Over)`.
  egui_stack keeps its area ids private; a host without ranks keeps
  today's behaviour.
- **The public API** (`egui_pop_core::layers`, re-exported by the
  facade):
  - `enum Rank { Over, Overlay }`, the ranks a layer can be put at (the
    others are egui's orders or the default);
  - `layers::rank(ctx, layer, rank)`, called each pass the layer is
    shown, for a layer drawn some other way (a `layer_painter`);
  - `layers::area(ctx, area, rank, add_contents)`, which sets the
    area's order to `Foreground`, shows it and ranks its layer: what an
    application calls instead of `ctx.move_to_top`;
  - `layers::base(ctx, rank)`, for a host or a component drawing its
    own lying-over surface.
  egui-pop's own overlays (toasts, tips card and halo, hover cards,
  popovers, zen way out, quick look) take `Rank::Overlay`.
- **Several windows.** egui keeps one `Areas` per viewport, so each
  window (an application window of ADR 0025, a detached panel) has its
  own bases and its own order; a layer is ranked in the pass of the
  viewport it is drawn in, which is its window group's. Between OS
  windows, the system decides.
- **Popups opened from a lying-over panel** (a combo list, a context
  menu, a popover asked to be a popup) are unranked foreground layers,
  hence above the panel and above the overlays. A popover egui-pop
  opens from a lying-over panel is an overlay, above the panel too.
- **Escape hatch.** An application that wants a layer above the
  overlays leaves it unranked; one that wants it under the lying-over
  zones makes it a sublayer of `base(Over)` itself.

## Consequences

- The order inside the foreground no longer depends on clicks between
  ranks; inside a rank, egui's last-clicked rule still applies (two
  panels lying over, two toasts).
- A foreground layer shown before egui-pop's first pass would stay under
  the bases. egui-pop starts every pass of its viewports, so only a
  layer drawn outside egui-pop could.
- Dialogs (egui-pop's modal backdrop is a foreground area) stay above
  the overlays; a toast under a dialog's backdrop is what a modal
  wants.
- egui-pop needs an egui_stack release with `LayoutArea::sublayer_of`
  before the shell can use it.
- Should egui gain a z-index or a public `order()`, `layers` keeps its
  API and changes its enforcement.

## Slices

1. egui_stack: `LayoutArea::sublayer_of` (quazardous/egui_stack#74), then a
   release.
2. egui-pop core: `layers` (ranks, bases, `rank`, `area`,
   `begin_pass`/`end_pass` from the application's pass); the shell
   pins its lying-over zones and flyout; a headless test with an
   application overlay at `Foreground` and a lying-over panel clicked.
3. egui-pop's overlays take `Rank::Overlay`; the facade re-exports
   `layers`; the guide shows the application call.

## Implementation notes

- `layers::install`, called by the application on each pass before it
  shows anything, adds an egui plugin running `begin_pass` and
  `end_pass` in every pass of every window, and starts the current one.
- `layers::rank_from(ctx, layer, rank, opener)` ranks a layer only when
  the layer it opens from is at that rank or below it: a hover card or
  the file browser's quick look opened from a dialog or a menu stays
  unranked, above it. A popover is ranked unless a modal or a menu is
  open under it in the overlay stack; a menu (`Kind::Menu`) is never
  ranked.
- `layers::rank_of` tells the rank a layer had this pass or the
  previous one, the lying-over zones egui_stack pinned included.
