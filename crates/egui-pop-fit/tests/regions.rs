//! Slot regions: placement per anchor, gaps, around the surface, folding
//! by priority, resizing and the right-to-left mirror.

use egui_pop_fit::{
    Anchor, Axis, Card, CardStatus, PointRect, Reason, Region, RegionLayout, Size, Size2, Surface,
    layout_regions,
};

const UNIT: f32 = 24.0;

fn pr(x: f32, y: f32, w: f32, h: f32) -> PointRect {
    PointRect { x, y, w, h }
}

/// A surface `w` by `h` units at the origin.
fn surface(w: f32, h: f32) -> Surface {
    Surface::new(pr(0.0, 0.0, w * UNIT, h * UNIT), UNIT)
}

fn one(key: u64, region: u64) -> Card {
    Card::fixed(key, region, Size2::new(Size::ONE, Size::ONE))
}

fn wide(key: u64, region: u64, units: u32) -> Card {
    Card::fixed(key, region, Size2::new(Size::units(units), Size::ONE))
}

/// The card's rectangle as `(x, y, w, h)`.
fn at(l: &RegionLayout, key: u64) -> (f32, f32, f32, f32) {
    let r = l
        .card(key)
        .and_then(|c| c.rect)
        .unwrap_or_else(|| panic!("card {key} placed:\n{}", l.dump()));
    (r.x, r.y, r.w, r.h)
}

#[test]
fn each_anchor_places_its_card_at_its_corner_edge_or_centre() {
    let regions: Vec<Region> = Anchor::ALL[..9]
        .iter()
        .enumerate()
        .map(|(i, &a)| Region::new(i as u64, a))
        .collect();
    let cards: Vec<Card> = (0..9).map(|i| one(100 + i, i)).collect();
    let l = layout_regions(&surface(10.0, 6.0), &regions, &cards, None);
    let expected = [
        (0.0, 0.0),
        (108.0, 0.0),
        (216.0, 0.0),
        (0.0, 60.0),
        (108.0, 60.0),
        (216.0, 60.0),
        (0.0, 120.0),
        (108.0, 120.0),
        (216.0, 120.0),
    ];
    for (i, (x, y)) in expected.into_iter().enumerate() {
        let key = 100 + i as u64;
        assert_eq!(at(&l, key), (x, y, 24.0, 24.0), "{key}:\n{}", l.dump());
    }
    assert!(l.folded().is_empty(), "{}", l.dump());
}

#[test]
fn a_region_returns_the_rects_of_its_cards_in_order() {
    let regions = [
        Region::new(1, Anchor::TopStart),
        Region::new(2, Anchor::Top),
    ];
    let cards = [one(10, 1), one(11, 1), one(20, 2), one(21, 2)];
    let l = layout_regions(&surface(10.0, 6.0), &regions, &cards, None);
    assert_eq!(
        l.rects(1),
        vec![
            (10, pr(0.0, 0.0, 24.0, 24.0)),
            (11, pr(24.0, 0.0, 24.0, 24.0))
        ]
    );
    // Along a centred edge: in reading order, the block centred.
    assert_eq!(
        l.rects(2),
        vec![
            (20, pr(96.0, 0.0, 24.0, 24.0)),
            (21, pr(120.0, 0.0, 24.0, 24.0))
        ]
    );
}

#[test]
fn an_end_corner_fills_from_its_corner_outwards() {
    let regions = [Region::new(1, Anchor::BottomEnd)];
    let cards = [one(10, 1), one(11, 1)];
    let l = layout_regions(&surface(10.0, 6.0), &regions, &cards, None);
    assert_eq!(at(&l, 10), (216.0, 120.0, 24.0, 24.0));
    assert_eq!(at(&l, 11), (192.0, 120.0, 24.0, 24.0));
}

#[test]
fn the_edges_flow_down_and_wrap_unless_a_depth_folds_them() {
    let cards: Vec<Card> = (0..8).map(|i| one(10 + i, 1)).collect();
    let s = surface(10.0, 6.0);
    // Three cards: a column centred on the start edge.
    let l = layout_regions(&s, &[Region::new(1, Anchor::Start)], &cards[..3], None);
    assert_eq!(at(&l, 10), (0.0, 36.0, 24.0, 24.0));
    assert_eq!(at(&l, 11), (0.0, 60.0, 24.0, 24.0));
    assert_eq!(at(&l, 12), (0.0, 84.0, 24.0, 24.0));
    // Eight: six down the edge, two in a second column.
    let l = layout_regions(&s, &[Region::new(1, Anchor::Start)], &cards, None);
    assert!(l.folded().is_empty(), "{}", l.dump());
    assert_eq!(at(&l, 15), (0.0, 120.0, 24.0, 24.0));
    assert_eq!(at(&l, 16).0, 24.0);
    // One column deep: the last two fold.
    let region = Region::new(1, Anchor::Start).with_depth(UNIT);
    let l = layout_regions(&s, &[region], &cards, None);
    assert_eq!(l.folded(), vec![16, 17], "{}", l.dump());
    assert_eq!(
        l.card(17).unwrap().status,
        CardStatus::Folded(Reason::NoRoom)
    );
}

