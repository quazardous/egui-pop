//! Focus scrolls into view: the one hook behind every plain widget, part of
//! the focus plugin ([`focus::install`](crate::focus::install)).
//!
//! When the focus moves to a widget by anything but the pointer (Tab, an
//! arrow, a command, a screen reader) and that widget is cut off, the scroll
//! areas around it move just enough to show it, the innermost first, so that
//! it also shows through the areas nesting them. It happens once per move:
//! the wheel can then take a focused widget away again. A click on a
//! half-visible row does not make it jump.
//!
//! It never animates, so it needs nothing for reduced motion, and a key held
//! down does not chase a moving target.
//!
//! What is not drawn is no stop: windowed rows (`show_rows`, trees, lists) are
//! reached as long as the scrolling draws the next one, and a view with a
//! cursor of its own holds one stop and keeps scrolling to that cursor itself.
//!
//! # How it finds the areas
//!
//! upstream: egui has no way to say which scroll areas hold a widget, and
//! `Response::scroll_to_me` reaches only the innermost
//! (docs/upstream.md, "egui: a scroll target stops at the innermost scroll
//! area"). So this reads what the previous pass recorded, all of it public:
//!
//! - a scroll area checks its id for clashes (`PassState::used_ids`), and only
//!   a scroll area keeps a [`State`] under that id: that gives every area of
//!   the pass, whatever its salt;
//! - an area that can scroll has a bar, a widget of id `id.with(axis)` covering
//!   its length on that axis: that gives its reach, in points on screen;
//! - the first widget before the bars with the same parent is the area's
//!   content `Ui`, and all the widgets the area holds lie between the two.
//!
//! An area whose bars are hidden (`ScrollBarVisibility::AlwaysHidden`) has no
//! bar to read, so it does not follow the focus: what it holds calls
//! `scroll_to_me` itself, as the stack of sections does.

use egui::containers::scroll_area::State;
use egui::{Context, Id, Rect, WidgetRect};

use crate::focus_mode;
use crate::state::{lock, pass, shared_in_window};

/// How long after a press, a release or a button held down (in passes) a new
/// focus is the pointer's: a widget may take it as the button goes up, and an
/// overlay gives it back a pass or two after closing.
const POINTER_PASSES: u64 = 2;

/// How many passes a move of the focus keeps trying: a widget not drawn yet,
/// or a layout still settling after the first scroll.
const TRIES: u8 = 4;

#[derive(Clone, Copy, Debug)]
struct Pending {
    id: Id,
    tries: u8,
}

#[derive(Debug, Default)]
struct Reveal {
    seen: Option<Id>,
    pending: Option<Pending>,
    pointer_at: Option<u64>,
}

/// A scroll area around a widget.
struct Around {
    id: Id,
    /// Where its widgets start in its layer.
    start: usize,
    /// Its bar on each axis, where it has one.
    bars: [Option<Rect>; 2],
}

/// Runs at the start of each pass, once the input has settled who holds the
/// focus and before anything is drawn: a scroll done here shows in this pass.
pub(crate) fn begin_pass(ctx: &Context) {
    let now = pass(ctx);
    let st = shared_in_window::<Reveal>(ctx, "core.focus.reveal");
    let mut s = lock(&st);
    let pointer = ctx
        .input(|i| i.pointer.any_pressed() || i.pointer.any_released() || i.pointer.any_down())
        || focus_mode::pointer_may_focus(ctx);
    if pointer {
        s.pointer_at = Some(now);
    }
    let focused = ctx.memory(|m| m.focused());
    if std::mem::replace(&mut s.seen, focused) != focused {
        let by_pointer = s.pointer_at.is_some_and(|at| now <= at + POINTER_PASSES);
        s.pending = focused
            .filter(|_| !by_pointer)
            .map(|id| Pending { id, tries: 0 });
    }
    let Some(mut pending) = s.pending else {
        return;
    };
    s.pending = None;
    if pending.tries >= TRIES {
        return;
    }
    pending.tries += 1;
    let Some(widget) = ctx.viewport(|v| v.prev_pass.widgets.get(pending.id).copied()) else {
        // Focused before it is drawn, as `request_focus` may be.
        s.pending = Some(pending);
        ctx.request_repaint();
        return;
    };
    if widget.interact_rect.expand(0.5).contains_rect(widget.rect) || !scroll_to(ctx, &widget) {
        return;
    }
    // Once more, in case the layout moves under the new scroll.
    s.pending = Some(pending);
    ctx.request_repaint();
}

