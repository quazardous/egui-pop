//! Scenario tests: each checks one rule on a small layout, mostly through
//! its ASCII dump.

use slotflow::{
    layout, Axis, Container, Element, GridRect, Orientation, Reason, Size, Size2, SizePolicy,
    Status,
};

const UNIT: f32 = 24.0;

fn sz(w: Size, h: Size) -> Size2 {
    Size2::new(w, h)
}

fn fixed(key: u64, w: Size, h: Size) -> Element {
    Element::new(key, SizePolicy::fixed(sz(w, h)))
}

/// A container `units` wide, growing down.
fn wide(units: f32) -> Container {
    Container::new(units * UNIT, UNIT)
}

fn gr(x: u32, y: u32, w: u32, h: u32) -> GridRect {
    GridRect { x, y, w, h }
}

fn rect(l: &slotflow::Layout, key: u64) -> GridRect {
    l.get(key).and_then(|p| p.rect).expect("placed")
}

#[test]
fn a_later_element_moves_up_into_a_gap() {
    let one = Size::ONE;
    let elements = [
        fixed(1, one, one),
        fixed(2, Size::units(2), one),
        fixed(3, one, one),
    ];
    let l = layout(&wide(2.0), &elements, None);
    assert_eq!(
        l.dump(),
        "grid 24x24 twelfths, one char = 12x12\n\
         ac\n\
         bb\n\
         a key=1 form=0 horizontal at 0,0 size 12x12\n\
         b key=2 form=0 horizontal at 0,12 size 24x12\n\
         c key=3 form=0 horizontal at 12,0 size 12x12\n"
    );
    // The logical order is the input; the visual order is reading order.
    let visual: Vec<u64> = l.visual_order().iter().map(|p| p.key).collect();
    assert_eq!(visual, [1, 3, 2]);
    assert_eq!(l.get(3).unwrap().logical, 2);
}

#[test]
fn thirds_and_halves_mix_exactly() {
    let one = Size::ONE;
    let elements = [
        fixed(1, Size::THIRD, one),
        fixed(2, Size::HALF, one),
        fixed(3, Size::THIRD, one),
        fixed(4, Size::QUARTER, one),
        fixed(5, Size::TWO_THIRDS, one),
    ];
    let l = layout(&wide(1.0), &elements, None);
    assert_eq!(
        l.dump(),
        "grid 12x36 twelfths, one char = 1x12\n\
         aaaabbbbbb..\n\
         ccccddd.....\n\
         eeeeeeee....\n\
         a key=1 form=0 horizontal at 0,0 size 4x12\n\
         b key=2 form=0 horizontal at 4,0 size 6x12\n\
         c key=3 form=0 horizontal at 0,12 size 4x12\n\
         d key=4 form=0 horizontal at 4,12 size 3x12\n\
         e key=5 form=0 horizontal at 0,24 size 8x12\n"
    );
}

#[test]
fn half_height_elements_stack_beside_a_tall_one() {
    let elements = [
        fixed(1, Size::ONE, Size::ONE),
        fixed(2, Size::ONE, Size::HALF),
        fixed(3, Size::ONE, Size::HALF),
    ];
    let l = layout(&wide(2.0), &elements, None);
    assert_eq!(rect(&l, 2), gr(12, 0, 12, 6));
    assert_eq!(rect(&l, 3), gr(12, 6, 12, 6));
    assert_eq!(l.rows, 12);
}

#[test]
fn growth_takes_leftover_room_in_slot_steps() {
    let half = sz(Size::HALF, Size::ONE);
    let max = sz(Size::units(4), Size::ONE);
    let elements = [
        Element::new(1, SizePolicy::growable(half, half, max, Axis::Width)),
        Element::new(2, SizePolicy::growable(half, half, max, Axis::Width)),
    ];
    // Three units: both grow to one unit in turn, then the first takes 2
    // and the second, with no room for 2, stays at 1.
    let l = layout(&wide(3.0), &elements, None);
    assert_eq!(rect(&l, 1), gr(0, 0, 24, 12));
    assert_eq!(rect(&l, 2), gr(24, 0, 12, 12));
    // Four units: they share it evenly.
    let l = layout(&wide(4.0), &elements, None);
    assert_eq!(rect(&l, 1), gr(0, 0, 24, 12));
    assert_eq!(rect(&l, 2), gr(24, 0, 24, 12));
}

