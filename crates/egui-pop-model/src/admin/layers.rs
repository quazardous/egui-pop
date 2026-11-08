//! Value scopes: where a setting's value is set. `app` is this person on
//! this machine (the settings store, as today); `library` and `workspace`
//! are layers above it that the application opens with its library and
//! workspace; `run` is the command line's overrides, shown, never edited.
//!
//! The store keeps the **effective** value at `settings/<id>`, so every
//! reader (`AppHandle::setting`, bindings, listeners) sees it unchanged;
//! [`Layers`] keeps the layers above `app`, and the `app` value of a path
//! while a layer above holds it. Saving the user's settings takes the
//! `app` values back ([`Layers::app_doc`]), so a library's value never
//! leaks into the user's file.
//!
//! Each change returns the effective value to write when it changes:
//! the caller writes it (the model stays free of the store's locks).

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value as Json;

/// A layer of values, lowest first.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ValueScope {
    App,
    Library,
    Workspace,
    /// The command line's overrides: shown as an origin, never edited.
    Run,
}

impl ValueScope {
    pub const ALL: [ValueScope; 4] = [
        ValueScope::App,
        ValueScope::Library,
        ValueScope::Workspace,
        ValueScope::Run,
    ];

    pub fn name(self) -> &'static str {
        match self {
            ValueScope::App => "app",
            ValueScope::Library => "library",
            ValueScope::Workspace => "workspace",
            ValueScope::Run => "run",
        }
    }

    pub fn editable(self) -> bool {
        self != ValueScope::Run
    }
}

/// The layers above `app`, by path.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct Layers {
    /// The editable layers the application has open (`app` always).
    offered: Vec<ValueScope>,
    library: BTreeMap<String, Json>,
    workspace: BTreeMap<String, Json>,
    run: BTreeMap<String, Json>,
    /// A path's `app` value while a layer above holds it.
    app: BTreeMap<String, Json>,
}

impl Layers {
    pub fn new() -> Layers {
        Layers {
            offered: vec![ValueScope::App],
            ..Layers::default()
        }
    }

    /// Offers a layer for editing (the application opened a library or a
    /// workspace), or takes it back.
    pub fn offer(&mut self, scope: ValueScope, on: bool) {
        if !scope.editable() || scope == ValueScope::App {
            return;
        }
        self.offered.retain(|s| *s != scope);
        if on {
            self.offered.push(scope);
            self.offered.sort();
        }
    }

    /// The editable layers open, lowest first.
    pub fn offered(&self) -> &[ValueScope] {
        &self.offered
    }

    fn map(&self, scope: ValueScope) -> Option<&BTreeMap<String, Json>> {
        match scope {
            ValueScope::App => None,
            ValueScope::Library => Some(&self.library),
            ValueScope::Workspace => Some(&self.workspace),
            ValueScope::Run => Some(&self.run),
        }
    }

    fn map_mut(&mut self, scope: ValueScope) -> Option<&mut BTreeMap<String, Json>> {
        match scope {
            ValueScope::App => None,
            ValueScope::Library => Some(&mut self.library),
            ValueScope::Workspace => Some(&mut self.workspace),
            ValueScope::Run => Some(&mut self.run),
        }
    }

    /// The layer the effective value comes from: the highest holding the
    /// path, else `app`.
    pub fn origin(&self, path: &str) -> ValueScope {
        [ValueScope::Run, ValueScope::Workspace, ValueScope::Library]
            .into_iter()
            .find(|s| self.map(*s).is_some_and(|m| m.contains_key(path)))
            .unwrap_or(ValueScope::App)
    }

    /// The scopes holding a value for the path (`app` when it differs
    /// from the default, as `store` and `default` say).
    pub fn holders(&self, path: &str, store: &Json, default: &Json) -> Vec<ValueScope> {
        let mut out = Vec::new();
        if self.app_value(path, store) != *default {
            out.push(ValueScope::App);
        }
        for s in [ValueScope::Library, ValueScope::Workspace, ValueScope::Run] {
            if self.map(s).is_some_and(|m| m.contains_key(path)) {
                out.push(s);
            }
        }
        out
    }

