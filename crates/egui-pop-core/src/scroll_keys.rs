//! Keys on a scroll area: Page Up and Down, Home and End, the arrows, for
//! [`UiPop::scroll_area`](crate::ui::UiPop::scroll_area).
//!
//! egui's `ScrollArea` reads no key. After its content is drawn, this moves it:
//!
//! - **Which area**: the one holding the focus; with nothing focused, the one
//!   under the pointer. The innermost goes first, and a key it has no use for (at
//!   its end) goes on to the area around it, as the wheel does.
//! - **A widget inside goes first.** A text input keeps every key. A widget
//!   sensing drags (a slider, a canvas, an editor, a terminal) or naming itself
//!   with [`focus::takes_keys`] keeps them too. Any other focused widget (a
//!   button, a link) has no use for Page Up, Page Down, Home or End, and the arrows
//!   stay egui's, which move the focus.
//! - **A binding goes first.** The key dispatcher ran before anything was drawn
//!   and took the keys that resolve, in whatever scope they are bound, so nothing
//!   is left here to read.
//! - **A stop only when nothing else can be** ([`stop`]): with nothing focusable
//!   inside and something to scroll, the area takes the focus itself, as a
//!   browser's scroller does, and then has the arrows too. With nothing to scroll
//!   it is no stop, as it would answer no key.
//!
//! A move shows in the next pass, and is not animated, as when the focus comes
//! into view. An area whose bars are hidden (`ScrollBarVisibility::AlwaysHidden`),
//! or without scrolling by its bar (`ScrollSource`), has no keys either: the bars
//! are how this knows which way it scrolls (see [`axes`]).
//!
//! upstream: egui's `ScrollArea` has no keys and does not say which ways it may
//! scroll (docs/upstream.md, "egui: a scroll area reads no key").

use egui::accesskit::Role;
use egui::containers::scroll_area::{ScrollAreaOutput, State};
use egui::{
    Context, Event, EventFilter, Id, Key, LayerId, Rect, Sense, Stroke, StrokeKind, TextStyle, Ui,
    Vec2, WidgetType,
};

use crate::focus;
use crate::strings::{self, tr};
use crate::theme::Tokens;

/// Which keys the area may take this pass.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Reach {
    /// All of them: it holds the focus itself, or it is under the pointer.
    All,
    /// A widget inside holds the focus, and the arrows are egui's.
    Pages,
}

/// How many widgets this pass drew so far in `ui`'s layer: where the area's own
/// start, to pass to [`answer`].
pub(crate) fn start(ui: &Ui) -> usize {
    let layer = ui.layer_id();
    ui.ctx()
        .viewport(|v| v.this_pass.widgets.get_layer(layer).count())
}

/// Makes the area just drawn a stop if it should be, and answers the keys.
pub(crate) fn answer<R>(ui: &mut Ui, out: &ScrollAreaOutput<R>, start: usize) {
    if !ui.is_enabled() {
        return;
    }
    let ctx = ui.ctx().clone();
    let layer = ui.layer_id();
    let end = self::start(ui);
    let axes = axes(&ctx, out);
    if !axes[0] && !axes[1] {
        return;
    }
    let own = out.id.with("egui_pop.keys");
    let empty = ctx.viewport(|v| {
        let widgets = v.this_pass.widgets.get_layer(layer);
        !widgets
            .skip(start)
            .take(end - start)
            .any(crate::place::can_focus)
    });
    if empty {
        stop(ui, own, out.inner_rect);
    }
    let Some(reach) = reach(ui, out.inner_rect, own, layer, start..end) else {
        return;
    };
    let line = ui.text_style_height(&TextStyle::Body) + ui.spacing().item_spacing.y;
    let view = out.inner_rect.size();
    let max = (out.content_size - view).max(Vec2::ZERO);
    let presses = ctx.input(|i| {
        let plain = i.events.iter().filter_map(|e| match e {
            Event::Key {
                key,
                pressed: true,
                modifiers,
                ..
            } if modifiers.is_none() => Some(*key),
            _ => None,
        });
        plain.collect::<Vec<_>>()
    });
    let mut offset = out.state.offset;
    let mut used = Vec::new();
    for key in presses {
        let Some(next) = moved(key, reach, axes, offset, view, max, line) else {
            continue;
        };
        if (next - offset).length() > 0.01 {
            offset = next;
            used.push(key);
        }
    }
    if used.is_empty() {
        return;
    }
    let Some(mut state) = State::load(&ctx, out.id) else {
        return;
    };
    state.offset = offset;
    state.store(&ctx, out.id);
    ctx.input_mut(|i| {
        i.events.retain(|e| {
            !matches!(
                e,
                Event::Key { key, pressed: true, modifiers, .. }
                    if modifiers.is_none() && used.contains(key)
            )
        });
    });
    crate::place::scrolled(&ctx);
    ctx.request_repaint();
}

/// The ways the area can scroll: there is more than it shows, and a bar to move
/// it. A bar is a widget of id `id.with(axis)`, as the focus reads it to come
/// into view ([`reveal`](crate::reveal)).
///
/// upstream: `ScrollArea` keeps to itself which ways it may scroll, and content
/// wider than a vertical area overflows a way it cannot go.
fn axes<R>(ctx: &Context, out: &ScrollAreaOutput<R>) -> [bool; 2] {
    let more = out.content_size - out.inner_rect.size();
    [0_usize, 1_usize].map(|d| {
        more[d] > 0.5
            && ctx.viewport(|v| {
                let bar = v.this_pass.widgets.get(out.id.with(d));
                bar.is_some_and(|w| w.sense.senses_drag())
            })
    })
}

