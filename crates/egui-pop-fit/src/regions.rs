//! Slot regions: named places around and over a surface (an image, a
//! canvas), where cards sit.
//!
//! A [`Region`] has an [`Anchor`] (a corner, an edge, the centre, or the
//! whole surface as an overlay), a [`Side`] (over the surface, or around
//! it in the margin up to the bounds), a gap and a priority. A [`Card`] is
//! one element of the slot engine, placed in one region. The regions are
//! solved in priority order: each takes the room at its anchor that the
//! regions before it left, packs its cards there with [`layout`](crate::layout),
//! and its cards' block becomes an obstacle for the regions after it.
//!
//! When the room runs out, a region folds its lowest-priority card (the
//! last in its order on a tie) and packs the others again, until the rest
//! fit; then each folded card, the highest priority first, comes back if
//! it fits beside them. A card is never placed over another one, nor cut.

use crate::element::{Axis, Element, Orientation, Size2, SizePolicy};
use crate::layout::{Container, Layout, PointRect, Reason, Status, layout};
use crate::size::{Size, TWELFTHS_PER_UNIT};
use core::fmt;
use core::fmt::Write as _;

/// Where a region sits on its surface. Start and end follow the
/// direction: start is left in a left-to-right interface, right in a
/// right-to-left one ([`Surface::right_to_left`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[non_exhaustive]
pub enum Anchor {
    /// The top start corner.
    TopStart,
    /// The top edge, centred.
    Top,
    /// The top end corner.
    TopEnd,
    /// The start edge, centred vertically.
    Start,
    /// The centre, between the regions around it.
    Center,
    /// The end edge, centred vertically.
    End,
    /// The bottom start corner.
    BottomStart,
    /// The bottom edge, centred.
    Bottom,
    /// The bottom end corner.
    BottomEnd,
    /// The whole surface, over the other regions: it neither avoids them
    /// nor makes them move. Its cards are centred.
    Overlay,
}

impl Anchor {
    /// Every anchor, corners and edges in reading order, then the centre
    /// and the overlay.
    pub const ALL: [Anchor; 10] = [
        Anchor::TopStart,
        Anchor::Top,
        Anchor::TopEnd,
        Anchor::Start,
        Anchor::Center,
        Anchor::End,
        Anchor::BottomStart,
        Anchor::Bottom,
        Anchor::BottomEnd,
        Anchor::Overlay,
    ];

    /// Its name in kebab case (`top-start`).
    pub fn name(self) -> &'static str {
        match self {
            Anchor::TopStart => "top-start",
            Anchor::Top => "top",
            Anchor::TopEnd => "top-end",
            Anchor::Start => "start",
            Anchor::Center => "center",
            Anchor::End => "end",
            Anchor::BottomStart => "bottom-start",
            Anchor::Bottom => "bottom",
            Anchor::BottomEnd => "bottom-end",
            Anchor::Overlay => "overlay",
        }
    }

    /// The way its cards flow by default: down the start and end edges,
    /// across everywhere else.
    pub fn default_flow(self) -> Axis {
        match self {
            Anchor::Start | Anchor::End => Axis::Height,
            _ => Axis::Width,
        }
    }

    /// Its logical place on each axis: start, middle or end.
    fn places(self) -> (Place, Place) {
        use Place::{End, Mid, Start};
        match self {
            Anchor::TopStart => (Start, Start),
            Anchor::Top => (Mid, Start),
            Anchor::TopEnd => (End, Start),
            Anchor::Start => (Start, Mid),
            Anchor::Center | Anchor::Overlay => (Mid, Mid),
            Anchor::End => (End, Mid),
            Anchor::BottomStart => (Start, End),
            Anchor::Bottom => (Mid, End),
            Anchor::BottomEnd => (End, End),
        }
    }
}

impl fmt::Display for Anchor {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

/// Whether a region lies over the surface or around it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[non_exhaustive]
pub enum Side {
    /// On the surface, inside its edges.
    #[default]
    Over,
    /// Outside the surface, in the margin between its edges and the
    /// bounds ([`Surface::within`]), against the surface. The centre and
    /// the overlay have no outside: they stay over.
    Around,
}

impl fmt::Display for Side {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Side::Over => "over",
            Side::Around => "around",
        })
    }
}

