//! The arithmetic of a stack of collapsible sections: how heights share
//! a panel, and what a drag on a boundary between two sections does.
//!
//! **Deprecated:** it moved to egui_stack (`egui_stack::sections::plan`,
//! re-exported as `egui_pop_core::sections::plan`), with the section
//! widget; this copy is kept, unchanged and unused, for one release. It is
//! not a re-export, as this crate stays free of egui (ADR 0001) and
//! egui_stack depends on it. Its tests are egui_stack's. Its types are not egui_stack's: a
//! [`DragMode`] read from `egui_pop_core::sections::laid_out` is
//! `egui_pop_core::sections::DragMode`.
//! Heights are in points, a section's body alone (its title band is the
//! caller's); folded sections take no height and are skipped.
//!
//! - [`fit`]: the heights a stack shows in the room it has. A height set
//!   by hand is kept while it fits; the sections never set by hand share
//!   the rest equally, and the last open section takes what is left, less
//!   any empty room kept under it.
//! - [`plan_drag`]: what moving a boundary does, from the heights shown.
//!   Nothing gives way in proportion: sections give in turn, the nearest
//!   first, each down to its least height, as a code editor's split view
//!   does. [`DragMode::Pull`] moves the sections below as a block instead,
//!   opening empty room under the last.
//! - [`resized_above`]: the section a boundary resizes: under a folded
//!   section, the first open one above it.
//! - [`pin_plan`]: the heights to keep by hand so that [`fit`] shows a
//!   plan, the sections it did not touch staying sized by the list.
//! - [`keep_title`]: what folding or opening a section keeps so that its
//!   title stays where it is ([`held_above`]: the sections that must not
//!   move).
//! - [`rows_least`]: the least height of a section of rows.
//!
//! ```
//! # #![allow(deprecated)]
//! use egui_pop_model::sections::{DragMode, Placed, plan_drag};
//!
//! let section = |height| Placed { open: true, least: 50.0, height };
//! let stack = [section(100.0), section(100.0), section(100.0)];
//! // Down, the section just below gives first, down to its least height;
//! // the last gives only then.
//! let plan = plan_drag(&stack, 0, 70.0, 0.0, DragMode::Push);
//! assert_eq!(plan.heights, [170.0, 50.0, 80.0]);
//! assert_eq!(plan.moved, 70.0);
//! ```

use serde::Serialize;

/// A difference between two heights too small to see.
const SAME: f32 = 0.01;

/// A section as laid out: what a drag starts from.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Placed {
    /// Whether it is open; a folded section is skipped.
    pub open: bool,
    /// The height it never goes under.
    pub least: f32,
    /// The height it has.
    pub height: f32,
}

/// How a drag moves the sections around a boundary.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
#[non_exhaustive]
pub enum DragMode {
    /// Down, the sections below give way, the nearest first, then the
    /// last is pushed into the empty room under it, then it gives way.
    /// Up, the section above gives way, then those above it, the nearest
    /// first, and the first open section below takes all the room.
    #[default]
    Push,
    /// The sections below follow as a block, keeping their heights: only
    /// the section above changes. Up, it gives way, then those above it,
    /// the nearest first, and empty room opens under the last. Down, the
    /// block goes into the empty room, then the last section alone gives
    /// way, and the drag stops.
    Pull,
}

/// What a drag does ([`plan_drag`]).
#[derive(Clone, Debug, PartialEq)]
pub struct DragPlan {
    /// Every section's height after it, in the order given.
    pub heights: Vec<f32>,
    /// How far the boundary moved: down if positive. Less than asked
    /// when the stack stopped it; 0 when nothing could move.
    pub moved: f32,
    /// The empty room left under the last section.
    pub empty: f32,
}

/// The section a boundary resizes, the one on the line under the section
/// `line`: `line` itself when open, else the first open one above it, past
/// any folded ones (they have no height to change); `None` when nothing
/// above is open, and the boundary cannot move. `open`: whether each
/// section is, top first.
///
/// Folded bands between do not change the distance: they follow the
/// section, so the line stays under the pointer in both directions.
pub fn resized_above(open: &[bool], line: usize) -> Option<usize> {
    (0..=line).rev().find(|&i| open.get(i) == Some(&true))
}

