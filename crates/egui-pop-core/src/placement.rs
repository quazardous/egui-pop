//! Placement of a floating element (popover, menu, tooltip) next to an
//! anchor, inside bounds: on the preferred side if it fits, else flipped
//! to the opposite side, else on the side with more room; then shifted
//! along the side to stay inside, and shrunk only when larger than the
//! bounds.
//!
//! egui's own `RectAlign::find_best_align` flips among candidate
//! alignments but neither shifts nor shrinks; this is the small part of
//! floating-ui's model egui-pop needs, pure geometry, tested by
//! properties.
//!
//! ```
//! use egui::{Rect, pos2, vec2};
//! use egui_pop_core::placement::{Align, Placement, Side, place};
//!
//! let bounds = Rect::from_min_size(pos2(0.0, 0.0), vec2(400.0, 300.0));
//! // A button near the bottom: a menu asked below it opens above.
//! let anchor = Rect::from_min_size(pos2(20.0, 270.0), vec2(80.0, 24.0));
//! let p = place(&Placement::new(anchor, vec2(160.0, 120.0), bounds).side(Side::Bottom));
//! assert_eq!(p.side, Side::Top);
//! assert!(p.flipped);
//! assert!(bounds.contains_rect(p.rect));
//! ```

use egui::{Rect, Vec2, pos2, vec2};
use serde::Serialize;

/// Which side of the anchor. Start and end are left and right in a
/// left-to-right layout.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum Side {
    Top,
    Bottom,
    Start,
    End,
}

impl Side {
    pub fn opposite(self) -> Side {
        match self {
            Side::Top => Side::Bottom,
            Side::Bottom => Side::Top,
            Side::Start => Side::End,
            Side::End => Side::Start,
        }
    }

    fn vertical(self) -> bool {
        matches!(self, Side::Top | Side::Bottom)
    }
}

/// How the element lines up with the anchor along the side.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum Align {
    Start,
    Center,
    End,
}

/// What to place, where.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Placement {
    pub anchor: Rect,
    pub size: Vec2,
    pub bounds: Rect,
    pub side: Side,
    pub align: Align,
    /// Space between the anchor and the element.
    pub gap: f32,
}

impl Placement {
    pub fn new(anchor: Rect, size: Vec2, bounds: Rect) -> Self {
        Self {
            anchor,
            size,
            bounds,
            side: Side::Bottom,
            align: Align::Start,
            gap: 4.0,
        }
    }

    pub fn side(mut self, side: Side) -> Self {
        self.side = side;
        self
    }

    pub fn align(mut self, align: Align) -> Self {
        self.align = align;
        self
    }

    pub fn gap(mut self, gap: f32) -> Self {
        self.gap = gap;
        self
    }
}

/// Where it went.
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct Placed {
    #[serde(skip)]
    pub rect: Rect,
    /// The side used.
    pub side: Side,
    /// Not the preferred side.
    pub flipped: bool,
    /// Moved along the side to stay inside the bounds, in points.
    pub shift: f32,
    /// Smaller than asked: it did not fit the bounds.
    pub shrunk: bool,
}

/// Room for the element on a side of the anchor, within the bounds.
fn room(p: &Placement, side: Side) -> f32 {
    let (a, b) = (p.anchor, p.bounds);
    match side {
        Side::Top => a.top() - b.top() - p.gap,
        Side::Bottom => b.bottom() - a.bottom() - p.gap,
        Side::Start => a.left() - b.left() - p.gap,
        Side::End => b.right() - a.right() - p.gap,
    }
}

