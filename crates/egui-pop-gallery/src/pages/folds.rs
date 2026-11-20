//! The folds page: a pretend side panel of folds (a photo library's
//! Folders, Albums and Imports), each a band that folds and opens its
//! body, Folders with its count and an action at its right end, and a
//! hand-built folder tree in it, branches with their toggles and leaves
//! lined up with them, and a flat tree (Import folders: only the folders
//! that hold photos, each row drawn at its depth and selectable). The
//! playground counts the action's clicks and opens or folds everything.

use egui::{Id, Ui, vec2};
use egui_pop::core::fold::{self, Fold, TreeRow};
use egui_pop::core::ui::{Space, UiPop};

/// The pretend panel's size.
const PANEL: egui::Vec2 = vec2(220.0, 540.0);

/// A folder and the names of the folders in it.
type Folder = (&'static str, &'static [&'static str]);

/// The folder tree: a name and its children.
const TREE: &[(&str, &[Folder])] = &[
    (
        "Photos",
        &[
            ("2024", &["Holidays", "Winter"]),
            ("2025", &[]),
            ("Scans", &["Family"]),
        ],
    ),
    ("Downloads", &[]),
];

/// The flat tree: a folder's path and its depth, the folders between
/// left out (they hold no photos), as an import's folder list keeps them.
pub const FLAT: &[(&str, usize)] = &[
    ("Card", 0),
    ("Card/DCIM/100CANON", 2),
    ("Card/DCIM/100CANON/Burst", 3),
    ("Card/Misc", 1),
];

/// The page's state.
#[derive(Debug, Default)]
pub struct State {
    /// How many times the Folders fold's action was clicked.
    pub rescans: u32,
    /// What the user last did.
    pub last: Option<String>,
    /// The flat tree's selected folder.
    pub flat_selected: Option<&'static str>,
    /// The folds' ids, as last shown.
    ids: Vec<Id>,
}

pub fn playground(ui: &mut Ui, state: &mut State) {
    ui.label(format!("Rescans: {}", state.rescans));
    let ctx = ui.ctx().clone();
    let all = |open: bool| {
        for &id in &state.ids {
            fold::set_open(&ctx, id, open);
        }
    };
    if ui.button("Open everything").clicked() {
        all(true);
    }
    if ui.button("Fold everything").clicked() {
        all(false);
    }
}

/// A tree row's id: by its path, in the `Ui` it is shown in.
fn row_id(ui: &Ui, path: &str) -> Id {
    ui.make_persistent_id(("gallery.folds.folder", path))
}

pub fn show(ui: &mut Ui, state: &mut State) {
    ui.muted("A group folds under its band: click it, or Left, Right, Space and Enter.");
    ui.space_role(Space::Group);
    let (rect, _) = ui.allocate_exact_size(PANEL, egui::Sense::hover());
    let mut panel = ui.new_child(egui::UiBuilder::new().max_rect(rect));
    state.ids.clear();
    let mut rescan = false;
    let folders = Fold::new("gallery.folds.folders", "Folders")
        .count(8)
        .default_open(true)
        .actions(|ui| {
            rescan = ui.button("Rescan").clicked();
        })
        .show(&mut panel, |ui| {
            for (name, years) in TREE {
                let id = row_id(ui, name);
                state.ids.push(id);
                let r = TreeRow::new(id, *name)
                    .leaf(years.is_empty())
                    .default_open(true)
                    .show(ui, |ui| ui.label(*name));
                r.body(ui, |ui| {
                    for (year, months) in *years {
                        let path = format!("{name}/{year}");
                        let id = row_id(ui, &path);
                        state.ids.push(id);
                        TreeRow::new(id, *year)
                            .leaf(months.is_empty())
                            .show(ui, |ui| ui.label(*year))
                            .body(ui, |ui| {
                                for month in *months {
                                    TreeRow::new(row_id(ui, month), *month)
                                        .leaf(true)
                                        .show(ui, |ui| ui.label(*month));
                                }
                            });
                    }
                });
            }
        });
    state.ids.push(folders.id);
    if rescan {
        state.rescans += 1;
        state.last = Some("Rescan".into());
    }
    let albums = Fold::new("gallery.folds.albums", "Albums")
        .count(2)
        .default_open(true)
        .show(&mut panel, |ui| {
            ui.label("Summer");
            ui.label("Portraits");
        });
    state.ids.push(albums.id);
    let flat = Fold::new("gallery.folds.flat", "Import folders")
        .count(FLAT.len() as u64)
        .default_open(true)
        .show(&mut panel, |ui| {
            for &(path, depth) in FLAT {
                let name = path.rsplit('/').next().unwrap_or(path);
                let r = TreeRow::new(row_id(ui, path), name)
                    .leaf(true)
                    .depth(depth)
                    .show(ui, |ui| {
                        ui.selectable_label(state.flat_selected == Some(path), name)
                    });
                if r.row.inner.clicked() {
                    state.flat_selected = Some(path);
                    state.last = Some(format!("Selected {path}"));
                }
            }
        });
    state.ids.push(flat.id);
    let imports = Fold::new("gallery.folds.imports", "Imports").show(&mut panel, |ui| {
        ui.muted("No import yet");
    });
    state.ids.push(imports.id);
    for r in [
        &folders.toggled,
        &albums.toggled,
        &flat.toggled,
        &imports.toggled,
    ] {
        if *r {
            state.last = Some("Folded or opened".into());
        }
    }
    if let Some(last) = &state.last {
        ui.meta(last.clone());
    }
}
