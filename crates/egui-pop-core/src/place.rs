//! Focus keeps its place: part of the focus plugin
//! ([`focus::install`](crate::focus::install)).
//!
//! egui forgets a focus it drops: Escape, or a widget no longer drawn, leaves
//! nothing focused, and the next Tab starts from the top of the window. So the
//! plugin remembers the widget last focused and its rectangle:
//!
//! - **Tab and Shift+Tab with nothing focused** go on from that widget if it is
//!   still drawn; if not, to the stop nearest to where it was.
//! - **A widget removed while it holds the focus** hands it to the stop nearest
//!   to where it was, in the same scope first: a removed row's neighbour, the
//!   next tab. It happens as the pass ends, so nothing is ever focused that is
//!   not drawn, and nothing in between sees an empty focus.
//! - **A widget scrolled away, or removed by the pointer,** hands nothing on:
//!   the user is looking elsewhere, and a focus jumping about would chase the
//!   wheel. Tab still goes on from the place.
//! - **A press on nothing forgets the place**, as does a place in a layer now
//!   gone (a closed menu) or under a modal: Tab then does what egui does.
//!
//! A key binding on Tab goes first: nothing is focused, so it can resolve (the
//! palettes' toggle, with `!widgetFocus`), and it takes the key from the place.
//! So the place waits for the end of the pass to see whether the key is left.
//!
//! upstream: egui keeps no place when it drops the focus, and its Tab from
//! nothing starts at the window's first stop (docs/upstream.md, "egui: a focus
//! dropped leaves no place"). Its rule is otherwise "the widget after the
//! focused one", so in the next pass, before anything is drawn, that widget gets
//! the focus back and egui is told to move on. That pass holds no key press for
//! the widget to read. It follows egui's order: in a `tab::scope`, the order of
//! drawing rather than the scope's own.
//!
//! upstream: egui 0.36's `Popup` senses `Sense::click()`, so a popup's frame is a
//! stop with no role and no name (docs/upstream.md, "egui: a popup's frame is a
//! Tab stop"), and nothing lets `MenuButton` or `ComboBox` say otherwise. So Tab
//! landing on one moves on in the next pass, the way it was going. A focus put
//! there by anything else is dropped. Remove with egui 0.37.
//!
//! **A popover keeps Tab** ([`focus::keep_tab`]): past its last stop Tab comes
//! back to its first, and Shift+Tab the other way, as in a dialog; Tab from
//! outside it goes in. Only Escape or a click outside leaves, and a popover with
//! no stop closes as Tab goes on. A focused widget taking Tab itself (a terminal,
//! an editor) keeps it, as elsewhere.
//!
//! upstream: egui wraps Tab only in a modal layer, which would take the pointer
//! too, and it does not say whether a focused widget takes Tab (its event filter
//! is private), so the key cannot be judged before the pass (docs/upstream.md,
//! "egui: Tab wraps only in a modal layer"). So egui moves the focus by its own
//! rule and the plugin puts it right, before anything is drawn when it can. A stop
//! with no rectangle, no role and no pixel ends the popover's order, a focus guard
//! as on the web: Tab from the last stop lands there rather than on a widget
//! behind, which would see the focus for a pass. Going back needs none, as egui
//! hands the focus over at the start of the next pass.

use egui::{Context, FocusDirection, Id, Key, LayerId, Rect, Sense, Ui, Vec2, WidgetRect};

use crate::state::{lock, pass, shared_in_window};
use crate::{focus, focus_mode, overlay, reveal};

/// How long after a scroll (in passes) a widget going away went by that
/// scroll: the row may leave a pass after the offset moved.
const SCROLL_PASSES: u64 = 2;

/// How long after Tab (in passes) the focus it moved lands: in its own pass,
/// with Shift the next, one more going on from the place.
const TAB_PASSES: u64 = 2;

#[derive(Clone, Debug)]
struct Place {
    id: Id,
    layer: LayerId,
    /// Where it was last drawn, on screen.
    rect: Rect,
    /// The scopes it was drawn in, innermost first.
    scopes: Vec<String>,
}

