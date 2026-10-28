//! The tips page: the tip of the day, hints beside what they are about,
//! and the tips registered with whether they were dismissed.

use egui::Ui;
use egui_pop::api::stories::Story;
use egui_pop::api::tips::{self, CMD_TIPS_BROWSE, CMD_TIPS_RESET, TIPS, TIPS_PLUGIN, Tip};
use egui_pop::api::{AppHandle, Ctx, Plugin, PluginError};
use egui_pop::model::ids::{PluginId, StoryId};

use super::section;
use crate::shell::SHELL;

pub const TIPS_PAGE: PluginId = PluginId::from_static("gallery.tips");

/// The gallery's tips declared in files, `tips/<id>.md` (their French
/// is in `locales/fr/gallery.ftl`).
pub const TIP_FILES: &[(&str, &str)] = &[
    (
        "gallery.next-page.md",
        include_str!("../../tips/gallery.next-page.md"),
    ),
    (
        "gallery.next-theme.md",
        include_str!("../../tips/gallery.next-theme.md"),
    ),
    (
        "gallery.palette.md",
        include_str!("../../tips/gallery.palette.md"),
    ),
    (
        "gallery.import-here.md",
        include_str!("../../tips/gallery.import-here.md"),
    ),
];

/// The tip pointing at the page's Import button, and its target.
const POINTING_TIP: &str = "gallery.import-here";
const IMPORT_TARGET: &str = "gallery.tips.import";

/// The gallery's tips declared in code.
pub fn gallery_tips() -> Vec<Tip> {
    vec![
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
            let errors = tips.add_files("gallery", TIP_FILES);
            debug_assert!(errors.is_empty(), "the gallery's tips: {errors:?}");
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
    ui.label("Plugins register tips, in code or in files (tips/<id>.md: a header, a title, the text with {key} for the command's key in force); a dismissed tip is remembered in the settings (tips.dismissed) and comes back with \"Show every tip again\".");
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
    section(ui, "A tip beside what it is about");
    ui.label("A tip names its target; the card goes beside it, never over it, with a halo. Escape, \"Not now\" or \"Got it\" closes it.");
    ui.horizontal(|ui| {
        tips::mark(ui.button("Import"), IMPORT_TARGET);
        ui.add_space(ui.spacing().item_spacing.x);
        if ui.button("Show the tip again").clicked()
            && let Some(t) = app.service(&TIPS)
        {
            let _ = t.show(app, POINTING_TIP);
        }
    });
    // Shown once when the page first opens.
    let once = egui::Id::new("gallery.tips.pointing-shown");
    if !ui.ctx().data(|d| d.get_temp::<bool>(once).unwrap_or(false)) {
        ui.ctx().data_mut(|d| d.insert_temp(once, true));
        if let Some(t) = app.service(&TIPS) {
            let _ = t.show(app, POINTING_TIP);
        }
    }
    section(ui, "The tips registered");
    let Some(t) = app.service(&TIPS) else {
        return;
    };
    let dismissed = t.dismissed(app);
    let off = t.off(app);
    egui::Grid::new("gallery.tips.list")
        .num_columns(3)
        .striped(true)
        .show(ui, |ui| {
            for tip in t.all() {
                ui.monospace(&tip.id);
                ui.weak(if off.contains(tip.origin_name()) {
                    format!("{} (off)", tip.origin_name())
                } else {
                    tip.origin_name().to_string()
                });
                ui.weak(match (tip.contextual, dismissed.contains(&tip.id)) {
                    (_, true) => "dismissed",
                    (true, false) => "hint",
                    (false, false) => "tip of the day",
                });
                ui.end_row();
            }
        });
    ui.horizontal(|ui| {
        if ui.button("Show every tip again").clicked() {
            app.run(CMD_TIPS_RESET, serde_json::Value::Null);
        }
        // The browser, as Help > Tips… opens it.
        if ui.button("Browse the tips").clicked() {
            app.run(CMD_TIPS_BROWSE, serde_json::Value::Null);
        }
    });
}
