//! The file browser page: a `FileBrowser` over the demo images held in
//! memory, its mode switcher (details, thumbnails; `Ctrl+1`, `Ctrl+2`)
//! and the thumbnails' size slider. The mode is kept in the host's store
//! under this page's dialog id, as a file dialog's is.

use std::sync::{Arc, Mutex};

use egui::Ui;
use egui_pop::api::files::browser::{FileBrowser, FsSource, PreviewThumbnails};
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
                .description("One file browser, many views: the entries of a folder from a source (here held in memory), shown by a mode chosen in the switcher or with Ctrl+1, Ctrl+2 (details, thumbnails), the thumbnails' size on a slider, the mode remembered per dialog.")
                .group("Components", 53),
        )?;
        Ok(())
    }
}

/// The browser, made once per context.
const BROWSER: Slot<Arc<Mutex<FileBrowser>>> = Slot::new("gallery.file_browser.browser");

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
                .with_thumbnails(thumbnails),
        ))
    });
    let mut b = browser.lock().unwrap_or_else(|e| e.into_inner());
    let height = ui.available_height().clamp(160.0, 360.0);
    ui.allocate_ui(egui::vec2(ui.available_width(), height), |ui| {
        let out = b.show_in(ui, app, DIALOG);
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
}
