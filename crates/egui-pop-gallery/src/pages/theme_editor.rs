//! The theme editor page: the editor over the gallery's theme, its
//! preview pane drawing the gallery's component sampler in the draft's
//! theme, the generator of a scale from a colour, and the fonts tab.

use std::sync::Arc;

use egui::Ui;
use egui_pop::api::stories::Story;
use egui_pop::api::theme_editor::{self, THEME_EDITOR, THEME_EDITOR_PLUGIN};
use egui_pop::api::{AppHandle, Ctx, Plugin, PluginError};
use egui_pop::core::state::Slot;
use egui_pop::core::ui::{Tone, UiPop as _};
use egui_pop::core::widgets::{self, ToggleChip};
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
                .description("The current theme's tokens edited live or in a sandboxed preview, by role: colour pickers with hex fields, the contrast of each pair a token takes part in (AA, AAA), a scale generated from one colour, the font families (picked among those installed, previewed before they are applied) and files, undo, export and import as a token file.")
                .group("Theme", 25),
        )?;
        Ok(())
    }
}

fn show(ui: &mut Ui, app: &AppHandle) {
    ui.label("Every change applies to the whole gallery at once; tick \"Preview only\" to keep changes in the preview pane until you apply them. The theme switchers in the top bar start the editor again from the theme picked. Pick a palette token (blue.9) to generate its scale from a colour; the Fonts tab orders the families, picks them among those installed and previews them before they are applied, and adds font files.");
    if let Some(editor) = app.service(&THEME_EDITOR) {
        editor.set_sampler(Some(Arc::new(|ui: &mut Ui, _: &AppHandle| sampler(ui))));
    }
    section(ui, "Editor");
    theme_editor::show(ui, app);
}

/// The sampler's values, kept across frames.
#[derive(Clone, Debug)]
struct Sample {
    checked: bool,
    option: usize,
    selected: usize,
    chip: bool,
    value: f32,
    text: String,
}

impl Default for Sample {
    fn default() -> Self {
        Sample {
            checked: true,
            option: 0,
            selected: 0,
            chip: true,
            value: 0.6,
            text: "Some text".into(),
        }
    }
}

const SAMPLE: Slot<Sample> = Slot::new("gallery.theme_editor.sample");

/// The gallery's component sampler, drawn in the draft's theme in the
/// editor's preview pane: the kit's components in their states.
fn sampler(ui: &mut Ui) {
    let ctx = ui.ctx().clone();
    let mut s = SAMPLE.get(&ctx).unwrap_or_default();
    ui.heading("Heading");
    ui.horizontal_wrapped(|ui| {
        ui.label("Label");
        ui.strong("Strong");
        ui.weak("Weak");
        let (warn, error) = (ui.visuals().warn_fg_color, ui.visuals().error_fg_color);
        ui.colored_label(warn, "Warning");
        ui.colored_label(error, "Error");
    });
    ui.horizontal_wrapped(|ui| {
        let _ = ui.button("Button");
        ui.add(egui::Button::new("Selected").selected(true));
        let _ = ui.button_tone(Tone::Accent, "Accent");
        let _ = ui.button_tone(Tone::Danger, "Danger");
        ui.add_enabled(false, egui::Button::new("Disabled"));
    });
    ui.horizontal_wrapped(|ui| {
        widgets::checkbox(ui, &mut s.checked, "Checkbox");
        let mut mixed = false;
        ui.add(widgets::Checkbox::new(&mut mixed, "Mixed").indeterminate(true));
        widgets::radio_value(ui, &mut s.option, 0, "One");
        widgets::radio_value(ui, &mut s.option, 1, "Two");
        ui.add(ToggleChip::new(&mut s.chip, "Aa").name("Match case"));
    });
    ui.horizontal_wrapped(|ui| {
        widgets::selectable_value(ui, &mut s.selected, 0, "Selected row");
        widgets::selectable_value(ui, &mut s.selected, 1, "Other row");
    });
    ui.add(egui::Slider::new(&mut s.value, 0.0..=1.0).text("Slider"));
    ui.add(egui::ProgressBar::new(s.value).show_percentage());
    ui.horizontal(|ui| {
        let label = ui.label("Field");
        ui.add(egui::TextEdit::singleline(&mut s.text).desired_width(160.0))
            .labelled_by(label.id);
    });
    SAMPLE.set(&ctx, s);
}
