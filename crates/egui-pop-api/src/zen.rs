//! Zen mode's way out, drawn while it is on (see [`crate::workbench`]):
//! a strip at the top centre of the window, in the kit's
//! `fullscreen-exit` component, saying how to leave ("Zen mode: Escape
//! to leave", with the key bound to `workbench.leave_zen`) next to a
//! "Leave zen mode" button.
//!
//! - **On entering**, the strip shows for [`HINT_SECONDS`], then fades
//!   over `motion.duration` (at once under reduced motion), as a
//!   browser's "Press Esc to exit full screen"; screen readers hear the
//!   hint once, politely.
//! - **At the top edge**, the pointer brings it back, as the menu bar
//!   of a full screen window on macOS; it stays while the pointer is on
//!   it, and goes when the pointer leaves.
//!
//! Times are read on the motion clock, so a test freezes and steps them.

use egui::{Align2, Context, Id, Order, Vec2};
use egui_pop_core::a11y::{self, Politeness};
use egui_pop_core::kit::{Kit, State};
use egui_pop_core::motion;
use egui_pop_core::state::Slot;
use egui_pop_core::strings;
use egui_pop_core::theme::Tokens;
use serde_json::Value as Json;

use crate::app::AppHandle;
use crate::strings as txt;
use crate::workbench::{CMD_LEAVE_ZEN, CMD_ZEN};

/// Seconds the hint stays at full strength once zen mode starts.
pub const HINT_SECONDS: f64 = 3.0;
/// How near the top edge the pointer brings the strip back, in points.
pub const EDGE: f32 = 4.0;

/// When zen mode started, by the motion clock, while it is on.
const SINCE: Slot<f64> = Slot::new("api.zen.since");
/// The pointer held the strip on the last pass (at the edge, or on it).
const HELD: Slot<bool> = Slot::new("api.zen.held");

/// Draws the way out while zen mode is on; forgets it once off. Called
/// on each pass of the main window.
pub(crate) fn show(app: &AppHandle, ctx: &Context) {
    if !app.zen() {
        SINCE.remove(ctx);
        HELD.remove(ctx);
        return;
    }
    let now = motion::now(ctx);
    let since = match SINCE.get(ctx) {
        Some(s) => s,
        None => {
            SINCE.set(ctx, now);
            a11y::announce(ctx, hint(app, ctx), Politeness::Polite);
            now
        }
    };
    let elapsed = now - since;
    let fade = f64::from(Tokens::of(ctx).motion_duration);
    let strength = hint_strength(elapsed, fade);
    if elapsed < HINT_SECONDS {
        motion::wake_at(ctx, since + HINT_SECONDS);
    } else if strength > 0.0 {
        motion::animate(ctx);
    }
    let screen = ctx.content_rect();
    let at_edge = ctx
        .input(|i| i.pointer.hover_pos())
        .is_some_and(|p| p.y <= screen.top() + EDGE);
    let held_before = HELD.get(ctx).unwrap_or(false);
    let held = at_edge || held_before;
    if !held && strength <= 0.0 {
        return;
    }
    let r = egui::Area::new(Id::new("egui_pop.zen.way_out"))
        .order(Order::Foreground)
        .anchor(Align2::CENTER_TOP, Vec2::ZERO)
        .show(ctx, |ui| {
            if !held {
                ui.multiply_opacity(strength);
            }
            let shadow = ui.visuals().popup_shadow;
            Kit::of(ctx)
                .fullscreen_exit
                .show(ui, State::REST, shadow, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(hint(app, ctx));
                        let english = app
                            .command(&CMD_LEAVE_ZEN)
                            .map(|c| c.title)
                            .unwrap_or_default();
                        let leave = strings::command_title(ctx, CMD_LEAVE_ZEN.as_str(), &english);
                        if ui.button(leave).clicked() {
                            app.run(CMD_LEAVE_ZEN, Json::Null);
                        }
                    });
                })
        });
    let keep = at_edge || r.response.contains_pointer();
    if keep != held_before {
        // Shown or hidden on the next pass, without waiting for input.
        ctx.request_repaint();
    }
    HELD.set(ctx, keep);
}

/// How strong the hint is `elapsed` seconds after zen mode started:
/// whole for [`HINT_SECONDS`], then fading over `fade` seconds (none:
/// gone at once).
fn hint_strength(elapsed: f64, fade: f64) -> f32 {
    if elapsed < HINT_SECONDS {
        1.0
    } else if fade <= 0.0 {
        0.0
    } else {
        (1.0 - (elapsed - HINT_SECONDS) / fade).clamp(0.0, 1.0) as f32
    }
}

/// "Zen mode: Escape to leave", with the first key leaving it (or
/// toggling it); without one, where the way out is.
fn hint(app: &AppHandle, ctx: &Context) -> String {
    let key = app
        .keys_for(&CMD_LEAVE_ZEN)
        .into_iter()
        .chain(app.keys_for(&CMD_ZEN))
        .next();
    match key {
        Some(k) => strings::tr_args(ctx, &txt::ZEN_HINT, &[("keys", &k)]),
        None => strings::tr(ctx, &txt::ZEN_HINT_NO_KEY),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_hint_stays_three_seconds_then_fades_or_goes_at_once() {
        assert_eq!(hint_strength(0.0, 0.2), 1.0);
        assert_eq!(hint_strength(2.9, 0.2), 1.0);
        assert!((hint_strength(3.1, 0.2) - 0.5).abs() < 1e-4);
        assert_eq!(hint_strength(3.2, 0.2), 0.0);
        // Reduced motion: no fade.
        assert_eq!(hint_strength(3.0, 0.0), 0.0);
    }
}
