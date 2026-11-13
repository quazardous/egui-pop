//! Where the settings' values are kept: a contract the host provides
//! ([`SettingsStore`]), which the Settings app calls. egui-pop holds no
//! storage location and no default store: without one
//! ([`super::Admin::attach_store`]), the values live in memory for the
//! session and the Settings window says so.
//!
//! - **Load**: each scope the store holds is read when the store is
//!   attached (`app`, and a host scope when it is offered, or reloaded
//!   with [`super::Admin::reload`]); the values it gives become the
//!   layer's, or the user's own for `app`.
//! - **Save**: what changes in a scope the store holds is sent as the
//!   values changed, away from the UI thread (a worker thread; on the web,
//!   at once). A failing save is shown in the Settings window until a
//!   save of that scope succeeds.
//! - **Watch**: a store tells the app of changes made elsewhere (another
//!   client, another process); they are applied at the next pass, and
//!   nothing is written back.
//! - **Claims**: a store holds the settings it claims, the others stay
//!   with the app. [`CompositeStore`] routes each setting to the first
//!   handler claiming it, the rest to a default store.
//!
//! [`FileStore`] keeps a scope in a JSON file whose path the host gives;
//! [`MemoryStore`] keeps them in memory (tests, demos, a handler for the
//! settings that must not outlive the session).

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use egui_pop_core::lock;
use egui_pop_model::admin::ValueScope;
use serde_json::Value as Json;

/// A scope's values, by setting id (`appearance.theme`, not its store
/// path).
pub type Values = BTreeMap<String, Json>;

/// Values changed in a scope, by setting id: `Some` its new value, `None`
/// taken back (back to the default, or to the layer under it).
pub type Changes = BTreeMap<String, Option<Json>>;

/// Called by a store when values change elsewhere: the scope and what
/// changed. May be called from any thread.
pub type OnChange = Arc<dyn Fn(ValueScope, Changes) + Send + Sync>;

/// Where the settings are kept, as the host provides it. Every method
/// may be called from a thread other than the UI's: `save` always is,
/// except on the web.
pub trait SettingsStore: Send + Sync {
    /// The scopes it keeps (`app`, and host scopes such as a library).
    fn scopes(&self) -> Vec<ValueScope>;

    /// Whether it keeps this setting (by id). Everything by default.
    fn claims(&self, _setting: &str) -> bool {
        true
    }

    /// A scope's values as kept now.
    fn load(&self, scope: ValueScope) -> Result<Values, String>;

    /// Keeps the values that changed in a scope. Never called on the UI
    /// thread (but on the web).
    fn save(&self, scope: ValueScope, changes: &Changes) -> Result<(), String>;

    /// Follows the changes made elsewhere to a scope: `on_change` is
    /// called with them. Stores that see none keep the default.
    fn watch(&self, _scope: ValueScope, _on_change: OnChange) -> Result<(), String> {
        Ok(())
    }

    /// Problems worth showing in the Settings window that are not a
    /// failed load or save (a setting two handlers claim).
    fn problems(&self) -> Vec<String> {
        Vec::new()
    }
}

/// Applies changes to values.
pub fn apply(values: &mut Values, changes: &Changes) {
    for (k, v) in changes {
        match v {
            Some(v) => values.insert(k.clone(), v.clone()),
            None => values.remove(k),
        };
    }
}

/// What changed from `before` to `after`.
pub fn diff(before: &Values, after: &Values) -> Changes {
    let mut out = Changes::new();
    for (k, v) in after {
        if before.get(k) != Some(v) {
            out.insert(k.clone(), Some(v.clone()));
        }
    }
    for k in before.keys() {
        if !after.contains_key(k) {
            out.insert(k.clone(), None);
        }
    }
    out
}

/// An ordered list of handlers and a default store. Each setting
/// belongs to the first handler claiming it, else to the default; loads
/// merge what each one owns, saves are split by owner, and every
/// handler's changes are followed. A setting claimed by two handlers is
/// reported once ([`SettingsStore::problems`]); the first wins.
pub struct CompositeStore {
    handlers: Vec<Arc<dyn SettingsStore>>,
    default: Arc<dyn SettingsStore>,
    overlaps: Mutex<BTreeSet<String>>,
}