/// A named place for cards on a surface.
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[non_exhaustive]
pub struct Region {
    /// The caller's key, which cards name ([`Card::region`]).
    pub key: u64,
    /// Where it sits.
    pub anchor: Anchor,
    /// Over the surface or around it.
    pub side: Side,
    /// In points: from the surface's edges to the cards, between cards,
    /// and between this region's cards and another region's.
    pub gap: f32,
    /// Regions with a higher priority take their room first.
    pub priority: i32,
    /// The way its cards flow: across in rows (`Width`) or down in
    /// columns (`Height`), wrapping to a new row or column when one is
    /// full.
    pub flow: Axis,
    /// In points, how far the region reaches into the surface across its
    /// flow (a row band's height, a column band's width); `None` reaches
    /// as far as the room allows.
    pub depth: Option<f32>,
}

impl Region {
    /// A region over the surface at `anchor`, with no gap, priority 0, its
    /// anchor's default flow and no depth limit.
    pub fn new(key: u64, anchor: Anchor) -> Region {
        Region {
            key,
            anchor,
            side: Side::Over,
            gap: 0.0,
            priority: 0,
            flow: anchor.default_flow(),
            depth: None,
        }
    }

    /// The same region around the surface instead of over it.
    pub fn around(mut self) -> Region {
        self.side = Side::Around;
        self
    }

    /// The same region with a gap of `gap` points.
    pub fn with_gap(mut self, gap: f32) -> Region {
        self.gap = gap;
        self
    }

    /// The same region with a priority.
    pub fn with_priority(mut self, priority: i32) -> Region {
        self.priority = priority;
        self
    }

    /// The same region flowing its cards along `flow`.
    pub fn with_flow(mut self, flow: Axis) -> Region {
        self.flow = flow;
        self
    }

    /// The same region reaching at most `depth` points across its flow.
    pub fn with_depth(mut self, depth: f32) -> Region {
        self.depth = Some(depth);
        self
    }
}

/// A card: one element of the slot engine, placed in a region.
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[non_exhaustive]
pub struct Card {
    /// The region's key.
    pub region: u64,
    /// The element: its key (unique across the cards), forms,
    /// orientations, and its pin, counted from the region's anchor corner.
    pub element: Element,
    /// When room runs out, cards with a lower priority fold first.
    pub priority: i32,
    /// A size in points, `[width, height]`, instead of the element's
    /// forms: the card takes the smallest slot holding it and the gap,
    /// and its rectangle is this size, against the anchor in its slot.
    pub size: Option<[f32; 2]>,
}

impl Card {
    /// A card for `element` in the region keyed `region`, priority 0.
    pub fn new(region: u64, element: Element) -> Card {
        Card {
            region,
            element,
            priority: 0,
            size: None,
        }
    }

    /// A card of one fixed slot size.
    pub fn fixed(key: u64, region: u64, size: Size2) -> Card {
        Card::new(region, Element::new(key, SizePolicy::fixed(size)))
    }

    /// A card `width` by `height` points.
    pub fn sized(key: u64, region: u64, width: f32, height: f32) -> Card {
        let one = Size2::new(Size::ONE, Size::ONE);
        let mut card = Card::fixed(key, region, one);
        card.size = Some([width, height]);
        card
    }

    /// The same card with a priority.
    pub fn with_priority(mut self, priority: i32) -> Card {
        self.priority = priority;
        self
    }

    /// The card's key (its element's).
    pub fn key(&self) -> u64 {
        self.element.key
    }
}

/// The surface the regions sit on and around, in points.
#[derive(Clone, Copy, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[non_exhaustive]
pub struct Surface {
    /// The surface (an image, a canvas).
    pub rect: PointRect,
    /// What the regions around it may use: the surface and its margin.
    pub bounds: PointRect,
    /// The slot engine's base unit, in points.
    pub unit: f32,
    /// Whether start is on the right.
    pub rtl: bool,
}

impl Surface {
    /// A surface with no margin around it, left to right.
    pub fn new(rect: PointRect, unit: f32) -> Surface {
        Surface {
            rect,
            bounds: rect,
            unit,
            rtl: false,
        }
    }

    /// The same surface within `bounds`, the room for the regions around
    /// it.
    pub fn within(mut self, bounds: PointRect) -> Surface {
        self.bounds = bounds;
        self
    }

