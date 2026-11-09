//! Property tests over random elements and boxes.

use egui_gridflow::{
    layout, Axis, Container, Element, Grow, Layout, Orientation, Size, Size2, SizePolicy, Status,
};
use proptest::prelude::*;

const STEPS: [u32; 8] = [3, 4, 6, 8, 9, 12, 24, 36];
const UNIT: f32 = 24.0;

fn size() -> impl Strategy<Value = Size> {
    prop::sample::select(&STEPS[..]).prop_map(|t| Size::from_twelfths(t).unwrap())
}

fn size2() -> impl Strategy<Value = Size2> {
    (size(), size()).prop_map(|(w, h)| Size2::new(w, h))
}

fn policy() -> impl Strategy<Value = SizePolicy> {
    let grow = prop_oneof![
        Just(Grow::Fixed),
        Just(Grow::Along(Axis::Width)),
        Just(Grow::Along(Axis::Height)),
    ];
    let ratio = prop::option::weighted(0.2, prop::sample::select(&[(1, 1), (2, 1), (1, 2)][..]));
    (size2(), size2(), size2(), grow, ratio).prop_map(|(a, b, c, grow, ratio)| SizePolicy {
        min_size: a,
        size: b,
        max_size: c,
        aspect_ratio: ratio.map(|(w, h)| egui_gridflow::AspectRatio {
            width: w,
            height: h,
        }),
        grow,
    })
}

const H: Orientation = Orientation::Horizontal;
const V: Orientation = Orientation::Vertical;
const ORIENTATIONS: &[&[Orientation]] = &[&[], &[H], &[V], &[H, V], &[V, H]];

fn element(key: u64) -> impl Strategy<Value = Element> {
    let orientations = prop::sample::select(ORIENTATIONS);
    let pinned = prop::option::weighted(
        0.2,
        prop_oneof![Just(Orientation::Horizontal), Just(Orientation::Vertical)],
    );
    let pin = prop::option::weighted(0.2, (0u32..60, 0u32..60));
    (
        prop::collection::vec(policy(), 0..4),
        orientations,
        pinned,
        pin,
    )
        .prop_map(move |(forms, o, pinned, pin)| Element {
            key,
            forms,
            orientations: o.to_vec(),
            pinned_orientation: pinned,
            pin: pin.map(|(col, row)| egui_gridflow::Cell { col, row }),
        })
}

fn elements() -> impl Strategy<Value = Vec<Element>> {
    (0usize..12).prop_flat_map(|n| (0..n as u64).map(element).collect::<Vec<_>>())
}

fn container() -> impl Strategy<Value = Container> {
    (0.0f32..120.0, prop::option::weighted(0.4, 0.0f32..120.0)).prop_map(|(w, h)| Container {
        width: w,
        height: h,
        unit: UNIT,
    })
}

/// The structural invariants every layout must hold.
fn check(c: &Container, elements: &[Element], l: &Layout) -> Result<(), TestCaseError> {
    prop_assert_eq!(l.placements.len(), elements.len());
    let max_rows = c.height.map(|h| (h * 12.0 / UNIT).floor() as u32);
    let mut ranks = Vec::new();
    for (i, (p, e)) in l.placements.iter().zip(elements).enumerate() {
        prop_assert_eq!(p.logical, i);
        prop_assert_eq!(p.key, e.key);
        prop_assert_eq!(p.status.is_placed(), p.rect.is_some());
        prop_assert_eq!(p.rect.is_some(), p.visual.is_some());
        prop_assert_eq!(p.rect.is_some(), p.points.is_some());
        let Some(r) = p.rect else { continue };
        ranks.extend(p.visual);
        // Inside the grid.
        prop_assert!(r.right() <= l.columns);
        prop_assert!(r.bottom() <= l.rows);
        if let Some(m) = max_rows {
            prop_assert!(r.bottom() <= m);
        }
        // On allowed steps, within the chosen form's bounds.
        prop_assert!(Size::from_twelfths(r.w).is_some(), "width {}", r.w);
        prop_assert!(Size::from_twelfths(r.h).is_some(), "height {}", r.h);
        let f = &e.forms[p.form];
        let (min, max) = match p.orientation {
            Orientation::Horizontal => (f.min_size, f.max_size.max_of(f.min_size)),
            Orientation::Vertical => (f.min_size.swap(), f.max_size.max_of(f.min_size).swap()),
        };
        prop_assert!(r.w >= min.width.twelfths() && r.h >= min.height.twelfths());
        prop_assert!(r.w <= max.width.twelfths() && r.h <= max.height.twelfths());
        // Supported orientation.
        let supported = if e.orientations.is_empty() {
            vec![Orientation::Horizontal]
        } else {
            e.orientations.clone()
        };
        prop_assert!(supported.contains(&p.orientation));
        // Pins respected.
        if let (Some(pin), Status::Placed) = (e.pin, p.status) {
            prop_assert_eq!((r.x, r.y), (pin.col, pin.row));
        }
        if matches!(p.status, Status::Unpinned(_)) {
            prop_assert!(e.pin.is_some());
        }
    }
    // Visual ranks are a permutation.
    ranks.sort_unstable();
    prop_assert_eq!(ranks, (0..ranks_len(l)).collect::<Vec<_>>());
    // No overlaps.
    let rects: Vec<_> = l.placements.iter().filter_map(|p| p.rect).collect();
    for (i, a) in rects.iter().enumerate() {
        for b in &rects[i + 1..] {
            prop_assert!(!a.overlaps(b), "{:?} overlaps {:?}", a, b);
        }
    }
    Ok(())
}

fn ranks_len(l: &Layout) -> usize {
    l.placements.iter().filter(|p| p.rect.is_some()).count()
}

trait Size2Ext {
    fn swap(self) -> Size2;
    fn max_of(self, other: Size2) -> Size2;
}

impl Size2Ext for Size2 {
    fn swap(self) -> Size2 {
        Size2::new(self.height, self.width)
    }
    fn max_of(self, other: Size2) -> Size2 {
        Size2::new(self.width.max(other.width), self.height.max(other.height))
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(512))]

    #[test]
    fn invariants_hold(c in container(), e in elements()) {
        let l = layout(&c, &e, None);
        check(&c, &e, &l)?;
    }

    #[test]
    fn deterministic(c in container(), e in elements()) {
        prop_assert_eq!(layout(&c, &e, None), layout(&c, &e, None));
    }

    #[test]
    fn same_box_twice_is_stable(c in container(), c2 in container(), e in elements()) {
        let first = layout(&c, &e, None);
        prop_assert_eq!(&layout(&c, &e, Some(&first)), &first);
        // After a resize with hysteresis, the result is valid and stable too.
        let resized = layout(&c2, &e, Some(&first));
        check(&c2, &e, &resized)?;
        prop_assert_eq!(&layout(&c2, &e, Some(&resized)), &resized);
    }

    #[test]
    fn grid_never_exceeds_the_box(c in container(), c2 in container(), e in elements()) {
        let first = layout(&c, &e, None);
        let resized = layout(&c2, &e, Some(&first));
        prop_assert!(resized.columns as f32 * UNIT / 12.0 <= c2.width + 1e-3);
    }
}
