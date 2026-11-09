//! The engine: choosing forms and orientations, dense packing, growth.

use crate::element::{Axis, Element, Grow, Orientation, SizePolicy};
use crate::size::{floor_step, is_allowed, next_step, prev_step, HALF_UNIT, TWELFTHS_PER_UNIT};
use core::fmt;

/// The largest grid extent, in twelfths: ten thousand units.
const MAX_TWELFTHS: u32 = 10_000 * TWELFTHS_PER_UNIT;

/// The box to lay elements out in, in points.
#[derive(Clone, Copy, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Container {
    /// The width in points.
    pub width: f32,
    /// The height in points, or `None` for a container that grows down.
    pub height: Option<f32>,
    /// The base unit in points: one row, scaled by density.
    pub unit: f32,
}

impl Container {
    /// A container of `width` points that grows down, with a unit of `unit`
    /// points.
    pub const fn new(width: f32, unit: f32) -> Container {
        Container {
            width,
            height: None,
            unit,
        }
    }

    /// The same container bounded to `height` points.
    pub const fn with_height(mut self, height: f32) -> Container {
        self.height = Some(height);
        self
    }

    fn twelfths(&self, points: f32) -> u32 {
        if !(self.unit.is_finite() && self.unit > 0.0) {
            return 0;
        }
        let t = points * TWELFTHS_PER_UNIT as f32 / self.unit;
        // `as` saturates and maps NaN to 0: a bad box gives an empty grid.
        (t.floor() as u32).min(MAX_TWELFTHS)
    }
}

/// A rectangle on the grid, in twelfths of the unit.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct GridRect {
    /// Left edge.
    pub x: u32,
    /// Top edge.
    pub y: u32,
    /// Width.
    pub w: u32,
    /// Height.
    pub h: u32,
}

impl GridRect {
    /// The right edge.
    pub fn right(&self) -> u32 {
        self.x + self.w
    }

    /// The bottom edge.
    pub fn bottom(&self) -> u32 {
        self.y + self.h
    }

    /// Whether two rectangles share any area.
    pub fn overlaps(&self, other: &GridRect) -> bool {
        self.x < other.right()
            && other.x < self.right()
            && self.y < other.bottom()
            && other.y < self.bottom()
    }
}

/// A rectangle in points, relative to the container's top left.
#[derive(Clone, Copy, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct PointRect {
    /// Left edge.
    pub x: f32,
    /// Top edge.
    pub y: f32,
    /// Width.
    pub w: f32,
    /// Height.
    pub h: f32,
}

/// Why an element is not placed, or not where it was pinned.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum Reason {
    /// The element declares no form.
    NoForms,
    /// Even its smallest form is wider than the container.
    TooNarrow,
    /// Even its smallest form is taller than the bounded container.
    TooShort,
    /// It fits the container's shape, but the bounded container is full.
    NoRoom,
    /// Its pin puts it outside the container.
    PinOutOfBounds,
    /// Its pin overlaps an element pinned before it.
    PinOverlap,
}

impl fmt::Display for Reason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Reason::NoForms => "no-forms",
            Reason::TooNarrow => "too-narrow",
            Reason::TooShort => "too-short",
            Reason::NoRoom => "no-room",
            Reason::PinOutOfBounds => "pin-out-of-bounds",
            Reason::PinOverlap => "pin-overlap",
        })
    }
}

/// The outcome for one element.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum Status {
    /// Placed, at its pin if it has one.
    Placed,
    /// Placed by the flow because its pin could not be honoured.
    Unpinned(Reason),
    /// Not placed: the caller folds it or shows it elsewhere.
    Overflow(Reason),
}

impl Status {
    /// Whether the element has a rectangle.
    pub fn is_placed(&self) -> bool {
        !matches!(self, Status::Overflow(_))
    }
}

/// The result for one element.
#[derive(Clone, Copy, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Placement {
    /// The element's key.
    pub key: u64,
    /// Its index in the input: the logical order (Tab, accessibility, device
    /// banks).
    pub logical: usize,
    /// Its rank in reading order (top to bottom, then left to right), or
    /// `None` when not placed.
    pub visual: Option<usize>,
    /// The chosen form's index. For an overflow, the last form tried.
    pub form: usize,
    /// The chosen orientation.
    pub orientation: Orientation,
    /// The slot, in twelfths, when placed.
    pub rect: Option<GridRect>,
    /// The slot, in points, when placed.
    pub points: Option<PointRect>,
    /// Placed or overflow, with a reason.
    pub status: Status,
}

