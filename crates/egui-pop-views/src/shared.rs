//! A view bound to the application-wide selection
//! (`egui_pop_api::selection`): what it shares with it.
//!
//! The view keeps its own [`Selection`] as a mirror of the shared one,
//! limited to its own items, so its cursor, anchor and keyboard work as
//! before; what it selects goes to the service, and what others select
//! comes back into the mirror when the service's revision moves.

use std::collections::HashSet;

#[cfg(feature = "api")]
use egui_pop_api::selection;
#[cfg(feature = "api")]
pub(crate) use egui_pop_api::selection::SelectionService;
use egui_pop_model::selection::SelectionItem;
#[cfg(not(feature = "api"))]
pub(crate) use stand_in::SelectionService;

use crate::select::{Key, Selection};

/// Draws `add` in the selection's focus surface when `shared`: there
/// Alt+Left and Alt+Right walk the selection's history.
pub(crate) fn surface<R>(
    ui: &mut egui::Ui,
    shared: bool,
    add: impl FnOnce(&mut egui::Ui) -> R,
) -> R {
    #[cfg(feature = "api")]
    if shared {
        return selection::surface(ui, add);
    }
    #[cfg(not(feature = "api"))]
    let _ = shared;
    add(ui)
}

/// Brings the mirror in line with the service when its revision (or the
/// view's `version`, its rows) moved since `seen`.
pub(crate) fn follow<K: Key>(
    service: &SelectionService,
    selection: &mut Selection<K>,
    seen: &mut Option<(u64, u64)>,
    version: u64,
    keys: impl Iterator<Item = (K, SelectionItem)>,
) {
    let now = (service.revision(), version);
    if *seen == Some(now) {
        return;
    }
    *seen = Some(now);
    let selected: HashSet<SelectionItem> = service.items().into_iter().collect();
    let mine: Vec<K> = if selected.is_empty() {
        Vec::new()
    } else {
        keys.filter(|(_, i)| selected.contains(i))
            .map(|(k, _)| k)
            .collect()
    };
    if mine != selection.keys() {
        selection.set_keys(mine);
    }
}

/// Writes what the view selected (`done`, its mirror was `before`) to
/// the service: with Ctrl held (`keep_others`), only the view's own
/// items change and the others' stay; else the view's selection
/// replaces the whole.
pub(crate) fn write<K: Key>(
    service: &SelectionService,
    source: &str,
    before: &[K],
    done: &[K],
    keep_others: bool,
    item_of: &dyn Fn(&K) -> SelectionItem,
) {
    if keep_others {
        let was: HashSet<&K> = before.iter().collect();
        let now: HashSet<&K> = done.iter().collect();
        let add = done.iter().filter(|k| !was.contains(k)).map(item_of);
        let remove: Vec<SelectionItem> = before
            .iter()
            .filter(|k| !now.contains(k))
            .map(item_of)
            .collect();
        service.apply(add, remove, source);
    } else {
        service.set(done.iter().map(item_of), source);
    }
}

/// Without the application layer there is no shared selection: no value
/// of this type exists, so a view's `shared` is always `None`.
#[cfg(not(feature = "api"))]
mod stand_in {
    use egui_pop_model::selection::SelectionItem;

    #[derive(Debug)]
    pub(crate) enum SelectionService {}

    impl SelectionService {
        pub(crate) fn revision(&self) -> u64 {
            match *self {}
        }

        pub(crate) fn items(&self) -> Vec<SelectionItem> {
            match *self {}
        }

        pub(crate) fn apply(
            &self,
            _: impl IntoIterator<Item = SelectionItem>,
            _: Vec<SelectionItem>,
            _: &str,
        ) {
            match *self {}
        }

        pub(crate) fn is_hovered(&self, _: &SelectionItem) -> bool {
            match *self {}
        }

        pub(crate) fn hover(&self, _: &str, _: Option<&SelectionItem>) {
            match *self {}
        }

        pub(crate) fn set(&self, _: impl IntoIterator<Item = SelectionItem>, _: &str) {
            match *self {}
        }
    }
}