#[derive(Debug, Default)]
struct Keep {
    place: Option<Place>,
    /// Tab was held back from egui this pass, Shift or not, for the end of the
    /// pass to place if no binding takes it.
    asked: Option<bool>,
    /// Tab is to go on from the place in the next pass, Shift or not.
    going: Option<bool>,
    /// The widget given the focus back this pass, for Tab to go on from.
    anchor: Option<Id>,
    scrolled_at: Option<u64>,
    /// When Tab was last pressed, and whether Shift was held.
    tabbed: Option<(u64, bool)>,
    /// A popup's frame holds the focus, to move on in the next pass, backwards
    /// or not.
    past_frame: Option<(Id, bool)>,
    /// What held the focus when Tab was last pressed.
    tab_from: Option<Id>,
    /// The popover keeping Tab, and the pass that last drew it.
    keeping: Option<(LayerId, u64)>,
}

/// Puts egui back at the place, or holds back a Tab pressed with nothing
/// focused. Runs once egui has read the key and before anything is drawn.
pub(crate) fn begin_pass(ctx: &Context) {
    let st = shared_in_window::<Keep>(ctx, "core.focus.place");
    let mut s = lock(&st);
    s.anchor = None;
    s.asked = None;
    // Here: a scroll area takes the delta as it is drawn.
    if ctx.input(|i| i.is_scrolling() || i.smooth_scroll_delta != Vec2::ZERO) {
        s.scrolled_at = Some(pass(ctx));
    }
    keep_in_popover(ctx, &mut s);
    let focused = ctx.memory(|m| m.focused());
    if let Some(back) = tab_pressed(ctx) {
        s.tabbed = Some((pass(ctx), back));
        s.tab_from = focused;
    }
    if let Some((frame, back)) = s.past_frame.take()
        && focused == Some(frame)
    {
        ctx.memory_mut(|m| m.move_focus(direction(back)));
        return;
    }
    if let Some(back) = s.going.take()
        && focused.is_none()
        && let Some(id) = s.place.as_ref().map(|p| p.id)
    {
        ctx.memory_mut(|m| {
            m.request_focus(id);
            m.move_focus(direction(back));
        });
        s.anchor = Some(id);
        return;
    }
    if focused.is_some() {
        return;
    }
    let Some(back) = tab_pressed(ctx) else {
        return;
    };
    let Some((id, layer)) = s.place.as_ref().map(|p| (p.id, p.layer)) else {
        return;
    };
    if !ctx.memory(|m| m.allows_interaction(layer)) {
        return;
    }
    let usable = ctx.viewport(|v| {
        let widgets = &v.prev_pass.widgets;
        widgets.contains(id) || widgets.get_layer(layer).any(|w| is_stop(w, id))
    });
    if usable {
        ctx.memory_mut(|m| m.move_focus(FocusDirection::None));
        s.asked = Some(back);
    }
}

