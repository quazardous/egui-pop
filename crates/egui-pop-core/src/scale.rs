//! Scale, per window. Three scales multiply into egui's
//! `pixels_per_point` (ppp) for the window being drawn:
//!
//! - the **native** scale, the system's for the window's screen (winit's
//!   scale factor), which differs between windows on screens of
//!   different densities;
//! - the **interface zoom**, the user's "make everything bigger", one
//!   value for every window (egui's `zoom_factor`), moved in fixed
//!   [`ZOOM_STEPS`] so that each step's glyphs are rasterised once;
//! - text size, which egui-pop's theme applies to fonts only.
//!
//! Layout is in points and does not move when the scale changes; only
//! rasterisation does. Read the scale where you paint
//! (`ui.ctx().pixels_per_point()` is the current window's), never from a
//! value kept by an application-wide object. Caches sized in physical
//! pixels (thumbnails, textures) listen to [`changed`], once per change.
//!
//! Lines: a 1 point stroke is 1.25 pixels at 1.25, feathered over two
//! pixel columns into a grey line. [`stroke`] snaps a width as CSS snaps
//! border widths: to whole pixels, at least one.
//!
//! ```
//! use egui_pop_core::scale;
//!
//! assert_eq!(scale::snap(1.0, 1.25), 0.8); // one pixel
//! assert_eq!(scale::snap(1.0, 2.0), 1.0); // two pixels
//! assert_eq!(scale::snap(0.25, 1.0), 1.0); // never under one pixel
//! assert_eq!(scale::zoom_step(1.0, 1), 1.1);
//! assert_eq!(scale::zoom_step(2.0, 1), 2.0);
//! ```

use egui::{Context, Id, ViewportId};
use serde::Serialize;

/// The interface zoom's steps.
pub const ZOOM_STEPS: [f32; 10] = [0.67, 0.75, 0.8, 0.9, 1.0, 1.1, 1.25, 1.5, 1.75, 2.0];

/// The scales of the window being drawn.
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct Scale {
    /// The system's scale for this window's screen (1 when unknown).
    pub native: f32,
    /// The interface zoom (every window).
    pub zoom: f32,
    /// Physical pixels per point: native × zoom.
    pub pixels_per_point: f32,
}

impl Scale {
    /// The current window's scales.
    pub fn of(ctx: &Context) -> Scale {
        Scale {
            native: ctx.native_pixels_per_point().unwrap_or(1.0),
            zoom: ctx.zoom_factor(),
            pixels_per_point: ctx.pixels_per_point(),
        }
    }
}

/// A width of `w` points snapped at `ppp`: a whole number of physical
/// pixels, at least one, back in points.
pub fn snap(w: f32, ppp: f32) -> f32 {
    if ppp <= 0.0 {
        return w;
    }
    (w * ppp).floor().max(1.0) / ppp
}

/// A stroke width of `w` points that stays crisp in the current window.
pub fn stroke(ctx: &Context, w: f32) -> f32 {
    snap(w, ctx.pixels_per_point())
}

/// One physical pixel, in points, in the current window.
pub fn hairline(ctx: &Context) -> f32 {
    1.0 / ctx.pixels_per_point()
}

/// A side of `s` points rounded to whole physical pixels (an icon's box).
pub fn snap_size(s: f32, ppp: f32) -> f32 {
    if ppp <= 0.0 {
        return s;
    }
    (s * ppp).round().max(1.0) / ppp
}

#[derive(Clone, Copy, Debug)]
struct Seen {
    pass: u64,
    before: f32,
    now: f32,
}

/// The current window's scale change this pass: `(old, new)` pixels per
/// point, or `None`. Asked any number of times in one pass, it answers
/// the same; the first pass of a window reports no change.
pub fn changed(ctx: &Context) -> Option<(f32, f32)> {
    changed_in(ctx, ctx.viewport_id())
}

fn changed_in(ctx: &Context, viewport: ViewportId) -> Option<(f32, f32)> {
    let id = Id::new(("egui_pop_core::scale::seen", viewport));
    let pass = ctx.cumulative_pass_nr();
    let ppp = ctx.pixels_per_point();
    let seen = ctx.data(|d| d.get_temp::<Seen>(id));
    let next = match seen {
        Some(s) if s.pass == pass => s,
        Some(s) => Seen {
            pass,
            before: s.now,
            now: ppp,
        },
        None => Seen {
            pass,
            before: ppp,
            now: ppp,
        },
    };
    ctx.data_mut(|d| d.insert_temp(id, next));
    (next.before != next.now).then_some((next.before, next.now))
}

/// The zoom step `by` steps from `current` (the nearest step first),
/// clamped to the ends.
pub fn zoom_step(current: f32, by: i32) -> f32 {
    let nearest = ZOOM_STEPS
        .iter()
        .enumerate()
        .min_by(|a, b| {
            (a.1 - current)
                .abs()
                .partial_cmp(&(b.1 - current).abs())
                .unwrap_or(std::cmp::Ordering::Equal)
        })
        .map_or(4, |(i, _)| i) as i32;
    let i = (nearest + by).clamp(0, ZOOM_STEPS.len() as i32 - 1) as usize;
    ZOOM_STEPS[i]
}

/// Turns off egui's own zoom keys (Ctrl and `+`, `-`, `0`), which would
/// fire together with an application's content zoom; egui-pop's
/// interface zoom commands replace them.
pub fn disable_keyboard_zoom(ctx: &Context) {
    ctx.options_mut(|o| o.zoom_with_keyboard = false);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn widths_snap_to_whole_pixels_at_every_quarter_scale() {
        for ppp in [1.0, 1.25, 1.5, 1.75, 2.0] {
            for w in [0.5, 1.0, 1.5, 2.0] {
                let px = snap(w, ppp) * ppp;
                assert!((px - px.round()).abs() < 1e-4, "{w} at {ppp}: {px}");
                assert!(px >= 1.0);
            }
        }
        assert_eq!(snap(1.0, 1.5) * 1.5, 1.0);
        assert_eq!(snap(2.0, 1.5) * 1.5, 3.0);
    }

    #[test]
    fn zoom_moves_by_steps_from_the_nearest() {
        assert_eq!(zoom_step(1.0, 0), 1.0);
        assert_eq!(zoom_step(1.0, -1), 0.9);
        assert_eq!(zoom_step(1.07, 1), 1.25);
        assert_eq!(zoom_step(0.67, -3), 0.67);
        assert_eq!(zoom_step(1.3, 0), 1.25);
    }

    #[test]
    fn a_change_is_reported_once_per_pass_and_window() {
        let ctx = Context::default();
        let mut input = egui::RawInput::default();
        let mut seen = Vec::new();
        for (i, zoom) in [1.0, 1.0, 1.5, 1.5].into_iter().enumerate() {
            ctx.set_zoom_factor(zoom);
            let mut out = ctx.run_ui(input.take(), |ui| {
                let a = changed(ui.ctx());
                let b = changed(ui.ctx());
                assert_eq!(a, b, "asked twice in one pass");
                seen.push((i, a));
            });
            out.textures_delta.clear();
        }
        assert_eq!(seen[0].1, None);
        assert_eq!(seen[1].1, None);
        assert_eq!(seen[2].1, Some((1.0, 1.5)));
        assert_eq!(seen[3].1, None);
    }
}