    /// The same surface, start on the right when `rtl`.
    pub fn right_to_left(mut self, rtl: bool) -> Surface {
        self.rtl = rtl;
        self
    }
}

/// What became of a card.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[non_exhaustive]
pub enum CardStatus {
    /// Placed: it has a rectangle.
    Placed,
    /// Not placed: the host folds it (a menu, a "more" chip) or shows it
    /// elsewhere. `NoRoom` when cards with a higher priority took the
    /// room; `TooNarrow` or `TooShort` when even the region's whole room
    /// cannot hold it.
    Folded(Reason),
    /// Its region is not declared.
    NoRegion,
}

impl CardStatus {
    /// Whether the card has a rectangle.
    pub fn is_placed(&self) -> bool {
        matches!(self, CardStatus::Placed)
    }
}

impl fmt::Display for CardStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CardStatus::Placed => f.write_str("placed"),
            CardStatus::Folded(r) => write!(f, "folded({r})"),
            CardStatus::NoRegion => f.write_str("no-region"),
        }
    }
}

/// Where a card went.
#[derive(Clone, Copy, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[non_exhaustive]
pub struct CardPlacement {
    /// The card's key.
    pub key: u64,
    /// Its region's key.
    pub region: u64,
    /// Its index in the input.
    pub logical: usize,
    /// Its rectangle, when placed: the slot less half the gap on each
    /// side, or its size against the anchor ([`Card::size`]).
    pub rect: Option<PointRect>,
    /// Its whole slot, gap included, when placed.
    pub slot: Option<PointRect>,
    /// The form chosen.
    pub form: usize,
    /// The orientation chosen.
    pub orientation: Orientation,
    /// Placed or folded.
    pub status: CardStatus,
}

/// Where a region went.
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[non_exhaustive]
pub struct RegionPlacement {
    /// The region's key.
    pub key: u64,
    /// Its anchor.
    pub anchor: Anchor,
    /// Its side.
    pub side: Side,
    /// The room it had: its place less the gap, the depth, and the blocks
    /// of the regions before it. `None` when none was left.
    pub area: Option<PointRect>,
    /// The extent of its placed cards, `None` when none is placed.
    pub block: Option<PointRect>,
    /// The slot engine's last layout of its cards, in the engine's frame
    /// (transposed for a region flowing down), kept for hysteresis.
    pub layout: Layout,
}

/// The cards laid out in their regions.
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[non_exhaustive]
pub struct RegionLayout {
    /// One per region, in the order given.
    pub regions: Vec<RegionPlacement>,
    /// One per card, in the order given.
    pub cards: Vec<CardPlacement>,
}

impl RegionLayout {
    /// The placement of the card keyed `key`.
    pub fn card(&self, key: u64) -> Option<&CardPlacement> {
        self.cards.iter().find(|c| c.key == key)
    }

    /// The placement of the region keyed `key`.
    pub fn region(&self, key: u64) -> Option<&RegionPlacement> {
        self.regions.iter().find(|r| r.key == key)
    }

    /// The rectangles of the cards placed in the region keyed `region`,
    /// with their keys, in the cards' order.
    pub fn rects(&self, region: u64) -> Vec<(u64, PointRect)> {
        self.cards
            .iter()
            .filter(|c| c.region == region)
            .filter_map(|c| c.rect.map(|r| (c.key, r)))
            .collect()
    }

    /// The keys of the cards not placed, in the cards' order.
    pub fn folded(&self) -> Vec<u64> {
        self.cards
            .iter()
            .filter(|c| !c.status.is_placed())
            .map(|c| c.key)
            .collect()
    }

    /// One line per region (its anchor, side, room and block), each
    /// followed by its cards (their rectangle, or why they are not
    /// placed), then the cards with no region.
    pub fn dump(&self) -> String {
        let mut out = String::new();
        for r in &self.regions {
            let _ = write!(out, "region {} {} {}", r.key, r.anchor, r.side);
            match r.area {
                Some(a) => {
                    let _ = write!(out, " area {}", show(a));
                }
                None => out.push_str(" no-area"),
            }
            if let Some(b) = r.block {
                let _ = write!(out, " block {}", show(b));
            }
            out.push('\n');
            for c in self.cards.iter().filter(|c| c.region == r.key) {
                card_line(&mut out, c);
            }
        }
        for c in &self.cards {
            if c.status == CardStatus::NoRegion {
                card_line(&mut out, c);
            }
        }
        out
    }
}

