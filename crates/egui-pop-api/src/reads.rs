//! The read kernel: what a page reads from the host (a status's value, a
//! list's rows), kept in one place and read away from the UI thread.
//!
//! <div class="warning">Experimental: may change in any release
//! (docs/stability.md).</div>
//!
//! - **Keyed**: a read is a command and its arguments. Two pages asking
//!   the same thing share one entry, and one request while it is in
//!   flight.
//! - **At once**: [`ReadKernel::read`] never waits. It answers what it
//!   holds now with a [`ReadState`]: `Pending` (nothing yet), `Fresh`,
//!   `Refreshing` (the last value, known to be out of date, while a new
//!   one is read) or `Failed` (the last read failed; the value before it,
//!   if any, is kept).
//! - **Stale while revalidating**: a value is never dropped for being
//!   old. Out of date, it is shown until the new one comes.
//! - **Policies**: the host says, per command, how long a value stays
//!   fresh ([`ReadPolicy::fresh_for`]) and which events make it out of
//!   date ([`ReadPolicy::invalidated_by`], `name` or `prefix.*`). A
//!   value past its freshness is read again in the background while it
//!   is shown, and keeps the state `Fresh` meanwhile; one an event made
//!   out of date is `Refreshing`. Without a freshness, a value stays
//!   fresh until an event or [`ReadKernel::invalidate_all`].
//! - **The host contract**: the host gives a [`ReadTransport`], called
//!   on a thread of the kernel's (at once on the web, or when no thread
//!   starts). The kernel asks the window to repaint when an answer comes,
//!   and when a value shown will be past its freshness.
//!
//! The Settings app reads through it the commands its transport serves
//! ([`crate::admin::Admin::attach_reads`]); other commands keep being run
//! on the command bus.
//!
//! The cache is a map under a lock, not a cache crate: the entries are a
//! page's reads (tens, not millions), they are never evicted (a stale
//! value is what is shown while revalidating), and in-flight sharing
//! must not block the caller, which the caches surveyed (`moka`,
//! `quick_cache`) do by waiting on the first loader.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use egui_pop_core::lock;
use serde_json::{Value as Json, json};
use web_time::Instant;

/// What the host reads for the kernel: its node, its database, a
/// service. Called away from the UI thread; may take its time.
pub trait ReadTransport: Send + Sync + 'static {
    /// Whether this transport reads `command`; the others are not the
    /// kernel's.
    fn serves(&self, command: &str) -> bool;

    /// Reads `command` with `args` (an object, `{}` without arguments).
    ///
    /// # Errors
    /// The read failed: the text is shown in place of the value, or
    /// beside the value kept from before.
    fn read(&self, command: &str, args: &Json) -> Result<Json, String>;
}

/// How a command's reads are kept.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ReadPolicy {
    /// How long a value stays fresh; `None`: until an event says it
    /// changed.
    pub fresh_for: Option<Duration>,
    /// The events that make its values out of date: `name` or
    /// `prefix.*`.
    pub invalidated_by: Vec<String>,
}

impl ReadPolicy {
    pub fn new() -> Self {
        Self::default()
    }

    /// Fresh for `d`, then read again in the background while shown.
    #[must_use]
    pub fn fresh_for(mut self, d: Duration) -> Self {
        self.fresh_for = Some(d);
        self
    }

    /// Out of date after any of these events (`name` or `prefix.*`).
    #[must_use]
    pub fn invalidated_by(mut self, patterns: &[&str]) -> Self {
        self.invalidated_by
            .extend(patterns.iter().map(|p| (*p).to_string()));
        self
    }
}

/// Where a read stands.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReadState {
    /// Nothing read yet: being read.
    Pending,
    /// The value is fresh (or read again in the background past its
    /// freshness, as nothing said it changed).
    Fresh,
    /// The value shown is out of date (an event said so) while a new one
    /// is read.
    Refreshing,
    /// The last read failed; the value read before it, if any, is kept.
    Failed,
}

/// A read's answer now.
#[derive(Clone, Debug, PartialEq)]
pub struct Read {
    pub state: ReadState,
    /// The last value read, if any.
    pub value: Option<Json>,
    /// The last read's error, when it failed.
    pub error: Option<String>,
}

impl Read {
    /// What a page shows: the value, else the error, else nothing yet.
    pub fn answer(&self) -> Option<Result<Json, String>> {
        match (&self.value, &self.error) {
            (Some(v), _) => Some(Ok(v.clone())),
            (None, Some(e)) => Some(Err(e.clone())),
            (None, None) => None,
        }
    }
}

