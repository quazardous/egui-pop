//! The pointer gestures page: the five profiles, what each binds on the
//! image viewport, the node graph and sliders, and the hint a canvas
//! under the pointer gives.

use egui::Ui;
use egui_pop::api::pointer;
use egui_pop::api::stories::Story;
use egui_pop::api::{AppHandle, Ctx, Plugin, PluginError};
use egui_pop::core::widgets;
use egui_pop::model::ids::{PluginId, StoryId};
use egui_pop::model::pointer::{GESTURES, SCOPES};
use egui_pop::theme::Tokens;

use super::section;
use crate::shell::SHELL;

pub const POINTER_PAGE: PluginId = PluginId::from_static("gallery.pointer");

/// The pointer gestures page.
#[derive(Debug, Default)]
pub struct PointerPage;

impl Plugin for PointerPage {
    fn id(&self) -> PluginId {
        POINTER_PAGE
    }

    fn title(&self) -> String {
        "Pointer gestures".into()
    }

    fn requires(&self) -> Vec<PluginId> {
        vec![SHELL]
    }

    fn connect(&mut self, ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
        ctx.story(
            Story::new(StoryId::from_static("pointer"), "Pointer gestures", show)
                .description("Pointer profiles (darktable, Adobe, Document, Blender / node editors, Trackpad): what the wheel, a pinch and each button do on the image, the node graph and sliders, rebindable in the shortcuts editor.")
                .group("Framework", 107),
        )?;
        Ok(())
    }
}

fn show(ui: &mut Ui, app: &AppHandle) {
    let t = Tokens::of(ui.ctx());
    let current = pointer::profile(app);
    ui.horizontal_wrapped(|ui| {
        for p in egui_pop::model::pointer::builtin() {
            if widgets::selectable_label(ui, p.id == current.id, &p.title).clicked() {
                let _ = pointer::set_profile(app, &p.id);
            }
        }
    });
    ui.add_space(t.space.xs);
    ui.weak(&current.description);
    let changed = pointer::overrides(app).len();
    if changed > 0 {
        ui.label(format!(
            "{changed} gestures changed in the shortcuts editor"
        ));
    }
    if ui.button("Open the shortcuts editor").clicked() {
        app.run(
            egui_pop::api::settings_window::CMD_SETTINGS,
            serde_json::json!({ "section": egui_pop::api::settings_window::SHORTCUTS }),
        );
    }

    section(ui, "Gestures in force");
    let map = egui_pop::core::pointer::map(ui.ctx());
    egui::Grid::new("gallery.pointer.gestures")
        .num_columns(2)
        .striped(true)
        .spacing([t.space.l, t.space.xs])
        .show(ui, |ui| {
            for scope in SCOPES {
                for g in GESTURES.iter().filter(|g| g.scope == scope) {
                    let chords: Vec<String> =
                        map.chords(scope, &g.id).map(|c| c.to_string()).collect();
                    ui.label(g.title);
                    ui.monospace(if chords.is_empty() {
                        "—".to_string()
                    } else {
                        chords.join(", ")
                    });
                    ui.end_row();
                }
            }
        });

    section(ui, "Options");
    ui.label(format!(
        "Wheel over sliders: {}; double click on the image: {}; wheel device: {}.",
        map.options.slider_wheel.as_str(),
        map.options.image_double_click.as_str(),
        map.wheel_device.as_str()
    ));
    ui.weak("Each option follows the profile unless set in the settings.");
}
