//! The motion clock: where spinners, travelling bars, shimmer and the
//! busy delay take their time from, instead of `input.time` directly.
//!
//! - [`now`]: the clock's time, in seconds: the interface's time, or a
//!   clock the application or a test gives ([`set_clock`], [`freeze`]).
//!   A given clock moves only when told: a test freezes motion, steps
//!   it, and gets the same frames on every run;
//! - [`phase`]: where a looping animation is (0 to 1), static under
//!   reduced motion (`motion.duration` is 0) so spinners and bars give
//!   still frames;
//! - [`animate`]: asks for the next frame of an animation shown: never
//!   at once (a frame later, at most 60 a second; four a second while
//!   the window is not focused), never under reduced motion or a given
//!   clock; [`wake_at`] asks for the frame where something changes;
//! - [`Busy`]: the delayed busy indicator: nothing before
//!   `motion.spinner.delay` (300 ms), then shown at least
//!   `motion.spinner.min` (500 ms), counted from when the work started
//!   (a job's or a command's start, not when the widget was first
//!   drawn).
//!
//! ```
//! use egui_pop_core::motion::{self, Busy};
//! use egui_pop_model::clock::ManualClock;
//!
//! let ctx = egui::Context::default();
//! let clock = ManualClock::new(0.0);
//! motion::set_clock(&ctx, Some(clock.shared()));
//! let id = egui::Id::new("export");
//! clock.set(0.29);
//! assert!(!Busy::new(id, 0.0).visible(&ctx, true), "not before 300 ms");
//! clock.set(0.31);
//! assert!(Busy::new(id, 0.0).visible(&ctx, true));
//! // The work ends at once: the indicator stays 500 ms.
//! clock.set(0.4);
//! assert!(Busy::new(id, 0.0).visible(&ctx, false));
//! clock.set(0.82);
//! assert!(!Busy::new(id, 0.0).visible(&ctx, false));
//! ```

use std::collections::HashMap;
use std::time::Duration;

use egui::{Context, Id};
use egui_pop_model::clock::{ManualClock, SharedClock};

use crate::state::{lock, shared};
use crate::theme::Tokens;

/// Frames of an animation, at most this often (seconds).
pub const FRAME: f64 = 1.0 / 60.0;
/// Frames while the window is not focused.
pub const BACKGROUND_FRAME: f64 = 0.25;

#[derive(Default)]
struct State {
    clock: Option<SharedClock>,
    /// When each busy indicator was first shown, and when its work
    /// ended.
    busy: HashMap<Id, (Option<f64>, Option<f64>)>,
}

impl std::fmt::Debug for State {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("State")
            .field("clock", &self.clock.is_some())
            .field("busy", &self.busy.len())
            .finish()
    }
}

fn state(ctx: &Context) -> std::sync::Arc<std::sync::Mutex<State>> {
    shared::<State>(ctx, "core.motion")
}

/// Gives the motion clock a clock of its own (the application's, a
/// test's), or gives it back the interface's time (`None`).
pub fn set_clock(ctx: &Context, clock: Option<SharedClock>) {
    lock(&state(ctx)).clock = clock;
    ctx.request_repaint();
}

/// Stops the motion clock at `at` seconds; returns the clock, to step
/// it by hand.
pub fn freeze(ctx: &Context, at: f64) -> ManualClock {
    let clock = ManualClock::new(at);
    set_clock(ctx, Some(clock.shared()));
    clock
}

/// Whether a clock was given (motion moves only when told).
pub fn controlled(ctx: &Context) -> bool {
    lock(&state(ctx)).clock.is_some()
}

/// Now, by the motion clock, in seconds.
pub fn now(ctx: &Context) -> f64 {
    let clock = lock(&state(ctx)).clock.clone();
    match clock {
        Some(c) => c.now(),
        None => ctx.input(|i| i.time),
    }
}

/// Whether motion is reduced (`motion.duration` is 0): animations give
/// still frames.
pub fn reduced(ctx: &Context) -> bool {
    Tokens::of(ctx).motion_duration <= 0.0
}

/// Where a looping animation of `period` seconds is, from 0 to 1;
/// `still` under reduced motion.
pub fn phase(ctx: &Context, period: f64, still: f32) -> f32 {
    if reduced(ctx) || period <= 0.0 {
        return still;
    }
    (now(ctx) / period).rem_euclid(1.0) as f32
}

/// Asks for the next frame of an animation on screen: a frame later
/// (never at once, so a headless harness settles), four a second while
/// the window is not focused; nothing under reduced motion or a given
/// clock.
pub fn animate(ctx: &Context) {
    if reduced(ctx) || controlled(ctx) {
        return;
    }
    let focused = ctx.input(|i| i.focused);
    let after = if focused { FRAME } else { BACKGROUND_FRAME };
    after_frame(ctx, after);
}

