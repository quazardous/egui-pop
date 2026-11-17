//! Scenario tests for groups: dense within, strict order across.

use egui_fit::{
    layout, layout_grouped, Container, Element, GridRect, Group, Reason, Size, Size2, SizePolicy,
    Status,
};

const UNIT: f32 = 24.0;

fn fixed(key: u64, w: Size, h: Size) -> Element {
    Element::new(key, SizePolicy::fixed(Size2::new(w, h)))
}

fn wide(units: f32) -> Container {
    Container::new(units * UNIT, UNIT)
}

fn gr(x: u32, y: u32, w: u32, h: u32) -> GridRect {
    GridRect { x, y, w, h }
}

/// A loose control, a case (legend, two controls, a wide one, one more),
/// and a loose control after it.
fn panel() -> Vec<Element> {
    let one = Size::ONE;
    vec![
        fixed(1, one, one),
        fixed(10, Size::units(2), Size::HALF),
        fixed(2, one, one),
        fixed(3, Size::units(2), one),
        fixed(4, one, one),
        fixed(5, one, one),
    ]
}

#[test]
fn a_later_element_stays_out_of_an_earlier_group() {
    // Without groups, `c` moves up beside `a` and `f` beside `e`.
    assert_eq!(
        layout(&wide(2.0), &panel(), None).dump(),
        "grid 24x42 twelfths, one char = 12x6\n\
         ac\nac\nbb\ndd\ndd\nef\nef\n\
         a key=1 form=0 horizontal at 0,0 size 12x12\n\
         b key=10 form=0 horizontal at 0,12 size 24x6\n\
         c key=2 form=0 horizontal at 12,0 size 12x12\n\
         d key=3 form=0 horizontal at 0,18 size 24x12\n\
         e key=4 form=0 horizontal at 0,30 size 12x12\n\
         f key=5 form=0 horizontal at 12,30 size 12x12\n"
    );
    // In a case under its legend `b`: dense inside (`e` moves up beside
    // `c`), strict outside (`c` stays below the legend, `f` below the case).
    let l = layout_grouped(
        &wide(2.0),
        &panel(),
        &[Group::new(10, 1..5).with_header()],
        None,
    );
    assert_eq!(
        l.dump(),
        "grid 24x54 twelfths, one char = 12x6\n\
         \x20 a.\n\
         \x20 a.\n\
         / bb\n\
         | ce\n\
         | ce\n\
         | dd\n\
         \\ dd\n\
         \x20 f.\n\
         \x20 f.\n\
         group key=10 at 0,12 size 24x30 body at row 18\n\
         a key=1 form=0 horizontal at 0,0 size 12x12\n\
         b key=10 form=0 horizontal at 0,12 size 24x6\n\
         c key=2 form=0 horizontal at 0,18 size 12x12\n\
         d key=3 form=0 horizontal at 0,30 size 24x12\n\
         e key=4 form=0 horizontal at 12,18 size 12x12\n\
         f key=5 form=0 horizontal at 0,42 size 12x12\n"
    );
    let g = l.group(10).unwrap();
    assert_eq!(g.rect, Some(gr(0, 12, 24, 30)));
    let points = g.points.unwrap();
    assert_eq!((points.y, points.h), (24.0, 60.0));
    assert_eq!(l.placements[1].group, Some(0), "the header is in its group");
    assert_eq!(l.placements[0].group, None);
    assert_eq!(l.placements[5].group, None);
    // The visual order follows the bands.
    let visual: Vec<u64> = l.visual_order().iter().map(|p| p.key).collect();
    assert_eq!(visual, [1, 10, 2, 4, 3, 5]);
}

#[test]
fn layout_is_layout_grouped_without_groups() {
    let c = wide(2.0);
    assert_eq!(
        layout(&c, &panel(), None),
        layout_grouped(&c, &panel(), &[], None)
    );
}

