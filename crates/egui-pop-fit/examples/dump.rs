//! Lays out a small panel at a few widths and prints each layout.
//!
//! Run with `cargo run --example dump`.

use egui_fit::{layout, Axis, Container, Element, Orientation, Size, Size2, SizePolicy};

fn main() {
    let unit = 24.0;
    let row = Size::ONE;
    let elements = [
        // A toggle and a short choice: half a unit each.
        Element::new(1, SizePolicy::fixed(Size2::new(Size::HALF, row))),
        Element::new(2, SizePolicy::fixed(Size2::new(Size::HALF, row))),
        // A slider: one unit at least, two preferred, up to four.
        Element::new(
            3,
            SizePolicy::growable(
                Size2::new(Size::ONE, row),
                Size2::new(Size::units(2), row),
                Size2::new(Size::units(4), row),
                Axis::Width,
            ),
        ),
        // A colour wheel: square, two units, growing to three.
        Element::new(
            4,
            SizePolicy::growable(
                Size2::new(Size::units(2), Size::units(2)),
                Size2::new(Size::units(2), Size::units(2)),
                Size2::new(Size::units(3), Size::units(3)),
                Axis::Width,
            )
            .with_aspect_ratio(1, 1),
        )
        // Its smaller form: a one-unit swatch.
        .with_form(SizePolicy::fixed(Size2::new(Size::ONE, row))),
        // A gradient bar: wide and thin, turned when the panel is narrow.
        Element::new(5, SizePolicy::fixed(Size2::new(Size::units(3), Size::HALF)))
            .with_orientations(&[Orientation::Horizontal, Orientation::Vertical]),
        // Another toggle, which fills a gap left above.
        Element::new(6, SizePolicy::fixed(Size2::new(Size::HALF, row))),
    ];

    let mut previous = None;
    for units in [4.0, 3.0, 2.0, 2.2, 1.5] {
        let container = Container::new(units * unit, unit);
        let result = layout(&container, &elements, previous.as_ref());
        println!("--- {units} units wide ---");
        print!("{}", result.dump());
        previous = Some(result);
    }
}
