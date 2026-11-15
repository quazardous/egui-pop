//! The canvas in use (docs/pointer-gestures.md, "The canvas in use"):
//! which canvas the navigation commands go to, the commands waiting for
//! it, its published zoom and the shared hand mode, frame by frame.

use std::time::Duration;

use egui::{Context, Id, RawInput, ViewportId};
use egui_pop_core::canvas;

/// One frame of `ctx`, `f` drawn in it; the frame's output.
fn frame(ctx: &Context, mut f: impl FnMut(&Context)) -> egui::FullOutput {
    ctx.run_ui(RawInput::default(), |ui| f(ui.ctx()))
}

#[test]
fn a_command_with_no_canvas_ever_marked_is_dropped() {
    let ctx = Context::default();
    frame(&ctx, |_| {});
    assert_eq!(canvas::active(&ctx), None);
    assert_eq!(canvas::last(&ctx), None);
    assert!(
        !canvas::request(&ctx, "view.fit"),
        "nowhere to go: request answers false"
    );
}

#[test]
fn the_canvas_in_use_is_the_one_marked_this_frame_or_the_one_before() {
    let ctx = Context::default();
    let a = Id::unique("canvas a");
    frame(&ctx, |ctx| {
        canvas::mark_active(ctx, a);
        assert_eq!(canvas::active(ctx), Some(a), "marked this frame");
    });
    let mut seen = Vec::new();
    for _ in 0..2 {
        frame(&ctx, |ctx| seen.push(canvas::active(ctx)));
    }
    assert_eq!(seen, [Some(a), None], "the frame after, then no longer");
    assert_eq!(canvas::last(&ctx), Some(a), "the one marked last stays");
}

#[test]
fn a_command_goes_to_the_canvas_in_use_else_to_the_one_marked_last() {
    let ctx = Context::default();
    let (a, b) = (Id::unique("canvas a"), Id::unique("canvas b"));
    frame(&ctx, |ctx| {
        canvas::mark_active(ctx, a);
        canvas::mark_active(ctx, b);
    });
    assert_eq!(canvas::active(&ctx), Some(b), "the mark made last wins");
    assert!(canvas::request(&ctx, "view.fit"));
    assert_eq!(canvas::take(&ctx, b), ["view.fit"]);
    assert!(canvas::take(&ctx, a).is_empty(), "the other gets nothing");

    // No canvas in use any more: the one marked last takes it (where an
    // agent's command goes).
    frame(&ctx, |_| {});
    frame(&ctx, |_| {});
    assert_eq!(canvas::active(&ctx), None);
    assert!(canvas::request(&ctx, "view.zoom_in"));
    assert_eq!(canvas::take(&ctx, b), ["view.zoom_in"]);
}

#[test]
fn commands_wait_per_canvas_in_order_and_are_taken_once_at_the_next_frame() {
    let ctx = Context::default();
    let (a, b) = (Id::unique("canvas a"), Id::unique("canvas b"));
    let out = frame(&ctx, |ctx| {
        canvas::request_to(ctx, a, "view.fit");
        canvas::request_to(ctx, a, "view.zoom_in");
        canvas::request_to(ctx, b, "view.hand");
    });
    assert_eq!(
        out.viewport_output[&ViewportId::ROOT].repaint_delay,
        Duration::ZERO,
        "a command asks for the next frame, where the canvas takes it"
    );
    assert_eq!(canvas::take(&ctx, a), ["view.fit", "view.zoom_in"]);
    assert!(canvas::take(&ctx, a).is_empty(), "taken once");
    assert_eq!(canvas::take(&ctx, b), ["view.hand"]);
    assert!(
        canvas::take(&ctx, Id::unique("never sent to")).is_empty(),
        "a canvas nobody wrote to has nothing waiting"
    );
}

#[test]
fn hand_mode_is_one_switch_shared_by_every_canvas() {
    let ctx = Context::default();
    assert!(!canvas::hand(&ctx), "off by default");
    frame(&ctx, |ctx| canvas::set_hand(ctx, true));
    assert!(canvas::hand(&ctx));
    frame(&ctx, |ctx| canvas::set_hand(ctx, false));
    assert!(!canvas::hand(&ctx));
}

#[test]
fn a_canvas_zoom_is_its_own_and_the_last_it_published() {
    let ctx = Context::default();
    let (a, b) = (Id::unique("canvas a"), Id::unique("canvas b"));
    assert_eq!(canvas::zoom(&ctx, a), None, "nothing published yet");
    frame(&ctx, |ctx| {
        canvas::set_zoom(ctx, a, 1.0);
        canvas::set_zoom(ctx, b, 2.0);
    });
    frame(&ctx, |ctx| canvas::set_zoom(ctx, a, 4.0));
    assert_eq!(canvas::zoom(&ctx, a), Some(4.0));
    assert_eq!(canvas::zoom(&ctx, b), Some(2.0));
}
