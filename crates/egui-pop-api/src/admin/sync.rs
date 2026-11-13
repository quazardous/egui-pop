//! The Settings app and the host's store: loading, saving away from the
//! UI thread, following changes made elsewhere, keeping the errors to
//! show ([`super::store`]).

use std::collections::{BTreeMap, BTreeSet};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use egui_pop_core::lock;
use egui_pop_model::admin::ValueScope;
use egui_pop_model::admin::layers::Layers;
use egui_pop_model::ids::SettingId;

use super::store::{Changes, SettingsStore, Values, apply, diff};
use super::{Admin, write};
use crate::app::AppHandle;

/// What the store's threads (its worker, its watchers) hand to the UI
/// thread.
#[derive(Default)]
pub(super) struct Shared {
    /// To wake the UI thread.
    ctx: Mutex<Option<egui::Context>>,
    /// Changes made elsewhere, applied at the next pass.
    events: Mutex<Vec<(ValueScope, Changes)>>,
    /// The last failed load or save of each scope.
    errors: Mutex<BTreeMap<ValueScope, String>>,
    /// Saves sent and not done.
    pending: AtomicUsize,
}

impl Shared {
    fn wake(&self) {
        if let Some(ctx) = lock(&self.ctx).as_ref() {
            ctx.request_repaint();
        }
    }

    fn done(&self, scope: ValueScope, result: Result<(), String>) {
        match result {
            Ok(()) => {
                lock(&self.errors).remove(&scope);
            }
            Err(e) => {
                log::warn!(target: "egui_pop", "settings: {} not saved: {e}", scope.name());
                lock(&self.errors).insert(scope, e);
            }
        }
        self.pending.fetch_sub(1, Ordering::AcqRel);
        self.wake();
    }
}

/// The UI thread's side.
#[derive(Default)]
pub(super) struct Synced {
    store: Option<Arc<dyn SettingsStore>>,
    /// The worker saving in order, away from the UI thread.
    worker: Option<std::sync::mpsc::Sender<(ValueScope, Changes)>>,
    /// Each scope's values as the store holds them: the last loaded or
    /// saved. A scope not loaded yet is not saved.
    kept: BTreeMap<ValueScope, Values>,
    /// Scopes to read at the next pass.
    to_load: BTreeSet<ValueScope>,
    /// Scopes followed.
    watched: BTreeSet<ValueScope>,
    /// The value store's sequence and the layers when last compared.
    seen: Option<(u64, Layers)>,
}

impl Admin {
    /// Gives the Settings app the host's store: its scopes are read at
    /// the next pass (a host scope once offered), what changes is saved to
    /// it, and its changes made elsewhere are followed. Replaces a store
    /// attached before.
    pub fn attach_store(&self, store: impl SettingsStore + 'static) {
        let store: Arc<dyn SettingsStore> = Arc::new(store);
        let mut s = lock(&self.sync);
        s.to_load = store.scopes().into_iter().collect();
        s.kept.clear();
        s.watched.clear();
        s.seen = None;
        s.worker = None;
        s.store = Some(store);
        lock(&self.shared.errors).clear();
    }

    /// Whether the host gave a store.
    pub fn has_store(&self) -> bool {
        lock(&self.sync).store.is_some()
    }

    /// Reads a scope from the store again at the next pass (the host
    /// opened another library): what it holds replaces the layer.
    pub fn reload(&self, scope: ValueScope) {
        let mut s = lock(&self.sync);
        if s.store
            .as_ref()
            .is_some_and(|st| st.scopes().contains(&scope))
        {
            s.to_load.insert(scope);
        }
    }

    /// The store's errors to show: a failed load or save, by scope, and
    /// its other problems (a setting two handlers claim).
    pub fn store_errors(&self) -> Vec<(Option<ValueScope>, String)> {
        let mut out: Vec<(Option<ValueScope>, String)> = lock(&self.shared.errors)
            .iter()
            .map(|(s, e)| (Some(*s), e.clone()))
            .collect();
        if let Some(store) = lock(&self.sync).store.clone() {
            out.extend(store.problems().into_iter().map(|p| (None, p)));
        }
        out
    }

    /// Whether saves are on their way to the store.
    pub fn saving(&self) -> bool {
        self.shared.pending.load(Ordering::Acquire) > 0
    }