impl std::fmt::Debug for CompositeStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CompositeStore")
            .field("handlers", &self.handlers.len())
            .field("overlaps", &self.overlaps)
            .finish_non_exhaustive()
    }
}

impl CompositeStore {
    /// A composite whose unclaimed settings go to `default`.
    pub fn new(default: impl SettingsStore + 'static) -> CompositeStore {
        CompositeStore {
            handlers: Vec::new(),
            default: Arc::new(default),
            overlaps: Mutex::new(BTreeSet::new()),
        }
    }

    /// Adds a handler after those added before (an earlier one wins).
    #[must_use]
    pub fn handler(mut self, handler: impl SettingsStore + 'static) -> CompositeStore {
        self.handlers.push(Arc::new(handler));
        self
    }

    /// The index of the handler owning a setting, `None` for the default.
    /// A second handler claiming it is reported, once.
    pub fn owner(&self, setting: &str) -> Option<usize> {
        let mut claiming = self
            .handlers
            .iter()
            .enumerate()
            .filter(|(_, h)| h.claims(setting))
            .map(|(i, _)| i);
        let first = claiming.next()?;
        if let Some(second) = claiming.next()
            && lock(&self.overlaps).insert(setting.to_string())
        {
            log::warn!(
                target: "egui_pop",
                "settings: `{setting}` is claimed by handlers {first} and {second}: the first keeps it"
            );
        }
        Some(first)
    }

    /// The settings two handlers claimed, as seen so far.
    pub fn overlaps(&self) -> Vec<String> {
        lock(&self.overlaps).iter().cloned().collect()
    }

    /// Every store: the handlers, then the default.
    fn stores(&self) -> impl Iterator<Item = (Option<usize>, &Arc<dyn SettingsStore>)> {
        self.handlers
            .iter()
            .enumerate()
            .map(|(i, h)| (Some(i), h))
            .chain(std::iter::once((None, &self.default)))
    }

    /// Whether `store` owns `setting`.
    fn owns(&self, store: Option<usize>, setting: &str) -> bool {
        match self.owner(setting) {
            Some(i) => store == Some(i),
            None => store.is_none() && self.default.claims(setting),
        }
    }
}

impl SettingsStore for CompositeStore {
    fn scopes(&self) -> Vec<ValueScope> {
        let mut out = Vec::new();
        for (_, s) in self.stores() {
            for scope in s.scopes() {
                if !out.contains(&scope) {
                    out.push(scope);
                }
            }
        }
        out
    }

    fn claims(&self, setting: &str) -> bool {
        self.owner(setting).is_some() || self.default.claims(setting)
    }

    fn load(&self, scope: ValueScope) -> Result<Values, String> {
        let mut out = Values::new();
        let mut errors = Vec::new();
        for (i, s) in self.stores() {
            if !s.scopes().contains(&scope) {
                continue;
            }
            match s.load(scope) {
                Ok(values) => out.extend(values.into_iter().filter(|(k, _)| self.owns(i, k))),
                Err(e) => errors.push(e),
            }
        }
        if errors.is_empty() {
            Ok(out)
        } else {
            Err(errors.join("; "))
        }
    }

