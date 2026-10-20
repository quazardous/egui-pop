//! A minimal egui-pop application: one plugin with a panel, a setting
//! and a command, over the default plugins (palette, dialogs, toasts,
//! settings, window). The `cargo generate` template starts from this
//! file (see `template/`).
//!
//! Run it with `cargo run -p egui-pop --example minimal`.

use egui_pop::api::app::MENU_EDIT;
use egui_pop::api::toasts::{TOASTS, Toast};
use egui_pop::core::binding::{self, Binding};
use egui_pop::eframe;
use egui_pop::egui;
use egui_pop::model::panels::{Area, PanelSpec};
use egui_pop::prelude::*;

/// Where the note is kept.
const NOTE: SettingId = SettingId::from_static("notes.text");
const CLEAR: CommandId = CommandId::from_static("notes.clear");

/// The application's one plugin.
#[derive(Debug)]
pub struct Notes;

impl Plugin for Notes {
    fn id(&self) -> PluginId {
        PluginId::from_static("notes")
    }

    fn title(&self) -> String {
        "Notes".into()
    }

    fn connect(&mut self, ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
        // A setting: saved, in the settings window (Ctrl+,), with a reset.
        ctx.setting(
            NOTE,
            Descriptor::text("Note", "Write here.").description("The note the panel shows."),
        )?;
        // A command: its key, menu entry and palette entry come with it.
        ctx.command(
            CommandSpec::new(CLEAR, "Clear the note", "Notes")
                .key("Ctrl+Shift+K")
                .menu(MENU_EDIT, "5_notes", 0),
            |call| {
                let store = call.app.store();
                egui_pop::core::lock(&store)
                    .set(egui_pop::model::values::Set::new(
                        "settings/notes.text",
                        egui_pop::model::values::Change::To(egui_pop::model::values::Value::Text(
                            String::new(),
                        )),
                        call.origin.clone(),
                    ))
                    .map_err(|e| e.to_string())?;
                if let Some(t) = call.app.service(&TOASTS) {
                    t.notify(Toast::info("The note is cleared (Ctrl+Z brings it back)."));
                }
                Ok(serde_json::Value::Null)
            },
        )?;
        // A panel in the centre, its text bound to the setting.
        ctx.panel(
            PanelSpec::new(PanelId::from_static("notes.panel"), "Note", Area::Center),
            |ui, app| {
                let store = app.store();
                let heading = ui.heading("Note");
                binding::edit(
                    ui,
                    Binding::bound(&store, "settings/notes.text"),
                    |ui, v: &mut String| {
                        ui.add(egui::TextEdit::multiline(v).desired_width(f32::INFINITY))
                            .labelled_by(heading.id)
                    },
                );
                if ui.button("Clear").clicked() {
                    app.run(CLEAR, serde_json::Value::Null);
                }
            },
        )?;
        Ok(())
    }
}

fn main() -> eframe::Result {
    let dir = std::env::temp_dir().join("egui-pop-minimal");
    App::builder()
        .plugins(DefaultPlugins)
        .plugin(Notes)
        .persist(dir)
        .run_native("Notes", eframe::NativeOptions::default())
}