    /// Whether the values changed here last only the session: no store,
    /// and neither the application's settings file nor the host's own
    /// layer handling keeps them.
    pub fn session_only(&self, app: &AppHandle) -> bool {
        !self.has_store()
            && !self.legacy.load(Ordering::Relaxed)
            && (app.persist().is_none() || self.offered().len() > 1)
    }

    /// One pass: saves what changed since the last, reads the scopes to
    /// read, applies the changes made elsewhere.
    pub(crate) fn sync(&self, app: &AppHandle, ctx: &egui::Context) {
        let Some(store) = lock(&self.sync).store.clone() else {
            return;
        };
        {
            let mut c = lock(&self.shared.ctx);
            if c.is_none() {
                *c = Some(ctx.clone());
            }
        }
        self.follow(&store);
        self.save_changes(app, &store);
        let to_load = std::mem::take(&mut lock(&self.sync).to_load);
        for scope in to_load {
            self.load_scope(app, &store, scope);
        }
        let events = std::mem::take(&mut *lock(&self.shared.events));
        let applied = !events.is_empty();
        for (scope, changes) in events {
            self.apply_elsewhere(app, &store, scope, &changes);
        }
        if applied {
            // What was applied is what the store holds: nothing goes back.
            self.mark_seen(app);
        }
    }

    /// Follows every scope of the store not followed yet.
    fn follow(&self, store: &Arc<dyn SettingsStore>) {
        let new: Vec<ValueScope> = {
            let mut s = lock(&self.sync);
            store
                .scopes()
                .into_iter()
                .filter(|sc| s.watched.insert(*sc))
                .collect()
        };
        for scope in new {
            let shared = self.shared.clone();
            let r = store.watch(
                scope,
                Arc::new(move |scope, changes| {
                    lock(&shared.events).push((scope, changes));
                    shared.wake();
                }),
            );
            if let Err(e) = r {
                lock(&self.shared.errors).insert(scope, e);
            }
        }
    }

    /// Whether a scope is held now: `app` always, a host scope offered.
    fn live(&self, scope: ValueScope) -> bool {
        scope == ValueScope::App || self.offered().contains(&scope)
    }

    /// The values of a scope the store keeps, as shown now: `app`'s are
    /// the user's own (not those a layer above holds) away from their
    /// default.
    fn values_of(&self, app: &AppHandle, store: &dyn SettingsStore, scope: ValueScope) -> Values {
        if scope != ValueScope::App {
            return self
                .layer_values(scope)
                .into_iter()
                .filter(|(k, _)| store.claims(k))
                .collect();
        }
        let layers = lock(&self.layers).clone();
        let st = app.store();
        let st = lock(&st);
        let mut out = Values::new();
        for e in app.settings() {
            let id = e.id.as_str();
            if !store.claims(id) {
                continue;
            }
            let Some(entry) = st.get(&e.path) else {
                continue;
            };
            let v = layers.app_value(&e.path, &entry.value.to_json());
            if v != entry.descriptor.default.to_json() {
                out.insert(id.to_string(), v);
            }
        }
        out
    }

    /// Sends what changed in each scope loaded since the last pass.
    fn save_changes(&self, app: &AppHandle, store: &Arc<dyn SettingsStore>) {
        let seq = lock(&app.store()).seq();
        let layers = self.layers();
        let changed = lock(&self.sync)
            .seen
            .as_ref()
            .is_none_or(|(s, l)| *s != seq || *l != layers);
        if !changed {
            return;
        }
        let scopes: Vec<ValueScope> = lock(&self.sync).kept.keys().copied().collect();
        for scope in scopes {
            if !self.live(scope) {
                continue;
            }
            let now = self.values_of(app, store.as_ref(), scope);
            let changes = {
                let mut s = lock(&self.sync);
                let kept = s.kept.entry(scope).or_default();
                let d = diff(kept, &now);
                *kept = now;
                d
            };
            if !changes.is_empty() {
                self.send(store, scope, changes);
            }
        }
        lock(&self.sync).seen = Some((seq, layers));
    }

