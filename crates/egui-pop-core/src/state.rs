//! Per-context state kept in egui's memory, rotated once per pass.

use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use egui::{Context, Id};

/// Locks a mutex, ignoring poisoning (a panic caught by a fault boundary
/// must not make the framework's own state unusable).
pub fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Shared state of type `T` in a context, created on first use.
pub(crate) fn shared<T: Default + Send + 'static>(ctx: &Context, key: &str) -> Arc<Mutex<T>> {
    let id = Id::new(("egui_pop_core::state", key));
    ctx.data_mut(|d| {
        d.get_temp_mut_or_insert_with(id, || Arc::new(Mutex::new(T::default())))
            .clone()
    })
}

/// The pass being drawn.
pub(crate) fn pass(ctx: &Context) -> u64 {
    ctx.cumulative_pass_nr()
}