#[test]
fn the_gap_keeps_cards_off_the_edges_and_apart() {
    let regions = [
        Region::new(1, Anchor::TopStart).with_gap(8.0),
        Region::new(2, Anchor::BottomEnd).with_gap(8.0),
    ];
    let cards = [one(10, 1), one(11, 1), one(20, 2)];
    let l = layout_regions(&surface(10.0, 6.0), &regions, &cards, None);
    // Each card is its slot less half the gap on each side.
    assert_eq!(at(&l, 10), (8.0, 8.0, 16.0, 16.0));
    assert_eq!(at(&l, 11), (32.0, 8.0, 16.0, 16.0));
    assert_eq!(l.card(10).unwrap().slot, Some(pr(4.0, 4.0, 24.0, 24.0)));
    assert_eq!(at(&l, 20), (216.0, 120.0, 16.0, 16.0));
}

#[test]
fn a_card_sized_in_points_keeps_its_size_against_the_anchor() {
    let regions = [
        Region::new(1, Anchor::TopStart).with_gap(8.0),
        Region::new(2, Anchor::BottomEnd).with_gap(8.0),
    ];
    let cards = [
        Card::sized(10, 1, 40.0, 20.0),
        Card::sized(20, 2, 40.0, 20.0),
    ];
    let l = layout_regions(&surface(10.0, 6.0), &regions, &cards, None);
    assert_eq!(at(&l, 10), (8.0, 8.0, 40.0, 20.0));
    // Its slot holds the size and the gap, in slot sizes: two units by
    // two (28 points down is more than one unit).
    assert_eq!(l.card(10).unwrap().slot, Some(pr(4.0, 4.0, 48.0, 48.0)));
    assert_eq!(at(&l, 20), (192.0, 116.0, 40.0, 20.0));
}

#[test]
fn regions_around_the_surface_lie_against_it_within_the_bounds() {
    let s = Surface::new(pr(40.0, 40.0, 160.0, 100.0), UNIT).within(pr(0.0, 0.0, 240.0, 180.0));
    let regions = [
        Region::new(1, Anchor::Top).around().with_gap(4.0),
        Region::new(2, Anchor::TopStart).around().with_gap(4.0),
        Region::new(3, Anchor::BottomEnd).around().with_gap(4.0),
    ];
    let cards = [one(10, 1), one(20, 2), one(30, 3)];
    let l = layout_regions(&s, &regions, &cards, None);
    // Above the surface, centred on it, its bottom a gap above it.
    assert_eq!(at(&l, 10), (110.0, 16.0, 20.0, 20.0));
    // Off its top start corner, against it.
    assert_eq!(at(&l, 20), (16.0, 16.0, 20.0, 20.0));
    // Off its bottom end corner.
    assert_eq!(at(&l, 30), (204.0, 144.0, 20.0, 20.0));
    // No margin, no room.
    let bare = Surface::new(pr(40.0, 40.0, 160.0, 100.0), UNIT);
    let l = layout_regions(&bare, &regions, &cards, None);
    assert_eq!(l.folded(), vec![10, 20, 30], "{}", l.dump());
}

#[test]
fn lower_priority_cards_fold_first_when_room_runs_out() {
    let region = Region::new(1, Anchor::TopStart).with_depth(UNIT);
    let cards = [
        wide(10, 1, 4).with_priority(1),
        wide(11, 1, 4),
        wide(12, 1, 4).with_priority(2),
    ];
    let l = layout_regions(&surface(10.0, 6.0), &[region], &cards, None);
    assert_eq!(l.folded(), vec![11], "{}", l.dump());
    assert_eq!(
        l.card(11).unwrap().status,
        CardStatus::Folded(Reason::NoRoom)
    );
    // The others keep their order.
    assert_eq!(at(&l, 10).0, 0.0);
    assert_eq!(at(&l, 12).0, 96.0);
    // On a tie, the last folds.
    let tied = [wide(10, 1, 4), wide(11, 1, 4), wide(12, 1, 4)];
    let region = Region::new(1, Anchor::TopStart).with_depth(UNIT);
    let l = layout_regions(&surface(10.0, 6.0), &[region], &tied, None);
    assert_eq!(l.folded(), vec![12], "{}", l.dump());
}

#[test]
fn a_card_wider_than_the_whole_region_folds_alone() {
    let cards = [wide(10, 1, 12).with_priority(5), one(11, 1)];
    let l = layout_regions(
        &surface(10.0, 6.0),
        &[Region::new(1, Anchor::TopStart)],
        &cards,
        None,
    );
    assert_eq!(
        l.card(10).unwrap().status,
        CardStatus::Folded(Reason::TooNarrow)
    );
    assert_eq!(at(&l, 11), (0.0, 0.0, 24.0, 24.0));
}