fn card_line(out: &mut String, c: &CardPlacement) {
    let _ = write!(out, "  card {}", c.key);
    match (c.status, c.rect) {
        (CardStatus::Placed, Some(r)) => {
            let _ = write!(out, " at {}", show(r));
        }
        (s, _) => {
            let _ = write!(out, " {s}");
        }
    }
    out.push('\n');
}

/// `x,y wxh`, each to a tenth of a point, without trailing zeros.
fn show(r: PointRect) -> String {
    fn n(v: f32) -> String {
        let t = (v * 10.0).round() / 10.0;
        if t == t.trunc() {
            format!("{}", t as i64)
        } else {
            format!("{t:.1}")
        }
    }
    format!("{},{} {}x{}", n(r.x), n(r.y), n(r.w), n(r.h))
}

/// A place on one axis, logical (start, end) or physical (low, high).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Place {
    Start,
    Mid,
    End,
}

/// One axis of a rectangle: its low edge and length.
#[derive(Clone, Copy, Debug)]
struct Span {
    lo: f32,
    len: f32,
}

impl Span {
    fn hi(self) -> f32 {
        self.lo + self.len
    }

    /// From `lo` to `hi`, empty when reversed.
    fn between(lo: f32, hi: f32) -> Span {
        Span {
            lo,
            len: (hi - lo).max(0.0),
        }
    }

    /// The point of the span at `place`.
    fn at(self, place: Place) -> f32 {
        match place {
            Place::Start => self.lo,
            Place::Mid => self.lo + self.len / 2.0,
            Place::End => self.hi(),
        }
    }

    /// Where a block `len` long starts in the span, at `place`, centred
    /// on `centre` when in the middle, kept inside when it fits.
    fn put(self, len: f32, place: Place, centre: f32) -> f32 {
        match place {
            Place::Start => self.lo,
            Place::End => self.hi() - len,
            Place::Mid => {
                let lo = centre - len / 2.0;
                if len >= self.len {
                    self.lo + (self.len - len) / 2.0
                } else {
                    lo.clamp(self.lo, self.hi() - len)
                }
            }
        }
    }

    /// At most `len` long, kept against `place` (around `centre` when in
    /// the middle).
    fn limit(self, len: f32, place: Place, centre: f32) -> Span {
        if len >= self.len {
            return self;
        }
        Span {
            lo: self.put(len, place, centre),
            len: len.max(0.0),
        }
    }
}

fn rect(x: Span, y: Span) -> PointRect {
    PointRect {
        x: x.lo,
        y: y.lo,
        w: x.len,
        h: y.len,
    }
}

fn spans(r: PointRect) -> (Span, Span) {
    (
        Span {
            lo: r.x,
            len: r.w.max(0.0),
        },
        Span {
            lo: r.y,
            len: r.h.max(0.0),
        },
    )
}

/// `r` grown by `d` on each side (shrunk when negative, to nothing at
/// most).
fn inflate(r: PointRect, d: f32) -> PointRect {
    let w = (r.w + 2.0 * d).max(0.0);
    let h = (r.h + 2.0 * d).max(0.0);
    PointRect {
        x: r.x + (r.w - w) / 2.0,
        y: r.y + (r.h - h) / 2.0,
        w,
        h,
    }
}

fn overlaps(a: PointRect, b: PointRect) -> bool {
    a.x < b.x + b.w && b.x < a.x + a.w && a.y < b.y + b.h && b.y < a.y + a.h
}

/// The largest of the four cuts of `area` that leaves out `o` and keeps
/// the point `p`; `None` when `o` covers or touches `p`.
fn cut(area: PointRect, o: PointRect, p: (f32, f32)) -> Option<PointRect> {
    if !overlaps(area, o) {
        return Some(area);
    }
    let (x, y) = spans(area);
    let mut best: Option<PointRect> = None;
    let mut keep = |c: PointRect| {
        if best.is_none_or(|b| c.w * c.h > b.w * b.h) {
            best = Some(c);
        }
    };
    if o.x >= p.0 {
        keep(rect(Span::between(x.lo, o.x.min(x.hi())), y));
    }
    if o.x + o.w <= p.0 {
        keep(rect(Span::between((o.x + o.w).max(x.lo), x.hi()), y));
    }
    if o.y >= p.1 {
        keep(rect(x, Span::between(y.lo, o.y.min(y.hi()))));
    }
    if o.y + o.h <= p.1 {
        keep(rect(x, Span::between((o.y + o.h).max(y.lo), y.hi())));
    }
    // A cut to nothing (an obstacle touching the point) leaves no room.
    best.filter(|b| b.w > 0.0 && b.h > 0.0)
}

