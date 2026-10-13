//! Clocks: where "now" comes from, injected so tests can fix it.
//!
//! The jobs model (time left, how long a job ran) and the motion clock
//! (spinners, delays, shimmer) read a [`Clock`] instead of the system's
//! time: an application gives none and gets the interface's own time; a
//! test gives a [`ManualClock`] and moves it by hand.
//!
//! ```
//! use egui_pop_model::clock::{Clock, ManualClock};
//!
//! let clock = ManualClock::new(10.0);
//! clock.advance(0.25);
//! assert_eq!(clock.now(), 10.25);
//! ```

use std::fmt;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

/// A source of time, in seconds from an origin of its own.
pub trait Clock: Send + Sync {
    /// Now, in seconds.
    fn now(&self) -> f64;
}

/// A clock shared between its users.
pub type SharedClock = Arc<dyn Clock>;

/// A clock that moves only when told: tests freeze, set and step it.
/// Clones share the same time.
#[derive(Clone, Default)]
pub struct ManualClock(Arc<AtomicU64>);

impl fmt::Debug for ManualClock {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "ManualClock({})", self.now())
    }
}

impl ManualClock {
    /// A clock showing `now` seconds.
    pub fn new(now: f64) -> Self {
        let c = Self::default();
        c.set(now);
        c
    }

    /// Sets the time.
    pub fn set(&self, now: f64) {
        self.0.store(now.to_bits(), Ordering::Relaxed);
    }

    /// Moves the time on by `seconds`.
    pub fn advance(&self, seconds: f64) {
        self.set(self.now() + seconds);
    }

    /// Shared, as the jobs model and the motion clock take it.
    pub fn shared(&self) -> SharedClock {
        Arc::new(self.clone())
    }
}

impl Clock for ManualClock {
    fn now(&self) -> f64 {
        f64::from_bits(self.0.load(Ordering::Relaxed))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clones_share_the_time() {
        let a = ManualClock::new(1.0);
        let b = a.clone();
        b.advance(2.5);
        assert_eq!(a.now(), 3.5);
        assert_eq!(a.shared().now(), 3.5);
    }
}