#[test]
fn a_resize_moves_the_end_regions_and_folds_or_unfolds_cards() {
    let regions = [
        Region::new(1, Anchor::TopStart).with_depth(UNIT),
        Region::new(2, Anchor::TopEnd),
    ];
    let cards = [
        wide(10, 1, 4).with_priority(1),
        wide(11, 1, 4),
        wide(12, 1, 4).with_priority(2),
        one(20, 2).with_priority(9),
    ];
    // Same priority: the start corner, given first, takes its row first;
    // the end corner's card goes beside its block.
    let l = layout_regions(&surface(13.0, 6.0), &regions, &cards, None);
    assert!(l.folded().is_empty(), "{}", l.dump());
    assert_eq!(at(&l, 20).0, 12.0 * UNIT);
    let previous = l;
    let l = layout_regions(&surface(10.0, 6.0), &regions, &cards, Some(&previous));
    assert_eq!(l.folded(), vec![11], "{}", l.dump());
    assert_eq!(at(&l, 20).0, 9.0 * UNIT);
    let l = layout_regions(&surface(7.0, 6.0), &regions, &cards, Some(&l));
    assert_eq!(l.folded(), vec![10, 11], "{}", l.dump());
    assert_eq!(at(&l, 12).0, 0.0);
    // And back.
    let l = layout_regions(&surface(13.0, 6.0), &regions, &cards, Some(&l));
    assert!(l.folded().is_empty(), "{}", l.dump());
}

#[test]
fn a_higher_priority_region_takes_its_room_first() {
    // Both reach for the top row; the end corner's wide card wins it.
    let regions = [
        Region::new(1, Anchor::TopStart).with_depth(UNIT),
        Region::new(2, Anchor::TopEnd)
            .with_depth(UNIT)
            .with_priority(1),
    ];
    let cards = [wide(10, 1, 6), wide(20, 2, 6)];
    let l = layout_regions(&surface(10.0, 6.0), &regions, &cards, None);
    assert_eq!(at(&l, 20), (96.0, 0.0, 144.0, 24.0));
    // Beside it there is no room in a row one unit deep: the start
    // corner's card folds.
    assert_eq!(l.folded(), vec![10], "{}", l.dump());
}

#[test]
fn the_overlay_ignores_the_other_regions() {
    let regions = [
        Region::new(1, Anchor::TopStart),
        Region::new(2, Anchor::Overlay),
    ];
    let cards = [wide(10, 1, 10), one(20, 2)];
    let l = layout_regions(&surface(10.0, 6.0), &regions, &cards, None);
    assert_eq!(at(&l, 10), (0.0, 0.0, 240.0, 24.0));
    assert_eq!(at(&l, 20), (108.0, 60.0, 24.0, 24.0));
}

#[test]
fn a_second_region_at_the_same_anchor_gets_no_room() {
    let regions = [
        Region::new(1, Anchor::TopStart),
        Region::new(2, Anchor::TopStart),
    ];
    let cards = [one(10, 1), one(20, 2)];
    let l = layout_regions(&surface(10.0, 6.0), &regions, &cards, None);
    assert_eq!(
        l.card(20).unwrap().status,
        CardStatus::Folded(Reason::NoRoom)
    );
    assert_eq!(l.region(2).unwrap().area, None);
}

#[test]
fn a_card_with_no_region_is_said() {
    let l = layout_regions(&surface(10.0, 6.0), &[], &[one(10, 7)], None);
    assert_eq!(l.card(10).unwrap().status, CardStatus::NoRegion);
    assert_eq!(l.dump(), "  card 10 no-region\n");
}

#[test]
fn right_to_left_mirrors_start_and_end() {
    let regions = [
        Region::new(1, Anchor::TopStart),
        Region::new(2, Anchor::Top),
        Region::new(3, Anchor::Start),
        Region::new(4, Anchor::BottomEnd),
    ];
    let cards = [
        one(10, 1),
        one(11, 1),
        one(20, 2),
        one(21, 2),
        one(30, 3),
        one(40, 4),
    ];
    let ltr = layout_regions(&surface(10.0, 6.0), &regions, &cards, None);
    let rtl = layout_regions(
        &surface(10.0, 6.0).right_to_left(true),
        &regions,
        &cards,
        None,
    );
    for c in &ltr.cards {
        let a = c.rect.unwrap();
        let b = rtl.card(c.key).unwrap().rect.unwrap();
        assert_eq!(
            (b.x, b.y, b.w, b.h),
            (240.0 - a.x - a.w, a.y, a.w, a.h),
            "{}:\n{}\n{}",
            c.key,
            ltr.dump(),
            rtl.dump()
        );
    }
    // The first card of the top start corner is in the corner: right.
    assert_eq!(at(&rtl, 10).0, 216.0);
    // Along the top edge, reading order runs right to left.
    assert!(at(&rtl, 20).0 > at(&rtl, 21).0);
}

#[test]
fn the_dump_says_each_region_and_card() {
    let regions = [Region::new(1, Anchor::TopEnd)
        .with_gap(4.0)
        .with_flow(Axis::Width)];
    let cards = [one(10, 1), wide(11, 1, 20)];
    let l = layout_regions(&surface(10.0, 6.0), &regions, &cards, None);
    assert_eq!(
        l.dump(),
        "region 1 top-end over area 4,4 232x136 block 216,4 20x20\n  \
         card 10 at 216,4 20x20\n  \
         card 11 folded(too-narrow)\n"
    );
}