/// The area as a stop, for a keyboard to get to what it holds, with a ring as
/// nothing inside shows the focus.
fn stop(ui: &mut Ui, id: Id, rect: Rect) {
    let ctx = ui.ctx().clone();
    let r = ui.interact(rect, id, Sense::FOCUSABLE);
    crate::a11y::name(
        &r,
        WidgetType::Other,
        &tr(&ctx, &strings::SCROLLABLE_CONTENT),
    );
    ctx.accesskit_node_builder(id, |node| node.set_role(Role::ScrollView));
    if r.has_focus() {
        // Or egui's arrows would move the focus out rather than the area.
        ui.memory_mut(|m| {
            m.set_focus_lock_filter(
                id,
                EventFilter {
                    horizontal_arrows: true,
                    vertical_arrows: true,
                    ..Default::default()
                },
            );
        });
        let ring = Stroke::new(2.0, Tokens::of(&ctx).focus_ring);
        ui.painter()
            .rect_stroke(rect, 0.0, ring, StrokeKind::Inside);
    }
}

/// Whether the keys are this area's, and which.
fn reach(
    ui: &Ui,
    rect: Rect,
    own: Id,
    layer: LayerId,
    span: std::ops::Range<usize>,
) -> Option<Reach> {
    let ctx = ui.ctx();
    if focus::text_input_owns_keyboard(ctx) || !ctx.memory(|m| m.allows_interaction(layer)) {
        return None;
    }
    let Some(id) = ctx.memory(|m| m.focused()) else {
        let under = ui.rect_contains_pointer(rect) && !egui::Popup::is_any_open(ctx);
        return under.then_some(Reach::All);
    };
    if id == own {
        return Some(Reach::All);
    }
    let widget = ctx.viewport(|v| {
        let widgets = &v.this_pass.widgets;
        let (at, order) = widgets.order(id)?;
        let inside = at == layer && span.contains(&order);
        widgets.get(id).copied().filter(|_| inside)
    })?;
    let takes = widget.sense.senses_drag() || focus::keys_taken_by(ctx, id);
    (!takes).then_some(Reach::Pages)
}

/// Where a key takes the offset, if it is one the area answers.
fn moved(
    key: Key,
    reach: Reach,
    axes: [bool; 2],
    offset: Vec2,
    view: Vec2,
    max: Vec2,
    line: f32,
) -> Option<Vec2> {
    // Up and Down, Home and End and the pages go down the area, or along it
    // when it scrolls no other way.
    let along = usize::from(axes[1]);
    let page = (view[along] - line).max(line);
    let (d, to) = match key {
        Key::PageDown => (along, offset[along] + page),
        Key::PageUp => (along, offset[along] - page),
        Key::Home => (along, 0.0),
        Key::End => (along, max[along]),
        Key::ArrowDown if reach == Reach::All => (1, offset.y + line),
        Key::ArrowUp if reach == Reach::All => (1, offset.y - line),
        Key::ArrowRight if reach == Reach::All => (0, offset.x + line),
        Key::ArrowLeft if reach == Reach::All => (0, offset.x - line),
        _ => return None,
    };
    if !axes[d] {
        return None;
    }
    let mut next = offset;
    next[d] = to.clamp(0.0, max[d]);
    Some(next)
}

#[cfg(test)]
mod tests {
    use super::*;

    const VIEW: Vec2 = Vec2::new(100.0, 200.0);
    const MAX: Vec2 = Vec2::new(0.0, 500.0);

    fn from(key: Key, reach: Reach, y: f32) -> Option<f32> {
        moved(
            key,
            reach,
            [false, true],
            Vec2::new(0.0, y),
            VIEW,
            MAX,
            20.0,
        )
        .map(|o| o.y)
    }

    #[test]
    fn a_page_is_the_view_less_a_line() {
        assert_eq!(from(Key::PageDown, Reach::Pages, 0.0), Some(180.0));
        assert_eq!(from(Key::PageUp, Reach::Pages, 300.0), Some(120.0));
    }

    #[test]
    fn a_move_stops_at_the_ends() {
        assert_eq!(from(Key::PageDown, Reach::All, 450.0), Some(500.0));
        assert_eq!(from(Key::PageUp, Reach::All, 50.0), Some(0.0));
        assert_eq!(from(Key::End, Reach::All, 50.0), Some(500.0));
        assert_eq!(from(Key::Home, Reach::All, 50.0), Some(0.0));
    }

    #[test]
    fn a_widget_inside_keeps_the_arrows() {
        assert_eq!(from(Key::ArrowDown, Reach::Pages, 0.0), None);
        assert_eq!(from(Key::ArrowDown, Reach::All, 0.0), Some(20.0));
        assert_eq!(from(Key::ArrowUp, Reach::All, 30.0), Some(10.0));
    }

    #[test]
    fn a_way_it_cannot_scroll_takes_nothing() {
        assert_eq!(from(Key::ArrowRight, Reach::All, 0.0), None);
        assert_eq!(from(Key::A, Reach::All, 0.0), None);
    }

    #[test]
    fn pages_go_along_an_area_that_scrolls_only_sideways() {
        let max = Vec2::new(500.0, 0.0);
        let at = |key| {
            moved(
                key,
                Reach::All,
                [true, false],
                Vec2::new(100.0, 0.0),
                VIEW,
                max,
                20.0,
            )
            .map(|o| o.x)
        };
        assert_eq!(at(Key::PageDown), Some(180.0));
        assert_eq!(at(Key::Home), Some(0.0));
        assert_eq!(at(Key::End), Some(500.0));
        assert_eq!(at(Key::ArrowDown), None);
    }
}
