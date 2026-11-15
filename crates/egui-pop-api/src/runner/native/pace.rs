//! The frame pacer: at most one paint per window each interval.
//!
//! A window that asks to repaint every frame (an animation) is held back
//! by the present: with vsync it blocks until the next vertical blank, on
//! Wayland the compositor's frame callback paces it. Where neither holds
//! (an X server without DRI3, Mesa's software path, a present mode that
//! does not wait), nothing does, and the window paints as fast as the CPU
//! goes: a thousand frames a second and several cores spent drawing what
//! no screen shows. The pacer keeps a window's paints at least an
//! interval apart (a little under the fastest screen's refresh period),
//! holding a redraw that comes too early until its time. A paced present
//! never sees it: its redraws come one period apart already.

use std::collections::HashMap;
use std::hash::Hash;
use std::time::{Duration, Instant};

/// The refresh rate assumed when no screen tells its own.
const FALLBACK_HZ: f64 = 60.0;

/// The interval is this share of the refresh period, so that a present
/// paced by the screen (whose redraws come about one period apart, give or
/// take its jitter) is never held back.
const SHARE_OF_PERIOD: f64 = 0.85;

/// The interval between two paints of a window, for screens refreshing at
/// `refresh_mhz` (millihertz, 0 when unknown): from the fastest screen,
/// bounded to 30–360 Hz.
pub(super) fn interval(refresh_mhz: impl IntoIterator<Item = u32>) -> Duration {
    let hz = refresh_mhz
        .into_iter()
        .filter(|&m| m > 0)
        .map(|m| f64::from(m) / 1000.0)
        .fold(None, |best: Option<f64>, hz| {
            Some(best.map_or(hz, |b| b.max(hz)))
        })
        .unwrap_or(FALLBACK_HZ)
        .clamp(30.0, 360.0);
    Duration::from_secs_f64(SHARE_OF_PERIOD / hz)
}

/// When each window last painted, and the redraws held until their time.
pub(super) struct Pacer<K> {
    interval: Duration,
    last: HashMap<K, Instant>,
    held: HashMap<K, Instant>,
}

impl<K: Copy + Eq + Hash> Pacer<K> {
    pub(super) fn new(interval: Duration) -> Self {
        Self {
            interval,
            last: HashMap::new(),
            held: HashMap::new(),
        }
    }

    pub(super) fn set_interval(&mut self, interval: Duration) {
        self.interval = interval;
    }

    /// A redraw of `window` arrives at `now`: true when it paints now (and
    /// counts as its last paint), false when it is held until its time
    /// ([`Pacer::due`]).
    pub(super) fn arrive(&mut self, window: K, now: Instant) -> bool {
        let ready = self
            .last
            .get(&window)
            .is_none_or(|&last| now >= last + self.interval);
        if ready {
            self.held.remove(&window);
            self.last.insert(window, now);
        } else {
            let at = self.last[&window] + self.interval;
            self.held.insert(window, at);
        }
        ready
    }

    /// The held redraws whose time came at `now`, taken; each counts as a
    /// paint at `now`.
    pub(super) fn due(&mut self, now: Instant) -> Vec<K> {
        let due: Vec<K> = self
            .held
            .iter()
            .filter(|&(_, &at)| at <= now)
            .map(|(&w, _)| w)
            .collect();
        for w in &due {
            self.held.remove(w);
            self.last.insert(*w, now);
        }
        due
    }

    /// When the next held redraw is due.
    pub(super) fn next(&self) -> Option<Instant> {
        self.held.values().min().copied()
    }

    /// A destroyed window is forgotten.
    pub(super) fn forget(&mut self, window: K) {
        self.last.remove(&window);
        self.held.remove(&window);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MS: Duration = Duration::from_millis(1);

    #[test]
    fn the_interval_follows_the_fastest_screen() {
        assert_eq!(interval([60_000]), Duration::from_secs_f64(0.85 / 60.0));
        assert_eq!(
            interval([60_000, 144_000]),
            Duration::from_secs_f64(0.85 / 144.0)
        );
        // Unknown rates: 60 Hz; absurd ones are bounded.
        assert_eq!(interval([0]), Duration::from_secs_f64(0.85 / 60.0));
        assert_eq!(interval([]), Duration::from_secs_f64(0.85 / 60.0));
        assert_eq!(interval([1_000]), Duration::from_secs_f64(0.85 / 30.0));
        assert_eq!(interval([1_000_000]), Duration::from_secs_f64(0.85 / 360.0));
    }

    #[test]
    fn redraws_an_interval_apart_paint_at_once() {
        let t = Instant::now();
        let mut p = Pacer::new(10 * MS);
        assert!(p.arrive(1, t));
        assert!(p.arrive(1, t + 10 * MS));
        assert!(p.arrive(1, t + 25 * MS));
        assert_eq!(p.next(), None);
    }

    #[test]
    fn an_early_redraw_is_held_until_its_time() {
        let t = Instant::now();
        let mut p = Pacer::new(10 * MS);
        assert!(p.arrive(1, t));
        assert!(!p.arrive(1, t + MS));
        // A second early one is the same held redraw.
        assert!(!p.arrive(1, t + 2 * MS));
        assert_eq!(p.next(), Some(t + 10 * MS));
        assert!(p.due(t + 9 * MS).is_empty());
        assert_eq!(p.due(t + 10 * MS), vec![1]);
        assert_eq!(p.next(), None);
        // The held redraw counted as a paint.
        assert!(!p.arrive(1, t + 15 * MS));
    }

    #[test]
    fn windows_are_paced_apart() {
        let t = Instant::now();
        let mut p = Pacer::new(10 * MS);
        assert!(p.arrive(1, t));
        assert!(p.arrive(2, t + MS));
        assert!(!p.arrive(1, t + 2 * MS));
        assert!(p.arrive(2, t + 11 * MS));
        p.forget(1);
        assert_eq!(p.next(), None);
        assert!(p.arrive(1, t + 12 * MS));
    }
}
