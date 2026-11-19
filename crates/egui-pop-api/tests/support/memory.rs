//! An in-memory host journal and relay for the kernel (a window and a
//! pump thread, after bus-contract's reference implementation), as
//! bus-kernel's own tests have it: an application's journal for the
//! host journal and conformance tests.

#![allow(dead_code)]

use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex, Weak};
use std::thread;
use std::time::{Duration, Instant};

use bus_kernel::bus_contract::{AnyInput, Bus, FreshFn, PointKey};
use bus_kernel::{
    Cursor, IngestError, Journal, JournalEntry, Kernel, Lost, Refused, Relay, RelayedEntry,
    Resumed, Since, kind_matches,
};

// --- The host's journal ---

/// Each journal a run of its own, as a process restart would give.
static NEXT_RUN: AtomicU64 = AtomicU64::new(1);

/// A delivery the pump makes: one entry, to its point when it has one.
struct Queued {
    seq: u64,
    deliver: Option<(PointKey, AnyInput, FreshFn)>,
}

#[derive(Default)]
struct Log {
    /// The window, oldest first.
    entries: VecDeque<JournalEntry>,
    last: u64,
    delivered: u64,
    queue: VecDeque<Queued>,
}

struct Shared {
    kernel: Weak<Kernel>,
    log: Mutex<Log>,
    changed: Condvar,
    run: u64,
    window: usize,
}

/// A host's journal and relay over a kernel: it keeps the entries and
/// delivers them through the kernel from its pump thread.
pub struct Memory(Arc<Shared>);

impl Memory {
    pub fn new(kernel: Weak<Kernel>, window: usize) -> Memory {
        let shared = Arc::new(Shared {
            kernel,
            log: Mutex::default(),
            changed: Condvar::new(),
            run: NEXT_RUN.fetch_add(1, Ordering::Relaxed),
            window,
        });
        let pump = Arc::downgrade(&shared);
        thread::spawn(move || Shared::pump(pump));
        Memory(shared)
    }

    fn kernel(&self) -> Arc<Kernel> {
        self.0.kernel.upgrade().expect("the kernel is alive")
    }
}

impl Shared {
    fn record(
        &self,
        mut entry: JournalEntry,
        deliver: Option<(PointKey, AnyInput, FreshFn)>,
    ) -> u64 {
        let mut log = self.log.lock().unwrap();
        log.last += 1;
        entry.seq = log.last;
        let seq = entry.seq;
        log.entries.push_back(entry);
        while log.entries.len() > self.window {
            log.entries.pop_front();
        }
        log.queue.push_back(Queued { seq, deliver });
        self.changed.notify_all();
        seq
    }

    /// Delivers each entry in order through the kernel, until the
    /// journal is gone.
    fn pump(me: Weak<Shared>) {
        loop {
            let Some(me) = me.upgrade() else { return };
            let next = {
                let log = me.log.lock().unwrap();
                let (mut log, _) = me
                    .changed
                    .wait_timeout_while(log, Duration::from_millis(20), |l| l.queue.is_empty())
                    .unwrap();
                log.queue.pop_front()
            };
            let Some(q) = next else { continue };
            if let (Some((point, input, fresh)), Some(kernel)) = (q.deliver, me.kernel.upgrade()) {
                Bus::collect_any(&*kernel, point, input, fresh);
            }
            me.log.lock().unwrap().delivered = q.seq;
            me.changed.notify_all();
        }
    }
}

impl Journal for Memory {
    fn last_seq(&self) -> u64 {
        self.0.log.lock().unwrap().last
    }

    fn since(&self, seq: u64, limit: usize) -> Since {
        let log = self.0.log.lock().unwrap();
        let oldest = log.entries.front().map_or(log.last + 1, |e| e.seq);
        let entries = log
            .entries
            .iter()
            .filter(|e| e.seq > seq)
            .take(limit)
            .cloned()
            .collect();
        let mut out = Since::new(entries, log.last);
        out.gap = seq < log.last && oldest > seq + 1;
        out
    }

