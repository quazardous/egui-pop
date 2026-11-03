//! A focus you can see: the ring the focus plugin paints around the focused
//! widget, part of the focus plugin ([`focus::install`](crate::focus::install)).
//!
//! egui's widgets draw no focus ring: a focused button has the pressed
//! button's look, and other widgets show nothing. So at the end of each pass
//! the plugin outlines the focused widget with the `focus.ring` token, inside
//! the part of it that is shown (a ring never spills out of the area that
//! clips the widget).
//!
//! Only when the keyboard put the focus there (Tab, an arrow, F6, a command,
//! a screen reader): a click focuses without a ring, as GTK's and Qt's focus
//! flags and the web's `:focus-visible` have it. The ring stays while the
//! focus does, and goes at the next press of the pointer.
//!
//! A widget that shows its focus itself (egui-pop's sliders, fields, views,
//! the graph, the viewport) says so with [`own_ring`](crate::focus::own_ring)
//! each pass, and gets none. egui's text fields show theirs and are left
//! alone too.

use egui::{Context, CornerRadius, Id, Stroke, StrokeKind};

use crate::state::{lock, shared_in_window};
use crate::theme::Tokens;

/// The ring's width, in points.
const WIDTH: f32 = 2.0;

#[derive(Debug, Default)]
struct Ring {
    /// The widget holding the focus when last looked at.
    seen: Option<Id>,
    /// Whether the keyboard moved the focus there.
    visible: bool,
    /// The widgets that said this pass that they show their focus.
    own: Vec<Id>,
}

fn ring(ctx: &Context) -> std::sync::Arc<std::sync::Mutex<Ring>> {
    shared_in_window::<Ring>(ctx, "core.focus.ring")
}

/// Says that a widget shows its focus itself this pass.
pub(crate) fn own(ctx: &Context, id: Id) {
    lock(&ring(ctx)).own.push(id);
}

/// Whether the keyboard moved the focus to where it is now, so that a ring
/// shows.
pub(crate) fn visible(ctx: &Context) -> bool {
    let st = ring(ctx);
    let s = lock(&st);
    s.visible && s.seen.is_some() && s.seen == ctx.memory(|m| m.focused())
}

/// Runs at the end of each pass, once every widget is drawn.
pub(crate) fn end_pass(ctx: &Context) {
    let st = ring(ctx);
    let mut s = lock(&st);
    let own = std::mem::take(&mut s.own);
    if ctx.input(|i| i.pointer.any_pressed()) {
        s.visible = false;
    }
    let focused = ctx.memory(|m| m.focused());
    if std::mem::replace(&mut s.seen, focused) != focused {
        s.visible = focused.is_some() && !crate::reveal::pointer_lately(ctx);
    }
    let Some(id) = focused.filter(|_| s.visible) else {
        return;
    };
    drop(s);
    if own.contains(&id) || egui::TextEdit::load_state(ctx, id).is_some() {
        return;
    }
    let Some(widget) = ctx.viewport(|v| v.this_pass.widgets.get(id).copied()) else {
        return;
    };
    let shown = widget.interact_rect;
    if !shown.is_positive() {
        return;
    }
    let t = Tokens::of(ctx);
    let radius = CornerRadius::same(t.radius.widget.round().clamp(0.0, 255.0) as u8);
    let w = crate::scale::snap(WIDTH, ctx.pixels_per_point());
    ctx.layer_painter(widget.layer_id)
        .with_clip_rect(shown)
        .rect_stroke(
            widget.rect,
            radius,
            Stroke::new(w, t.focus_ring),
            StrokeKind::Inside,
        );
}