/// A region's room before the obstacles: its area, its physical place
/// on each axis (where its block goes) and its anchor point.
fn zone(surface: &Surface, region: &Region) -> (PointRect, (Place, Place), (f32, f32)) {
    let (h, v) = region.anchor.places();
    let flip = |p: Place| match p {
        Place::Start => Place::End,
        Place::End => Place::Start,
        Place::Mid => Place::Mid,
    };
    let h = if surface.rtl { flip(h) } else { h };
    let gap = region.gap.max(0.0);
    let (sx, sy) = spans(surface.rect);
    let (bx, by) = spans(surface.bounds);
    let around =
        region.side == Side::Around && !matches!(region.anchor, Anchor::Center | Anchor::Overlay);
    if around {
        // Per axis: before the surface, along it, or after it; a block
        // before or after the surface lies against it.
        let axis = |s: Span, b: Span, place: Place| match place {
            Place::Start => (Span::between(b.lo, s.lo - gap), Place::End),
            Place::Mid => (s, Place::Mid),
            Place::End => (Span::between(s.hi() + gap, b.hi()), Place::Start),
        };
        let (x, ph) = axis(sx, bx, h);
        let (y, pv) = axis(sy, by, v);
        (rect(x, y), (ph, pv), (x.at(ph), y.at(pv)))
    } else {
        let area = inflate(surface.rect, -gap);
        let (x, y) = spans(area);
        (area, (h, v), (x.at(h), y.at(v)))
    }
}

/// The slot holding `points` and the gap: an allowed size at least as
/// large, a quarter of a unit at least.
fn slot_size(points: f32, unit: f32) -> Size {
    let t = if unit > 0.0 && points.is_finite() {
        // `as` saturates and maps NaN to 0.
        (points.max(0.0) * TWELFTHS_PER_UNIT as f32 / unit).ceil() as u32
    } else {
        0
    };
    let t = if t <= TWELFTHS_PER_UNIT {
        [3, 4, 6, 8, 9, 12]
            .into_iter()
            .find(|&f| f >= t)
            .unwrap_or(TWELFTHS_PER_UNIT)
    } else {
        t.div_ceil(TWELFTHS_PER_UNIT)
            .saturating_mul(TWELFTHS_PER_UNIT)
    };
    Size::from_twelfths(t).unwrap_or(Size::ONE)
}

fn turned(o: Orientation) -> Orientation {
    match o {
        Orientation::Horizontal => Orientation::Vertical,
        Orientation::Vertical => Orientation::Horizontal,
    }
}

/// The element the engine lays out for `card` in a region with `gap`,
/// turned when the region flows down (the engine fills rows).
fn element_of(card: &Card, unit: f32, gap: f32, down: bool) -> Element {
    let mut e = match card.size {
        Some([w, h]) => {
            let size = Size2::new(slot_size(w + gap, unit), slot_size(h + gap, unit));
            let mut e = Element::new(card.element.key, SizePolicy::fixed(size));
            e.pin = card.element.pin;
            e
        }
        None => card.element.clone(),
    };
    if down {
        if e.orientations.is_empty() {
            e.orientations = vec![Orientation::Horizontal];
        }
        e.orientations = e.orientations.iter().map(|&o| turned(o)).collect();
        e.pinned_orientation = e.pinned_orientation.map(turned);
        if let Some(pin) = e.pin.as_mut() {
            core::mem::swap(&mut pin.col, &mut pin.row);
        }
    }
    e
}

