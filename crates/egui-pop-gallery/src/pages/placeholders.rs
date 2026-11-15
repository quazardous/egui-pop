//! The placeholders page: skeletons standing in for a list and a
//! paragraph until they load (no shimmer in the tests: motion is
//! frozen), empty states with their actions, and late values in their
//! four states.

use egui::Ui;
use egui_pop::core::icon::Icon;
use egui_pop::core::late::{Late, LateValue, Waiting};
use egui_pop::core::placeholder::{EmptyState, Skeleton};
use egui_pop::core::ui::{Space, UiPop};
use egui_pop::core::widgets;

use super::section;

/// Whether the pretend content loaded, and what the actions did.
#[derive(Debug, Default)]
pub struct State {
    loaded: bool,
    imports: u32,
    retries: u32,
    late_retries: u32,
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
        ui.space_role(Space::Group);
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
    ui.muted(format!(
        "Actions run: import {} times, retry {} times.",
        state.imports, state.retries
    ));
    section(ui, "Late values");
    ui.label("A value worked out asynchronously keeps its room in every state: a skeleton or a delayed spinner sized like the expected value, the value, the last value with a small spinner while it is worked out again, an error with Retry.");
    let rows: [(&str, Late<String>, Waiting); 5] = [
        ("Photos", Late::Pending, Waiting::Skeleton),
        ("Folders", Late::Pending, Waiting::Spinner),
        (
            "Free space",
            Late::Fresh("412.7 GB".into()),
            Waiting::Skeleton,
        ),
        (
            "Folder size",
            Late::Refreshing("18.3 GB".into()),
            Waiting::Skeleton,
        ),
        (
            "Backup disk",
            Late::Failed("Not mounted".into()),
            Waiting::Skeleton,
        ),
    ];
    egui::Grid::new("gallery.placeholders.late").show(ui, |ui| {
        for (name, late, waiting) in rows {
            ui.muted(format!("{name}:"));
            if LateValue::new(name, late)
                .sample("000.0 GB")
                .waiting(waiting)
                .show(ui)
                .retry
            {
                state.late_retries += 1;
            }
            ui.end_row();
        }
    });
    ui.muted(format!("Late values retried {} times.", state.late_retries));
}
