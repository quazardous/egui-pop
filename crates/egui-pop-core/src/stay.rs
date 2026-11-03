//! Arrows stay in their region: part of the focus plugin
//! ([`focus::install`](crate::focus::install)).
//!
//! A plain arrow on a widget that does not read it moves the focus to the
//! nearest widget that way, wherever it lies: from the last row of a side list,
//! Right went on to the line beside it, then into the page, and Down from a menu
//! button to a row behind it. Regions ([`focus::region`](crate::focus::region))
//! are where F6 goes; an arrow is for moving inside one. So a plain arrow that
//! takes the focus out of the region holding it is undone, the focus going back
//! to the widget it left. Tab and F6 cross regions.
//!
//! What a widget does with an arrow itself (a list moving its cursor, a widget
//! handing the focus on) is its own: only a move done in the pass of the press,
//! out of a region, is undone. A widget outside every region, or a window with
//! none, keeps egui's rule.
//!
//! upstream: egui moves the focus by arrow to the best widget in a cone, across
//! any container, with nothing to bound it but a modal layer
//! (docs/upstream.md, "egui: arrows move the focus anywhere").

use egui::{Context, Event, Id, Key};

use crate::focus::{self, DrawnRegion};
use crate::state::{lock, pass, shared_in_window};

#[derive(Debug, Default)]
struct Stay {
    /// The widget holding the focus as a pass with a plain arrow began, and that
    /// pass.
    from: Option<(Id, u64)>,
}

/// Which of the regions holds a widget of the last pass.
fn region_of(ctx: &Context, regions: &[DrawnRegion], id: Id) -> Option<usize> {
    let (layer, order) = ctx.viewport(|v| v.prev_pass.widgets.order(id))?;
    regions
        .iter()
        .position(|r| r.layer == layer && r.widgets.contains(&order))
}

/// Whether the input holds a press of an arrow alone.
fn plain_arrow(ctx: &Context) -> bool {
    ctx.input(|i| {
        i.events.iter().any(|e| {
            matches!(
                e,
                Event::Key {
                    key: Key::ArrowUp | Key::ArrowDown | Key::ArrowLeft | Key::ArrowRight,
                    pressed: true,
                    modifiers,
                    ..
                } if !modifiers.any()
            )
        })
    })
}

/// Runs at the start of each pass, before anything is drawn: egui moved the
/// focus by the last pass's arrow as that pass ended.
pub(crate) fn begin_pass(ctx: &Context) {
    let now = pass(ctx);
    let st = shared_in_window::<Stay>(ctx, "core.focus.stay");
    let mut s = lock(&st);
    let focused = ctx.memory(|m| m.focused());
    if let Some((from, at)) = s.from.take()
        && at + 1 == now
        && let Some(to) = focused
        && to != from
    {
        let (regions, _) = focus::drawn_regions(ctx);
        if let Some(left) = region_of(ctx, &regions, from)
            && region_of(ctx, &regions, to) != Some(left)
        {
            log::debug!(target: "egui_pop", "Focus: an arrow stays in region {}", regions[left].name);
            ctx.memory_mut(|m| m.request_focus(from));
        }
    }
    if let Some(id) = focused
        && plain_arrow(ctx)
    {
        s.from = Some((id, now));
    }
}
