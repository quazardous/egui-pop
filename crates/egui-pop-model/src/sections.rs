//! The arithmetic of a stack of collapsible sections: how heights share
//! a panel, and what a drag on a boundary between two sections does.
//! Heights are in points, a section's body alone (its title band is the
//! caller's); folded sections take no height and are skipped.
//!
//! - [`fit`]: the heights a stack shows in the room it has. A height set
//!   by hand is kept while it fits; the sections never set by hand share
//!   the rest equally, and the last open section takes what is left.
//! - [`plan_drag`]: what moving a boundary does, from the heights shown.
//!   Nothing gives way in proportion: sections give in turn, the nearest
//!   first, each down to its least height, as a code editor's split view
//!   does.
//! - [`pin_plan`]: the heights to keep by hand so that [`fit`] shows a
//!   plan, the sections it did not touch staying sized by the list.
//!
//! ```
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
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub enum DragMode {
    /// Down, the sections below give way, the nearest first, then the
    /// last is pushed into the empty room under it, then it gives way.
    /// Up, the section above gives way, then those above it, the nearest
    /// first, and the first open section below takes all the room.
    #[default]
    Push,
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

/// What moving the boundary under the section `above` by `delta` does
/// (down if positive), from the heights in `placed`; `empty` is the room
/// under the last section.
///
/// A plan is always computed from the same starting heights during a
/// drag, with the whole distance from where it began, so a boundary
/// follows the pointer without drifting and gives back what it took when
/// the pointer returns. When `above` is not an open section or no open
/// section lies below it, no boundary is there: nothing moves and the
/// plan says so (`moved` is 0).
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
    let below: Vec<usize> = (above + 1..placed.len())
        .filter(|&i| placed[i].open)
        .collect();
    let above_open = placed.get(above).is_some_and(|p| p.open);
    if !above_open || below.is_empty() || !delta.is_finite() {
        return plan;
    }
    match mode {
        DragMode::Push if delta > 0.0 => push_down(placed, above, &below, delta, &mut plan),
        DragMode::Push if delta < 0.0 => push_up(placed, above, &below, -delta, &mut plan),
        DragMode::Push => {}
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
    let Some((&last, between)) = below.split_last() else {
        return;
    };
    for &i in between {
        owed -= give(&mut plan.heights[i], placed[i].least, owed);
    }
    let into_empty = owed.min(plan.empty);
    plan.empty -= into_empty;
    owed -= into_empty;
    owed -= give(&mut plan.heights[last], placed[last].least, owed);
    plan.moved = asked - owed;
    plan.heights[above] += plan.moved;
}

fn push_up(placed: &[Placed], above: usize, below: &[usize], asked: f32, plan: &mut DragPlan) {
    let mut owed = asked;
    for i in (0..=above).rev().filter(|&i| placed[i].open) {
        owed -= give(&mut plan.heights[i], placed[i].least, owed);
    }
    let given = asked - owed;
    plan.moved = -given;
    plan.heights[below[0]] += given;
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
}

/// Lays sections out in `room` points.
///
/// Heights set by hand are kept and the others (the last open section
/// among them) share the rest equally, each at least its least height.
/// In a panel too short for that, the sections sized by the list are
/// already at their least, so those set by hand give way, from the
/// bottom up. When even the least heights do not fit, each has its least
/// and [`Fit::overflow`] says by how much the stack is too tall.
pub fn fit(asked: &[Asked], room: f32) -> Fit {
    let room = if room.is_finite() { room.max(0.0) } else { 0.0 };
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

    let least: f32 = open.iter().map(|&i| asked[i].least).sum();
    if least >= room {
        for &i in &open {
            heights[i] = asked[i].least;
        }
        return Fit {
            heights,
            by_hand,
            overflow: least - room,
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
/// shows `plan` in `room`.
///
/// A section set by hand takes its height from the plan, which aligns it
/// on what is shown. One sized by the list stays so as long as [`fit`]
/// gives it the plan's height: a section the drag did not touch gets a
/// height of its own only if the list would otherwise move it. The last
/// open section and folded ones keep what they had.
pub fn pin_plan(asked: &[Asked], room: f32, plan: &[f32]) -> Vec<Option<f32>> {
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
        let shown = fit(&asked, room).heights;
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

#[cfg(test)]
mod tests {
    use super::*;

    fn at(least: f32, height: f32) -> Placed {
        Placed {
            open: true,
            least,
            height,
        }
    }

    fn folded() -> Placed {
        Placed {
            open: false,
            least: 0.0,
            height: 0.0,
        }
    }

    fn asked(least: f32, hand: Option<f32>) -> Asked {
        Asked {
            open: true,
            least,
            hand,
        }
    }

    /// Drags from `from` to the height `wanted` of the section `above`.
    fn drag(stack: &[Placed], above: usize, wanted: f32, empty: f32) -> DragPlan {
        let delta = wanted - stack[above].height;
        plan_drag(stack, above, delta, empty, DragMode::Push)
    }

    #[test]
    fn a_drag_down_shrinks_those_below_nearest_first_then_the_last_to_a_hard_stop() {
        let stack = [at(56.0, 300.0), at(56.0, 200.0), at(28.0, 128.0)];
        // The one just below gives first, down to its least (144 to give).
        assert_eq!(drag(&stack, 0, 400.0, 0.0).heights, [400.0, 100.0, 128.0]);
        // Then the last (100 more).
        assert_eq!(drag(&stack, 0, 520.0, 0.0).heights, [520.0, 56.0, 52.0]);
        // And no further: the whole stack is at its least.
        let stop = drag(&stack, 0, 900.0, 0.0);
        assert_eq!(stop.heights, [544.0, 56.0, 28.0]);
        assert_eq!(stop.moved, 244.0);
    }

    #[test]
    fn the_empty_room_is_taken_after_the_sections_between_and_before_the_last() {
        let stack = [at(56.0, 300.0), at(56.0, 200.0), at(28.0, 128.0)];
        // The one just below gives before the last moves into the room.
        let plan = drag(&stack, 0, 400.0, 150.0);
        assert_eq!(plan.heights, [400.0, 100.0, 128.0]);
        assert_eq!(plan.empty, 150.0);
        // Then the room is taken, then the last gives.
        let plan = drag(&stack, 0, 600.0, 150.0);
        assert_eq!(plan.heights, [600.0, 56.0, 122.0]);
        assert_eq!(plan.empty, 0.0);
    }

    #[test]
    fn a_drag_up_shrinks_those_above_and_only_the_first_below_grows() {
        let stack = [at(56.0, 300.0), at(56.0, 200.0), at(28.0, 128.0)];
        assert_eq!(drag(&stack, 0, 250.0, 0.0).heights, [250.0, 250.0, 128.0]);
        // Down to its own least, the one just below growing by as much.
        let stop = drag(&stack, 0, 10.0, 0.0);
        assert_eq!(stop.heights, [56.0, 444.0, 128.0]);
        assert_eq!(stop.moved, -244.0);
        // Past its least, the sections above give, the nearest first.
        let stack = [
            at(56.0, 200.0),
            at(56.0, 100.0),
            at(56.0, 56.0),
            at(56.0, 200.0),
            at(28.0, 128.0),
        ];
        let plan = plan_drag(&stack, 2, -156.0, 0.0, DragMode::Push);
        assert_eq!(plan.heights, [88.0, 56.0, 56.0, 356.0, 128.0]);
        assert_eq!(plan.moved, -156.0);
    }

    #[test]
    fn folded_sections_are_skipped() {
        // Up: the next open one grows.
        let stack = [at(56.0, 300.0), folded(), at(28.0, 128.0)];
        assert_eq!(drag(&stack, 0, 250.0, 0.0).heights, [250.0, 0.0, 178.0]);
        // Down: a folded one gives nothing.
        assert_eq!(drag(&stack, 0, 350.0, 0.0).heights, [350.0, 0.0, 78.0]);
        // Up, past the least, through a folded one above.
        let stack = [at(10.0, 60.0), folded(), at(20.0, 20.0), at(20.0, 20.0)];
        let plan = plan_drag(&stack, 2, -30.0, 0.0, DragMode::Push);
        assert_eq!(plan.heights, [30.0, 0.0, 20.0, 50.0]);
    }

    #[test]
    fn a_plan_comes_back_when_the_pointer_does() {
        let stack = [at(56.0, 300.0), at(56.0, 200.0), at(28.0, 128.0)];
        let there = drag(&stack, 0, 700.0, 0.0);
        assert_eq!(there.heights[0], 544.0);
        // From the same start, the distance is all that counts.
        assert_eq!(drag(&stack, 0, 300.0, 0.0).heights, [300.0, 200.0, 128.0]);
    }

    #[test]
    fn where_no_boundary_is_nothing_moves() {
        let stack = [at(10.0, 50.0), folded(), at(10.0, 50.0)];
        for (above, delta) in [(2, 10.0), (1, 10.0), (9, 10.0), (0, 0.0), (0, f32::NAN)] {
            let plan = plan_drag(&stack, above, delta, 5.0, DragMode::Push);
            assert_eq!(plan.heights, [50.0, 0.0, 50.0], "{above} {delta}");
            assert_eq!(plan.moved, 0.0);
            assert_eq!(plan.empty, 5.0);
        }
    }

    #[test]
    fn a_stack_at_its_least_stops_both_ways_when_no_section_can_give() {
        let stack = [at(40.0, 40.0), at(40.0, 40.0)];
        assert_eq!(drag(&stack, 0, 90.0, 0.0).moved, 0.0);
        assert_eq!(drag(&stack, 0, 10.0, 0.0).moved, 0.0);
    }

    #[test]
    fn sections_never_set_by_hand_share_the_rest_equally() {
        let f = fit(
            &[asked(10.0, None), asked(10.0, None), asked(10.0, None)],
            300.0,
        );
        assert_eq!(f.heights, [100.0, 100.0, 100.0]);
        assert_eq!(f.by_hand, [false, false, false]);
        assert_eq!(f.overflow, 0.0);
        // One set by hand: the others share what it leaves.
        let f = fit(
            &[
                asked(10.0, Some(200.0)),
                asked(10.0, None),
                asked(10.0, None),
            ],
            300.0,
        );
        assert_eq!(f.heights, [200.0, 50.0, 50.0]);
        assert_eq!(f.by_hand, [true, false, false]);
        // A least height holds, the others share the rest.
        let f = fit(&[asked(80.0, None), asked(10.0, None)], 100.0);
        assert_eq!(f.heights, [80.0, 20.0]);
    }

    #[test]
    fn the_last_open_section_takes_what_is_left_and_never_its_own() {
        let f = fit(&[asked(10.0, Some(50.0)), asked(10.0, Some(70.0))], 300.0);
        assert_eq!(f.heights, [50.0, 250.0]);
        assert_eq!(f.by_hand, [true, false]);
        // A folded one below does not count: the one above it is last.
        let stack = [
            asked(10.0, Some(50.0)),
            asked(10.0, Some(70.0)),
            Asked {
                open: false,
                least: 10.0,
                hand: Some(90.0),
            },
        ];
        let f = fit(&stack, 300.0);
        assert_eq!(f.heights, [50.0, 250.0, 0.0]);
        assert_eq!(f.by_hand, [true, false, false]);
    }

    #[test]
    fn in_a_short_panel_the_list_gives_first_then_those_set_by_hand_from_the_bottom() {
        let stack = [
            asked(20.0, Some(100.0)),
            asked(20.0, Some(100.0)),
            asked(20.0, None),
            asked(20.0, None),
        ];
        // Room for all: the list has the rest.
        assert_eq!(fit(&stack, 300.0).heights, [100.0, 100.0, 50.0, 50.0]);
        // The list at its least first, those set by hand untouched.
        assert_eq!(fit(&stack, 240.0).heights, [100.0, 100.0, 20.0, 20.0]);
        // Then the lowest set by hand, then the one above it.
        assert_eq!(fit(&stack, 200.0).heights, [100.0, 60.0, 20.0, 20.0]);
        assert_eq!(fit(&stack, 90.0).heights, [30.0, 20.0, 20.0, 20.0]);
        // Then even the least heights do not fit: the stack overflows.
        let f = fit(&stack, 50.0);
        assert_eq!(f.heights, [20.0, 20.0, 20.0, 20.0]);
        assert_eq!(f.overflow, 30.0);
        assert_eq!(f.by_hand, [true, true, false, false]);
    }

    #[test]
    fn a_height_set_under_the_least_is_shown_at_the_least() {
        let f = fit(&[asked(40.0, Some(1.0)), asked(10.0, None)], 100.0);
        assert_eq!(f.heights, [40.0, 60.0]);
    }

    #[test]
    fn nothing_open_takes_no_room_and_no_room_means_the_least_heights() {
        let folded = Asked {
            open: false,
            least: 10.0,
            hand: None,
        };
        let f = fit(&[folded, folded], 100.0);
        assert_eq!(f.heights, [0.0, 0.0]);
        assert_eq!(f.overflow, 0.0);
        assert!(fit(&[], 100.0).heights.is_empty());
        let f = fit(&[asked(10.0, None)], f32::INFINITY);
        assert_eq!(f.heights, [10.0]);
    }

    #[test]
    fn pinning_a_plan_keeps_the_list_where_it_can() {
        let room = 300.0;
        // The boundary between the first two: the third, last, stays.
        let stack = [asked(10.0, None), asked(10.0, None), asked(10.0, None)];
        assert_eq!(
            pin_plan(&stack, room, &[120.0, 80.0, 100.0]),
            [Some(120.0), Some(80.0), None]
        );
        // The last gives: the first would grow or shrink with the list,
        // so it gets its height too, and nothing moves in proportion.
        let hands = pin_plan(&stack, room, &[100.0, 120.0, 80.0]);
        assert_eq!(hands, [Some(100.0), Some(120.0), None]);
        // With four, one in the list not touched stays in the list.
        let stack = [
            asked(10.0, None),
            asked(10.0, None),
            asked(10.0, None),
            asked(10.0, None),
        ];
        let hands = pin_plan(&stack, 400.0, &[120.0, 80.0, 100.0, 100.0]);
        assert_eq!(hands, [Some(120.0), Some(80.0), None, None]);
    }

    #[test]
    fn a_pinned_plan_is_what_fit_shows() {
        let cases: [(&[Asked], f32, &[f32]); 3] = [
            (
                &[asked(10.0, None), asked(10.0, None), asked(10.0, None)],
                300.0,
                &[100.0, 120.0, 80.0],
            ),
            // One at its least in the list: pinning another must not free it.
            (
                &[asked(90.0, None), asked(10.0, None), asked(10.0, None)],
                210.0,
                &[90.0, 40.0, 80.0],
            ),
            (
                &[asked(10.0, Some(50.0)), asked(10.0, None)],
                100.0,
                &[70.0, 30.0],
            ),
        ];
        for (stack, room, plan) in cases {
            let hands = pin_plan(stack, room, plan);
            let pinned: Vec<Asked> = stack
                .iter()
                .zip(hands)
                .map(|(a, hand)| Asked { hand, ..*a })
                .collect();
            assert_eq!(fit(&pinned, room).heights, plan, "{stack:?}");
        }
    }

    #[test]
    fn pinning_aligns_stored_heights_on_what_is_shown_and_keeps_the_others() {
        // Set by hand at 100, shown at 60 in a short panel: the next drag
        // starts from 60.
        let stack = [
            asked(20.0, Some(100.0)),
            asked(20.0, Some(100.0)),
            asked(20.0, None),
        ];
        let shown = fit(&stack, 140.0).heights;
        assert_eq!(shown, [100.0, 20.0, 20.0]);
        let hands = pin_plan(&stack, 140.0, &shown);
        assert_eq!(hands, [Some(100.0), Some(20.0), None]);
        // A folded one and the last keep what they had, for when they
        // are not last or folded any more.
        let stack = [
            asked(10.0, Some(50.0)),
            Asked {
                open: false,
                least: 10.0,
                hand: Some(33.0),
            },
            asked(10.0, Some(77.0)),
        ];
        let hands = pin_plan(&stack, 100.0, &[60.0, 0.0, 40.0]);
        assert_eq!(hands, [Some(60.0), Some(33.0), Some(77.0)]);
    }
}
