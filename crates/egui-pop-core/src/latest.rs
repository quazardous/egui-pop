//! A slow load that follows a selection: the latest asked wins.
//!
//! A panel previewing what is selected elsewhere asks for the current key
//! each time it draws ([`Latest::want`]). A key that changed starts a load
//! on a worker thread and the panel goes on drawing; a load asked before
//! it and not started yet is dropped, one running is told through its
//! [`Cancel`] and its result is dropped. One load runs at a time, so going
//! down a list with an arrow key does not start one thread per row.
//!
//! Nothing runs behind a hidden panel: only `want` starts a load, and a
//! hidden panel's `ui()` does not run. A panel shown again asks for what
//! is selected then, and only that. A load already running when the panel
//! is closed goes on to its end; the panel keeps its result if the key is
//! still the one selected when it comes back.
//!
//! ```
//! use egui_pop_core::latest::{Latest, Loaded};
//!
//! fn preview(ui: &mut egui::Ui, latest: &mut Latest<String, String>, selected: Option<&String>) {
//!     let shown = latest.want(ui.ctx(), selected, |key, cancel| {
//!         // On a worker thread; a long load looks at `cancel` now and then.
//!         if cancel.cancelled() {
//!             return String::new();
//!         }
//!         format!("the preview of {key}")
//!     });
//!     match shown {
//!         Loaded::Nothing => ui.weak("Nothing selected."),
//!         Loaded::Loading { .. } => ui.weak("Loading…"),
//!         Loaded::Ready(text) => ui.label(text.as_str()),
//!         Loaded::Failed(reason) => ui.label(reason),
//!     };
//! }
//! ```
//!
//! On the web there are no threads: the load runs in the frame that asks
//! for it. The file picker's previews have a worker of their own, tied to
//! files (`egui-pop-api`'s `files::preview`); this one knows nothing of
//! what it loads. See `docs/linked-panels.md`.

use std::fmt;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use egui::ViewportId;

use crate::lock;

/// Whether a load is still wanted: a long one looks now and then, and may
/// stop. What it returns then is dropped.
#[derive(Clone, Debug)]
pub struct Cancel {
    wanted: Arc<AtomicU64>,
    mine: u64,
}

impl Cancel {
    /// Another key was asked for since this load was.
    pub fn cancelled(&self) -> bool {
        self.wanted.load(Ordering::Relaxed) != self.mine
    }
}

/// What [`Latest::want`] has to show.
#[derive(Debug)]
pub enum Loaded<'a, V> {
    /// No key is asked for.
    Nothing,
    /// The key's load is not done; `previous` is the last value received,
    /// to show dimmed instead of a blank.
    Loading { previous: Option<&'a V> },
    /// The key's value.
    Ready(&'a V),
    /// The key's load panicked: the panic's message.
    Failed(&'a str),
}

type Job<V> = Box<dyn FnOnce(&Cancel) -> V + Send>;

/// What the frame and the worker share.
struct Slot<V> {
    /// The load to run next, with its generation.
    next: Option<(u64, Job<V>)>,
    /// A worker is at work.
    working: bool,
    /// The newest result, with its generation.
    done: Option<(u64, Result<V, String>)>,
    /// The window to wake with a result (a detached panel has its own).
    viewport: ViewportId,
}

struct Shared<V> {
    /// The generation asked for last.
    wanted: Arc<AtomicU64>,
    slot: Mutex<Slot<V>>,
}

/// One slow value following a key; see the [module](self).
pub struct Latest<K, V> {
    /// The key asked for last.
    key: Option<K>,
    generation: u64,
    /// The last value received: of `key` when `ready`, else of an earlier
    /// one.
    value: Option<V>,
    ready: bool,
    failed: Option<String>,
    shared: Arc<Shared<V>>,
}

impl<K, V> Default for Latest<K, V> {
    fn default() -> Self {
        Self {
            key: None,
            generation: 0,
            value: None,
            ready: false,
            failed: None,
            shared: Arc::new(Shared {
                wanted: Arc::new(AtomicU64::new(0)),
                slot: Mutex::new(Slot {
                    next: None,
                    working: false,
                    done: None,
                    viewport: ViewportId::ROOT,
                }),
            }),
        }
    }
}

impl<K: fmt::Debug, V> fmt::Debug for Latest<K, V> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Latest")
            .field("key", &self.key)
            .field("ready", &self.ready)
            .field("failed", &self.failed)
            .finish_non_exhaustive()
    }
}

impl<K, V> Drop for Latest<K, V> {
    /// A load running is told, one waiting never starts.
    fn drop(&mut self) {
        self.shared.wanted.fetch_add(1, Ordering::Relaxed);
        lock(&self.shared.slot).next = None;
    }
}