    fn wait(&self, patterns: &[&str], seq: u64, timeout: Duration) -> Option<JournalEntry> {
        let until = Instant::now() + timeout;
        let mut log = self.0.log.lock().unwrap();
        loop {
            let found = log.entries.iter().find(|e| {
                e.seq > seq
                    && (patterns.is_empty() || patterns.iter().any(|p| kind_matches(p, &e.kind)))
            });
            if let Some(e) = found {
                return Some(e.clone());
            }
            let left = until.checked_duration_since(Instant::now())?;
            log = self.0.changed.wait_timeout(log, left).unwrap().0;
        }
    }

    /// The kernel checked the point before calling.
    fn publish_any(
        &self,
        point: PointKey,
        input: AnyInput,
        fresh: FreshFn,
        cause: Option<u64>,
    ) -> Result<u64, Refused> {
        let mut entry = JournalEntry::new(0, point.id);
        entry.cause = cause;
        entry.input = Some(input.clone());
        Ok(self.0.record(entry, Some((point, input, fresh))))
    }

    fn flush(&self, timeout: Duration) -> bool {
        let log = self.0.log.lock().unwrap();
        let target = log.last;
        let (log, _) = self
            .0
            .changed
            .wait_timeout_while(log, timeout, |l| l.delivered < target)
            .unwrap();
        log.delivered >= target
    }
}

impl Relay for Memory {
    fn cursor(&self) -> Cursor {
        Cursor {
            run: self.0.run,
            seq: self.last_seq(),
        }
    }

    fn resume(&self, from: Cursor, limit: usize) -> Resumed {
        let (lost, after) = if from == Cursor::START {
            (None, 0)
        } else if from.run != self.0.run {
            (Some(Lost::Restarted), 0)
        } else {
            (self.since(from.seq, 0).gap.then_some(Lost::Gap), from.seq)
        };
        let kernel = self.kernel();
        let read = self.since(after, usize::MAX);
        let mut entries = Vec::new();
        let mut next = read.last_seq;
        for e in read.entries {
            if entries.len() == limit {
                next = e.seq - 1;
                break;
            }
            let Some((_, spec)) = kernel.declaration(&e.kind) else {
                continue;
            };
            let (Some(codec), None, Some(input)) = (spec.relay, &e.source, &e.input) else {
                continue;
            };
            let Some(text) = (codec.encode)(&**input) else {
                continue;
            };
            let mut out = RelayedEntry::new(e.seq, e.kind.clone(), text);
            out.cause = e.cause;
            entries.push(out);
        }
        let mut out = Resumed::new(
            entries,
            Cursor {
                run: self.0.run,
                seq: next,
            },
        );
        out.lost = lost;
        out
    }

    fn ingest(&self, source: &str, relayed: RelayedEntry) -> Result<u64, IngestError> {
        let mut entry = JournalEntry::new(0, relayed.kind.clone());
        entry.cause = relayed.cause;
        entry.source = Some(source.to_owned());
        let Some((key, spec)) = self.kernel().declaration(&relayed.kind) else {
            entry.text = Some(relayed.text);
            return Ok(self.0.record(entry, None));
        };
        let Some(codec) = spec.relay else {
            return Err(IngestError::NotRelayable { kind: relayed.kind });
        };
        let input = (codec.decode)(&relayed.text).map_err(|reason| IngestError::Decode {
            kind: relayed.kind.clone(),
            reason,
        })?;
        entry.input = Some(input.clone());
        entry.text = Some(relayed.text);
        Ok(self.0.record(entry, Some((key, input, codec.fresh))))
    }
}

/// A kernel with a host journal (and relay when `relay`) of `window`
/// entries.
pub fn kernel(relay: bool, window: usize) -> Arc<Kernel> {
    Arc::new_cyclic(|me| {
        let host = Arc::new(Memory::new(me.clone(), window));
        let k = Kernel::new().with_journal(host.clone());
        if relay { k.with_relay(host) } else { k }
    })
}

pub fn full() -> Arc<Kernel> {
    kernel(true, 1024)
}