#[test]
fn growth_keeps_the_aspect_ratio() {
    let one = sz(Size::ONE, Size::ONE);
    let max = sz(Size::units(3), Size::units(3));
    let wheel = SizePolicy::growable(one, one, max, Axis::Width).with_aspect_ratio(1, 1);
    let l = layout(
        &wide(3.0).with_height(3.0 * UNIT),
        &[Element::new(1, wheel)],
        None,
    );
    assert_eq!(rect(&l, 1), gr(0, 0, 36, 36));
    // Growing down: the content height bounds an unbounded container.
    let l = layout(&wide(3.0), &[Element::new(1, wheel)], None);
    assert_eq!(rect(&l, 1).w, 12);
}

#[test]
fn orientation_preferred_then_other_then_pinned() {
    let bar = Element::new(1, SizePolicy::fixed(sz(Size::units(2), Size::HALF)))
        .with_orientations(&[Orientation::Horizontal, Orientation::Vertical]);
    let l = layout(&wide(2.0), std::slice::from_ref(&bar), None);
    assert_eq!(l.get(1).unwrap().orientation, Orientation::Horizontal);
    assert_eq!(rect(&l, 1), gr(0, 0, 24, 6));
    let l = layout(&wide(1.0), std::slice::from_ref(&bar), None);
    assert_eq!(l.get(1).unwrap().orientation, Orientation::Vertical);
    assert_eq!(rect(&l, 1), gr(0, 0, 6, 24));
    let pinned = bar.clone().with_pinned_orientation(Orientation::Vertical);
    let l = layout(&wide(2.0), &[pinned], None);
    assert_eq!(l.get(1).unwrap().orientation, Orientation::Vertical);
    // A pinned orientation the element does not support is ignored.
    let horizontal_only = Element::new(2, SizePolicy::fixed(sz(Size::ONE, Size::ONE)))
        .with_pinned_orientation(Orientation::Vertical);
    let l = layout(&wide(2.0), &[horizontal_only], None);
    assert_eq!(l.get(2).unwrap().orientation, Orientation::Horizontal);
}

#[test]
fn steps_down_to_smaller_forms_then_overflows() {
    let curve = Element::new(1, SizePolicy::fixed(sz(Size::units(3), Size::units(3))))
        .with_form(SizePolicy::fixed(sz(Size::ONE, Size::ONE)));
    assert_eq!(
        layout(&wide(3.0), std::slice::from_ref(&curve), None)
            .get(1)
            .unwrap()
            .form,
        0
    );
    let p = *layout(&wide(2.0), std::slice::from_ref(&curve), None)
        .get(1)
        .unwrap();
    assert_eq!((p.form, p.status), (1, Status::Placed));
    let p = *layout(&wide(0.5), std::slice::from_ref(&curve), None)
        .get(1)
        .unwrap();
    assert_eq!(
        (p.form, p.status, p.rect),
        (1, Status::Overflow(Reason::TooNarrow), None)
    );
    assert_eq!(p.visual, None);
    let short = wide(3.0).with_height(0.5 * UNIT);
    let p = *layout(&short, std::slice::from_ref(&curve), None)
        .get(1)
        .unwrap();
    assert_eq!(p.status, Status::Overflow(Reason::TooShort));
}

#[test]
fn preferred_size_shrinks_to_an_allowed_step() {
    let min = sz(Size::HALF, Size::ONE);
    let pref = sz(Size::units(3), Size::ONE);
    let slider = Element::new(1, SizePolicy::growable(min, pref, pref, Axis::Width));
    // 1.6 units: the slot is one unit, not 1.6 or 1⅓.
    let l = layout(&Container::new(1.6 * UNIT, UNIT), &[slider], None);
    assert_eq!(l.columns, 19);
    assert_eq!(rect(&l, 1).w, 12);
}

