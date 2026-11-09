//! The file browser page: a `FileBrowser` over the demo images held in
//! memory, its mode switcher (details, thumbnails, filmstrip, columns,
//! carousel, agenda; `Ctrl+1` to `Ctrl+6`), the thumbnails' size slider and quick look
//! (`Ctrl+Y`, or Space on an entry), the agenda by day, month or year, and
//! drops: a file dragged from the page (or from the system) onto a folder,
//! the folder shown or the page's own target. The mode is kept in the
//! host's store under this page's dialog id, as a file dialog's is.

use std::sync::{Arc, Mutex};

use egui::Ui;
use egui_pop::api::files::browser::{
    AppDropTarget, DraggedPaths, DropTarget, FileBrowser, FsSource, PreviewThumbnails, ViewModes,
};
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
                .description("One file browser, many views: the entries of a folder from a source (here held in memory), shown by a mode chosen in the switcher or with Ctrl+1 to Ctrl+6 (details, thumbnails, filmstrip, Finder's columns, a carousel, an agenda by day, month or year), the thumbnails' size on a slider, quick look with Ctrl+Y or Space, drops onto a folder, the folder shown or an application's target, the mode remembered per dialog.")
                .group("Components", 53),
        )?;
        Ok(())
    }
}

/// The browser, made once per context.
const BROWSER: Slot<Arc<Mutex<FileBrowser>>> = Slot::new("gallery.file_browser.browser");

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
    let height = ui.available_height().clamp(160.0, 360.0);
    ui.allocate_ui(egui::vec2(ui.available_width(), height), |ui| {
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
    section(ui, "State");
    ui.label(format!(
        "Mode: {}. Selected: {}. Focus: {}.",
        b.mode(),
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
