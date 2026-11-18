//! Listener and pass times: a fixed log histogram of relaxed atomics,
//! cheap enough to record every run (a bucket index and a few adds).

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use bus_contract::TimingStats;

/// Buckets of √2 steps (as halves of each power of two) from 1 µs
/// (1024 ns): bucket 0 holds what took under 1 µs, the last one
/// everything from about 17 s up.
const BUCKETS: usize = 50;

/// The power of two bucket 1 starts at (1024 ns).
const FIRST: u32 = 10;

/// How long something took, over every run since the last reset.
pub(crate) struct Stats {
    count: AtomicU64,
    total_ns: AtomicU64,
    max_ns: AtomicU64,
    buckets: [AtomicU64; BUCKETS],
}

impl Default for Stats {
    fn default() -> Stats {
        Stats {
            count: AtomicU64::new(0),
            total_ns: AtomicU64::new(0),
            max_ns: AtomicU64::new(0),
            buckets: std::array::from_fn(|_| AtomicU64::new(0)),
        }
    }
}

/// The bucket of a time in nanoseconds.
fn bucket(ns: u64) -> usize {
    if ns < 1 << FIRST {
        return 0;
    }
    let power = 63 - ns.leading_zeros();
    // The bit under the top one: the upper half of the power of two.
    let half = (ns >> (power - 1)) & 1;
    (1 + 2 * (power - FIRST) as usize + half as usize).min(BUCKETS - 1)
}

/// The upper bound of a bucket, in nanoseconds.
fn upper(bucket: usize) -> u64 {
    if bucket == 0 {
        return 1 << FIRST;
    }
    let base = 1u64 << ((bucket - 1) / 2 + FIRST as usize);
    if (bucket - 1).is_multiple_of(2) {
        base + base / 2
    } else {
        base * 2
    }
}

fn nanos(d: Duration) -> u64 {
    u64::try_from(d.as_nanos()).unwrap_or(u64::MAX)
}

impl Stats {
    pub(crate) fn record(&self, took: Duration) {
        let ns = nanos(took);
        self.count.fetch_add(1, Ordering::Relaxed);
        self.total_ns.fetch_add(ns, Ordering::Relaxed);
        self.max_ns.fetch_max(ns, Ordering::Relaxed);
        self.buckets[bucket(ns)].fetch_add(1, Ordering::Relaxed);
    }

    pub(crate) fn count(&self) -> u64 {
        self.count.load(Ordering::Relaxed)
    }

    pub(crate) fn reset(&self) {
        self.count.store(0, Ordering::Relaxed);
        self.total_ns.store(0, Ordering::Relaxed);
        self.max_ns.store(0, Ordering::Relaxed);
        for b in &self.buckets {
            b.store(0, Ordering::Relaxed);
        }
    }

    /// The stats now: count, total and max exact, the quantiles the upper
    /// bound of their bucket (within √2), never over the max.
    pub(crate) fn read(&self) -> TimingStats {
        let counts: Vec<u64> = self
            .buckets
            .iter()
            .map(|b| b.load(Ordering::Relaxed))
            .collect();
        let max = self.max_ns.load(Ordering::Relaxed);
        let mut out = TimingStats::new();
        out.count = self.count.load(Ordering::Relaxed);
        out.total = Duration::from_nanos(self.total_ns.load(Ordering::Relaxed));
        out.max = Duration::from_nanos(max);
        let n: u64 = counts.iter().sum();
        let quantile = |q: f64| -> Duration {
            if n == 0 {
                return Duration::ZERO;
            }
            // The rank of the quantile, 1-based, rounded up.
            let rank = ((n as f64 * q).ceil() as u64).max(1);
            let mut seen = 0;
            for (i, c) in counts.iter().enumerate() {
                seen += c;
                if seen >= rank {
                    return Duration::from_nanos(upper(i).min(max));
                }
            }
            Duration::from_nanos(max)
        };
        out.p50 = quantile(0.5);
        out.p95 = quantile(0.95);
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn buckets_step_by_halves_of_powers_of_two() {
        assert_eq!(bucket(0), 0);
        assert_eq!(bucket(1023), 0);
        assert_eq!(bucket(1024), 1);
        assert_eq!(bucket(1535), 1);
        assert_eq!(bucket(1536), 2);
        assert_eq!(bucket(2048), 3);
        assert_eq!(bucket(u64::MAX), BUCKETS - 1);
        for b in 0..BUCKETS - 1 {
            // Every bucket's upper bound starts the next one.
            assert_eq!(bucket(upper(b)), b + 1, "bucket {b}");
            assert_eq!(bucket(upper(b) - 1), b, "bucket {b}");
        }
        assert!(
            upper(BUCKETS - 2) > 15_000_000_000,
            "the last bucket is past 15 s"
        );
    }

    #[test]
    fn quantiles_read_within_a_bucket_and_never_over_the_max() {
        let s = Stats::default();
        assert_eq!(s.read(), TimingStats::new());
        for _ in 0..19 {
            s.record(Duration::from_micros(100));
        }
        s.record(Duration::from_millis(5));
        let r = s.read();
        assert_eq!(r.count, 20);
        assert_eq!(r.max, Duration::from_millis(5));
        assert_eq!(r.total, Duration::from_micros(1900 + 5000));
        assert!(r.p50 >= Duration::from_micros(100) && r.p50 <= Duration::from_micros(142));
        assert!(r.p95 <= Duration::from_micros(142), "19 of 20 are fast");
        s.record(Duration::from_millis(5));
        assert_eq!(s.read().p95, Duration::from_millis(5), "capped at the max");
        s.reset();
        assert_eq!(s.read(), TimingStats::new());
    }
}