/// Remembers what holds the focus, or hands it on. Runs after the widgets are
/// drawn and before egui's own end of pass, which would drop the focus of a
/// widget it did not see.
pub(crate) fn end_pass(ctx: &Context) {
    let now = pass(ctx);
    let st = shared_in_window::<Keep>(ctx, "core.focus.place");
    let mut s = lock(&st);
    let anchor = s.anchor.take();
    let asked = s.asked.take();
    let Some(id) = ctx.memory(|m| m.focused()) else {
        if asked.is_some() && tab_pressed(ctx).is_some() {
            let drawn = s
                .place
                .as_ref()
                .is_some_and(|p| ctx.viewport(|v| v.this_pass.widgets.contains(p.id)));
            if drawn {
                s.going = asked;
                ctx.request_repaint();
            } else {
                hand_on(ctx, &s);
            }
        } else if asked.is_none() && pointer_acted(ctx) {
            s.place = None;
        }
        return;
    };
    let drawn = ctx.viewport(|v| v.this_pass.widgets.get(id).copied());
    if let Some(widget) = drawn {
        if is_tab_end(&widget) {
            // Not a place: the next pass moves it on.
            ctx.request_repaint();
            return;
        }
        if is_popup_frame(&widget) {
            // Not a place. Tab long gone, or the frame is all it finds (a popup
            // of text alone, and no stop in the window): let go.
            match s.tabbed.filter(|&(at, _)| now <= at + TAB_PASSES) {
                Some((_, back)) => {
                    s.past_frame = Some((id, back));
                    ctx.request_repaint();
                }
                None => ctx.memory_mut(|m| m.surrender_focus(id)),
            }
            return;
        }
        let scopes = match s.place.take() {
            Some(place) if place.id == id => place.scopes,
            _ => focus::scopes_holding(ctx, id),
        };
        s.place = Some(Place {
            id,
            layer: widget.layer_id,
            rect: widget.rect,
            scopes,
        });
        return;
    }
    // Focused and not drawn: asked for this pass and drawn in the next, unless
    // it held the focus already or was given it back above.
    if anchor != Some(id) && !ctx.memory(|m| m.had_focus_last_frame(id)) {
        return;
    }
    let scrolled = s.scrolled_at.is_some_and(|at| now <= at + SCROLL_PASSES);
    let by_hand = anchor != Some(id) && (scrolled || reveal::pointer_lately(ctx));
    let known = s.place.as_ref().is_some_and(|p| p.id == id);
    if (by_hand || !known || !hand_on(ctx, &s)) && anchor == Some(id) {
        // egui keeps a focus it was just given, and AccessKit panics on a
        // focus without a node.
        ctx.memory_mut(|m| m.surrender_focus(id));
    }
}

/// Makes the popover being drawn keep Tab, its content drawn: see
/// [`focus::keep_tab`].
pub(crate) fn keep_tab(ui: &mut Ui) {
    let ctx = ui.ctx().clone();
    let layer = ui.layer_id();
    {
        let st = shared_in_window::<Keep>(&ctx, "core.focus.place");
        lock(&st).keeping = Some((layer, pass(&ctx)));
    }
    let has_stop = ctx.viewport(|v| v.this_pass.widgets.get_layer(layer).any(can_focus));
    if !has_stop {
        return;
    }
    let at = Rect::from_min_size(ui.min_rect().left_bottom(), Vec2::ZERO);
    let end = ui.interact(at, tab_end(layer), Sense::FOCUSABLE);
    ctx.accesskit_node_builder(end.id, |node| {
        node.set_role(egui::accesskit::Role::GenericContainer);
        node.set_hidden();
    });
}

/// Puts right where Tab left the focus, with a popover keeping it: on the end of
/// its order, on its frame, or behind it.
fn keep_in_popover(ctx: &Context, s: &mut Keep) {
    let now = pass(ctx);
    let Some((layer, _)) = s.keeping.filter(|&(_, at)| at + 1 >= now) else {
        s.keeping = None;
        return;
    };
    let Some(id) = ctx.memory(|m| m.focused()) else {
        return;
    };
    let on_end = id == tab_end(layer);
    let Some((_, back)) = s.tabbed.filter(|&(at, _)| now <= at + TAB_PASSES) else {
        if on_end {
            // An arrow's cone, or a screen reader: it stays where it was.
            let place = s.place.as_ref().map(|p| p.id);
            let drawn = |p: &Id| ctx.viewport(|v| v.prev_pass.widgets.contains(*p));
            match place.filter(drawn) {
                Some(place) => ctx.memory_mut(|m| m.request_focus(place)),
                None => ctx.memory_mut(|m| m.surrender_focus(id)),
            }
        }
        return;
    };
    if !ctx.memory(|m| m.allows_interaction(layer)) {
        return;
    }
    let landed = ctx.viewport(|v| v.prev_pass.widgets.get(id).map(|w| w.layer_id));
    let behind = s.tab_from != Some(id)
        && landed.is_some_and(|l| l != layer && !overlay::opened_by_top(ctx, l));
    if !(on_end || behind || id == frame_of(layer)) {
        return;
    }
    let stop = ctx.viewport(|v| {
        let mut stops = v
            .prev_pass
            .widgets
            .get_layer(layer)
            .filter(|w| can_focus(w));
        let stop = if back { stops.last() } else { stops.next() };
        stop.map(|w| w.id)
    });
    match stop {
        Some(stop) => {
            log::debug!(target: "egui_pop", "Focus: Tab stays in the popover, on {stop:?}");
            ctx.memory_mut(|m| m.request_focus(stop));
            s.tabbed = None;
        }
        None if behind => overlay::close_for_tab(ctx, layer.id),
        None => {}
    }
}

