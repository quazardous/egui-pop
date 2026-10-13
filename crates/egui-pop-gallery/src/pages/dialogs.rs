//! The dialogs page: a confirmation, a dangerous one, a form with a path
//! field, a choice, a notice and a confirmation with a close button, each
//! opened by a button; what the accepted ones ran shows below.

use egui::Ui;
use egui_pop::api::dialogs::{Choose, Confirm, DIALOGS, DialogSpec, Form, Info};
use egui_pop::api::stories::Story;
use egui_pop::api::{AppHandle, Ctx, Plugin, PluginError};
use egui_pop::model::commands::CommandSpec;
use egui_pop::model::ids::{CommandId, PluginId, StoryId};
use egui_pop::model::values::{Descriptor, PathKind};
use serde_json::json;

use super::section;
use crate::shell::SHELL;

pub const DIALOGS_PAGE: PluginId = PluginId::from_static("gallery.dialogs");
/// What an accepted dialog runs: keeps its arguments for the page.
pub const CMD_DONE: CommandId = CommandId::from_static("gallery.dialogs.done");

/// The dialogs page.
#[derive(Debug, Default)]
pub struct DialogsPage;

impl Plugin for DialogsPage {
    fn id(&self) -> PluginId {
        DIALOGS_PAGE
    }

    fn title(&self) -> String {
        "Dialogs".into()
    }

    fn requires(&self) -> Vec<PluginId> {
        vec![SHELL]
    }

    fn connect(&mut self, ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
        ctx.command(
            CommandSpec::new(CMD_DONE, "Show what a dialog sent", "Dialogs")
                .hidden()
                .no_agent(),
            |call| {
                call.app
                    .set_context("galleryDialogSent", &call.args.to_string());
                Ok(json!(null))
            },
        )?;
        ctx.story(
            Story::new(StoryId::from_static("dialogs"), "Dialogs", show)
                .description("Confirmations named by their verb, a dangerous one, a form from descriptors with a path picker, a choice, a notice, a close button.")
                .group("Components", 50),
        )?;
        Ok(())
    }
}

fn show(ui: &mut Ui, app: &AppHandle) {
    let Some(d) = app.service(&DIALOGS) else {
        ui.weak("The dialogs plugin is not connected.");
        return;
    };
    ui.label("Each button opens a dialog. Enter accepts (except a danger), Escape cancels.");
    section(ui, "Open");
    ui.horizontal_wrapped(|ui| {
        if ui.button("Confirm an export").clicked() {
            d.open(
                Confirm::new("Export the page?", "Export", CMD_DONE)
                    .detail("A PNG of the page goes next to the document.")
                    .args(json!({ "what": "export" })),
            );
        }
        if ui.button("Delete three images").clicked() {
            d.open(
                Confirm::new("Delete 3 images?", "Delete 3 images", CMD_DONE)
                    .detail("They go to the trash; undo brings them back.")
                    .danger()
                    .args(json!({ "what": "delete" })),
            );
        }
        if ui.button("Export settings").clicked() {
            d.open(
                Form::new("Export settings", "Export", CMD_DONE)
                    .field(
                        "format",
                        Descriptor::choice("Format", &["png", "jpeg", "tiff"], "png"),
                    )
                    .field("quality", Descriptor::int("Quality", 1, 100, 90))
                    .field("metadata", Descriptor::bool("Keep metadata", true))
                    .field("folder", Descriptor::path("Folder", PathKind::Dir, &[])),
            );
        }
        if ui.button("Pick a side").clicked() {
            d.open(
                Choose::new("Which side?", &["Left", "Right", "Both"], CMD_DONE)
                    .detail("Where the panel goes."),
            );
        }
        if ui.button("Show a notice").clicked() {
            d.open(Info::new("Saved", "The document was saved."));
        }
        if ui.button("Confirm, with a close button").clicked() {
            d.open(
                DialogSpec::new(
                    Confirm::new("Share the page?", "Share", CMD_DONE)
                        .detail("A link to the page goes to the clipboard.")
                        .args(json!({ "what": "share" })),
                )
                .closable(true),
            );
        }
    });
    section(ui, "Sent by the last accepted dialog");
    let sent = app.context().get("galleryDialogSent").to_string();
    ui.monospace(if sent.is_empty() {
        "(nothing yet)".into()
    } else {
        sent
    });
}