#[derive(Debug, Default)]
struct Entry {
    command: String,
    value: Option<Json>,
    error: Option<String>,
    /// When the last answer came.
    at: Option<Instant>,
    /// An event made it out of date.
    stale: bool,
    /// The read in flight, by its number.
    flight: Option<u64>,
    /// An event came while a read was in flight: its answer may predate
    /// the change, so it is out of date as it lands.
    again: bool,
}

impl Entry {
    fn read(&self) -> Read {
        let state = if self.error.is_some() && self.flight.is_none() {
            ReadState::Failed
        } else if self.value.is_none() && self.error.is_none() {
            ReadState::Pending
        } else if self.stale {
            ReadState::Refreshing
        } else {
            ReadState::Fresh
        };
        Read {
            state,
            value: self.value.clone(),
            error: self.error.clone(),
        }
    }
}

#[derive(Default)]
struct Inner {
    entries: BTreeMap<String, Entry>,
    policies: BTreeMap<String, ReadPolicy>,
    next: u64,
    fetches: u64,
}

/// The kernel: the host's [`ReadTransport`], the policies and the
/// values read. Shared ([`Arc`]); every method takes `&self`.
pub struct ReadKernel {
    transport: Arc<dyn ReadTransport>,
    inner: Arc<Mutex<Inner>>,
    repaint: Arc<Mutex<Option<egui::Context>>>,
}

impl std::fmt::Debug for ReadKernel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let i = lock(&self.inner);
        f.debug_struct("ReadKernel")
            .field("entries", &i.entries.len())
            .field("policies", &i.policies)
            .finish_non_exhaustive()
    }
}

/// A read's key: its command and arguments (`{}` for none).
fn key_of(command: &str, args: &Json) -> (String, Json) {
    let args = if args.is_null() {
        json!({})
    } else {
        args.clone()
    };
    (format!("{command} {args}"), args)
}

/// Whether `name` matches `pattern` (`name`, or `prefix.*`).
pub fn matches(pattern: &str, name: &str) -> bool {
    match pattern.strip_suffix('*') {
        Some(prefix) => name.starts_with(prefix),
        None => name == pattern,
    }
}

impl ReadKernel {
    pub fn new(transport: Arc<dyn ReadTransport>) -> Self {
        Self {
            transport,
            inner: Arc::default(),
            repaint: Arc::default(),
        }
    }

    /// Sets `command`'s policy (a later one replaces it).
    pub fn policy(&self, command: &str, policy: ReadPolicy) {
        lock(&self.inner).policies.insert(command.into(), policy);
    }

    /// Whether the kernel reads `command` (its transport serves it).
    pub fn serves(&self, command: &str) -> bool {
        self.transport.serves(command)
    }

    /// The context repainted when an answer comes.
    pub fn set_context(&self, ctx: &egui::Context) {
        let mut r = lock(&self.repaint);
        if r.is_none() {
            *r = Some(ctx.clone());
        }
    }

    /// What `command` with `args` answers now, never waiting: a read
    /// starts when there is nothing yet, when the value is out of date
    /// or past its freshness, and none is in flight.
    pub fn read(&self, command: &str, args: &Json) -> Read {
        let (key, args) = key_of(command, args);
        let now = Instant::now();
        let (read, start, wake_in) = {
            let mut i = lock(&self.inner);
            let fresh_for = i.policies.get(command).and_then(|p| p.fresh_for);
            let e = i.entries.entry(key.clone()).or_insert_with(|| Entry {
                command: command.into(),
                ..Entry::default()
            });
            let expired = |at: Instant| fresh_for.is_some_and(|f| now.duration_since(at) >= f);
            let due = match e.at {
                None => true,
                Some(at) => e.stale || expired(at),
            };
            let mut start = None;
            if due && e.flight.is_none() {
                start = Some(());
            }
            let wake_in = match (e.at, fresh_for) {
                (Some(at), Some(f)) if !due => Some(f.saturating_sub(now.duration_since(at))),
                _ => None,
            };
            let start = start.map(|()| {
                i.next += 1;
                i.fetches += 1;
                let n = i.next;
                if let Some(e) = i.entries.get_mut(&key) {
                    e.flight = Some(n);
                }
                n
            });
            let read = i.entries.get(&key).map(Entry::read).unwrap_or(Read {
                state: ReadState::Pending,
                value: None,
                error: None,
            });
            (read, start, wake_in)
        };
        if let (Some(d), Some(ctx)) = (wake_in, lock(&self.repaint).as_ref()) {
            ctx.request_repaint_after(d);
        }
        if let Some(n) = start {
            self.fetch(key.clone(), command.to_string(), args, n);
            // A read run at once (on the web) answers this frame.
            if let Some(e) = lock(&self.inner).entries.get(&key) {
                return e.read();
            }
        }
        read
    }