impl<K, V> Latest<K, V>
where
    K: Clone + PartialEq + Send + 'static,
    V: Send + 'static,
{
    pub fn new() -> Self {
        Self::default()
    }

    /// What to show for `key` (`None`: nothing is selected). Call it each
    /// time the panel draws: `load` runs only when `key` changed, on the
    /// worker, and the window is woken when its result is in.
    pub fn want(
        &mut self,
        ctx: &egui::Context,
        key: Option<&K>,
        load: impl FnOnce(&K, &Cancel) -> V + Send + 'static,
    ) -> Loaded<'_, V> {
        // Before a load can start: its result wakes the window drawing it.
        let done = {
            let mut slot = lock(&self.shared.slot);
            slot.viewport = ctx.viewport_id();
            slot.done.take()
        };
        if let Some((generation, result)) = done
            && generation == self.generation
        {
            match result {
                Ok(value) => {
                    self.value = Some(value);
                    self.ready = true;
                }
                Err(reason) => {
                    self.value = None;
                    self.failed = Some(reason);
                }
            }
        }
        if self.key.as_ref() != key {
            self.ask(ctx, key, load);
        }
        if self.key.is_none() {
            return Loaded::Nothing;
        }
        if let Some(reason) = &self.failed {
            return Loaded::Failed(reason);
        }
        match &self.value {
            Some(value) if self.ready => Loaded::Ready(value),
            previous => Loaded::Loading {
                previous: previous.as_ref(),
            },
        }
    }

    /// The key changed: what was asked before is not wanted any more.
    fn ask(
        &mut self,
        ctx: &egui::Context,
        key: Option<&K>,
        load: impl FnOnce(&K, &Cancel) -> V + Send + 'static,
    ) {
        self.key = key.cloned();
        self.generation += 1;
        self.shared.wanted.store(self.generation, Ordering::Relaxed);
        self.ready = false;
        self.failed = None;
        let Some(key) = key.cloned() else {
            lock(&self.shared.slot).next = None;
            self.value = None;
            return;
        };
        let job: Job<V> = Box::new(move |cancel: &Cancel| load(&key, cancel));
        let start = {
            let mut slot = lock(&self.shared.slot);
            slot.next = Some((self.generation, job));
            !std::mem::replace(&mut slot.working, true)
        };
        if start {
            self.start(ctx);
        }
    }

    /// Starts the worker: in this frame on the web, or without a thread.
    fn start(&self, ctx: &egui::Context) {
        if cfg!(target_arch = "wasm32") {
            work(&self.shared, ctx);
            return;
        }
        let shared = self.shared.clone();
        let wake = ctx.clone();
        let spawned = std::thread::Builder::new()
            .name("egui-pop-latest".into())
            .spawn(move || work(&shared, &wake));
        if let Err(e) = spawned {
            log::warn!(target: "egui_pop", "latest: no thread for a load: {e}");
            work(&self.shared, ctx);
        }
    }
}

