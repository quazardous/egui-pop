//! The placeholders page: skeletons standing in for a list and a
//! paragraph until they load (no shimmer in the tests: motion is
//! frozen), and empty states with their actions.

use egui::Ui;
use egui_pop::core::icon::Icon;
use egui_pop::core::placeholder::{EmptyState, Skeleton};
use egui_pop::core::widgets;

use super::section;

/// Whether the pretend content loaded, and what the actions did.
#[derive(Debug, Default)]
pub struct State {
    loaded: bool,
    imports: u32,
    retries: u32,
}

pub fn show(ui: &mut Ui, state: &mut State) {
    ui.label("A skeleton stands in for content that fills progressively, in its layout; its region is busy for assistive technologies. An empty state says why a place is empty and what to do.");
    section(ui, "Skeletons");
    widgets::checkbox(ui, &mut state.loaded, "Loaded");
    if state.loaded {
        for name in ["IMG_0001.raw", "IMG_0002.raw", "IMG_0003.raw"] {
            ui.label(name);
        }
        ui.label("Taken on 12 October, 18:42, at f/2.8 and 1/250 s, ISO 400.");
    } else {
        ui.add(Skeleton::rows(3).width(360.0).label("Loading the queue"));
        ui.add_space(8.0);
        ui.add(Skeleton::text(3).width(360.0).label("Loading the metadata"));
    }
    section(ui, "Empty states");
    ui.columns(2, |cols| {
        if EmptyState::new(Icon::Folder, "No images in this folder")
            .text("Import images to see them here.")
            .action("Import images")
            .show(&mut cols[0])
        {
            state.imports += 1;
        }
        if EmptyState::new(Icon::Error, "The previews could not load")
            .text("The cache folder is read-only.")
            .action("Retry")
            .danger()
            .show(&mut cols[1])
        {
            state.retries += 1;
        }
    });
    ui.weak(format!(
        "Actions run: import {} times, retry {} times.",
        state.imports, state.retries
    ));
}
