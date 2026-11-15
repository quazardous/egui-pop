# Overlays, placement, layers and header bands

The contract of the family "Overlays, placement and layers"
([ADR 0027](adr/0027-component-families.md)): what floats above the
interface, where it goes, in which order, and the handle a movable
surface is dragged by. Four modules of `egui-pop-core`:

| Module | What it does |
|---|---|
| `egui_pop_core::overlay` | The overlay stack: modals, popovers and menus, their dismissal, the focus they give back |
| `egui_pop_core::placement` | Pure geometry: a floating element next to its anchor, inside bounds |
| `egui_pop_core::layers` | Layer ranks: a fixed order between kinds of foreground layers (ADR 0026) |
| `egui_pop_core::band` | A header band: the handle of a movable dialog or palette |

**Owns**: the overlay stack and its rules, the placement of a floating
element next to its anchor, the ranks between kinds of foreground
layers, the header bands of movable surfaces.

**Refuses**: a z-index beyond the ranks; system windows drawn as
overlays (a separate window is an application window, ADR 0025).

## The overlay stack

`OverlayStack` is the model, pure and tested by its invariants;
`egui_pop_core::overlay::show` is the egui side. An overlay is declared
by an `OverlaySpec` (an id, a label, a `Kind`: `Modal`, `Popover` or
`Menu`) and shown while a `bool` says it is open:

```rust
use egui_pop_core::overlay::{self, Anchor, Kind, OverlaySpec};
use egui_pop_core::placement::Side;

fn ui(ui: &mut egui::Ui, open: &mut bool) {
    let button = ui.button("Options");
    if button.clicked() {
        *open = true;
    }
    let spec = OverlaySpec::new("options", "Options", Kind::Popover);
    let anchor = Anchor { rect: button.rect, side: Side::Bottom };
    overlay::show(ui.ctx(), &spec, open, Some(anchor), |ui| {
        ui.checkbox(&mut true, "Snap to grid");
    });
}
```

The rules (each one a test of `OverlayStack`):

- **Escape** dismisses the top overlay only, if it allows it
  (`OverlaySpec::keep_on_escape` refuses).
- **A click outside** dismisses the dismissable overlays above the one
  clicked, or all of them, and stops at a modal: nothing below a modal
  gets the click. A press on the overlay's anchor, or on an egui popup
  opened from inside it (a combo box's list, a context menu), is inside.
- **Closing an overlay closes every overlay opened above it**; each
  `Closed` says why (`Reason::Escape`, `Outside`, `Closed`, `Parent`).
- **The focus goes back** to the widget focused when the overlay opened,
  once that widget can hold it again, unless something else took the
  focus meanwhile.
- **Keys held** when a modal comes up (the Enter that opened it) are not
  read by it, nor by what is behind it when it closes.
- **Tab** cycles inside a popover on top of the stack; a menu's keys are
  its own; an overlay its owner draws itself (`egui_pop_core::overlay::hold`,
  a flyout) lets the focus leave.
- **Each window has its own stack**; a modal open in one window blocks
  the others of its group (`egui_pop_core::overlay::block_window`), and
  `egui_pop_core::overlay::modal_open` says whether one is open.
- **Resizable modals** (`OverlaySpec::resizable`) are dragged by their
  edges and a corner grip, which is also a keyboard splitter named
  "Resize".

Reads for tests and agents: `egui_pop_core::overlay::stack`, `rects`,
`recently_closed`, `modal_size`.

## Placement

`egui_pop_core::placement::place` puts an element of a given size next
to an anchor, inside bounds: on the preferred `Side` if it fits, else
flipped, else on the side with more room; then shifted along the side to
stay inside, and shrunk only when larger than the bounds. `Placed` says
where it went and whether it flipped. It is pure geometry, tested by
properties; every popover, menu and tooltip uses it.

```rust
use egui::{Rect, pos2, vec2};
use egui_pop_core::placement::{Placement, Side, place};

let bounds = Rect::from_min_size(pos2(0.0, 0.0), vec2(400.0, 300.0));
let anchor = Rect::from_min_size(pos2(20.0, 270.0), vec2(80.0, 24.0));
let p = place(&Placement::new(anchor, vec2(160.0, 120.0), bounds).side(Side::Bottom));
assert!(p.flipped && bounds.contains_rect(p.rect));
```

## Layer ranks

egui has no z-index: in one `Order`, the layer clicked last is on top.
Ranks fix the order between kinds of layers, bottom to top: the docked
panels; `Rank::Over` (zones lying over their neighbour, flyouts);
`Rank::Overlay` (egui-pop's and the application's overlays: cards,
popovers, toasts); every foreground layer nobody ranked (menus, combo
lists, dialogs); tooltips. Inside a rank, the last clicked is on top.

An application overlay calls `egui_pop_core::layers::area` where it
would show an `egui::Area`; a layer drawn otherwise calls
`egui_pop_core::layers::rank` each pass it is shown. The application
installs the ranks (`egui_pop_core::layers::install`); a context without
them draws as plain egui does.

```rust
use egui_pop_core::layers::{self, Rank};

fn card(ctx: &egui::Context) {
    layers::area(
        ctx,
        egui::Area::new(egui::Id::new("my_app.card")).anchor(egui::Align2::LEFT_BOTTOM, [8.0, -8.0]),
        Rank::Overlay,
        |ui| ui.label("Did you know?"),
    );
}
```

## Header bands

`egui_pop_core::band::begin` lays out the band's row (a title, its
buttons) over a drag area registered first, so the buttons stay
clickable; `egui_pop_core::band::paint` has the kit's `header-band`
paint it across the frame's width, `hover` under the pointer, `active`
while dragged. The `Band` says what the pointer did (`dragging`,
`delta`, `double_clicked`, `hovered`); moving the surface is the
owner's. The band is not focusable: a surface moves by keyboard through
a command of its own (`dialogs.move`, Alt+F7, for dialogs).