/// A laid-out container.
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Layout {
    /// The grid's width in twelfths (its column count).
    pub columns: u32,
    /// The height used, in twelfths.
    pub rows: u32,
    /// The unit, in points, the layout was made with.
    pub unit: f32,
    /// One placement per element, in logical order.
    pub placements: Vec<Placement>,
}

impl Layout {
    /// The placement of the element with this key.
    pub fn get(&self, key: u64) -> Option<&Placement> {
        self.placements.iter().find(|p| p.key == key)
    }

    /// The placed elements in visual order.
    pub fn visual_order(&self) -> Vec<&Placement> {
        let mut placed: Vec<&Placement> = self
            .placements
            .iter()
            .filter(|p| p.visual.is_some())
            .collect();
        placed.sort_by_key(|p| p.visual);
        placed
    }

    /// The height used, in points.
    pub fn height_points(&self) -> f32 {
        self.rows as f32 * self.unit / TWELFTHS_PER_UNIT as f32
    }
}

/// A form and orientation, with the slot size it gets.
#[derive(Clone, Copy)]
struct Choice {
    form: usize,
    orientation: Orientation,
    policy: SizePolicy,
    w: u32,
    h: u32,
}

/// Lays `elements` out in `container`.
///
/// Pass the previous result of the same container as `previous` to keep its
/// choices (column count, forms, orientations) while the box stays within
/// half a unit of a threshold. The result has one placement per element, in
/// input order. This function never panics.
pub fn layout(container: &Container, elements: &[Element], previous: Option<&Layout>) -> Layout {
    let unit = container.unit;
    let natural = container.twelfths(container.width);
    let columns = columns(natural, container.unit, previous);
    let max_rows = container.height.map(|h| container.twelfths(h));

    // 1. Choose a form and an orientation for each element.
    let choices: Vec<Result<Choice, (Reason, usize, Orientation)>> = elements
        .iter()
        .map(|e| {
            // An overflow's choice is kept too: dropping it would make the
            // next layout of the same box differ.
            let prev = previous.and_then(|l| l.get(e.key));
            choose(e, columns, natural, max_rows, prev)
        })
        .collect();

    // 2. Place: pins first, then the flow, densely.
    let mut sizes: Vec<Option<(u32, u32)>> = choices
        .iter()
        .map(|c| c.as_ref().ok().map(|c| (c.w, c.h)))
        .collect();
    let (mut rects, mut status) = place(elements, &sizes, columns, max_rows);

    // 3. Grow into the leftover room, one step per element per round. A step
    // is kept when the dense placement at the new sizes changes no status and
    // stays within the bounded height, or for a container growing down, the
    // height used before growth.
    let rows_of = |rects: &[Option<GridRect>]| {
        rects
            .iter()
            .flatten()
            .map(GridRect::bottom)
            .max()
            .unwrap_or(0)
    };
    let row_limit = max_rows.unwrap_or_else(|| rows_of(&rects));
    loop {
        let mut grew = false;
        for i in 0..elements.len() {
            let (Ok(c), Some(size)) = (&choices[i], sizes[i]) else {
                continue;
            };
            if !status[i].is_placed() {
                continue;
            }
            let Some(next) = grown(&c.policy, size, columns) else {
                continue;
            };
            let mut tried = sizes.clone();
            tried[i] = Some(next);
            let (r, s) = place(elements, &tried, columns, max_rows);
            if s == status && rows_of(&r) <= row_limit {
                (sizes, rects, grew) = (tried, r, true);
            }
        }
        if !grew {
            break;
        }
    }
    for (i, c) in choices.iter().enumerate() {
        if let Err((reason, _, _)) = c {
            status[i] = Status::Overflow(*reason);
        }
    }

    // 4. Visual order and output.
    let mut order: Vec<usize> = (0..elements.len())
        .filter(|&i| rects[i].is_some())
        .collect();
    order.sort_by_key(|&i| rects[i].map(|r| (r.y, r.x, i)));
    let mut visual = vec![None; elements.len()];
    for (rank, &i) in order.iter().enumerate() {
        visual[i] = Some(rank);
    }
    let to_points = |t: u32| t as f32 * unit / TWELFTHS_PER_UNIT as f32;
    let placements = elements
        .iter()
        .enumerate()
        .map(|(i, e)| {
            let (form, orientation) = match &choices[i] {
                Ok(c) => (c.form, c.orientation),
                Err((_, f, o)) => (*f, *o),
            };
            let rect = rects[i];
            Placement {
                key: e.key,
                logical: i,
                visual: visual[i],
                form,
                orientation,
                rect,
                points: rect.map(|r| PointRect {
                    x: to_points(r.x),
                    y: to_points(r.y),
                    w: to_points(r.w),
                    h: to_points(r.h),
                }),
                status: status[i],
            }
        })
        .collect();
    Layout {
        columns,
        rows: rows_of(&rects),
        unit,
        placements,
    }
}

