//! The canvas in use, and the navigation commands sent to it.
//!
//! A canvas (the node graph, the image viewport, a plot) marks itself
//! active each frame the pointer is over it or it has the keyboard
//! ([`mark_active`]). The application's `view.*` commands
//! (`view.fit`, `view.frame_selection`, `view.zoom_in`, ...) go to the
//! active canvas through [`request`], and the canvas takes them at its
//! next frame ([`take`]); the `canvasActive` context key says whether
//! one is active, so the commands' keys reach nothing else. The
//! navigation model itself is the viewport crate's
//! (`egui_pop_viewport::navigation`).

use egui::{Context, Id};

use crate::state::Slot;

/// The canvas marked last, and the frame it was marked in.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct Active {
    id: Option<Id>,
    frame: u64,
}

const ACTIVE: Slot<Active> = Slot::new("core.canvas.active");
/// Commands waiting for a canvas, by its id.
const QUEUE: Slot<Vec<String>> = Slot::new("core.canvas.queue");

/// Marks a canvas as the one in use this frame (the pointer is over it,
/// or it has the keyboard).
pub fn mark_active(ctx: &Context, id: Id) {
    let frame = ctx.cumulative_frame_nr();
    ACTIVE.set(
        ctx,
        Active {
            id: Some(id),
            frame,
        },
    );
}

/// The canvas in use: one marked this frame or the one before.
pub fn active(ctx: &Context) -> Option<Id> {
    let now = ctx.cumulative_frame_nr();
    let a = ACTIVE.get_or_default(ctx);
    a.id.filter(|_| a.frame + 1 >= now)
}

/// The canvas marked last, however long ago (where an agent's command
/// goes when no canvas is in use).
pub fn last(ctx: &Context) -> Option<Id> {
    ACTIVE.get_or_default(ctx).id
}

/// Sends a navigation command (`view.fit`, ...) to the canvas in use,
/// else to the one marked last. Whether there was one.
pub fn request(ctx: &Context, command: &str) -> bool {
    let Some(id) = active(ctx).or_else(|| last(ctx)) else {
        return false;
    };
    request_to(ctx, id, command);
    true
}

/// Sends a navigation command to a canvas by its id.
pub fn request_to(ctx: &Context, id: Id, command: &str) {
    let command = command.to_string();
    QUEUE.keyed(id).update(ctx, |q| q.push(command));
    ctx.request_repaint();
}

/// The commands waiting for a canvas, taken.
pub fn take(ctx: &Context, id: Id) -> Vec<String> {
    let slot = QUEUE.keyed(id);
    if slot.get(ctx).is_none_or(|q| q.is_empty()) {
        return Vec::new();
    }
    let mut out = Vec::new();
    slot.update(ctx, |q| out = std::mem::take(q));
    out
}
