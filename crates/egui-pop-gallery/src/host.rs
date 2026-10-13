//! The gallery's host: egui-pop's local host, except for the commands
//! named `gallery.slow.*`, which it answers later, when the progress
//! page says so, as a remote kernel would.

use std::sync::{Arc, Mutex, PoisonError};

use egui_pop::api::host::{
    self, Dispatch, Event, Host, Journal, LocalHost, Reply, Resolver, Seq, Waker,
};
use egui_pop::core::binding::SharedStore;

/// The prefix of the commands answered later.
pub const SLOW: &str = "gallery.slow.";

/// The commands waiting for an answer, shared by the host and the page.
#[derive(Clone, Debug, Default)]
pub struct Waiting(Arc<Mutex<Vec<(String, Resolver)>>>);

impl Waiting {
    /// How many wait.
    pub fn len(&self) -> usize {
        self.0.lock().unwrap_or_else(PoisonError::into_inner).len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Answers every waiting command with `result`; how many.
    pub fn answer(&self, result: &Result<serde_json::Value, String>) -> usize {
        let waiting = std::mem::take(&mut *self.0.lock().unwrap_or_else(PoisonError::into_inner));
        let n = waiting.len();
        for (_, r) in waiting {
            r.resolve(result.clone());
        }
        n
    }
}

/// The gallery's host.
#[derive(Debug)]
pub struct GalleryHost {
    local: LocalHost,
    waiting: Waiting,
}

impl GalleryHost {
    pub fn new(waiting: Waiting) -> Self {
        Self {
            local: LocalHost::default(),
            waiting,
        }
    }
}

impl Host for GalleryHost {
    fn dispatch(&mut self, d: &Dispatch<'_>) -> Reply {
        if !d.command.as_str().starts_with(SLOW) {
            return self.local.dispatch(d);
        }
        let (pending, resolver) = host::pending();
        self.waiting
            .0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push((d.command.to_string(), resolver));
        Reply::Later(pending)
    }

    fn journal(&self) -> Option<&Journal> {
        self.local.journal()
    }

    fn journal_mut(&mut self) -> Option<&mut Journal> {
        self.local.journal_mut()
    }

    fn since(&self, seq: Seq) -> Option<Vec<Event>> {
        self.local.since(seq)
    }

    fn store(&self) -> SharedStore {
        self.local.store()
    }

    fn set_waker(&mut self, waker: Waker) {
        self.local.set_waker(waker);
    }

    fn layout_document(&self) -> Option<String> {
        self.local.layout_document()
    }
}
