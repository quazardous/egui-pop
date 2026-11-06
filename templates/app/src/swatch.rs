//! The application's plugin: everything it registers through `Ctx` is
//! its own, and goes away if it is disconnected.

use egui_pop::api::app::MENU_EDIT;
use egui_pop::core::param;
use egui_pop::egui;
use egui_pop::model::panels::{Area, PanelSpec};
use egui_pop::model::values::{Change, Value};
use egui_pop::prelude::*;
use serde_json::{Value as Json, json};

/// The plugin's id; every other id starts with it.
pub const PLUGIN: PluginId = PluginId::from_static("swatch");
/// The swatch's hue, in degrees: a value of the store.
pub const HUE: &str = "swatch/hue";
/// The swatch's side, in points.
pub const SIZE: &str = "swatch/size";
/// Whether the colour's code is shown under the swatch: a setting, kept
/// in the store at `SHOW_CODE.path()` (`settings/swatch.show_code`).
pub const SHOW_CODE: SettingId = SettingId::from_static("swatch.show_code");
/// Puts the hue and the size back to their defaults, as one undo step.
pub const RESET: CommandId = CommandId::from_static("swatch.reset");
/// Sets the hue and the size (`hue`, `size`); offered to agents.
pub const SET: CommandId = CommandId::from_static("swatch.set");
/// The panel showing the swatch, in the centre.
pub const VIEW: PanelId = PanelId::from_static("swatch.view");
/// The panel with its parameters, in the end side bar.
pub const PARAMETERS: PanelId = PanelId::from_static("swatch.parameters");

fn hue() -> Descriptor {
    Descriptor::float("Hue", 0.0, 360.0, 200.0).unit("°")
}

fn size() -> Descriptor {
    Descriptor::float("Size", 40.0, 300.0, 160.0).unit("pt")
}

/// The application's one plugin.
#[derive(Debug)]
pub struct Swatch;

impl Plugin for Swatch {
    fn id(&self) -> PluginId {
        PLUGIN
    }

    fn title(&self) -> String {
        "Swatch".into()
    }

    fn connect(&mut self, ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
        // The state: two values of the store, owned by the plugin, each
        // change an undo step.
        ctx.value(HUE, hue())?;
        ctx.value(SIZE, size())?;
        // A setting: in the settings window (Ctrl+,), saved, with a reset.
        ctx.setting(
            SHOW_CODE,
            Descriptor::bool("Show the colour's code", true)
                .description("Writes the colour's hexadecimal code under the swatch."),
        )?;
        // A command for people: its key, its menu entry and its palette
        // entry come with it.
        ctx.command(
            CommandSpec::new(RESET, "Reset the swatch", "Swatch")
                .key("Ctrl+Shift+R")
                .menu(MENU_EDIT, "5_swatch", 0),
            |call| {
                let changes = [(HUE, Change::Reset), (SIZE, Change::Reset)];
                write(call, "Reset the swatch", &changes)
            },
        )?;
        // A command for agents (and scripts): its arguments described,
        // kept out of the palette.
        ctx.command(
            CommandSpec::new(SET, "Set the swatch", "Swatch")
                .description("Sets the hue (`hue`, 0 to 360) and the size (`size`, 40 to 300).")
                .args(json!({ "hue": 30.0, "size": 120.0 }))
                .hidden(),
            |call| {
                let mut changes = Vec::new();
                for (key, path) in [("hue", HUE), ("size", SIZE)] {
                    if let Some(v) = call.args[key].as_f64() {
                        changes.push((path, Change::To(Value::Float(v))));
                    }
                }
                write(call, "Set the swatch", &changes)
            },
        )?;
        // Two panels: the workbench places them, the user moves them,
        // and the layout is kept.
        ctx.panel(PanelSpec::new(VIEW, "Swatch", Area::Center), view)?;
        ctx.panel(
            PanelSpec::new(PARAMETERS, "Parameters", Area::EndSide),
            parameters,
        )?;
        Ok(())
    }
}

/// Writes `changes` as one undo step named `label`, as the caller (the
/// user, an agent): all of them, or none (an error puts them back).
fn write(call: &Call<'_>, label: &str, changes: &[(&str, Change)]) -> Result<Json, String> {
    call.group(label, |g| {
        for (path, change) in changes {
            g.set(path, change.clone())?;
        }
        Ok(Json::Null)
    })
}

/// The swatch, its colour read from the store each frame.
fn view(ui: &mut egui::Ui, app: &AppHandle) {
    let hue: f64 = app.value(HUE).unwrap_or(200.0);
    let size: f64 = app.value(SIZE).unwrap_or(160.0);
    let show_code: bool = app.setting(&SHOW_CODE).unwrap_or(true);
    let [r, g, b] = egui::ecolor::rgb_from_hsv((hue as f32 / 360.0, 0.6, 0.85))
        .map(|c| (c * 255.0).round() as u8);
    let colour = egui::Color32::from_rgb(r, g, b);
    // Custom painting reads the theme's tokens, never literal colours.
    let tokens = Tokens::of(ui.ctx());
    ui.vertical_centered(|ui| {
        ui.space_role(Space::Section);
        let side = size as f32;
        let (rect, _) = ui.allocate_exact_size(egui::vec2(side, side), egui::Sense::hover());
        ui.painter().rect(
            rect,
            tokens.radius.window,
            colour,
            egui::Stroke::new(1.0, tokens.border.strong),
            egui::StrokeKind::Inside,
        );
        if show_code {
            ui.meta(format!("#{r:02x}{g:02x}{b:02x}"));
        }
    });
}

/// The parameters, each widget bound to its value (the editor its
/// descriptor calls for): a drag is one undo step.
fn parameters(ui: &mut egui::Ui, app: &AppHandle) {
    let store = app.store();
    param::bound(ui, &store, HUE);
    param::bound(ui, &store, SIZE);
    if ui.button("Reset").clicked() {
        app.run(RESET, Json::Null);
    }
}