    /// What `command` with `args` answers now, starting nothing.
    pub fn peek(&self, command: &str, args: &Json) -> Read {
        let (key, _) = key_of(command, args);
        lock(&self.inner).entries.get(&key).map_or(
            Read {
                state: ReadState::Pending,
                value: None,
                error: None,
            },
            Entry::read,
        )
    }

    /// An event happened: the values of the commands whose policy names
    /// it are out of date. How many were.
    pub fn event(&self, name: &str) -> usize {
        let mut i = lock(&self.inner);
        let hit: Vec<String> = i
            .policies
            .iter()
            .filter(|(_, p)| p.invalidated_by.iter().any(|pat| matches(pat, name)))
            .map(|(c, _)| c.clone())
            .collect();
        let mut n = 0;
        for e in i.entries.values_mut() {
            if hit.contains(&e.command) {
                Self::stale(e);
                n += 1;
            }
        }
        drop(i);
        if n > 0 {
            self.wake();
        }
        n
    }

    /// Every value of `command` is out of date.
    pub fn invalidate(&self, command: &str) {
        for e in lock(&self.inner).entries.values_mut() {
            if e.command == command {
                Self::stale(e);
            }
        }
        self.wake();
    }

    /// Every value is out of date (something changed, nobody says what).
    pub fn invalidate_all(&self) {
        for e in lock(&self.inner).entries.values_mut() {
            Self::stale(e);
        }
        self.wake();
    }

    /// Reads in flight now.
    pub fn in_flight(&self) -> usize {
        lock(&self.inner)
            .entries
            .values()
            .filter(|e| e.flight.is_some())
            .count()
    }

    /// How many reads the transport was asked for since the start.
    pub fn fetches(&self) -> u64 {
        lock(&self.inner).fetches
    }

    fn stale(e: &mut Entry) {
        if e.flight.is_some() {
            e.again = true;
        }
        if e.at.is_some() {
            e.stale = true;
        }
    }

    fn wake(&self) {
        if let Some(ctx) = lock(&self.repaint).as_ref() {
            ctx.request_repaint();
        }
    }

    /// Reads on a thread of its own, at once where none starts.
    fn fetch(&self, key: String, command: String, args: Json, n: u64) {
        let (transport, inner, repaint) = (
            self.transport.clone(),
            self.inner.clone(),
            self.repaint.clone(),
        );
        let job = move || {
            let answer = transport.read(&command, &args);
            land(&inner, &key, n, answer);
            if let Some(ctx) = lock(&repaint).as_ref() {
                ctx.request_repaint();
            }
        };
        #[cfg(not(target_arch = "wasm32"))]
        {
            let job = Arc::new(Mutex::new(Some(job)));
            let j = job.clone();
            let spawned = std::thread::Builder::new()
                .name("egui-pop-read".into())
                .spawn(move || {
                    if let Some(job) = lock(&j).take() {
                        job();
                    }
                });
            if let Err(e) = spawned {
                log::warn!(target: "egui_pop", "reads: no thread to read on: {e}");
                if let Some(job) = lock(&job).take() {
                    job();
                }
            }
        }
        #[cfg(target_arch = "wasm32")]
        job();
    }
}