    /// A layer's own value for the path; `app`'s is the store's unless a
    /// layer above holds the path.
    pub fn value_in(&self, scope: ValueScope, path: &str, store: &Json) -> Option<Json> {
        match scope {
            ValueScope::App => Some(self.app_value(path, store)),
            s => self.map(s).and_then(|m| m.get(path)).cloned(),
        }
    }

    /// The `app` value: the one kept aside, else the store's.
    pub fn app_value(&self, path: &str, store: &Json) -> Json {
        self.app.get(path).cloned().unwrap_or_else(|| store.clone())
    }

    /// Sets a value in a layer. `store` is the store's value now. Returns
    /// the effective value when it changes (the caller writes it).
    /// `None` for `run` (set with [`Layers::set_run`]).
    pub fn set(
        &mut self,
        scope: ValueScope,
        path: &str,
        value: Json,
        store: &Json,
    ) -> Option<Json> {
        match scope {
            ValueScope::Run => None,
            ValueScope::App => {
                if self.origin(path) == ValueScope::App {
                    (value != *store).then_some(value)
                } else {
                    self.app.insert(path.into(), value);
                    None
                }
            }
            s => {
                if self.origin(path) == ValueScope::App {
                    self.app.insert(path.into(), store.clone());
                }
                self.map_mut(s)?.insert(path.into(), value);
                let effective = self.effective(path)?;
                (effective != *store).then_some(effective)
            }
        }
    }

    /// The command line's override of a path.
    pub fn set_run(&mut self, path: &str, value: Json, store: &Json) -> Option<Json> {
        if self.origin(path) == ValueScope::App {
            self.app.insert(path.into(), store.clone());
        }
        self.run.insert(path.into(), value.clone());
        (value != *store).then_some(value)
    }

    /// The value of the highest layer above `app` holding the path.
    fn effective(&self, path: &str) -> Option<Json> {
        let o = self.origin(path);
        self.map(o).and_then(|m| m.get(path)).cloned()
    }

    /// Takes a layer's value back (its reset): `app` goes back to the
    /// default. Returns the effective value when it changes.
    pub fn clear(
        &mut self,
        scope: ValueScope,
        path: &str,
        store: &Json,
        default: &Json,
    ) -> Option<Json> {
        match scope {
            ValueScope::Run => None,
            ValueScope::App => {
                if self.origin(path) == ValueScope::App {
                    (default != store).then(|| default.clone())
                } else {
                    self.app.insert(path.into(), default.clone());
                    None
                }
            }
            s => {
                self.map_mut(s)?.remove(path)?;
                let effective = match self.effective(path) {
                    Some(v) => v,
                    None => self.app.remove(path).unwrap_or_else(|| store.clone()),
                };
                (effective != *store).then_some(effective)
            }
        }
    }

    /// A layer's values (what the application saves with its library or
    /// workspace).
    pub fn export(&self, scope: ValueScope) -> BTreeMap<String, Json> {
        self.map(scope).cloned().unwrap_or_default()
    }

    /// Replaces a layer's values (a library or workspace opened, or
    /// closed with an empty map). `store` reads the store's value of a
    /// path. Returns the effective values to write.
    pub fn load(
        &mut self,
        scope: ValueScope,
        values: BTreeMap<String, Json>,
        store: &dyn Fn(&str) -> Option<Json>,
    ) -> Vec<(String, Json)> {
        if scope == ValueScope::App {
            return Vec::new();
        }
        let mut paths: Vec<String> = self.export(scope).into_keys().collect();
        paths.extend(values.keys().cloned());
        paths.sort();
        paths.dedup();
        let mut writes = Vec::new();
        for path in &paths {
            let Some(now) = store(path) else { continue };
            let before = self.origin(path);
            if before == ValueScope::App {
                self.app.insert(path.clone(), now.clone());
            }
            if let Some(m) = self.map_mut(scope) {
                match values.get(path) {
                    Some(v) => m.insert(path.clone(), v.clone()),
                    None => m.remove(path),
                };
            }
            let effective = match self.effective(path) {
                Some(v) => v,
                None => self.app.remove(path).unwrap_or_else(|| now.clone()),
            };
            if effective != now {
                writes.push((path.clone(), effective));
            }
        }
        writes
    }