/// Lays `cards` out in `regions` on and around `surface`.
///
/// Pass the previous result for the same regions as `previous` to keep
/// each region's choices (column count, forms, orientations) while its
/// room stays within half a unit of a threshold. The result has one
/// placement per region and per card, in input order. This function
/// never panics.
///
/// Regions go by priority, highest first (the first given on a tie):
/// each takes the room at its anchor less the blocks of the regions
/// before it (and the gap), cutting the side that keeps the most room. Its
/// cards fill from its anchor outwards: in a corner the first card is in
/// the corner, along a centred edge they go in reading order. Of two
/// regions at one anchor, the second gets no room once the first placed a
/// card: its cards fold.
pub fn layout_regions(
    surface: &Surface,
    regions: &[Region],
    cards: &[Card],
    previous: Option<&RegionLayout>,
) -> RegionLayout {
    let unit = surface.unit;
    let mut placements: Vec<CardPlacement> = cards
        .iter()
        .enumerate()
        .map(|(i, c)| CardPlacement {
            key: c.key(),
            region: c.region,
            logical: i,
            rect: None,
            slot: None,
            form: 0,
            orientation: Orientation::Horizontal,
            status: CardStatus::NoRegion,
        })
        .collect();
    let mut order: Vec<usize> = (0..regions.len()).collect();
    order.sort_by_key(|&i| (core::cmp::Reverse(regions[i].priority), i));
    let mut done: Vec<Option<RegionPlacement>> = vec![None; regions.len()];
    let mut obstacles: Vec<PointRect> = Vec::new();
    for i in order {
        let region = &regions[i];
        // A key declared twice: the first region has the cards.
        let owner = regions.iter().position(|r| r.key == region.key) == Some(i);
        let members: Vec<usize> = if owner {
            (0..cards.len())
                .filter(|&c| cards[c].region == region.key)
                .collect()
        } else {
            Vec::new()
        };
        let gap = if region.gap.is_finite() {
            region.gap.max(0.0)
        } else {
            0.0
        };
        let (mut area, (ph, pv), p) = zone(surface, region);
        let overlay = region.anchor == Anchor::Overlay;
        let mut room = true;
        if !overlay {
            for &o in &obstacles {
                match cut(area, inflate(o, gap), p) {
                    Some(a) => area = a,
                    None => {
                        room = false;
                        break;
                    }
                }
            }
        }
        let down = region.flow == Axis::Height;
        if let Some(depth) = region.depth {
            let (x, y) = spans(area);
            area = if down {
                rect(x.limit(depth, ph, p.0), y)
            } else {
                rect(x, y.limit(depth, pv, p.1))
            };
        }
        // The engine's box: the room and half a gap around it, so the
        // outer cards' edges, inset by half a gap, meet the room's.
        let engine_box = inflate(area, gap / 2.0);
        let (ew, eh) = if room {
            (engine_box.w, engine_box.h)
        } else {
            (0.0, 0.0)
        };
        let (cw, ch) = if down { (eh, ew) } else { (ew, eh) };
        let container = Container::new(cw, unit).with_height(ch);
        let prev = previous
            .and_then(|l| l.region(region.key))
            .map(|r| &r.layout);
        let elements: Vec<Element> = members
            .iter()
            .map(|&c| element_of(&cards[c], unit, gap, down))
            .collect();
        // Fold until the rest fit: first what even the whole room cannot
        // hold, then the lowest priority.
        let mut active: Vec<usize> = (0..members.len()).collect();
        let mut folded: Vec<(usize, Reason)> = Vec::new();
        if !room {
            // The regions before it took its anchor.
            folded = active.drain(..).map(|a| (a, Reason::NoRoom)).collect();
        }
        let result = loop {
            let input: Vec<Element> = active.iter().map(|&a| elements[a].clone()).collect();
            let l = layout(&container, &input, prev);
            let mut hopeless = Vec::new();
            let mut crowded = false;
            for (at, pl) in l.placements.iter().enumerate() {
                match pl.status {
                    Status::Overflow(Reason::NoRoom) => crowded = true,
                    Status::Overflow(r) => hopeless.push((active[at], r)),
                    _ => {}
                }
            }
            if !hopeless.is_empty() {
                active.retain(|a| !hopeless.iter().any(|(h, _)| h == a));
                folded.extend(hopeless);
                continue;
            }
            if !crowded {
                break l;
            }
            // The lowest priority, the last in order on a tie.
            let Some(at) = (0..active.len())
                .min_by_key(|&k| (cards[members[active[k]]].priority, core::cmp::Reverse(k)))
            else {
                break l;
            };
            folded.push((active.remove(at), Reason::NoRoom));
        };
        // Then each card folded for room, the highest priority first, comes
        // back if it fits beside the others: a smaller card of a lower
        // priority may fit where a larger one did not.
        let mut result = result;
        let mut retry: Vec<usize> = if room {
            folded
                .iter()
                .filter(|(_, r)| *r == Reason::NoRoom)
                .map(|&(m, _)| m)
                .collect()
        } else {
            Vec::new()
        };
        retry.sort_by_key(|&m| (core::cmp::Reverse(cards[members[m]].priority), m));
        for m in retry {
            let mut trial = active.clone();
            trial.push(m);
            trial.sort_unstable();
            let input: Vec<Element> = trial.iter().map(|&a| elements[a].clone()).collect();
            let l = layout(&container, &input, prev);
            if l.placements.iter().all(|p| p.status.is_placed()) {
                active = trial;
                result = l;
                folded.retain(|&(f, _)| f != m);
            }
        }
        // Back to the surface's frame: turned back, then the block put
        // against its anchor.
        let mut slots: Vec<(usize, PointRect, usize, Orientation)> = Vec::new();
        for (at, pl) in result.placements.iter().enumerate() {
            let Some(r) = pl.points else { continue };
            let (r, o) = if down {
                (
                    PointRect {
                        x: r.y,
                        y: r.x,
                        w: r.h,
                        h: r.w,
                    },
                    turned(pl.orientation),
                )
            } else {
                (r, pl.orientation)
            };
            slots.push((active[at], r, pl.form, o));
        }
        let bw = slots.iter().map(|s| s.1.x + s.1.w).fold(0.0, f32::max);
        let bh = slots.iter().map(|s| s.1.y + s.1.h).fold(0.0, f32::max);
        let (ex, ey) = spans(engine_box);
        let ox = ex.put(bw, ph, p.0);
        let oy = ey.put(bh, pv, p.1);
        let mut block: Option<PointRect> = None;
        for &(m, r, form, orientation) in &slots {
            let x = match ph {
                Place::End => bw - r.x - r.w,
                Place::Mid if surface.rtl => bw - r.x - r.w,
                _ => r.x,
            };
            let y = match pv {
                Place::End => bh - r.y - r.h,
                _ => r.y,
            };
            let slot = PointRect {
                x: ox + x,
                y: oy + y,
                w: r.w,
                h: r.h,
            };
            let inner = inflate(slot, -gap / 2.0);
            let card = &cards[members[m]];
            let shown = match card.size {
                Some([w, h]) => {
                    let (sx, sy) = spans(inner);
                    let w = w.clamp(0.0, sx.len);
                    let h = h.clamp(0.0, sy.len);
                    rect(
                        Span {
                            lo: sx.put(w, ph, sx.at(Place::Mid)),
                            len: w,
                        },
                        Span {
                            lo: sy.put(h, pv, sy.at(Place::Mid)),
                            len: h,
                        },
                    )
                }
                None => inner,
            };
            block = Some(match block {
                None => inner,
                Some(b) => {
                    let x0 = b.x.min(inner.x);
                    let y0 = b.y.min(inner.y);
                    let x1 = (b.x + b.w).max(inner.x + inner.w);
                    let y1 = (b.y + b.h).max(inner.y + inner.h);
                    PointRect {
                        x: x0,
                        y: y0,
                        w: x1 - x0,
                        h: y1 - y0,
                    }
                }
            });
            let pl = &mut placements[members[m]];
            pl.rect = Some(shown);
            pl.slot = Some(slot);
            pl.form = form;
            pl.orientation = orientation;
            pl.status = CardStatus::Placed;
        }
        for (m, reason) in folded {
            let reason = match (down, reason) {
                (true, Reason::TooNarrow) => Reason::TooShort,
                (true, Reason::TooShort) => Reason::TooNarrow,
                (_, r) => r,
            };
            let pl = &mut placements[members[m]];
            pl.status = CardStatus::Folded(reason);
        }
        if let (Some(b), false) = (block, overlay) {
            obstacles.push(b);
        }
        done[i] = Some(RegionPlacement {
            key: region.key,
            anchor: region.anchor,
            side: region.side,
            area: room.then_some(area),
            block,
            layout: result,
        });
    }
    RegionLayout {
        regions: done.into_iter().flatten().collect(),
        cards: placements,
    }
}