/// The grid width in twelfths. A wider grid than before waits until the box
/// is half a unit wider than the previous one; a narrower one is taken at
/// once, so the grid never exceeds the box.
fn columns(natural: u32, unit: f32, previous: Option<&Layout>) -> u32 {
    match previous {
        Some(p) if p.unit == unit && natural > p.columns => {
            if natural < p.columns.saturating_add(HALF_UNIT) {
                p.columns
            } else {
                natural
            }
        }
        _ => natural,
    }
}

/// The first form and orientation that fits, keeping the previous one unless
/// a better one fits with half a unit to spare in the box's natural width
/// (not the grid's, which may lag behind it).
fn choose(
    e: &Element,
    columns: u32,
    natural: u32,
    max_rows: Option<u32>,
    prev: Option<&Placement>,
) -> Result<Choice, (Reason, usize, Orientation)> {
    let orientations = e.orientation_order();
    let mut candidates = Vec::new();
    for (form, policy) in e.forms.iter().enumerate() {
        for &o in &orientations {
            candidates.push((form, o, policy.resolved(o)));
        }
    }
    let fits = |&(form, orientation, policy): &(usize, Orientation, SizePolicy), width: u32| {
        fit(&policy, width, max_rows).map(|(w, h)| Choice {
            form,
            orientation,
            policy,
            w,
            h,
        })
    };
    let kept = prev.and_then(|p| {
        let at = candidates
            .iter()
            .position(|c| c.0 == p.form && c.1 == p.orientation)?;
        fits(&candidates[at], columns).ok().map(|c| (at, c))
    });
    if let Some((at, kept)) = kept {
        let margin = natural.saturating_sub(HALF_UNIT);
        // A better choice must fit with half a unit to spare; its slot is
        // still sized at the real width.
        let better = candidates[..at]
            .iter()
            .filter(|c| fits(c, margin.min(columns)).is_ok())
            .find_map(|c| fits(c, columns).ok());
        return Ok(better.unwrap_or(kept));
    }
    // Overflow reports the last form, in its first orientation.
    let mut last = (Reason::NoForms, 0, orientations[0]);
    for c in &candidates {
        match fits(c, columns) {
            Ok(choice) => return Ok(choice),
            Err(reason) => last = (reason, c.0, orientations[0]),
        }
    }
    Err(last)
}

/// The slot an oriented policy gets in a grid `width` twelfths wide: the
/// largest allowed size up to its preferred size that fits.
fn fit(p: &SizePolicy, width: u32, max_rows: Option<u32>) -> Result<(u32, u32), Reason> {
    let (min_w, min_h) = (p.min_size.width.twelfths(), p.min_size.height.twelfths());
    if min_w > width {
        return Err(Reason::TooNarrow);
    }
    if max_rows.is_some_and(|m| min_h > m) {
        return Err(Reason::TooShort);
    }
    let mut w = floor_step(p.size.width.twelfths().min(width)).ok_or(Reason::TooNarrow)?;
    while w >= min_w {
        if let Some(h) = height_for(p, w, max_rows) {
            return Ok((w, h));
        }
        w = match prev_step(w) {
            Some(w) => w,
            None => break,
        };
    }
    Err(if p.aspect_ratio.is_some() {
        Reason::TooShort
    } else {
        Reason::TooNarrow
    })
}

/// The height an oriented policy takes at width `w`.
fn height_for(p: &SizePolicy, w: u32, max_rows: Option<u32>) -> Option<u32> {
    let (min_h, max_h) = (p.min_size.height.twelfths(), p.max_size.height.twelfths());
    let cap = max_rows.unwrap_or(u32::MAX);
    match p.aspect_ratio {
        Some(r) => {
            let num = w.checked_mul(r.height)?;
            let h = num / r.width;
            let ok =
                num % r.width == 0 && is_allowed(h) && (min_h..=max_h).contains(&h) && h <= cap;
            ok.then_some(h)
        }
        None => floor_step(p.size.height.twelfths().min(cap)).filter(|&h| h >= min_h),
    }
}