    fn save(&self, scope: ValueScope, changes: &Changes) -> Result<(), String> {
        let mut errors = Vec::new();
        for (i, s) in self.stores() {
            let mine: Changes = changes
                .iter()
                .filter(|(k, _)| self.owns(i, k))
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect();
            if mine.is_empty() {
                continue;
            }
            if !s.scopes().contains(&scope) {
                errors.push(format!(
                    "{} setting(s) have no store for the {} scope",
                    mine.len(),
                    scope.name()
                ));
                continue;
            }
            if let Err(e) = s.save(scope, &mine) {
                errors.push(e);
            }
        }
        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors.join("; "))
        }
    }

    fn watch(&self, scope: ValueScope, on_change: OnChange) -> Result<(), String> {
        // Each store's changes, less the settings it does not own: the
        // composite is shared through the callback, so it holds the
        // routing itself.
        let routes: Arc<Vec<Arc<dyn SettingsStore>>> = Arc::new(self.handlers.clone());
        let default = self.default.clone();
        let mut errors = Vec::new();
        for (i, s) in self.stores() {
            if !s.scopes().contains(&scope) {
                continue;
            }
            let routes = routes.clone();
            let default = default.clone();
            let on_change = on_change.clone();
            let filtered: OnChange = Arc::new(move |scope, changes: Changes| {
                let owner = |k: &str| routes.iter().position(|h| h.claims(k));
                let mine: Changes = changes
                    .into_iter()
                    .filter(|(k, _)| match owner(k) {
                        Some(o) => i == Some(o),
                        None => i.is_none() && default.claims(k),
                    })
                    .collect();
                if !mine.is_empty() {
                    on_change(scope, mine);
                }
            });
            if let Err(e) = s.watch(scope, filtered) {
                errors.push(e);
            }
        }
        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors.join("; "))
        }
    }

    fn problems(&self) -> Vec<String> {
        let mut out: Vec<String> = self
            .overlaps()
            .into_iter()
            .map(|s| format!("`{s}` is claimed by two handlers: the first keeps it"))
            .collect();
        for (_, s) in self.stores() {
            out.extend(s.problems());
        }
        out
    }
}

/// A store keeping each scope in a JSON file (an object of setting ids
/// to values) at the path the host gives. It knows no folder of its own:
/// a scope without a path is not kept. Saving reads the file, applies the
/// changes and replaces it whole (written beside it, then renamed). It
/// sees no change made elsewhere.
#[derive(Debug, Default)]
pub struct FileStore {
    files: BTreeMap<ValueScope, PathBuf>,
    /// One save at a time: each reads, then writes the file.
    writing: Mutex<()>,
}

impl FileStore {
    pub fn new() -> FileStore {
        FileStore::default()
    }

    /// Keeps a scope in a file.
    #[must_use]
    pub fn file(mut self, scope: ValueScope, path: impl Into<PathBuf>) -> FileStore {
        self.files.insert(scope, path.into());
        self
    }

    /// The file a scope is kept in.
    pub fn path(&self, scope: ValueScope) -> Option<&std::path::Path> {
        self.files.get(&scope).map(PathBuf::as_path)
    }

    fn read(&self, scope: ValueScope) -> Result<Values, String> {
        let path = self
            .path(scope)
            .ok_or_else(|| format!("no file for the {} scope", scope.name()))?;
        match std::fs::read_to_string(path) {
            Ok(text) if text.trim().is_empty() => Ok(Values::new()),
            Ok(text) => serde_json::from_str(&text).map_err(|e| format!("{}: {e}", path.display())),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Values::new()),
            Err(e) => Err(format!("{}: {e}", path.display())),
        }
    }
}

impl SettingsStore for FileStore {
    fn scopes(&self) -> Vec<ValueScope> {
        self.files.keys().copied().collect()
    }

    fn load(&self, scope: ValueScope) -> Result<Values, String> {
        let _one = lock(&self.writing);
        self.read(scope)
    }

    fn save(&self, scope: ValueScope, changes: &Changes) -> Result<(), String> {
        let _one = lock(&self.writing);
        let mut values = self.read(scope)?;
        apply(&mut values, changes);
        let path = self
            .path(scope)
            .ok_or_else(|| format!("no file for the {} scope", scope.name()))?;
        let text = serde_json::to_string_pretty(&values).map_err(|e| e.to_string())?;
        let at = |e: std::io::Error| format!("{}: {e}", path.display());
        if let Some(dir) = path.parent().filter(|d| !d.as_os_str().is_empty()) {
            std::fs::create_dir_all(dir).map_err(at)?;
        }
        let mut tmp = path.as_os_str().to_owned();
        tmp.push(".tmp");
        std::fs::write(&tmp, text + "\n").map_err(at)?;
        std::fs::rename(&tmp, path).map_err(at)
    }
}

type Claim = Arc<dyn Fn(&str) -> bool + Send + Sync>;