/// What moving the boundary under the section `above` by `delta` does
/// (down if positive), from the heights in `placed`; `empty` is the room
/// under the last section.
///
/// A drag plans each move from the heights shown, by the distance between
/// the boundary and the pointer, so that it stays under the pointer: what a
/// drag shrank stays shrunk, and a move back gives the room where a fresh
/// drag would. Plans compose: a move of `a` then of `b` the same way is a
/// move of `a + b`. When `above` is not an open section or no section
/// lies below it, no boundary is there: nothing moves and the
/// plan says so (`moved` is 0). With only folded sections below it, nothing
/// else can give or take: down, it takes the empty room, and up, it opens
/// some, the folded bands following.
pub fn plan_drag(
    placed: &[Placed],
    above: usize,
    delta: f32,
    empty: f32,
    mode: DragMode,
) -> DragPlan {
    let mut plan = DragPlan {
        heights: placed.iter().map(|p| p.height).collect(),
        moved: 0.0,
        empty: empty.max(0.0),
    };
    let above_open = placed.get(above).is_some_and(|p| p.open);
    if !above_open || above + 1 >= placed.len() || !delta.is_finite() {
        return plan;
    }
    let below: Vec<usize> = (above + 1..placed.len())
        .filter(|&i| placed[i].open)
        .collect();
    match mode {
        DragMode::Push if delta > 0.0 => push_down(placed, above, &below, delta, &mut plan),
        DragMode::Push if delta < 0.0 => push_up(placed, above, &below, -delta, &mut plan),
        DragMode::Pull if delta > 0.0 => pull_down(placed, above, &below, delta, &mut plan),
        DragMode::Pull if delta < 0.0 => pull_up(placed, above, -delta, &mut plan),
        DragMode::Push | DragMode::Pull => {}
    }
    plan
}

/// Takes up to `owed` from a section, down to its least height; says how
/// much it gave.
fn give(height: &mut f32, least: f32, owed: f32) -> f32 {
    let given = (*height - least).max(0.0).min(owed);
    *height -= given;
    given
}

fn push_down(placed: &[Placed], above: usize, below: &[usize], asked: f32, plan: &mut DragPlan) {
    let mut owed = asked;
    let (last, between) = match below.split_last() {
        Some((&last, between)) => (Some(last), between),
        None => (None, below),
    };
    for &i in between {
        owed -= give(&mut plan.heights[i], placed[i].least, owed);
    }
    let into_empty = owed.min(plan.empty);
    plan.empty -= into_empty;
    owed -= into_empty;
    if let Some(last) = last {
        owed -= give(&mut plan.heights[last], placed[last].least, owed);
    }
    plan.moved = asked - owed;
    plan.heights[above] += plan.moved;
}

fn push_up(placed: &[Placed], above: usize, below: &[usize], asked: f32, plan: &mut DragPlan) {
    let given = give_above(placed, above, asked, plan);
    plan.moved = -given;
    match below.first() {
        Some(&next) => plan.heights[next] += given,
        None => plan.empty += given,
    }
}

fn pull_down(placed: &[Placed], above: usize, below: &[usize], asked: f32, plan: &mut DragPlan) {
    let into_empty = asked.min(plan.empty);
    plan.empty -= into_empty;
    let mut owed = asked - into_empty;
    if let Some(&last) = below.last() {
        owed -= give(&mut plan.heights[last], placed[last].least, owed);
    }
    plan.moved = asked - owed;
    plan.heights[above] += plan.moved;
}

fn pull_up(placed: &[Placed], above: usize, asked: f32, plan: &mut DragPlan) {
    let given = give_above(placed, above, asked, plan);
    plan.moved = -given;
    plan.empty += given;
}