## States and failure

- An overlay closed by a rule (Escape, a click outside, a parent
  closing) sets the caller's `bool` to false; `recently_closed` keeps
  why.
- A focus target gone when the overlay closes takes nothing (no panic,
  the focus stays where it is).
- `place` never returns a rectangle outside the bounds; an element
  larger than the bounds is shrunk to them.
- A layer opened from something above its rank (a hover card from a
  dialog, `egui_pop_core::layers::rank_from`) is left unranked, so it
  stays above its opener instead of sinking under it.

## Accessibility

A modal's first focus is inside it and the focus returns on close; a
resizable modal's grip is a named splitter with its size as value; band
buttons (`egui_pop_core::band::button`) carry the name they are given.
Popovers keep Tab inside; Escape always has a way out unless the spec
says `keep_on_escape`.

## Tier

From [API stability](stability.md): overlays are **Stable** ("the theme
…, focus scopes and the focus chain, the key dispatcher, overlays");
placement and header bands are **Supported** ("Placement of a floating
element next to its anchor … the geometry every popover, menu and
tooltip uses since 0.1"; "Header bands … their behaviour is kept, their
drawing follows the kit"); layer ranks are **Experimental** ("the order
between kinds of foreground layers may still gain ranks").

## Tests

- The stack's invariants, placement properties, keys held and focus
  return: `crates/egui-pop-core/tests/core.rs` and the module's unit
  tests; focus coming back across scales:
  `crates/egui-pop-test/tests/focus_place.rs`.
- Overlays per window and blocking: `crates/egui-pop-api/tests/windows.rs`,
  `window_scope.rs`, `app_windows.rs`; flyouts held by their owner:
  `flyouts.rs`.
- Ranks: `crates/egui-pop-core/tests/layers.rs` (the order between
  ranks and the unranked layers whatever was raised last, a rank kept
  one pass, sublayers of a base noted at its rank, `rank_from`, a layer
  outside the foreground left alone);
  `crates/egui-pop-api/tests/layer_ranks.rs` (an application area and
  an overlay stay above a panel lying over, a popup opened from it too).
- Bands: through the movable dialogs in
  `crates/egui-pop-api/tests/dialogs.rs`; the band has no test of its
  own yet.
