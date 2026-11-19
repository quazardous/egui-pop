//! The connections open now, shared by the transport's threads and the
//! UI thread: which agent each one said `hello` as, how many each agent
//! has, and revoking one, which closes its connections.

use std::collections::BTreeMap;
use std::rc::Rc;
use std::sync::{Arc, Mutex};

use egui_pop_api::AppHandle;

/// Closes a connection from another thread (its reading side ends).
pub(crate) type Closer = Box<dyn Fn() + Send>;

struct Conn {
    agent: Option<String>,
    revoked: bool,
    close: Option<Closer>,
}

#[derive(Default)]
struct State {
    next: u64,
    conns: BTreeMap<u64, Conn>,
}

/// The open connections.
#[derive(Clone, Default)]
pub(crate) struct Live(Arc<Mutex<State>>);

impl std::fmt::Debug for Live {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Live")
    }
}

impl Live {
    fn with<R>(&self, f: impl FnOnce(&mut State) -> R) -> R {
        let mut s = self
            .0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        f(&mut s)
    }

    /// A connection opened; `close` ends it from another thread, where
    /// the transport can.
    pub(crate) fn open(&self, close: Option<Closer>) -> u64 {
        self.with(|s| {
            s.next += 1;
            let n = s.next;
            s.conns.insert(
                n,
                Conn {
                    agent: None,
                    revoked: false,
                    close,
                },
            );
            n
        })
    }

    /// The connection said `hello` as `agent`.
    pub(crate) fn named(&self, n: u64, agent: &str) {
        self.with(|s| {
            if let Some(c) = s.conns.get_mut(&n) {
                c.agent = Some(agent.into());
            }
        });
    }

    /// The connection closed.
    pub(crate) fn end(&self, n: u64) {
        self.with(|s| s.conns.remove(&n));
    }

    /// Whether the connection's pairing was revoked: it serves nothing
    /// more.
    pub(crate) fn revoked(&self, n: u64) -> bool {
        self.with(|s| s.conns.get(&n).is_some_and(|c| c.revoked))
    }

    /// How many connections `agent` has open.
    pub(crate) fn count(&self, agent: &str) -> usize {
        self.with(|s| {
            s.conns
                .values()
                .filter(|c| !c.revoked && c.agent.as_deref() == Some(agent))
                .count()
        })
    }

    /// Closes `agent`'s connections; how many.
    pub(crate) fn revoke(&self, agent: &str) -> usize {
        self.with(|s| {
            let mut n = 0;
            for c in s.conns.values_mut() {
                if !c.revoked && c.agent.as_deref() == Some(agent) {
                    c.revoked = true;
                    n += 1;
                    if let Some(close) = &c.close {
                        close();
                    }
                }
            }
            n
        })
    }
}

/// The application's connections, set when its socket plugin connects.
#[derive(Default)]
struct Slot(std::cell::RefCell<Option<Live>>);

fn slot(app: &AppHandle) -> Rc<Slot> {
    app.local::<Slot>()
}

/// The application's connections (made the first time).
pub(crate) fn of(app: &AppHandle) -> Live {
    slot(app)
        .0
        .borrow_mut()
        .get_or_insert_with(Live::default)
        .clone()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[test]
    fn revoking_closes_the_agents_connections_only() {
        let live = Live::default();
        let closed = Arc::new(AtomicUsize::new(0));
        let closer = || -> Option<Closer> {
            let c = closed.clone();
            Some(Box::new(move || {
                c.fetch_add(1, Ordering::Relaxed);
            }))
        };
        let a = live.open(closer());
        let b = live.open(closer());
        let other = live.open(closer());
        live.open(None);
        live.named(a, "bot");
        live.named(b, "bot");
        live.named(other, "other");
        assert_eq!(live.count("bot"), 2);
        assert_eq!(live.revoke("bot"), 2);
        assert_eq!(closed.load(Ordering::Relaxed), 2);
        assert!(live.revoked(a) && live.revoked(b) && !live.revoked(other));
        assert_eq!(live.count("bot"), 0);
        assert_eq!(live.count("other"), 1);
        live.end(a);
        assert!(!live.revoked(a));
    }
}