/// A store keeping its scopes in memory: a handler for settings that
/// last the session, a stand-in in tests. [`MemoryStore::change`] stands
/// for a change made elsewhere: its watchers are told. Clones share the
/// values.
#[derive(Clone)]
pub struct MemoryStore {
    inner: Arc<MemoryInner>,
    claim: Option<Claim>,
}

#[derive(Default)]
struct MemoryInner {
    scopes: Vec<ValueScope>,
    values: Mutex<BTreeMap<ValueScope, Values>>,
    watchers: Mutex<Vec<(ValueScope, OnChange)>>,
    fail: Mutex<Option<String>>,
    saves: Mutex<Vec<(ValueScope, Changes)>>,
}

impl std::fmt::Debug for MemoryStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MemoryStore")
            .field("scopes", &self.inner.scopes)
            .field("values", &self.inner.values)
            .finish_non_exhaustive()
    }
}

impl MemoryStore {
    /// A store of these scopes, claiming every setting.
    pub fn new(scopes: &[ValueScope]) -> MemoryStore {
        MemoryStore {
            inner: Arc::new(MemoryInner {
                scopes: scopes.to_vec(),
                ..MemoryInner::default()
            }),
            claim: None,
        }
    }

    /// Claims only the settings `claim` accepts.
    #[must_use]
    pub fn claiming(mut self, claim: impl Fn(&str) -> bool + Send + Sync + 'static) -> Self {
        self.claim = Some(Arc::new(claim));
        self
    }

    /// Sets a scope's values, as kept before the app starts (no watcher
    /// is told).
    #[must_use]
    pub fn with(self, scope: ValueScope, values: Values) -> Self {
        lock(&self.inner.values).insert(scope, values);
        self
    }

    /// A scope's values as kept now.
    pub fn values(&self, scope: ValueScope) -> Values {
        lock(&self.inner.values)
            .get(&scope)
            .cloned()
            .unwrap_or_default()
    }

    /// Every save asked, in order.
    pub fn saves(&self) -> Vec<(ValueScope, Changes)> {
        lock(&self.inner.saves).clone()
    }

    /// Makes every save fail with `error` (`None`: they succeed again).
    pub fn fail_saves(&self, error: Option<&str>) {
        *lock(&self.inner.fail) = error.map(str::to_string);
    }

    /// A change made elsewhere: kept, and its watchers told.
    pub fn change(&self, scope: ValueScope, changes: Changes) {
        apply(lock(&self.inner.values).entry(scope).or_default(), &changes);
        let watchers: Vec<OnChange> = lock(&self.inner.watchers)
            .iter()
            .filter(|(s, _)| *s == scope)
            .map(|(_, w)| w.clone())
            .collect();
        for w in watchers {
            w(scope, changes.clone());
        }
    }
}

impl SettingsStore for MemoryStore {
    fn scopes(&self) -> Vec<ValueScope> {
        self.inner.scopes.clone()
    }

    fn claims(&self, setting: &str) -> bool {
        self.claim.as_ref().is_none_or(|c| c(setting))
    }

    fn load(&self, scope: ValueScope) -> Result<Values, String> {
        Ok(self.values(scope))
    }

    fn save(&self, scope: ValueScope, changes: &Changes) -> Result<(), String> {
        lock(&self.inner.saves).push((scope, changes.clone()));
        if let Some(e) = lock(&self.inner.fail).clone() {
            return Err(e);
        }
        apply(lock(&self.inner.values).entry(scope).or_default(), changes);
        Ok(())
    }