/// Takes up to `asked` from the section `above`, then those above it, the
/// nearest first; says how much they gave.
fn give_above(placed: &[Placed], above: usize, asked: f32, plan: &mut DragPlan) -> f32 {
    let mut owed = asked;
    for i in (0..=above).rev().filter(|&i| placed[i].open) {
        owed -= give(&mut plan.heights[i], placed[i].least, owed);
    }
    asked - owed
}

/// The open sections above `section`, with their shown heights: what must
/// not move for its title to stay where it is.
pub fn held_above(placed: &[Placed], section: usize) -> Vec<(usize, f32)> {
    if section >= placed.len() {
        return Vec::new();
    }
    placed[..section]
        .iter()
        .enumerate()
        .filter(|(_, p)| p.open)
        .map(|(i, p)| (i, p.height))
        .collect()
}

/// What to keep so that folding or opening a section leaves its title where
/// it is ([`keep_title`]).
#[derive(Clone, Debug, PartialEq)]
pub struct Kept {
    /// Every section's height to keep by hand, in the order given.
    pub hands: Vec<Option<f32>>,
    /// The empty room to keep under the last section.
    pub empty: f32,
}

/// What folding or opening the section `section` keeps, from the `heights`
/// shown and the `empty` room under them, so that its title does not move.
///
/// The open sections above keep their shown height, as if set by hand. Those
/// below move: folding, they take the room it frees; opening, they give it
/// its room, and the empty room gives only when they are at their least
/// ([`fit`]). With no open section below, the room stays empty when folding;
/// opening takes it back, up to the section's own height when it has one.
pub fn keep_title(asked: &[Asked], heights: &[f32], empty: f32, section: usize) -> Kept {
    let placed: Vec<Placed> = asked
        .iter()
        .zip(heights)
        .map(|(a, &height)| Placed {
            open: a.open,
            least: a.least,
            height,
        })
        .collect();
    let mut hands: Vec<Option<f32>> = asked.iter().map(|a| a.hand).collect();
    for (i, height) in held_above(&placed, section) {
        hands[i] = Some(height);
    }
    let empty = match placed.get(section) {
        Some(_) if asked[section + 1..].iter().any(|a| a.open) => empty,
        Some(this) if this.open => empty + this.height,
        Some(this) => match asked[section].hand {
            Some(own) => (empty - own.max(this.least)).max(0.0),
            None => 0.0,
        },
        None => empty,
    };
    Kept { hands, empty }
}

/// The least height of a section of `count` rows, each `row` tall: two
/// rows, or all of them when it has fewer.
pub fn rows_least(row: f32, count: usize) -> f32 {
    count.min(2) as f32 * row
}

/// A section as a stack asks for it: what [`fit`] lays out.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Asked {
    /// Whether it is open; a folded section takes no height.
    pub open: bool,
    /// The height it never goes under.
    pub least: f32,
    /// The height set by hand, if any. The last open section ignores
    /// its own: it takes what the others leave.
    pub hand: Option<f32>,
}

/// The heights a stack shows ([`fit`]).
#[derive(Clone, Debug, PartialEq)]
pub struct Fit {
    /// Every section's height, in the order given (0 when folded).
    pub heights: Vec<f32>,
    /// Whether each section's height is its own, set by hand.
    pub by_hand: Vec<bool>,
    /// What the least heights lack to fit in the room: the stack scrolls
    /// by this much. 0 when they fit.
    pub overflow: f32,
    /// The empty room shown under the last section: the room left when no
    /// section is open.
    pub empty: f32,
}