#[test]
fn pins_count_from_their_band() {
    let one = Size::ONE;
    let elements = [
        fixed(1, one, one),
        fixed(10, Size::units(2), Size::HALF),
        fixed(2, one, one),
        fixed(3, one, one).with_pin(0, 0),
        // Loose, after the case: its pin counts from the case's bottom.
        fixed(4, one, one).with_pin(12, 0),
    ];
    let l = layout_grouped(
        &wide(2.0),
        &elements,
        &[Group::new(10, 1..4).with_header()],
        None,
    );
    assert_eq!(
        l.dump(),
        "grid 24x42 twelfths, one char = 12x6\n\
         \x20 a.\n\
         \x20 a.\n\
         / bb\n\
         | dc\n\
         \\ dc\n\
         \x20 .e\n\
         \x20 .e\n\
         group key=10 at 0,12 size 24x18 body at row 18\n\
         a key=1 form=0 horizontal at 0,0 size 12x12\n\
         b key=10 form=0 horizontal at 0,12 size 24x6\n\
         c key=2 form=0 horizontal at 12,18 size 12x12\n\
         d key=3 form=0 horizontal at 0,18 size 12x12\n\
         e key=4 form=0 horizontal at 12,30 size 12x12\n"
    );
    assert_eq!(l.get(3).unwrap().band_top, 18);
    assert_eq!(l.get(4).unwrap().band_top, 30);
    assert_eq!(l.group(10).unwrap().body_top, 18);
    // A pin past the group's room in a bounded box is out of bounds.
    let bounded = wide(2.0).with_height(2.5 * UNIT);
    let l = layout_grouped(
        &bounded,
        &elements,
        &[Group::new(10, 1..4).with_header()],
        None,
    );
    assert_eq!(l.get(4).unwrap().status, Status::Overflow(Reason::NoRoom));
}

#[test]
fn groups_nest() {
    let one = Size::ONE;
    let legend = |key| fixed(key, Size::units(3), Size::HALF);
    let elements = [
        legend(10),
        fixed(1, one, one),
        legend(20),
        fixed(2, Size::units(2), one),
        fixed(3, one, one),
        fixed(4, one, one),
    ];
    let groups = [
        Group::new(10, 0..5).with_header(),
        Group::new(20, 2..4).with_header(),
    ];
    let l = layout_grouped(&wide(3.0), &elements, &groups, None);
    assert_eq!(
        l.dump(),
        "grid 36x60 twelfths, one char = 12x6\n\
         /  aaa\n\
         |  b..\n\
         |  b..\n\
         |/ ccc\n\
         || dd.\n\
         |\\ dd.\n\
         |  e..\n\
         \\  e..\n\
         \x20  f..\n\
         \x20  f..\n\
         group key=10 at 0,0 size 36x48 body at row 6\n\
         group key=20 in key=10 at 0,18 size 36x18 body at row 24\n\
         a key=10 form=0 horizontal at 0,0 size 36x6\n\
         b key=1 form=0 horizontal at 0,6 size 12x12\n\
         c key=20 form=0 horizontal at 0,18 size 36x6\n\
         d key=2 form=0 horizontal at 0,24 size 24x12\n\
         e key=3 form=0 horizontal at 0,36 size 12x12\n\
         f key=4 form=0 horizontal at 0,48 size 12x12\n"
    );
}

#[test]
fn a_barrier_splits_loose_elements() {
    let one = Size::ONE;
    let elements = [
        fixed(1, one, one),
        fixed(2, Size::units(2), one),
        fixed(3, one, one),
    ];
    let l = layout_grouped(&wide(2.0), &elements, &[Group::barrier(9, 2)], None);
    assert_eq!(
        l.dump(),
        "grid 24x36 twelfths, one char = 12x12\n\
         a.\n\
         bb\n\
         c.\n\
         group key=9 empty\n\
         a key=1 form=0 horizontal at 0,0 size 12x12\n\
         b key=2 form=0 horizontal at 0,12 size 24x12\n\
         c key=3 form=0 horizontal at 0,24 size 12x12\n"
    );
    assert_eq!(l.group(9).unwrap().rect, None);
    assert!(!l.group(9).unwrap().ignored);
}

