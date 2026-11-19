//! The file browser page: a `FileBrowser` over the demo images held in
//! memory, its mode switcher (details, thumbnails, filmstrip, columns,
//! carousel, agenda; `Ctrl+1` to `Ctrl+6`), the sort menu (name, modified,
//! taken, size, type; folders first or not; files grouped or not, a RAW with
//! its sidecars and its JPEG one entry by the gallery's grouping), the
//! thumbnails' size slider and quick look
//! (`Ctrl+Y`, or Space on an entry), the agenda by day, month or year, and
//! drops: a file dragged from the page (or from the system) onto a folder,
//! the folder shown or the page's own target. A panel of places beside it
//! (the picker's, with the gallery's own "Import" section) opens a folder
//! in the browser. A field above it ranks the entries by a fuzzy query, a
//! filter keeps images or folders, and a check box lets only files be
//! selected (the browser's filter, query and selection policy, as a picker
//! sets them). The mode is kept in the host's store under this page's
//! dialog id, as a file dialog's is.

use std::sync::{Arc, Mutex};

use egui::Ui;
use egui_pop::api::files::browser::{
    AppDropTarget, DraggedPaths, DropTarget, EntryFilter, EntryKind, FileBrowser, FsSource,
    PreviewThumbnails, SelectMode, Selectable, SelectionPolicy, ViewModes,
};
use egui_pop::api::files::places::PlacesPanel;
use egui_pop::api::files::{self, FILES};
use egui_pop::api::stories::Story;
use egui_pop::api::{AppHandle, Ctx, Plugin, PluginError};
use egui_pop::core::state::Slot;
use egui_pop::model::ids::{PluginId, StoryId};

use super::files::{demo_memory_folder, demo_root};
use super::section;
use crate::shell::SHELL;

pub const FILE_BROWSER_PAGE: PluginId = PluginId::from_static("gallery.file_browser");

/// The dialog id its mode is kept under.
pub const DIALOG: &str = "gallery.file_browser";

/// The page.
#[derive(Debug, Default)]
pub struct FileBrowserPage;

impl Plugin for FileBrowserPage {
    fn id(&self) -> PluginId {
        FILE_BROWSER_PAGE
    }

    fn title(&self) -> String {
        "File browser".into()
    }

    fn requires(&self) -> Vec<PluginId> {
        vec![SHELL, files::PLUGIN]
    }

    fn connect(&mut self, ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
        ctx.story(
            Story::new(StoryId::from_static("file-browser"), "File browser", show)
                .description("One file browser, many views: the entries of a folder from a source (here held in memory), shown by a mode chosen in the switcher or with Ctrl+1 to Ctrl+6 (details, thumbnails, filmstrip, Finder's columns, a carousel, an agenda by day, month or year), the order chosen in the sort menu (name, modified, taken, size or type, up or down, folders first or among the files, files grouped or not: in Photos/2026 a RAW with its sidecar and its JPEG is one entry badged +XMP +JPEG), the thumbnails' size on a slider, a rubber band dragged from the room around the thumbnails, quick look with Ctrl+Y or Space, drops onto a folder, the folder shown or an application's target, the mode remembered per dialog, and a panel of places beside it: the picker's places and an application's own section, driven by the keyboard (Up, Down, Enter).")
                .group("Components", 53),
        )?;
        Ok(())
    }
}

/// The browser, made once per context.
const BROWSER: Slot<Arc<Mutex<FileBrowser>>> = Slot::new("gallery.file_browser.browser");

/// The item last chosen in the panel of places, by id.
const PLACE: Slot<Arc<Mutex<Option<String>>>> = Slot::new("gallery.file_browser.place");

/// What the last drop was, said under the browser.
const LAST_DROP: Slot<Arc<Mutex<String>>> = Slot::new("gallery.file_browser.last_drop");

/// The id of the page's own drop target.
pub const IMPORT_TARGET: &str = "gallery.import";