/// Runs the load waiting, then the one asked meanwhile, until none waits.
fn work<V>(shared: &Shared<V>, ctx: &egui::Context) {
    loop {
        let job = {
            let mut slot = lock(&shared.slot);
            let job = slot.next.take();
            slot.working = job.is_some();
            job
        };
        let Some((generation, load)) = job else {
            return;
        };
        let cancel = Cancel {
            wanted: shared.wanted.clone(),
            mine: generation,
        };
        if cancel.cancelled() {
            continue;
        }
        let result = catch_unwind(AssertUnwindSafe(|| load(&cancel)))
            .map_err(|payload| crate::fault::message(&*payload));
        if cancel.cancelled() {
            continue;
        }
        let viewport = {
            let mut slot = lock(&shared.slot);
            slot.done = Some((generation, result));
            slot.viewport
        };
        ctx.request_repaint_of(viewport);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicUsize;
    use std::sync::mpsc;
    use std::time::{Duration, Instant};

    /// Longer than any load here takes: a test that hangs fails instead.
    const DEADLINE: Duration = Duration::from_secs(30);

    fn never(_: &u32, _: &Cancel) -> u32 {
        panic!("this key was asked for already");
    }

    /// Asks for `key` until its value is in.
    fn value_of(latest: &mut Latest<u32, u32>, ctx: &egui::Context, key: u32) -> u32 {
        let start = Instant::now();
        loop {
            if let Loaded::Ready(v) = latest.want(ctx, Some(&key), never) {
                return *v;
            }
            assert!(start.elapsed() < DEADLINE, "the load of {key} never came");
            std::thread::sleep(Duration::from_millis(1));
        }
    }

    #[test]
    fn the_latest_wins_and_what_was_asked_between_never_runs() {
        let ctx = egui::Context::default();
        let mut latest = Latest::<u32, u32>::new();
        let ran = Arc::new(Mutex::new(Vec::new()));
        let (started, has_started) = mpsc::channel::<()>();
        let (release, released) = mpsc::channel::<()>();
        let r = ran.clone();
        let shown = latest.want(&ctx, Some(&1), move |k, _| {
            r.lock().unwrap().push(*k);
            started.send(()).unwrap();
            released.recv_timeout(DEADLINE).unwrap();
            *k
        });
        assert!(matches!(shown, Loaded::Loading { previous: None }));
        has_started.recv_timeout(DEADLINE).unwrap();
        // 1 is running: 2 waits behind it, then 3 takes its place.
        let r = ran.clone();
        let _ = latest.want(&ctx, Some(&2), move |k, _| {
            r.lock().unwrap().push(*k);
            *k
        });
        let r = ran.clone();
        let _ = latest.want(&ctx, Some(&3), move |k, _| {
            r.lock().unwrap().push(*k);
            *k
        });
        release.send(()).unwrap();
        assert_eq!(value_of(&mut latest, &ctx, 3), 3);
        assert_eq!(*ran.lock().unwrap(), vec![1, 3]);
    }

    #[test]
    fn a_running_load_is_told_it_is_not_wanted() {
        let ctx = egui::Context::default();
        let mut latest = Latest::<u32, u32>::new();
        let (started, has_started) = mpsc::channel::<()>();
        let (told, was_told) = mpsc::channel::<()>();
        let _ = latest.want(&ctx, Some(&1), move |_, cancel| {
            started.send(()).unwrap();
            let start = Instant::now();
            while !cancel.cancelled() && start.elapsed() < DEADLINE {
                std::thread::sleep(Duration::from_millis(1));
            }
            told.send(()).unwrap();
            1
        });
        has_started.recv_timeout(DEADLINE).unwrap();
        let _ = latest.want(&ctx, Some(&2), |k, _| *k);
        was_told.recv_timeout(DEADLINE).unwrap();
        assert_eq!(value_of(&mut latest, &ctx, 2), 2);
    }

    #[test]
    fn a_key_loads_once_and_the_last_value_shows_while_the_next_loads() {
        let ctx = egui::Context::default();
        let mut latest = Latest::<u32, u32>::new();
        let runs = Arc::new(AtomicUsize::new(0));
        let n = runs.clone();
        let _ = latest.want(&ctx, Some(&1), move |k, _| {
            n.fetch_add(1, Ordering::Relaxed);
            *k * 10
        });
        assert_eq!(value_of(&mut latest, &ctx, 1), 10);
        assert_eq!(runs.load(Ordering::Relaxed), 1);
        let (release, released) = mpsc::channel::<()>();
        let shown = latest.want(&ctx, Some(&2), move |k, _| {
            released.recv_timeout(DEADLINE).unwrap();
            *k * 10
        });
        assert!(matches!(
            shown,
            Loaded::Loading {
                previous: Some(&10)
            }
        ));
        release.send(()).unwrap();
        assert_eq!(value_of(&mut latest, &ctx, 2), 20);
    }

    #[test]
    fn no_key_shows_nothing_and_the_same_key_afterwards_loads_again() {
        let ctx = egui::Context::default();
        let mut latest = Latest::<u32, u32>::new();
        assert!(matches!(latest.want(&ctx, None, never), Loaded::Nothing));
        let _ = latest.want(&ctx, Some(&1), |k, _| *k);
        assert_eq!(value_of(&mut latest, &ctx, 1), 1);
        assert!(matches!(latest.want(&ctx, None, never), Loaded::Nothing));
        let shown = latest.want(&ctx, Some(&1), |k, _| *k + 1);
        assert!(matches!(shown, Loaded::Loading { previous: None }));
        assert_eq!(value_of(&mut latest, &ctx, 1), 2);
    }

    #[test]
    fn a_load_that_panics_says_so_and_the_next_key_still_loads() {
        let ctx = egui::Context::default();
        let mut latest = Latest::<u32, u32>::new();
        let _ = latest.want(&ctx, Some(&1), |_, _| panic!("no such photo"));
        let start = Instant::now();
        loop {
            if let Loaded::Failed(reason) = latest.want(&ctx, Some(&1), never) {
                assert_eq!(reason, "no such photo");
                break;
            }
            assert!(start.elapsed() < DEADLINE, "the panic was never said");
            std::thread::sleep(Duration::from_millis(1));
        }
        let _ = latest.want(&ctx, Some(&2), |k, _| *k);
        assert_eq!(value_of(&mut latest, &ctx, 2), 2);
    }
}
