//! The tips page: the tip of the day, hints beside what they are about,
//! and the tips registered with whether they were dismissed.

use egui::Ui;
use egui_pop::api::stories::Story;
use egui_pop::api::tips::{self, CMD_TIPS_RESET, TIPS, TIPS_PLUGIN, Tip};
use egui_pop::api::{AppHandle, Ctx, Plugin, PluginError};
use egui_pop::model::ids::{PluginId, StoryId};

use super::section;
use crate::shell::SHELL;

pub const TIPS_PAGE: PluginId = PluginId::from_static("gallery.tips");

/// The gallery's tips (their French is in `locales/fr/gallery.ftl`).
pub fn gallery_tips() -> Vec<Tip> {
    vec![
        Tip::new(
            "gallery.next-page",
            "Go through the pages",
            "Alt+Down and Alt+Up go to the next and the previous page.",
        ),
        Tip::new(
            "gallery.next-theme",
            "Try every theme",
            "Ctrl+Shift+T switches to the next theme; the theme editor starts from it.",
        ),
        Tip::new(
            "gallery.palette",
            "Find any command",
            "Ctrl+Shift+P opens the command palette.",
        ),
        Tip::new(
            "gallery.export-hint",
            "Exports keep your choices",
            "This button exports with the settings used last time.",
        )
        .contextual(),
    ]
}

/// The tips page.
#[derive(Debug, Default)]
pub struct TipsPage;

impl Plugin for TipsPage {
    fn id(&self) -> PluginId {
        TIPS_PAGE
    }

    fn title(&self) -> String {
        "Tips page".into()
    }

    fn requires(&self) -> Vec<PluginId> {
        vec![SHELL, TIPS_PLUGIN]
    }

    fn connect(&mut self, ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
        if let Some(tips) = ctx.app().service(&TIPS) {
            for tip in gallery_tips() {
                tips.add(tip);
            }
        }
        ctx.story(
            Story::new(StoryId::from_static("tips"), "Tips", show)
                .description("A tip of the day and hints beside what they are about, dismissed for good through the settings, in the interface's language.")
                .group("Components", 57),
        )?;
        Ok(())
    }
}

fn show(ui: &mut Ui, app: &AppHandle) {
    ui.label("Plugins register tips; a dismissed tip is remembered in the settings (tips.dismissed) and comes back with \"Show every tip again\".");
    section(ui, "Tip of the day");
    if !tips::tip_of_the_day(ui, app) {
        ui.weak("No tip of the day to show: all dismissed, closed or turned off.");
    }
    section(ui, "A hint beside its button");
    ui.horizontal_top(|ui| {
        let _ = ui.button("Export");
        ui.vertical(|ui| {
            ui.set_max_width(320.0);
            if !tips::hint(ui, app, "gallery.export-hint") {
                ui.weak("The hint was dismissed.");
            }
        });
    });
    section(ui, "The tips registered");
    let Some(t) = app.service(&TIPS) else {
        return;
    };
    let dismissed = t.dismissed(app);
    egui::Grid::new("gallery.tips.list")
        .num_columns(2)
        .striped(true)
        .show(ui, |ui| {
            for tip in t.all() {
                ui.monospace(&tip.id);
                ui.weak(match (tip.contextual, dismissed.contains(&tip.id)) {
                    (_, true) => "dismissed",
                    (true, false) => "hint",
                    (false, false) => "tip of the day",
                });
                ui.end_row();
            }
        });
    if ui.button("Show every tip again").clicked() {
        app.run(CMD_TIPS_RESET, serde_json::Value::Null);
    }
}