#[test]
fn crossing_groups_are_ignored() {
    let one = Size::ONE;
    let elements: Vec<_> = (1..=5).map(|k| fixed(k, one, one)).collect();
    let groups = [
        Group::new(1, 0..3).with_header(),
        // Crosses the first group's end.
        Group::new(2, 2..5),
        // Starts on the first group's header.
        Group::new(3, 0..2),
        // Reaches past the elements.
        Group::new(4, 4..9),
        // Reversed.
        #[allow(clippy::reversed_empty_ranges)]
        Group {
            key: 5,
            range: 4..3,
            header: false,
        },
    ];
    let l = layout_grouped(&wide(3.0), &elements, &groups, None);
    let ignored: Vec<bool> = l.groups.iter().map(|g| g.ignored).collect();
    assert_eq!(ignored, [false, true, true, true, true]);
    assert!(l.groups[1..].iter().all(|g| g.rect.is_none()));
    // The same as with the first group alone.
    let alone = layout_grouped(&wide(3.0), &elements, &groups[..1], None);
    assert_eq!(l.placements, alone.placements);
}

#[test]
fn a_bounded_box_overflows_what_a_group_pushes_out() {
    let one = Size::ONE;
    let elements = [fixed(1, one, one), fixed(2, one, one), fixed(3, one, one)];
    let bounded = wide(2.0).with_height(2.0 * UNIT);
    // Loose, all three fit in two rows; the case starts on the second row
    // and holds only one of its two.
    let l = layout_grouped(&bounded, &elements, &[], None);
    assert!(l.placements.iter().all(|p| p.status.is_placed()));
    let l = layout_grouped(&bounded, &elements, &[Group::new(7, 1..3)], None);
    assert_eq!(l.get(2).unwrap().rect, Some(gr(0, 12, 12, 12)));
    assert_eq!(l.get(3).unwrap().rect, Some(gr(12, 12, 12, 12)));
    let bounded = wide(1.0).with_height(2.0 * UNIT);
    let l = layout_grouped(&bounded, &elements, &[Group::new(7, 1..3)], None);
    assert_eq!(l.get(3).unwrap().status, Status::Overflow(Reason::NoRoom));
    assert_eq!(l.group(7).unwrap().rect, Some(gr(0, 12, 12, 12)));
}

#[test]
fn growth_and_hysteresis_hold_in_groups() {
    use egui_fit::Axis;
    let row = |u| Size2::new(Size::units(u), Size::ONE);
    let slider = |key| {
        Element::new(
            key,
            SizePolicy::growable(row(1), row(1), row(4), Axis::Width),
        )
        .with_form(SizePolicy::fixed(Size2::new(Size::HALF, Size::ONE)))
    };
    let elements = [
        fixed(10, Size::units(1), Size::HALF),
        slider(1),
        slider(2),
        fixed(3, Size::ONE, Size::ONE),
    ];
    let groups = [Group::new(10, 0..3).with_header()];
    let first = layout_grouped(&wide(3.0), &elements, &groups, None);
    // The sliders grow inside the case, but `d` stays out of it.
    assert_eq!(first.get(3).unwrap().rect, Some(gr(0, 18, 12, 12)));
    assert!(first.get(1).unwrap().rect.unwrap().w > 12);
    // Same box twice: unchanged. A little wider: the grid holds.
    assert_eq!(
        layout_grouped(&wide(3.0), &elements, &groups, Some(&first)),
        first
    );
    let wider = layout_grouped(&wide(3.2), &elements, &groups, Some(&first));
    assert_eq!(wider.columns, first.columns);
    assert_eq!(wider.groups, first.groups);
}