/// An answer lands on its entry, if it is still the read in flight.
fn land(inner: &Mutex<Inner>, key: &str, n: u64, answer: Result<Json, String>) {
    let mut i = lock(inner);
    let Some(e) = i.entries.get_mut(key) else {
        return;
    };
    if e.flight != Some(n) {
        return;
    }
    e.flight = None;
    match answer {
        Ok(v) => {
            e.value = Some(v);
            e.error = None;
        }
        Err(err) => e.error = Some(err),
    }
    e.at = Some(Instant::now());
    e.stale = std::mem::take(&mut e.again);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc::{Receiver, Sender, channel};

    /// A read asked: its command, and where its answer goes.
    type Ask = (String, Sender<Result<Json, String>>);

    /// A transport whose reads wait for the test to answer them.
    struct Gated {
        asked: Mutex<Sender<Ask>>,
    }

    impl ReadTransport for Gated {
        fn serves(&self, command: &str) -> bool {
            command.starts_with("t.")
        }
        fn read(&self, command: &str, _: &Json) -> Result<Json, String> {
            let (tx, rx) = channel();
            lock(&self.asked).send((command.into(), tx)).unwrap();
            rx.recv_timeout(Duration::from_secs(10))
                .unwrap_or_else(|_| Err("no answer".into()))
        }
    }

    fn kernel() -> (ReadKernel, Receiver<Ask>) {
        let (tx, rx) = channel();
        let k = ReadKernel::new(Arc::new(Gated {
            asked: Mutex::new(tx),
        }));
        (k, rx)
    }

    fn settle(k: &ReadKernel) {
        let until = Instant::now() + Duration::from_secs(10);
        while k.in_flight() > 0 {
            assert!(Instant::now() < until, "a read never landed");
            std::thread::sleep(Duration::from_millis(2));
        }
    }

    fn next(rx: &Receiver<Ask>) -> Ask {
        rx.recv_timeout(Duration::from_secs(10))
            .expect("a read asked")
    }

    #[test]
    fn a_read_answers_at_once_and_shares_its_flight() {
        let (k, rx) = kernel();
        assert_eq!(k.read("t.a", &Json::Null).state, ReadState::Pending);
        assert_eq!(k.read("t.a", &json!({})).state, ReadState::Pending);
        let (cmd, answer) = next(&rx);
        assert_eq!(cmd, "t.a");
        assert_eq!(k.fetches(), 1, "two asks, one read");
        answer.send(Ok(json!(1))).unwrap();
        settle(&k);
        let r = k.read("t.a", &Json::Null);
        assert_eq!((r.state, r.value), (ReadState::Fresh, Some(json!(1))));
        assert_eq!(k.fetches(), 1, "a fresh value is not read again");
    }

    #[test]
    fn an_event_keeps_the_value_while_it_is_read_again() {
        let (k, rx) = kernel();
        k.policy("t.v", ReadPolicy::new().invalidated_by(&["storage.*"]));
        k.read("t.v", &Json::Null);
        next(&rx).1.send(Ok(json!("old"))).unwrap();
        settle(&k);
        assert_eq!(k.event("import.done"), 0);
        assert_eq!(k.event("storage.volume.changed"), 1);
        let r = k.read("t.v", &Json::Null);
        assert_eq!(r.state, ReadState::Refreshing);
        assert_eq!(r.value, Some(json!("old")), "a refresh never blanks");
        next(&rx).1.send(Ok(json!("new"))).unwrap();
        settle(&k);
        let r = k.read("t.v", &Json::Null);
        assert_eq!((r.state, r.value), (ReadState::Fresh, Some(json!("new"))));
    }

    #[test]
    fn an_event_during_a_read_reads_again() {
        let (k, rx) = kernel();
        k.policy("t.v", ReadPolicy::new().invalidated_by(&["storage.*"]));
        k.read("t.v", &Json::Null);
        next(&rx).1.send(Ok(json!(1))).unwrap();
        settle(&k);
        k.invalidate_all();
        k.read("t.v", &Json::Null);
        let (_, first) = next(&rx);
        k.event("storage.x");
        first.send(Ok(json!(2))).unwrap();
        settle(&k);
        let r = k.read("t.v", &Json::Null);
        assert_eq!(r.value, Some(json!(2)));
        assert_eq!(
            r.state,
            ReadState::Refreshing,
            "its answer may predate the event"
        );
        next(&rx).1.send(Ok(json!(3))).unwrap();
        settle(&k);
        assert_eq!(k.read("t.v", &Json::Null).value, Some(json!(3)));
    }

    #[test]
    fn a_failure_keeps_the_last_value_and_a_freshness_reads_again() {
        let (k, rx) = kernel();
        k.policy("t.f", ReadPolicy::new().fresh_for(Duration::ZERO));
        k.read("t.f", &Json::Null);
        next(&rx).1.send(Ok(json!(1))).unwrap();
        settle(&k);
        // Past its freshness: read again, shown meanwhile, still fresh.
        let r = k.read("t.f", &Json::Null);
        assert_eq!((r.state, r.value), (ReadState::Fresh, Some(json!(1))));
        next(&rx).1.send(Err("node gone".into())).unwrap();
        settle(&k);
        let r = k.peek("t.f", &Json::Null);
        assert_eq!(r.state, ReadState::Failed);
        assert_eq!(r.value, Some(json!(1)));
        assert_eq!(r.answer(), Some(Ok(json!(1))));
    }
}