#[test]
fn a_full_bounded_container_reports_no_room() {
    let elements = [
        fixed(1, Size::ONE, Size::ONE),
        fixed(2, Size::ONE, Size::ONE),
    ];
    let l = layout(&wide(1.0).with_height(UNIT), &elements, None);
    assert_eq!(l.get(1).unwrap().status, Status::Placed);
    assert_eq!(l.get(2).unwrap().status, Status::Overflow(Reason::NoRoom));
}

#[test]
fn pins_are_placed_first() {
    let one = Size::ONE;
    let elements = [
        fixed(1, one, one),
        fixed(2, one, one).with_pin(0, 0),
        fixed(3, one, one).with_pin(0, 0),
        fixed(4, one, one).with_pin(48, 0),
    ];
    let l = layout(&wide(2.0), &elements, None);
    assert_eq!(rect(&l, 2), gr(0, 0, 12, 12));
    assert_eq!(rect(&l, 1), gr(12, 0, 12, 12));
    let p3 = l.get(3).unwrap();
    assert_eq!(p3.status, Status::Unpinned(Reason::PinOverlap));
    assert_eq!(p3.rect.unwrap().y, 12);
    assert_eq!(
        l.get(4).unwrap().status,
        Status::Unpinned(Reason::PinOutOfBounds)
    );
}

#[test]
fn points_follow_the_unit() {
    let l = layout(
        &Container::new(100.0, 30.0),
        &[fixed(1, Size::HALF, Size::ONE).with_pin(4, 6)],
        None,
    );
    let p = l.get(1).unwrap().points.unwrap();
    assert_eq!((p.x, p.y, p.w, p.h), (10.0, 15.0, 15.0, 30.0));
    assert_eq!(l.height_points(), 45.0);
}

#[test]
fn hysteresis_keeps_choices_near_a_threshold() {
    let el = [
        Element::new(1, SizePolicy::fixed(sz(Size::units(2), Size::ONE)))
            .with_form(SizePolicy::fixed(sz(Size::ONE, Size::ONE))),
    ];
    let at = |units: f32| Container::new(units * UNIT, UNIT);
    let form = |l: &slotflow::Layout| l.get(1).unwrap().form;

    let narrow = layout(&at(1.99), &el, None);
    assert_eq!((narrow.columns, form(&narrow)), (23, 1));
    // Just past the threshold: without the previous result, the full form.
    assert_eq!(form(&layout(&at(2.1), &el, None)), 0);
    // With it, the column count and the small form are kept.
    let kept = layout(&at(2.1), &el, Some(&narrow));
    assert_eq!((kept.columns, form(&kept)), (23, 1));
    // Half a unit past the old column count, the columns follow the box,
    // but the full form waits until the box is half a unit past its
    // threshold; then it takes the grid as it is (29, lagging behind 30).
    let cols = layout(&at(2.45), &el, Some(&kept));
    assert_eq!((cols.columns, form(&cols)), (29, 1));
    let full = layout(&at(2.5), &el, Some(&cols));
    assert_eq!((full.columns, form(&full)), (29, 0));
    // The same box again changes nothing.
    assert_eq!(layout(&at(2.5), &el, Some(&full)), full);
    // Shrinking below the threshold switches at once: nothing is clipped.
    let back = layout(&at(1.9), &el, Some(&full));
    assert_eq!((back.columns, form(&back)), (22, 1));
}

#[test]
fn bad_input_never_panics() {
    let el = [
        fixed(1, Size::ONE, Size::ONE),
        Element {
            forms: vec![],
            ..fixed(2, Size::ONE, Size::ONE)
        },
    ];
    for c in [
        Container::new(f32::NAN, UNIT),
        Container::new(100.0, 0.0),
        Container::new(-5.0, UNIT),
        Container::new(f32::INFINITY, UNIT),
        Container::new(100.0, UNIT).with_height(f32::NAN),
    ] {
        let l = layout(&c, &el, None);
        assert_eq!(l.placements.len(), 2);
        assert_eq!(l.get(2).unwrap().status, Status::Overflow(Reason::NoForms));
    }
    let l = layout(&Container::new(f32::NAN, UNIT), &el, None);
    assert_eq!(
        l.get(1).unwrap().status,
        Status::Overflow(Reason::TooNarrow)
    );
}
