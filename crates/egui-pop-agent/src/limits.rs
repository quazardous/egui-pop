//! What the transport holds every client to, whatever the platform: how
//! many may be connected at once, and how long a request may be.
//!
//! Each connection serves one request at a time, so the waits held at
//! once are bounded by the connections too.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

/// The most clients connected at once; one more is answered with an
/// error ([`crate::RpcError::LIMIT`]) and closed.
pub const MAX_CONNECTIONS: usize = 8;
/// The longest request line, in bytes, its `\n` excluded; a longer one
/// is answered with an error ([`crate::RpcError::LIMIT`]) and its
/// connection closed.
pub const MAX_REQUEST_BYTES: usize = 1 << 20;

/// The connections open now.
#[derive(Clone, Debug, Default)]
pub(crate) struct Connections(Arc<AtomicUsize>);

impl Connections {
    /// A place for one more connection, `None` when [`MAX_CONNECTIONS`]
    /// are open; it frees its place when dropped.
    pub(crate) fn take(&self) -> Option<Slot> {
        self.0
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |n| {
                (n < MAX_CONNECTIONS).then_some(n + 1)
            })
            .ok()
            .map(|_| Slot(self.0.clone()))
    }

    /// How many are open.
    #[cfg(test)]
    pub(crate) fn open(&self) -> usize {
        self.0.load(Ordering::Acquire)
    }
}

/// One connection's place, freed when dropped.
#[derive(Debug)]
pub(crate) struct Slot(Arc<AtomicUsize>);

impl Drop for Slot {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::AcqRel);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_place_is_given_up_to_the_cap_and_freed_when_dropped() {
        let c = Connections::default();
        let slots: Vec<Slot> = (0..MAX_CONNECTIONS).map(|_| c.take().unwrap()).collect();
        assert_eq!(c.open(), MAX_CONNECTIONS);
        assert!(c.take().is_none());
        drop(slots);
        assert_eq!(c.open(), 0);
        assert!(c.take().is_some());
    }
}