/// Asks for a frame when the motion clock reaches `at` (seconds): the
/// moment a delayed indicator shows, or work ends. Nothing under a given
/// clock: it moves only when told, and whoever moves it asks for the
/// frame.
pub fn wake_at(ctx: &Context, at: f64) {
    if controlled(ctx) {
        return;
    }
    let wait = at - now(ctx);
    if wait > 0.0 {
        after_frame(ctx, wait);
    }
}

/// A repaint `seconds` after this frame. egui counts a delay from the
/// next frame (it takes the predicted frame time off); it is put back
/// so the delay never becomes an immediate repaint.
fn after_frame(ctx: &Context, seconds: f64) {
    let frame = f64::from(ctx.input(|i| i.predicted_dt));
    ctx.request_repaint_after(Duration::from_secs_f64(seconds.max(0.0) + frame));
}

/// A delayed busy indicator for work that started at `since` (motion
/// clock seconds): shown after the delay, kept for the minimum once
/// shown. Its state is kept per id.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Busy {
    pub id: Id,
    pub since: f64,
    /// Seconds before showing (`motion.spinner.delay` by default).
    pub delay: Option<f64>,
    /// Seconds shown at least (`motion.spinner.min` by default).
    pub min: Option<f64>,
}

impl Busy {
    pub fn new(id: Id, since: f64) -> Self {
        Self {
            id,
            since,
            delay: None,
            min: None,
        }
    }

    pub fn delay(mut self, seconds: f64) -> Self {
        self.delay = Some(seconds);
        self
    }

    pub fn min(mut self, seconds: f64) -> Self {
        self.min = Some(seconds);
        self
    }

    /// Whether the indicator shows now, the work going on (`busy`) or
    /// not; asks for the frame where that changes.
    pub fn visible(&self, ctx: &Context, busy: bool) -> bool {
        let t = Tokens::of(ctx);
        let delay = self.delay.unwrap_or(f64::from(t.motion.spinner_delay));
        let min = self.min.unwrap_or(f64::from(t.motion.spinner_min));
        let now = now(ctx);
        let (visible, wake) = self.decide(ctx, busy, now, delay, min);
        // The state's lock is let go before asking for a frame.
        if let Some(at) = wake {
            wake_at(ctx, at);
        }
        visible
    }

    /// Whether it shows, and when that changes next.
    fn decide(
        &self,
        ctx: &Context,
        busy: bool,
        now: f64,
        delay: f64,
        min: f64,
    ) -> (bool, Option<f64>) {
        let st = state(ctx);
        let mut s = lock(&st);
        let entry = s.busy.entry(self.id).or_insert((None, None));
        if busy {
            entry.1 = None;
            if entry.0.is_none() {
                let at = self.since + delay;
                if now < at {
                    return (false, Some(at));
                }
                entry.0 = Some(now);
            }
            return (true, None);
        }
        let Some(from) = entry.0 else {
            s.busy.remove(&self.id);
            return (false, None);
        };
        let until = from + min;
        if now < until {
            entry.1 = Some(entry.1.unwrap_or(now));
            (true, Some(until))
        } else {
            s.busy.remove(&self.id);
            (false, None)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_short_job_never_shows_and_a_long_one_stays_its_minimum() {
        let ctx = Context::default();
        let clock = freeze(&ctx, 10.0);
        let id = Id::new("a");
        // Done in 200 ms: nothing ever shown.
        clock.set(10.2);
        assert!(!Busy::new(id, 10.0).visible(&ctx, true));
        assert!(!Busy::new(id, 10.0).visible(&ctx, false));
        // A long one shows after the delay, until it ends.
        let id = Id::new("b");
        clock.set(11.0);
        assert!(Busy::new(id, 10.0).visible(&ctx, true));
        clock.set(12.0);
        assert!(Busy::new(id, 10.0).visible(&ctx, true));
        assert!(!Busy::new(id, 10.0).visible(&ctx, false), "shown 1 s");
    }

    #[test]
    fn a_frozen_clock_gives_the_same_phase_and_reduced_motion_a_still_one() {
        let ctx = Context::default();
        let clock = freeze(&ctx, 0.25);
        assert_eq!(phase(&ctx, 1.0, 0.0), 0.25);
        clock.advance(0.5);
        assert_eq!(phase(&ctx, 1.0, 0.0), 0.75);
        assert!(controlled(&ctx));
        crate::theme::install(
            &ctx,
            &crate::theme::ThemeSpec::new("pop-dark").reduced_motion(true),
        );
        assert!(reduced(&ctx));
        assert_eq!(phase(&ctx, 1.0, 0.4), 0.4);
        set_clock(&ctx, None);
        assert!(!controlled(&ctx));
    }
}
