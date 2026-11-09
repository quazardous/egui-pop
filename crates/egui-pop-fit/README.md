# slotflow

A slot layout engine over plain boxes. It packs ordered elements into a
container in standard slot sizes and says where each one goes. It knows
nothing of widgets, values or drawing, and depends on no GUI toolkit.

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
use slotflow::{layout, Axis, Container, Element, Size, Size2, SizePolicy};

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

`cargo run --example dump` lays out a small panel at several widths.

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