/// Moves the areas around a widget until it lies in each of them, the
/// innermost first. Whether any moved.
fn scroll_to(ctx: &Context, widget: &WidgetRect) -> bool {
    let spacing = ctx.global_style().spacing.item_spacing;
    let mut rect = widget.rect;
    let mut moved = false;
    for area in around(ctx, widget.id) {
        let Some(mut state) = State::load(ctx, area.id) else {
            continue;
        };
        let mut delta = egui::Vec2::ZERO;
        for d in 0..2 {
            let Some(bar) = area.bars[d] else {
                continue;
            };
            let mut end = bar.max[d];
            // The other bar takes room at this one's end, unless it floats
            // over the content, where a widget should not stay under it either.
            if let Some(other) = area.bars[1 - d] {
                end -= other.size()[d];
            }
            delta[d] = distance(rect.min[d], rect.max[d], bar.min[d], end, spacing[d]);
        }
        if delta != egui::Vec2::ZERO {
            state.offset += delta;
            state.store(ctx, area.id);
            rect = rect.translate(-delta);
            moved = true;
        }
    }
    moved
}

/// How far to scroll for a stretch to lie in a view, leaving `margin` around
/// it where there is room: nothing when it covers the whole view (a canvas, a
/// long text) or already lies in it.
fn distance(start: f32, end: f32, view_start: f32, view_end: f32, margin: f32) -> f32 {
    if start < view_start && end < view_end {
        -(view_start - start + margin).min(view_end - end - margin)
    } else if end > view_end && start > view_start {
        (end - view_end + margin).min(start - view_start - margin)
    } else {
        0.0
    }
}

/// The scroll areas that can scroll around a widget, the innermost first, as
/// the last pass drew them.
fn around(ctx: &Context, widget: Id) -> Vec<Around> {
    let used: Vec<Id> = ctx.viewport(|v| v.prev_pass.used_ids.keys().copied().collect());
    let areas: Vec<Id> = used
        .into_iter()
        .filter(|id| State::load(ctx, *id).is_some())
        .collect();
    let mut found = ctx.viewport(|v| {
        let widgets = &v.prev_pass.widgets;
        let Some((layer, at)) = widgets.order(widget) else {
            return Vec::new();
        };
        let drawn: Vec<&WidgetRect> = widgets.get_layer(layer).collect();
        let mut found = Vec::new();
        for id in areas {
            let bars = [id.with(0_usize), id.with(1_usize)];
            let bars = bars.map(|bar| widgets.get(bar).zip(widgets.order(bar)));
            let first = bars
                .iter()
                .flatten()
                .filter(|(_, (in_layer, _))| *in_layer == layer)
                .min_by_key(|(_, (_, order))| *order);
            let Some(&(bar, (_, first))) = first else {
                continue;
            };
            if at >= first {
                continue;
            }
            let Some(start) = drawn[..first]
                .iter()
                .rposition(|r| r.parent_id == bar.parent_id)
            else {
                continue;
            };
            if start < at {
                found.push(Around {
                    id,
                    start,
                    bars: bars.map(|b| b.map(|(bar, _)| bar.rect)),
                });
            }
        }
        found
    });
    found.sort_by_key(|a| std::cmp::Reverse(a.start));
    found
}

#[cfg(test)]
mod tests {
    use super::distance;

    #[test]
    fn a_stretch_moves_just_into_view_with_its_margin() {
        assert_eq!(distance(120.0, 140.0, 0.0, 100.0, 4.0), 44.0);
        assert_eq!(distance(-40.0, -20.0, 0.0, 100.0, 4.0), -44.0);
    }

    #[test]
    fn a_stretch_in_view_or_covering_it_stays() {
        assert_eq!(distance(10.0, 30.0, 0.0, 100.0, 4.0), 0.0);
        assert_eq!(distance(-50.0, 500.0, 0.0, 100.0, 4.0), 0.0);
    }

    #[test]
    fn a_stretch_longer_than_the_view_shows_its_near_end() {
        // Its start stays in view: the margin gives way first.
        assert_eq!(distance(50.0, 300.0, 0.0, 100.0, 4.0), 46.0);
        assert_eq!(distance(-300.0, 50.0, 0.0, 100.0, 4.0), -46.0);
    }
}
