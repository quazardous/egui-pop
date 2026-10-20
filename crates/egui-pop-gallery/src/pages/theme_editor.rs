//! The theme editor page: the editor over the gallery's theme, and a
//! few widgets beside it to watch follow each change.

use egui::Ui;
use egui_pop::api::stories::Story;
use egui_pop::api::theme_editor::{self, THEME_EDITOR_PLUGIN};
use egui_pop::api::{AppHandle, Ctx, Plugin, PluginError};
use egui_pop::core::widgets;
use egui_pop::model::ids::{PluginId, StoryId};

use super::section;
use crate::shell::SHELL;

pub const THEME_EDITOR_PAGE: PluginId = PluginId::from_static("gallery.theme_editor");

/// The theme editor page.
#[derive(Debug, Default)]
pub struct ThemeEditorPage;

impl Plugin for ThemeEditorPage {
    fn id(&self) -> PluginId {
        THEME_EDITOR_PAGE
    }

    fn title(&self) -> String {
        "Theme editor page".into()
    }

    fn requires(&self) -> Vec<PluginId> {
        vec![SHELL, THEME_EDITOR_PLUGIN]
    }

    fn connect(&mut self, ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
        ctx.story(
            Story::new(StoryId::from_static("theme-editor"), "Theme editor", show)
                .description("The current theme's tokens edited live, by role: colour pickers, the contrast of each pair a token takes part in (AA, AAA), undo, export and import as a token file.")
                .group("Theme", 25),
        )?;
        Ok(())
    }
}

fn show(ui: &mut Ui, app: &AppHandle) {
    ui.label("Every change applies to the whole gallery at once; the theme switchers in the top bar start the editor again from the theme picked.");
    section(ui, "Preview");
    ui.horizontal_wrapped(|ui| {
        let _ = ui.button("A button");
        let mut on = true;
        widgets::checkbox(ui, &mut on, "A checkbox");
        let mut v = 0.6_f32;
        ui.add(egui::Slider::new(&mut v, 0.0..=1.0).text("A slider"));
        ui.hyperlink_to("A link", "https://github.com/quazardous/egui-pop");
        ui.weak("Muted text");
    });
    section(ui, "Editor");
    theme_editor::show(ui, app);
}