    /// The user's settings document as it should be saved: the `app`
    /// values in place of the effective ones. `default` reads a path's
    /// default; a value at its default is left out.
    pub fn app_doc(
        &self,
        values: &mut BTreeMap<String, Json>,
        default: &dyn Fn(&str) -> Option<Json>,
    ) {
        for (path, v) in &self.app {
            if default(path).as_ref() == Some(v) {
                values.remove(path);
            } else {
                values.insert(path.clone(), v.clone());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const P: &str = "settings/appearance.theme";

    #[test]
    fn a_layer_above_app_wins_and_its_reset_gives_app_back() {
        let mut l = Layers::new();
        l.offer(ValueScope::Library, true);
        l.offer(ValueScope::Run, true);
        assert_eq!(l.offered(), &[ValueScope::App, ValueScope::Library]);
        let store = json!("dark");
        // Library over app: the effective value changes.
        assert_eq!(
            l.set(ValueScope::Library, P, json!("light"), &store),
            Some(json!("light"))
        );
        let store = json!("light");
        assert_eq!(l.origin(P), ValueScope::Library);
        assert_eq!(l.value_in(ValueScope::App, P, &store), Some(json!("dark")));
        // Setting app under it changes nothing shown.
        assert_eq!(l.set(ValueScope::App, P, json!("system"), &store), None);
        // Saving the user's settings takes the app value.
        let mut doc = BTreeMap::from([(P.to_string(), json!("light"))]);
        l.app_doc(&mut doc, &|_| Some(json!("dark")));
        assert_eq!(doc[P], json!("system"));
        // The library's reset gives app's value back.
        assert_eq!(
            l.clear(ValueScope::Library, P, &store, &json!("dark")),
            Some(json!("system"))
        );
        assert_eq!(l.origin(P), ValueScope::App);
        let mut doc = BTreeMap::new();
        l.app_doc(&mut doc, &|_| Some(json!("dark")));
        assert!(doc.is_empty(), "nothing kept aside any more");
    }

    #[test]
    fn workspace_wins_over_library_and_run_over_both() {
        let mut l = Layers::new();
        let mut store = json!(1);
        store = l.set(ValueScope::Library, P, json!(2), &store).unwrap();
        store = l.set(ValueScope::Workspace, P, json!(3), &store).unwrap();
        assert_eq!(l.origin(P), ValueScope::Workspace);
        assert_eq!(
            l.holders(P, &store, &json!(1)),
            vec![ValueScope::Library, ValueScope::Workspace]
        );
        store = l.set_run(P, json!(4), &store).unwrap();
        assert_eq!(l.origin(P), ValueScope::Run);
        assert_eq!(l.set(ValueScope::Run, P, json!(5), &store), None);
        // The workspace's reset under run changes nothing shown.
        assert_eq!(l.clear(ValueScope::Workspace, P, &store, &json!(1)), None);
        assert_eq!(l.value_in(ValueScope::Library, P, &store), Some(json!(2)));
    }

    #[test]
    fn loading_a_layer_writes_what_changes() {
        let mut l = Layers::new();
        let store = |_: &str| Some(json!(1));
        let w = l.load(
            ValueScope::Workspace,
            BTreeMap::from([(P.to_string(), json!(9))]),
            &store,
        );
        assert_eq!(w, vec![(P.to_string(), json!(9))]);
        let store = |_: &str| Some(json!(9));
        let w = l.load(ValueScope::Workspace, BTreeMap::new(), &store);
        assert_eq!(w, vec![(P.to_string(), json!(1))]);
        assert_eq!(l.origin(P), ValueScope::App);
    }
}