/// Places slots of the given sizes: pinned ones at their pins, in logical
/// order, then the others at the first free position in reading order.
/// Elements without a size are left out with a placeholder status, which
/// the caller replaces.
fn place(
    elements: &[Element],
    sizes: &[Option<(u32, u32)>],
    columns: u32,
    max_rows: Option<u32>,
) -> (Vec<Option<GridRect>>, Vec<Status>) {
    let mut rects: Vec<Option<GridRect>> = vec![None; elements.len()];
    let mut status = vec![Status::Placed; elements.len()];
    let mut taken: Vec<GridRect> = Vec::new();
    for (i, e) in elements.iter().enumerate() {
        let (Some((w, h)), Some(pin)) = (sizes[i], e.pin) else {
            continue;
        };
        let r = GridRect {
            x: pin.col,
            y: pin.row,
            w,
            h,
        };
        let inside = r.x.checked_add(w).is_some_and(|right| right <= columns)
            && r.y
                .checked_add(h)
                .is_some_and(|b| max_rows.is_none_or(|m| b <= m));
        if !inside {
            status[i] = Status::Unpinned(Reason::PinOutOfBounds);
        } else if taken.iter().any(|t| t.overlaps(&r)) {
            status[i] = Status::Unpinned(Reason::PinOverlap);
        } else {
            rects[i] = Some(r);
            taken.push(r);
        }
    }
    for i in 0..elements.len() {
        let Some((w, h)) = sizes[i] else {
            status[i] = Status::Overflow(Reason::NoForms);
            continue;
        };
        if rects[i].is_some() {
            continue;
        }
        match first_fit(&taken, w, h, columns, max_rows) {
            Some(r) => {
                rects[i] = Some(r);
                taken.push(r);
            }
            None => status[i] = Status::Overflow(Reason::NoRoom),
        }
    }
    (rects, status)
}

/// The size one growth step larger, if the policy allows it and it is no
/// wider than the grid.
fn grown(p: &SizePolicy, (w, h): (u32, u32), columns: u32) -> Option<(u32, u32)> {
    let Grow::Along(axis) = p.grow else {
        return None;
    };
    let (mut w, mut h) = (w, h);
    match axis {
        Axis::Width => w = next_step(w)?,
        Axis::Height => h = next_step(h)?,
    }
    if let Some(ratio) = p.aspect_ratio {
        match axis {
            Axis::Width => {
                let num = w.checked_mul(ratio.height)?;
                (num % ratio.width == 0).then_some(())?;
                h = num / ratio.width;
            }
            Axis::Height => {
                let num = h.checked_mul(ratio.width)?;
                (num % ratio.height == 0).then_some(())?;
                w = num / ratio.height;
            }
        }
        if !is_allowed(w) || !is_allowed(h) {
            return None;
        }
    }
    let within = w <= p.max_size.width.twelfths() && h <= p.max_size.height.twelfths();
    (within && w <= columns).then_some((w, h))
}

/// The first free position in reading order for a `w` by `h` slot.
///
/// The top-most, then left-most, free position always has its top at 0 or at
/// a slot's bottom, and its left at 0 or at a slot's right, so only those are
/// tried.
fn first_fit(
    taken: &[GridRect],
    w: u32,
    h: u32,
    columns: u32,
    max_rows: Option<u32>,
) -> Option<GridRect> {
    if w > columns {
        return None;
    }
    let mut ys: Vec<u32> = taken.iter().map(GridRect::bottom).chain([0]).collect();
    let mut xs: Vec<u32> = taken.iter().map(GridRect::right).chain([0]).collect();
    ys.sort_unstable();
    ys.dedup();
    xs.sort_unstable();
    xs.dedup();
    for &y in &ys {
        if max_rows.is_some_and(|m| y.saturating_add(h) > m) {
            break;
        }
        for &x in &xs {
            if x + w > columns {
                break;
            }
            let r = GridRect { x, y, w, h };
            if !taken.iter().any(|t| t.overlaps(&r)) {
                return Some(r);
            }
        }
    }
    None
}