    /// Saves away from the UI thread, in order (at once on the web, or
    /// when no thread starts).
    fn send(&self, store: &Arc<dyn SettingsStore>, scope: ValueScope, changes: Changes) {
        self.shared.pending.fetch_add(1, Ordering::AcqRel);
        let mut s = lock(&self.sync);
        #[cfg(not(target_arch = "wasm32"))]
        if s.worker.is_none() {
            let (tx, rx) = std::sync::mpsc::channel::<(ValueScope, Changes)>();
            let store = store.clone();
            let shared = self.shared.clone();
            let spawned = std::thread::Builder::new()
                .name("egui-pop-settings-store".into())
                .spawn(move || {
                    for (scope, changes) in rx {
                        let r = store.save(scope, &changes);
                        shared.done(scope, r);
                    }
                });
            match spawned {
                Ok(_) => s.worker = Some(tx),
                Err(e) => log::warn!(target: "egui_pop", "settings: no thread to save: {e}"),
            }
        }
        let job = match s.worker.as_ref() {
            Some(tx) => match tx.send((scope, changes)) {
                Ok(()) => return,
                Err(e) => e.0,
            },
            None => (scope, changes),
        };
        s.worker = None;
        drop(s);
        let r = store.save(job.0, &job.1);
        self.shared.done(job.0, r);
    }

    /// Reads a scope from the store: its values replace the layer's (or
    /// the user's own, for `app`); a host scope not offered is taken
    /// back.
    fn load_scope(&self, app: &AppHandle, store: &Arc<dyn SettingsStore>, scope: ValueScope) {
        if !self.live(scope) {
            self.put_layer(app, scope, BTreeMap::new());
            lock(&self.sync).kept.remove(&scope);
            self.mark_seen(app);
            return;
        }
        let values = match store.load(scope) {
            Ok(v) => {
                lock(&self.shared.errors).remove(&scope);
                v
            }
            Err(e) => {
                log::warn!(target: "egui_pop", "settings: {} not loaded: {e}", scope.name());
                lock(&self.shared.errors).insert(scope, e);
                // Not loaded: not saved over either.
                lock(&self.sync).kept.remove(&scope);
                return;
            }
        };
        let values: Values = values
            .into_iter()
            .filter(|(k, _)| store.claims(k))
            .collect();
        if scope == ValueScope::App {
            let now = self.values_of(app, store.as_ref(), scope);
            let mut changes = diff(&now, &values);
            // A setting the store has no word on keeps its value.
            changes.retain(|_, v| v.is_some());
            self.set_app(app, &changes);
        } else {
            self.put_layer(app, scope, values);
        }
        let kept = self.values_of(app, store.as_ref(), scope);
        lock(&self.sync).kept.insert(scope, kept);
        self.mark_seen(app);
    }

    /// Applies a change made elsewhere to a scope held now.
    fn apply_elsewhere(
        &self,
        app: &AppHandle,
        store: &Arc<dyn SettingsStore>,
        scope: ValueScope,
        changes: &Changes,
    ) {
        if !self.live(scope) || !lock(&self.sync).kept.contains_key(&scope) {
            return;
        }
        if scope == ValueScope::App {
            self.set_app(app, changes);
        } else {
            let mut values = self.layer_values(scope);
            apply(&mut values, changes);
            self.put_layer(app, scope, values);
        }
        let kept = self.values_of(app, store.as_ref(), scope);
        lock(&self.sync).kept.insert(scope, kept);
    }

    /// Sets the user's own values (`None`: the default), without an undo
    /// step: a layer above keeps showing its own.
    fn set_app(&self, app: &AppHandle, changes: &Changes) {
        for (id, v) in changes {
            let Ok(id) = SettingId::new(id) else { continue };
            let path = id.path();
            let (Ok(now), Ok(d)) = (super::current(app, &path), super::descriptor(app, &path))
            else {
                continue;
            };
            let target = v.clone().unwrap_or_else(|| d.default.to_json());
            let w = lock(&self.layers).set(ValueScope::App, &path, target, &now);
            if let Some(v) = w {
                write(app, &path, &v);
            }
        }
    }

    /// The value store and layers as they are now count as saved.
    fn mark_seen(&self, app: &AppHandle) {
        let seq = lock(&app.store()).seq();
        let layers = self.layers();
        lock(&self.sync).seen = Some((seq, layers));
    }
}
