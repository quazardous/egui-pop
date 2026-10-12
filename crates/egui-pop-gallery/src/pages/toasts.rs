//! The toasts page: one button per level, one with an action.

use egui::Ui;
use egui_pop::api::stories::Story;
use egui_pop::api::toasts::{TOASTS, Toast};
use egui_pop::api::{AppHandle, Ctx, Plugin, PluginError};
use egui_pop::model::ids::{PluginId, StoryId};
use serde_json::json;

use super::dialogs::CMD_DONE;
use super::section;
use crate::shell::SHELL;

pub const TOASTS_PAGE: PluginId = PluginId::from_static("gallery.toasts");

/// The toasts page.
#[derive(Debug, Default)]
pub struct ToastsPage;

impl Plugin for ToastsPage {
    fn id(&self) -> PluginId {
        TOASTS_PAGE
    }

    fn title(&self) -> String {
        "Toasts".into()
    }

    fn requires(&self) -> Vec<PluginId> {
        vec![SHELL, super::dialogs::DIALOGS_PAGE]
    }

    fn connect(&mut self, ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
        ctx.story(
            Story::new(StoryId::from_static("toasts"), "Toasts", show)
                .description("Notices in a corner that never take the focus: by level, with an action, closed by hand or by time.")
                .group("Components", 55),
        )?;
        Ok(())
    }
}

fn show(ui: &mut Ui, app: &AppHandle) {
    let Some(t) = app.service(&TOASTS) else {
        ui.weak("The toasts plugin is not connected.");
        return;
    };
    ui.label("Information and success go after 4 s, warnings after 8 s, errors stay; the pointer over one holds it.");
    section(ui, "Show a toast");
    ui.horizontal_wrapped(|ui| {
        if ui.button("Information").clicked() {
            t.notify(Toast::info("The library has 12 480 images."));
        }
        if ui.button("Success with an action").clicked() {
            t.notify(Toast::success("Exported 3 images.").action(
                "Show",
                CMD_DONE,
                json!({ "what": "show the export" }),
            ));
        }
        if ui.button("Warning").clicked() {
            t.notify(Toast::warning("The disk is nearly full."));
        }
        if ui.button("Error").clicked() {
            t.notify(Toast::error("The export failed: no space left."));
        }
        if ui.button("Long text").clicked() {
            t.notify(Toast::warning(
                "The export of 3 images was stopped: the destination folder on the external disk is read-only.",
            ));
            t.notify(Toast::error(
                "L'export de 3 images a échoué : il ne reste plus de place sur le disque de destination choisi.",
            ).action("Retry", CMD_DONE, json!({ "what": "retry the export" })));
        }
    });
    section(ui, "Sent by the last action");
    let sent = app.context().get("galleryDialogSent").to_string();
    ui.monospace(if sent.is_empty() {
        "(nothing yet)".into()
    } else {
        sent
    });
}
