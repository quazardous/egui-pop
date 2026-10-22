//! What this crate's components need, in the mount contract's terms
//! (ADR 0013): `docs/hosts.md` and `AppHandle::where_can` are made from
//! these.

use egui_pop_model::mount::{ClaimKey, Mountable};

/// The tree.
pub const TREE: &str = "tree";
/// The data table and the list.
pub const TABLE: &str = "table";
/// The library's thumbnail grid.
pub const LIBRARY_GRID: &str = "library-grid";
/// The library's filmstrip.
pub const FILMSTRIP: &str = "filmstrip";
/// The library's comparison.
pub const COMPARE: &str = "compare";

/// The tree: a lazy source, rename and drag and drop inside.
pub fn tree() -> Mountable {
    Mountable::new(TREE, "Tree")
        .crate_name("egui-pop-views")
        .claims(&[ClaimKey::Enter, ClaimKey::Space])
        .drag()
        .bounded_workers()
        .fills()
        .popups()
        .role("tree")
}

/// The data table and the list: widths and sorting in the store.
pub fn table() -> Mountable {
    Mountable::new(TABLE, "Table, list")
        .crate_name("egui-pop-views")
        .claims(&[ClaimKey::Enter, ClaimKey::Space])
        .drag()
        .fills()
        .popups()
        .role("grid")
}

/// The library's grid: culling keys, a rubber band, thumbnails from the
/// application's source.
pub fn library_grid() -> Mountable {
    Mountable::new(LIBRARY_GRID, "Library grid")
        .crate_name("egui-pop-views")
        .claims(&[ClaimKey::Space])
        .keymap()
        .drag()
        .wheel()
        .bounded_workers()
        .fills()
        .role("grid")
}

/// The filmstrip: the grid in one row of thumbnails.
pub fn filmstrip() -> Mountable {
    Mountable::new(FILMSTRIP, "Filmstrip")
        .crate_name("egui-pop-views")
        .claims(&[ClaimKey::Space])
        .keymap()
        .drag()
        .wheel()
        .bounded_workers()
        .min(120.0, 120.0)
        .fills()
        .role("grid")
}

/// The comparison: images side by side, panned and zoomed together.
pub fn compare() -> Mountable {
    Mountable::new(COMPARE, "Compare view")
        .crate_name("egui-pop-views")
        .keyboard(egui_pop_model::mount::Keyboard::None)
        .drag()
        .wheel()
        .bounded_workers()
        .min(0.0, 80.0)
        .fills()
        .role("image")
}

/// This crate's components.
pub fn components() -> Vec<Mountable> {
    vec![library_grid(), filmstrip(), compare(), tree(), table()]
}