fn show(ui: &mut Ui, app: &AppHandle) {
    let Some(f) = app.service(&FILES) else {
        ui.weak("The files plugin is not connected.");
        return;
    };
    ui.label("The demo images, held in memory. The mode is kept for this page in the host's store, as a file dialog's is for its dialog.");
    section(ui, "Browser");
    let browser = BROWSER.get_or_insert_with(ui.ctx(), || {
        let fs: egui_pop::api::files::fs::SharedFs = Arc::new(demo_memory_folder());
        let source = Arc::new(FsSource::new(fs.clone()));
        let thumbnails = Arc::new(PreviewThumbnails::new(f.previews(), fs));
        Arc::new(Mutex::new(
            FileBrowser::new(DIALOG, source, demo_root().join("Photos"))
                .with_root(demo_root())
                .with_modes(ViewModes::built_in())
                .with_thumbnails(thumbnails)
                // The gallery's photos grouping: a RAW heads its sidecars.
                .with_grouping(f.groupings())
                // Every drop is taken but one onto the Documents folder:
                // the refusal shows too.
                .with_drops(|offer| {
                    !matches!(&offer.target, DropTarget::Folder(f) if f.ends_with("Documents"))
                })
                .with_drop_targets(vec![AppDropTarget::new(
                    IMPORT_TARGET,
                    "Add to the import",
                )]),
        ))
    });
    let last_drop = LAST_DROP.get_or_insert_with(ui.ctx(), Arc::default);
    // Something to drag: a file from elsewhere in the demo folder.
    ui.horizontal(|ui| {
        ui.label("Drag onto the browser:");
        let plan = demo_root().join("Documents").join("plan.md");
        ui.dnd_drag_source(
            egui::Id::unique("gallery.file_browser.drag"),
            DraggedPaths(vec![plan]),
            |ui| {
                egui::Frame::group(ui.style()).show(ui, |ui| ui.label("plan.md"));
            },
        );
    });
    let mut b = browser.lock().unwrap_or_else(|e| e.into_inner());
    ui.horizontal(|ui| {
        let label = ui.label("Find:");
        let mut query = b.query().to_owned();
        let field = ui
            .add(egui::TextEdit::singleline(&mut query).desired_width(140.0))
            .labelled_by(label.id);
        if field.changed() {
            b.set_query(&query);
        }
        let filters = [
            ("All entries", EntryFilter::all()),
            (
                "Images",
                EntryFilter::extensions(&["png", "jpg", "jpeg", "cr3"]),
            ),
            ("Folders", EntryFilter::kind(EntryKind::Folder)),
        ];
        let mut at = filters
            .iter()
            .position(|(_, f)| f == b.filter())
            .unwrap_or(0);
        let before = at;
        egui::ComboBox::from_label("Show")
            .selected_text(filters[at].0)
            .show_ui(ui, |ui| {
                for (n, (name, _)) in filters.iter().enumerate() {
                    ui.selectable_value(&mut at, n, *name);
                }
            });
        if at != before {
            b.set_filter(filters[at].1.clone());
        }
        let mut files_only = b.selection_policy().selectable == Selectable::Files;
        if ui.checkbox(&mut files_only, "Select files only").changed() {
            b.set_selection_policy(if files_only {
                SelectionPolicy::files(SelectMode::Many)
            } else {
                SelectionPolicy::any(SelectMode::Many)
            });
        }
    });
    let height = ui.available_height().clamp(160.0, 360.0);
    let place = PLACE.get_or_insert_with(ui.ctx(), Arc::default);
    let sections = f.places(app).sections(ui.ctx());
    ui.allocate_ui(egui::vec2(ui.available_width(), height), |ui| {
        ui.horizontal_top(|ui| {
            // The places, as an import window's sources column.
            ui.allocate_ui(egui::vec2(150.0, height), |ui| {
                ui.set_width(150.0);
                let mut at = place.lock().unwrap_or_else(|e| e.into_inner());
                let out = PlacesPanel::new("gallery.file_browser.places", &sections)
                    .selected(at.as_deref())
                    .show(ui);
                if let Some((_, item)) = out.chosen {
                    if let Some(path) = &item.path {
                        b.open(path.clone());
                    }
                    *at = Some(item.id);
                }
            });
            ui.vertical(|ui| {
                let out = b.show_in(ui, app, DIALOG);
                if let Some(drop) = &out.dropped {
                    let into = match &drop.target {
                        DropTarget::Folder(p) | DropTarget::Place(p) => p.display().to_string(),
                        DropTarget::App(id) => id.clone(),
                    };
                    *last_drop.lock().unwrap_or_else(|e| e.into_inner()) =
                        format!("{} file(s) into {into}", drop.paths.len());
                }
                if let Some(path) = out.activated
                    && b.entries().iter().any(|e| e.path == path && e.is_dir())
                {
                    b.open(path);
                }
            });
        });
    });
    section(ui, "State");
    ui.label(format!(
        "Mode: {}. Order: {}{}. Selected: {}. Focus: {}.",
        b.mode(),
        b.sort().as_text(),
        if b.folders_first() {
            ", folders first"
        } else {
            ""
        },
        b.selection().len(),
        b.focus()
            .and_then(|p| p.file_name())
            .map_or("none".into(), |n| n.to_string_lossy().into_owned()),
    ));
    let last = last_drop.lock().unwrap_or_else(|e| e.into_inner()).clone();
    if !last.is_empty() {
        ui.label(format!("Last drop: {last}."));
    }
}
