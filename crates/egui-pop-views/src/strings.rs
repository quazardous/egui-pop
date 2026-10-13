//! The strings the views show, by key with English fallbacks,
//! translated through the provider installed with
//! `egui_pop_core::strings::install`. Retry buttons use core's
//! [`RETRY`](egui_pop_core::strings::RETRY).

use egui_pop_core::strings::StrKey;

/// A tree's accessible name, unless the application gives one.
pub const TREE: StrKey = StrKey::new("egui-pop-views-tree", "Tree");
/// A list's accessible name, unless the application gives one.
pub const LIST: StrKey = StrKey::new("egui-pop-views-list", "List");
/// A table's accessible name, unless the application gives one.
pub const TABLE: StrKey = StrKey::new("egui-pop-views-table", "Table");
/// A branch whose children are on their way.
pub const LOADING: StrKey = StrKey::new("egui-pop-views-loading", "Loading…");
/// A branch whose children could not be loaded.
pub const FAILED: StrKey = StrKey::new("egui-pop-views-failed", "Could not load: { $reason }");
/// An empty branch, shown under it when expanded.
pub const EMPTY: StrKey = StrKey::new("egui-pop-views-empty", "Empty");
/// The rename field's accessible name.
pub const RENAME: StrKey = StrKey::new("egui-pop-views-rename", "Rename { $name }");
/// A column header's accessible name when sorted up.
pub const SORTED_UP: StrKey =
    StrKey::new("egui-pop-views-sorted-up", "{ $column }, sorted ascending");
/// A column header's accessible name when sorted down.
pub const SORTED_DOWN: StrKey = StrKey::new(
    "egui-pop-views-sorted-down",
    "{ $column }, sorted descending",
);

/// Every string the views show, for catalogues and tests.
pub const ALL: &[StrKey] = &[
    TREE,
    LIST,
    TABLE,
    LOADING,
    FAILED,
    EMPTY,
    RENAME,
    SORTED_UP,
    SORTED_DOWN,
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_are_unique_fluent_ids() {
        let mut seen = std::collections::BTreeSet::new();
        for k in ALL {
            assert!(seen.insert(k.key), "`{}` twice", k.key);
            assert!(k.key.starts_with("egui-pop-views-"), "{}", k.key);
            assert!(
                k.key
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-'),
                "{}",
                k.key
            );
        }
    }
}