    fn watch(&self, scope: ValueScope, on_change: OnChange) -> Result<(), String> {
        lock(&self.inner.watchers).push((scope, on_change));
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const LIB: ValueScope = ValueScope::new("library");

    fn values(pairs: &[(&str, Json)]) -> Values {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.clone()))
            .collect()
    }

    fn composite() -> (MemoryStore, MemoryStore, MemoryStore, CompositeStore) {
        let node = MemoryStore::new(&[ValueScope::App, LIB]).claiming(|k| k.starts_with("lib."));
        let wide = MemoryStore::new(&[ValueScope::App]).claiming(|k| k.starts_with("lib.shared"));
        let file = MemoryStore::new(&[ValueScope::App]);
        let c = CompositeStore::new(file.clone())
            .handler(node.clone())
            .handler(wide.clone());
        (node, wide, file, c)
    }

    #[test]
    fn a_claimed_setting_goes_to_its_handler_and_the_rest_to_the_default() {
        let (node, wide, file, c) = composite();
        let changes: Changes = [
            ("lib.cache".to_string(), Some(json!(5))),
            ("ui.theme".to_string(), Some(json!("dark"))),
            ("lib.shared".to_string(), None),
        ]
        .into();
        c.save(ValueScope::App, &changes).unwrap();
        assert_eq!(node.saves().len(), 1);
        assert_eq!(node.saves()[0].1.len(), 2, "lib.cache and lib.shared");
        assert!(wide.saves().is_empty(), "the first handler wins");
        assert_eq!(
            file.values(ValueScope::App),
            values(&[("ui.theme", json!("dark"))])
        );
        // The overlap is reported once.
        assert_eq!(c.overlaps(), ["lib.shared"]);
        c.save(ValueScope::App, &changes).unwrap();
        assert_eq!(c.problems().len(), 1);
    }

    #[test]
    fn loads_merge_what_each_owns() {
        let (node, _, file, c) = composite();
        node.save(
            ValueScope::App,
            &[("lib.cache".into(), Some(json!(1)))].into(),
        )
        .unwrap();
        // The default's copy of a claimed setting is not its own.
        file.save(
            ValueScope::App,
            &[
                ("lib.cache".into(), Some(json!(9))),
                ("ui.theme".into(), Some(json!("light"))),
            ]
            .into(),
        )
        .unwrap();
        assert_eq!(
            c.load(ValueScope::App).unwrap(),
            values(&[("lib.cache", json!(1)), ("ui.theme", json!("light"))])
        );
        // A scope only the node keeps.
        assert_eq!(c.scopes(), [ValueScope::App, LIB]);
        assert!(
            c.save(LIB, &[("ui.theme".into(), Some(json!(1)))].into())
                .is_err()
        );
    }

    #[test]
    fn every_handlers_changes_are_followed_less_what_it_does_not_own() {
        let (node, _, file, c) = composite();
        let seen = Arc::new(Mutex::new(Vec::new()));
        let s = seen.clone();
        c.watch(
            ValueScope::App,
            Arc::new(move |scope, ch| lock(&s).push((scope, ch))),
        )
        .unwrap();
        node.change(
            ValueScope::App,
            [("lib.cache".into(), Some(json!(3)))].into(),
        );
        file.change(
            ValueScope::App,
            [("lib.cache".into(), Some(json!(4)))].into(),
        );
        file.change(ValueScope::App, [("ui.theme".into(), None)].into());
        let seen = lock(&seen).clone();
        assert_eq!(seen.len(), 2, "the default's copy of lib.cache is ignored");
        assert_eq!(seen[0].1["lib.cache"], Some(json!(3)));
        assert_eq!(seen[1].1["ui.theme"], None);
    }

    #[test]
    fn a_file_store_round_trips_at_the_host_s_path() {
        let dir = std::env::temp_dir().join(format!("egui-pop-file-store-{}", std::process::id()));
        let path = dir.join("nested").join("library.json");
        let s = FileStore::new().file(LIB, &path);
        assert_eq!(s.scopes(), [LIB]);
        assert!(s.load(LIB).unwrap().is_empty(), "no file yet");
        s.save(
            LIB,
            &[
                ("a.b".into(), Some(json!(1))),
                ("c.d".into(), Some(json!("x"))),
            ]
            .into(),
        )
        .unwrap();
        s.save(LIB, &[("a.b".into(), None)].into()).unwrap();
        assert_eq!(s.load(LIB).unwrap(), values(&[("c.d", json!("x"))]));
        assert!(s.load(ValueScope::App).is_err(), "no path given");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn diff_and_apply_agree() {
        let a = values(&[("x", json!(1)), ("y", json!(2))]);
        let b = values(&[("y", json!(3)), ("z", json!(4))]);
        let d = diff(&a, &b);
        assert_eq!(d["x"], None);
        let mut c = a.clone();
        apply(&mut c, &d);
        assert_eq!(c, b);
    }
}