/// Places the element.
pub fn place(p: &Placement) -> Placed {
    let need = |side: Side| if side.vertical() { p.size.y } else { p.size.x };
    let fits = |side: Side| room(p, side) >= need(side);
    let opposite = p.side.opposite();
    // The opposite side when the preferred one is too small and the
    // opposite fits, or at least has more room.
    let flip = !fits(p.side) && (fits(opposite) || room(p, opposite) > room(p, p.side));
    let (side, flipped) = if flip {
        (opposite, true)
    } else {
        (p.side, false)
    };
    let mut size = p.size;
    let mut shrunk = false;
    // Never larger than the bounds, nor than the room on the side.
    let main_room = room(p, side).max(0.0);
    if side.vertical() {
        if size.y > main_room {
            size.y = main_room;
            shrunk = true;
        }
        if size.x > p.bounds.width() {
            size.x = p.bounds.width();
            shrunk = true;
        }
    } else {
        if size.x > main_room {
            size.x = main_room;
            shrunk = true;
        }
        if size.y > p.bounds.height() {
            size.y = p.bounds.height();
            shrunk = true;
        }
    }
    let a = p.anchor;
    let cross = |start: f32, end: f32, len: f32| match p.align {
        Align::Start => start,
        Align::Center => (start + end - len) / 2.0,
        Align::End => end - len,
    };
    let min = match side {
        Side::Top => pos2(cross(a.left(), a.right(), size.x), a.top() - p.gap - size.y),
        Side::Bottom => pos2(cross(a.left(), a.right(), size.x), a.bottom() + p.gap),
        Side::Start => pos2(
            a.left() - p.gap - size.x,
            cross(a.top(), a.bottom(), size.y),
        ),
        Side::End => pos2(a.right() + p.gap, cross(a.top(), a.bottom(), size.y)),
    };
    let mut rect = Rect::from_min_size(min, size);
    // Shift along the side to stay inside.
    let b = p.bounds;
    let mut delta = vec2(0.0, 0.0);
    if side.vertical() {
        if rect.left() < b.left() {
            delta.x = b.left() - rect.left();
        } else if rect.right() > b.right() {
            delta.x = b.right() - rect.right();
        }
    } else if rect.top() < b.top() {
        delta.y = b.top() - rect.top();
    } else if rect.bottom() > b.bottom() {
        delta.y = b.bottom() - rect.bottom();
    }
    rect = rect.translate(delta);
    // Last resort when the room was negative: inside the bounds anyway.
    if !b.contains_rect(rect) {
        let min = pos2(
            rect.left()
                .clamp(b.left(), (b.right() - rect.width()).max(b.left())),
            rect.top()
                .clamp(b.top(), (b.bottom() - rect.height()).max(b.top())),
        );
        rect = Rect::from_min_size(min, rect.size().min(b.size()));
    }
    Placed {
        rect,
        side,
        flipped,
        shift: delta.x + delta.y,
        shrunk,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A small deterministic generator, so the properties run the same
    /// cases everywhere.
    struct Rng(u64);

    impl Rng {
        fn next(&mut self) -> f32 {
            self.0 ^= self.0 << 13;
            self.0 ^= self.0 >> 7;
            self.0 ^= self.0 << 17;
            (self.0 % 10_000) as f32 / 10_000.0
        }
    }

    const SIDES: [Side; 4] = [Side::Top, Side::Bottom, Side::Start, Side::End];
    const ALIGNS: [Align; 3] = [Align::Start, Align::Center, Align::End];

    #[test]
    fn placement_properties_hold_over_many_cases() {
        let mut rng = Rng(0x9E37_79B9_7F4A_7C15);
        let bounds = Rect::from_min_size(pos2(0.0, 0.0), vec2(800.0, 600.0));
        for case in 0..5_000 {
            let anchor = Rect::from_min_size(
                pos2(rng.next() * 780.0, rng.next() * 580.0),
                vec2(4.0 + rng.next() * 120.0, 4.0 + rng.next() * 40.0),
            )
            .intersect(bounds);
            let size = vec2(10.0 + rng.next() * 500.0, 10.0 + rng.next() * 400.0);
            let side = SIDES[case % 4];
            let align = ALIGNS[case % 3];
            let p = Placement::new(anchor, size, bounds).side(side).align(align);
            let r = place(&p);
            let ctx = format!("case {case}: {p:?} -> {r:?} {:?}", r.rect);
            // Always inside the bounds.
            let eps = 0.01;
            assert!(bounds.expand(eps).contains_rect(r.rect), "{ctx}");
            // Never larger than asked.
            assert!(
                r.rect.width() <= size.x + eps && r.rect.height() <= size.y + eps,
                "{ctx}"
            );
            // Where it fits on the preferred side, it goes there, unshrunk,
            // without covering the anchor.
            if room(&p, side) >= if side.vertical() { size.y } else { size.x }
                && size.x <= bounds.width()
                && size.y <= bounds.height()
            {
                assert_eq!(r.side, side, "{ctx}");
                assert!(!r.flipped && !r.shrunk, "{ctx}");
            }
            // Whenever it is not shrunk, it does not cover the anchor.
            if !r.shrunk {
                assert!(!r.rect.shrink(eps).intersects(anchor.shrink(eps)), "{ctx}");
            }
        }
    }

    #[test]
    fn it_shifts_along_the_side_to_stay_inside() {
        let bounds = Rect::from_min_size(pos2(0.0, 0.0), vec2(400.0, 300.0));
        let anchor = Rect::from_min_size(pos2(360.0, 20.0), vec2(30.0, 20.0));
        let r = place(&Placement::new(anchor, vec2(120.0, 80.0), bounds));
        assert_eq!(r.side, Side::Bottom);
        assert_eq!(r.rect.right(), 400.0);
        assert_eq!(r.shift, -80.0);
        assert!(!r.flipped && !r.shrunk);
    }

    #[test]
    fn it_shrinks_only_when_no_side_has_room() {
        let bounds = Rect::from_min_size(pos2(0.0, 0.0), vec2(400.0, 300.0));
        let anchor = Rect::from_min_size(pos2(10.0, 140.0), vec2(30.0, 20.0));
        let r = place(&Placement::new(anchor, vec2(100.0, 200.0), bounds));
        // 136 points above, 136 below: the bottom keeps it, shrunk.
        assert_eq!(r.side, Side::Bottom);
        assert!(r.shrunk);
        assert!(bounds.contains_rect(r.rect));
    }
}
