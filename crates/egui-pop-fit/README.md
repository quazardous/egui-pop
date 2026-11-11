# egui_fit

**[Live demo](https://quazardous.github.io/egui_fit/)**: real egui-pop
widgets laid out in a box you resize, with the engine's choices drawn over
them.

Formerly named `egui_gridflow`: the repository and the crate were renamed
`egui_fit`, and the old GitHub URL redirects here.

A slot layout engine over plain boxes. It packs ordered elements into a
container in standard slot sizes and says where each one goes. It knows
nothing of widgets, values or drawing.

**It has no egui dependency.** The `egui_` prefix only marks it as part of
the egui-pop family of crates: the engine works on plain boxes, in twelfths
of a unit and in points, and any toolkit can use its results.

- **Base unit:** one row, scaled by density, given in points by the caller.
- **Allowed sizes:** ¼, ⅓, ½, ⅔, ¾ of the unit and whole multiples. The
  engine counts in twelfths (12 is the least common multiple of 2, 3 and 4)
  with integer arithmetic only, so every sum is exact.
- **Elements:** each declares one or more forms (form 0 is the full form,
  the others smaller ones, known only by index), each with a size policy
  (`min_size`, `size`, `max_size`, `aspect_ratio`, growth axis); its
  supported orientations, preferred first; an optional pinned orientation;
  an optional pinned position.
- **Packing:** a grid as wide as the box, in twelfths, with rows going
  down. Pinned elements go first; the others follow in their order, each at
  the first free position in reading order, so a later element may move up
  into an earlier gap (like CSS `grid-auto-flow: dense`). Placement is
  deterministic.
- **Groups:** a run of consecutive elements can be a group, like the
  controls of a `fieldset`, with an optional header (its legend) placed
  first, on a band of its own. Packing stays dense within a group, but
  order is strict across groups: a group starts on a new band of rows
  below everything before it, and nothing after it moves up into its gaps.
  Groups nest to any depth; an empty group is a barrier. Pins count from
  the top of their band. Each group's band comes back with the result, for
  the host to draw a frame and legend around.
- **Choice of form and orientation:** the pinned orientation if any, else
  the preferred one when the grid allows it, else another supported one,
  else the next smaller form. When even the last form does not fit, the
  element is reported as an overflow with a reason code. Nothing panics.
- **Growth:** growable elements then take the leftover room one slot step
  at a time, in turns, up to their maximum and never at in-between sizes.
- **Hysteresis:** pass the previous result to keep the column count, forms
  and orientations while the box stays within half a unit of a threshold,
  so a resize does not flicker.
- **Order:** each placement keeps its logical order (the input order, for
  keyboard focus, accessibility or device banks) apart from its visual
  order.

## Example

```rust
use egui_fit::{layout, Axis, Container, Element, Size, Size2, SizePolicy};

let half = Size2::new(Size::HALF, Size::ONE);
let row = Size2::new(Size::units(2), Size::ONE);
let elements = [
    Element::new(1, SizePolicy::fixed(half)),
    Element::new(2, SizePolicy::fixed(row)),
    Element::new(3, SizePolicy::growable(half, half, row, Axis::Width)),
];
// Two units wide: 48 points with a 24-point unit, growing down.
let result = layout(&Container::new(48.0, 24.0), &elements, None);
for p in &result.placements {
    println!("{} {:?} {:?}", p.key, p.points, p.status);
}
print!("{}", result.dump());
```

The dump draws each element with a letter for its logical index:

```text
grid 24x24 twelfths, one char = 6x12
acc.
bbbb
a key=1 form=0 horizontal at 0,0 size 6x12
b key=2 form=0 horizontal at 0,12 size 24x12
c key=3 form=0 horizontal at 6,0 size 12x12
```

`b` does not fit beside `a`, so `c` moves up into the gap, then grows in
slot steps (½, ⅔, ¾, 1). The next step, 2 units, does not fit, so half a
unit stays empty.

### Groups

```rust
use egui_fit::{layout_grouped, Container, Element, Group, Size, Size2, SizePolicy};

let slot = |key, w, h| Element::new(key, SizePolicy::fixed(Size2::new(w, h)));
let one = Size::ONE;
let elements = [
    slot(1, one, one),
    // A case: its legend, then three controls.
    slot(10, Size::units(2), Size::HALF),
    slot(2, one, one),
    slot(3, Size::units(2), one),
    slot(4, one, one),
    // A loose control after the case.
    slot(5, one, one),
];
let case = Group::new(10, 1..5).with_header();
let result = layout_grouped(&Container::new(48.0, 24.0), &elements, &[case], None);
let band = result.group(10).unwrap().points.unwrap();
assert_eq!((band.y, band.h), (24.0, 60.0));
print!("{}", result.dump());
```

```text
grid 24x54 twelfths, one char = 12x6
  a.
  a.
/ bb
| ce
| ce
| dd
\ dd
  f.
  f.
group key=10 at 0,12 size 24x30 body at row 18
a key=1 form=0 horizontal at 0,0 size 12x12
b key=10 form=0 horizontal at 0,12 size 24x6
c key=2 form=0 horizontal at 0,18 size 12x12
d key=3 form=0 horizontal at 0,30 size 24x12
e key=4 form=0 horizontal at 12,18 size 12x12
f key=5 form=0 horizontal at 0,42 size 12x12
```

Within the case, `e` moves up beside `c`; across it, `c` stays below the
legend `b` instead of moving up beside `a`, and `f` stays below the case.
The gutter draws each group's band.

`cargo run --example dump` lays out a small panel at several widths.

## Demo

The demo (`demo/`, not published) lays out real
[egui-pop](https://github.com/quazardous/egui-pop) widgets (sliders, toggles,
drop-downs, fields, a curve editor, colour wheels, a meter and a histogram)
in a box you resize by its right edge. It lets you edit each element's
forms, sizes, orientations and pins, add, remove and reorder elements,
group ticked elements into a case under a legend (drawn in its frame), and
shows the grid, each slot's status, logical and visual order, the
hysteresis trail and the `dump()`. Only the demo depends on egui; the
engine does not.

Run it in a browser with [Trunk](https://trunkrs.dev):

```sh
rustup target add wasm32-unknown-unknown
cargo install --locked trunk
cd demo
trunk serve    # then open http://127.0.0.1:8080
```

or on the desktop with `cargo run -p egui_fit_demo`.

## Features

- `serde`: `Serialize` and `Deserialize` for every public type.

## Status

Version 0, not published. The API may change.

## License

Licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE))
- MIT license ([LICENSE-MIT](LICENSE-MIT))

at your option.

Unless you explicitly state otherwise, any contribution intentionally
submitted for inclusion in the work by you, as defined in the Apache-2.0
license, shall be dual licensed as above, without any additional terms or
conditions.
