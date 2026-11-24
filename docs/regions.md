# F6 regions

Part of the family "Focus and keyboard"
([ADR 0027](adr/0027-component-families.md)): F6 moves the keyboard
between the parts of a window (an area, a palette, the status bar), as
in most desktop applications, without changing any Tab order.

| Item | What it does |
|---|---|
| `egui_pop_core::focus::region` | Draws a part of the window as a region: a focus scope with a rank |
| `egui_pop_core::focus::prefer` | Names the widget a region lands on when nothing was focused there before |
| `egui_pop_core::regions::next` | Says where F6 (or Shift+F6, `back`) goes: a `Hop` |

**Owns**: the order of the regions, where a hop lands, what blocks it.

**Refuses**: binding the key (whoever binds F6 moves the focus; the
application binds the commands `workbench.next_region` and
`workbench.previous_region`); Tab order inside a region (that is
`egui_pop_core::tab`, see [Inputs, field states and Tab order](inputs.md)).

## The API

```rust
use egui_pop_core::{focus, regions};

fn window(ui: &mut egui::Ui) {
    focus::region(ui, &focus::Region::new("list", 0), |ui| {
        let _ = ui.button("Row");
    });
    focus::region(ui, &focus::Region::new("status", 1), |ui| {
        let _ = ui.button("Item");
    });
    if ui.input(|i| i.key_pressed(egui::Key::F6))
        && let Some(regions::Hop { target: Some(id), .. }) = regions::next(ui.ctx(), false)
    {
        ui.memory_mut(|m| m.request_focus(id));
    }
}
```

An application gets this without code: the workbench draws its areas,
palettes and status bar as regions and binds F6 and Shift+F6.

## The rules

- **The order** is the regions' ranks, and it wraps. From nothing it
  starts at the first (the last going back); after Escape or a press,
  nothing holds the focus but the focus chain still says where the user
  is, so it goes on from that region.
- **Where it lands**: the widget focused there last, if it can still
  take the focus; else the one the region names with `focus::prefer`;
  else the first focusable widget of a scope inside the region, the
  content before what stands around it (a dock's tabs, a palette's
  band); else nowhere, for a region made with `Region::holding`, whose
  content then holds the focus itself (a canvas); else any focusable
  widget of the region in view. A region with nothing to land on is
  passed over.
- **Panel Tab scopes** inside a region take that region's turn, each a
  stop of the round; F6 goes on out of them.
- **Nothing moves behind a modal**, overlay or scope: `next` answers
  `None`.
- **A focused widget taking every key** (a terminal) gets F6 before any
  binding does.
- **Each window has its own regions**, as it has its own focus.

## States and failure

`next` reads the last pass's widget rectangles (egui lists no focusable
widgets, but records each widget's sense in drawing order), so it can
answer before anything is drawn in this pass. With no region, or none
with anything to land on, it answers `None` and the focus stays.

## Accessibility

F6 is the keyboard's way between the window's parts; a region is a
focus scope, so screen readers follow the focus as usual. Every region
must have something to land on or say it holds the focus, else it is
unreachable by F6 (the keyboard walk test catches it in the gallery).

## Tier

**Supported**, decided in [API stability](stability.md) on 2026-11-15:
F6 regions shipped in 0.8, 0.9 and 0.11; their rules are kept, the
commands that bind them follow the workbench's tier (experimental).

## Tests

- `crates/egui-pop-api/tests/it/frame.rs` and `zen.rs` (F6 through the
  frame and in zen mode), `crates/egui-pop-test/tests/scale.rs` (at
  several scales).
- `crates/egui-pop-gallery/tests/it/keyboard_walk.rs` and `gallery.rs`
  (every page reachable by keys).