/// The stop that ends a popover's Tab order.
fn tab_end(layer: LayerId) -> Id {
    layer.id.with("egui_pop.tab_end")
}

fn is_tab_end(w: &WidgetRect) -> bool {
    w.id == tab_end(w.layer_id)
}

/// Gives the focus to the stop nearest to the place. Whether there was one.
fn hand_on(ctx: &Context, s: &Keep) -> bool {
    let Some(place) = &s.place else {
        return false;
    };
    let Some(next) = nearest(ctx, place) else {
        return false;
    };
    log::debug!(target: "egui_pop", "Focus: {:?} went away, {next:?} takes over", place.id);
    ctx.memory_mut(|m| m.request_focus(next));
    true
}

/// The stop this pass drew nearest to a place: in the innermost of its scopes
/// that still holds one, else anywhere in its layer. The later one on a tie, as
/// Tab would go there next.
fn nearest(ctx: &Context, place: &Place) -> Option<Id> {
    let screen = ctx.viewport_rect();
    let spans = focus::spans_of(ctx, place.layer, &place.scopes);
    ctx.viewport(|v| {
        let drawn: Vec<&WidgetRect> = v.this_pass.widgets.get_layer(place.layer).collect();
        let closest = |widgets: &[&WidgetRect]| {
            let mut best: Option<(f32, Id)> = None;
            for w in widgets {
                if !is_stop(w, place.id) || !w.interact_rect.intersect(screen).is_positive() {
                    continue;
                }
                let d = (w.rect.center() - place.rect.center()).length_sq();
                if best.is_none_or(|(b, _)| d <= b) {
                    best = Some((d, w.id));
                }
            }
            best.map(|(_, id)| id)
        };
        spans
            .iter()
            .filter_map(|span| drawn.get(span.clone()))
            .find_map(&closest)
            .or_else(|| closest(&drawn))
    })
}

/// A widget that can take the focus, other than `not`.
fn is_stop(w: &WidgetRect, not: Id) -> bool {
    w.id != not && can_focus(w)
}

/// A widget that can take the focus.
pub(crate) fn can_focus(w: &WidgetRect) -> bool {
    w.enabled && w.sense.is_focusable() && !is_popup_frame(w) && !is_tab_end(w)
}

/// The widget an `Area` makes of its own background, focusable only in a popup.
fn is_popup_frame(w: &WidgetRect) -> bool {
    w.id == frame_of(w.layer_id)
}

fn frame_of(layer: LayerId) -> Id {
    layer.id.with("move")
}

fn direction(back: bool) -> FocusDirection {
    if back {
        FocusDirection::Previous
    } else {
        FocusDirection::Next
    }
}

/// The pointer pressed or let go, or it moves the focus (hover mode).
fn pointer_acted(ctx: &Context) -> bool {
    ctx.input(|i| i.pointer.any_pressed() || i.pointer.any_released())
        || focus_mode::pointer_may_focus(ctx)
}

/// Tab as egui reads it: whether Shift was held.
fn tab_pressed(ctx: &Context) -> Option<bool> {
    ctx.input(|i| {
        i.events.iter().find_map(|e| match e {
            egui::Event::Key {
                key: Key::Tab,
                pressed: true,
                modifiers,
                ..
            } if !modifiers.any() => Some(false),
            egui::Event::Key {
                key: Key::Tab,
                pressed: true,
                modifiers,
                ..
            } if modifiers.shift_only() => Some(true),
            _ => None,
        })
    })
}