/// Lays sections out in `room` points, keeping `empty` points of it under
/// the last section.
///
/// Heights set by hand are kept and the others (the last open section
/// among them) share the rest equally, each at least its least height.
/// In a panel too short for that, the sections sized by the list are
/// already at their least, so those set by hand give way, from the
/// bottom up. When even the least heights do not fit, each has its least
/// and [`Fit::overflow`] says by how much the stack is too tall. The empty
/// room gives way after the sections sized by the list and before those set
/// by hand; it comes back with the room, as they do.
pub fn fit(asked: &[Asked], room: f32, empty: f32) -> Fit {
    let room = if room.is_finite() { room.max(0.0) } else { 0.0 };
    let empty = if empty.is_finite() {
        empty.max(0.0)
    } else {
        0.0
    };
    let open: Vec<usize> = (0..asked.len()).filter(|&i| asked[i].open).collect();
    let last = open.last().copied();
    let mut heights = vec![0.0; asked.len()];
    let mut by_hand = vec![false; asked.len()];
    let (mut own, mut listed) = (Vec::new(), Vec::new());
    for &i in &open {
        match asked[i].hand {
            Some(height) if Some(i) != last => {
                heights[i] = height.max(asked[i].least);
                by_hand[i] = true;
                own.push(i);
            }
            _ => listed.push(i),
        }
    }

    if open.is_empty() {
        return Fit {
            heights,
            by_hand,
            overflow: 0.0,
            empty: room,
        };
    }

    // What the sections need comes before the empty room.
    let needed: f32 = own.iter().map(|&i| heights[i]).sum::<f32>()
        + listed.iter().map(|&i| asked[i].least).sum::<f32>();
    let empty = empty.min((room - needed).max(0.0));
    let room = room - empty;

    let least: f32 = open.iter().map(|&i| asked[i].least).sum();
    if least >= room {
        for &i in &open {
            heights[i] = asked[i].least;
        }
        return Fit {
            heights,
            by_hand,
            overflow: least - room,
            empty,
        };
    }

    // What the list needs at least comes first: those set by hand give
    // way to it, from the bottom up.
    let listed_least: f32 = listed.iter().map(|&i| asked[i].least).sum();
    let kept: f32 = own.iter().map(|&i| heights[i]).sum();
    let mut owed = kept + listed_least - room;
    for &i in own.iter().rev() {
        if owed <= 0.0 {
            break;
        }
        owed -= give(&mut heights[i], asked[i].least, owed);
    }
    let kept: f32 = own.iter().map(|&i| heights[i]).sum();
    share_out(asked, &listed, room - kept, &mut heights);
    Fit {
        heights,
        by_hand,
        overflow: 0.0,
        empty,
    }
}

/// Divides `room` equally between sections, each at least its least
/// height (`room` holds them all).
fn share_out(asked: &[Asked], sections: &[usize], room: f32, heights: &mut [f32]) {
    let mut left = sections.to_vec();
    let mut room = room;
    loop {
        let each = room / left.len().max(1) as f32;
        let (pinned, free): (Vec<usize>, Vec<usize>) =
            left.iter().partition(|&&i| asked[i].least > each);
        if pinned.is_empty() {
            for i in free {
                heights[i] = each;
            }
            return;
        }
        for i in pinned {
            heights[i] = asked[i].least;
            room -= asked[i].least;
        }
        left = free;
    }
}

/// The heights to keep by hand, section by section, so that [`fit`]
/// shows `plan` in `room`, with `empty` kept under the last section.
///
/// A section set by hand takes its height from the plan, which aligns it
/// on what is shown. One sized by the list stays so as long as [`fit`]
/// gives it the plan's height: a section the drag did not touch gets a
/// height of its own only if the list would otherwise move it. The last
/// open section and folded ones keep what they had.
pub fn pin_plan(asked: &[Asked], room: f32, empty: f32, plan: &[f32]) -> Vec<Option<f32>> {
    let mut asked = asked.to_vec();
    let last = (0..asked.len()).rev().find(|&i| asked[i].open);
    let pinnable: Vec<usize> = (0..asked.len().min(plan.len()))
        .filter(|&i| asked[i].open && Some(i) != last)
        .collect();
    for &i in &pinnable {
        if asked[i].hand.is_some() {
            asked[i].hand = Some(plan[i]);
        }
    }
    loop {
        let shown = fit(&asked, room, empty).heights;
        let mut changed = false;
        for &i in &pinnable {
            if asked[i].hand.is_none() && (shown[i] - plan[i]).abs() > SAME {
                asked[i].hand = Some(plan[i]);
                changed = true;
            }
        }
        if !changed {
            return asked.iter().map(|a| a.hand).collect();
        }
    }
}
