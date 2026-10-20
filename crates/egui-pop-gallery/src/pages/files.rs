//! The files page: the file chooser's kinds of choice, each opened by a
//! button, the result as it arrives, the recent places and bookmarks.
//! The choices start in a demo folder the page makes, so that the
//! in-app picker shows the same entries everywhere.

use std::path::PathBuf;

use egui::Ui;
use egui_pop::api::files::{self, FILES, FileRequest};
use egui_pop::api::host::Pending;
use egui_pop::api::stories::Story;
use egui_pop::api::{AppHandle, Ctx, Plugin, PluginError};
use egui_pop::core::state::Slot;
use egui_pop::model::ids::{PluginId, StoryId};
use egui_pop::theme::Tokens;

use super::section;
use crate::shell::SHELL;

pub const FILES_PAGE: PluginId = PluginId::from_static("gallery.files");

/// The page.
#[derive(Debug, Default)]
pub struct FilesPage;

impl Plugin for FilesPage {
    fn id(&self) -> PluginId {
        FILES_PAGE
    }

    fn title(&self) -> String {
        "Files".into()
    }

    fn requires(&self) -> Vec<PluginId> {
        vec![SHELL]
    }

    fn connect(&mut self, ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
        ctx.story(
            Story::new(StoryId::from_static("files"), "Choosing files", show)
                .description("The system's file chooser off the interface thread, or the in-app picker: keyboard, typed paths with completion, filters, recent places and bookmarks.")
                .group("Components", 52),
        )?;
        Ok(())
    }
}

/// The demo folder: the same names on every machine.
pub fn demo_folder() -> PathBuf {
    let root = std::env::temp_dir().join("egui-pop-gallery-files");
    for d in ["Photos/2025", "Photos/2026", "Documents", "Exports"] {
        let _ = std::fs::create_dir_all(root.join(d));
    }
    for f in [
        "Photos/harbour.png",
        "Photos/lighthouse.jpg",
        "Photos/notes.txt",
        "Documents/plan.md",
        "Documents/budget.csv",
    ] {
        let p = root.join(f);
        if !p.exists() {
            let _ = std::fs::write(p, b"demo");
        }
    }
    root
}

/// The choice asked and not answered yet.
const PENDING: Slot<Option<Pending>> = Slot::new("gallery.files.pending");

/// What the last choice gave.
const LAST: Slot<String> = Slot::new("gallery.files.last");

fn show(ui: &mut Ui, app: &AppHandle) {
    let t = Tokens::of(ui.ctx());
    let Some(f) = app.service(&FILES) else {
        ui.weak("The files plugin is not connected.");
        return;
    };
    let ctx = ui.ctx().clone();
    ui.label(if files::native_available() {
        "The system's chooser is available (setting `files.native`); off, the in-app picker below answers."
    } else {
        "No system chooser here (the `native-dialogs` feature is off, or no portal): the in-app picker answers."
    });
    section(ui, "Choose");
    let root = demo_folder();
    let mut asked = None;
    ui.horizontal_wrapped(|ui| {
        if ui.button("Open an image…").clicked() {
            asked = Some(
                FileRequest::open("Open an image")
                    .directory(root.join("Photos"))
                    .filter("Images", &["png", "jpg"]),
            );
        }
        if ui.button("Open several…").clicked() {
            asked = Some(FileRequest::open_many("Open files").directory(&root));
        }
        if ui.button("Save as…").clicked() {
            asked = Some(
                FileRequest::save("Export the image", "harbour-edit.png")
                    .directory(root.join("Exports"))
                    .filter("PNG", &["png"]),
            );
        }
        if ui.button("Choose a folder…").clicked() {
            asked = Some(FileRequest::folder("Choose a folder").directory(&root));
        }
    });
    if let Some(request) = asked {
        let p = f.choose(app, request);
        PENDING.set(&ctx, Some(p));
    }
    let pending = PENDING.get(&ctx).flatten();
    if let Some(r) = pending.as_ref().and_then(Pending::take) {
        let text = match files::chosen(&r) {
            Ok(files::Chosen::Paths(paths)) => paths
                .iter()
                .map(|p| p.strip_prefix(&root).unwrap_or(p).display().to_string())
                .collect::<Vec<_>>()
                .join(", "),
            Ok(files::Chosen::Cancelled) => "cancelled".into(),
            Err(e) => format!("refused: {e}"),
        };
        PENDING.set(&ctx, None);
        LAST.set(&ctx, text);
    }
    ui.add_space(t.space.s);
    let last = LAST.get(&ctx).unwrap_or_else(|| "nothing yet".into());
    ui.label(format!("Last choice: {last}"));
    section(ui, "Places");
    let name = |p: &PathBuf| {
        p.strip_prefix(&root)
            .map(|r| format!("demo/{}", r.display()))
            .unwrap_or_else(|_| p.display().to_string())
    };
    let recent = files::recent(app);
    let marks = files::bookmarks(app);
    ui.label(format!(
        "Recent: {}",
        if recent.is_empty() {
            "none".into()
        } else {
            recent.iter().map(name).collect::<Vec<_>>().join(", ")
        }
    ));
    ui.label(format!(
        "Bookmarks: {}",
        if marks.is_empty() {
            "none".into()
        } else {
            marks.iter().map(name).collect::<Vec<_>>().join(", ")
        }
    ));
    section(ui, "Keys in the in-app picker");
    egui::Grid::new("gallery.files.keys")
        .num_columns(2)
        .spacing([t.space.l, t.space.xs])
        .show(ui, |ui| {
            for (k, what) in [
                (
                    "Typing",
                    "filters the folder (fuzzy), or takes a path with / or ~",
                ),
                ("Tab", "completes the typed path"),
                (
                    "Up, Down, Enter",
                    "move, then enter a folder or choose a file",
                ),
                ("Backspace, Alt+Up", "the parent folder"),
                ("Ctrl+Enter", "choose the folder, or the selection"),
                ("Ctrl+Space", "select a file (choosing several)"),
                ("Ctrl+D", "bookmark the folder"),
                ("Escape", "cancel (from the field: twice)"),
            ] {
                ui.monospace(k);
                ui.label(what);
                ui.end_row();
            }
        });
}
