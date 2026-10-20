//! The interface zoom, owned by egui-pop: a setting
//! (`appearance.interface_zoom`, one value for every window, in fixed
//! steps so each step's glyphs are rasterised once) and commands to move
//! it, bound by default to `Ctrl+Alt+=`, `Ctrl+Alt+-` and `Ctrl+Alt+0`
//! and rebindable like any command. egui's own zoom keys (Ctrl and `+`,
//! `-`, `0`) are turned off: they would fire with an application's
//! content zoom.
//!
//! Steps (ADR 0011): `interface.zoom` (`{ "from", "to" }`, the factors),
//! whose default sets the setting. Signal: `scale.changed` on the journal
//! (`{ "viewport", "from", "to" }`, pixels per point) whenever a window's
//! scale changes, from the zoom or from a move to another screen.

use egui_pop_core::lock;
use egui_pop_core::scale::{self, ZOOM_STEPS};
use egui_pop_model::commands::{CommandSpec, Origin};
use egui_pop_model::ids::{CommandId, OwnerId, SettingId, StepId};
use egui_pop_model::values::{Change, Descriptor, Set, Value};
use serde_json::json;

use crate::app::{AppHandle, MENU_VIEW};
use crate::ctx::{Ctx, CtxError};
use crate::steps::StepSpec;

/// The setting: the zoom as a percentage among the steps (`"100%"`).
pub const SETTING_ZOOM: SettingId = SettingId::from_static("appearance.interface_zoom");

pub const CMD_ZOOM_IN: CommandId = CommandId::from_static("view.interface.zoom_in");
pub const CMD_ZOOM_OUT: CommandId = CommandId::from_static("view.interface.zoom_out");
pub const CMD_ZOOM_RESET: CommandId = CommandId::from_static("view.interface.zoom_reset");

/// The step changing the zoom.
pub const STEP_ZOOM: StepId = StepId::from_static("interface.zoom");

/// A step's label: `"125%"`.
pub fn label(factor: f32) -> String {
    format!("{}%", (factor * 100.0).round() as i32)
}

/// The factor of a label (`"125%"` → 1.25).
pub fn parse(label: &str) -> Option<f32> {
    label
        .trim()
        .trim_end_matches('%')
        .parse::<f32>()
        .ok()
        .map(|p| p / 100.0)
}

/// The zoom the setting asks for.
pub fn current(app: &AppHandle) -> f32 {
    lock(&app.store())
        .get(&format!("settings/{SETTING_ZOOM}"))
        .and_then(|e| e.value.as_choice().and_then(parse))
        .unwrap_or(1.0)
}

/// Asks for a zoom, through the `interface.zoom` step.
pub fn set(app: &AppHandle, to: f32) -> crate::steps::Outcome {
    let from = current(app);
    app.step(
        &STEP_ZOOM,
        json!({ "from": from, "to": to }),
        |app, args| {
            let to = args["to"].as_f64().unwrap_or(1.0) as f32;
            let to = scale::zoom_step(to, 0);
            lock(&app.store())
                .set(
                    Set::new(
                        &format!("settings/{SETTING_ZOOM}"),
                        Change::To(Value::Choice(label(to))),
                        Origin::User,
                    )
                    .by(&OwnerId::FRAMEWORK)
                    .untracked(),
                )
                .map_err(|e| e.to_string())?;
            Ok(json!({ "zoom": to }))
        },
    )
}

/// The framework's registrations.
pub(crate) fn register(ctx: &mut Ctx<'_>) -> Result<(), CtxError> {
    let choices: Vec<String> = ZOOM_STEPS.iter().map(|f| label(*f)).collect();
    let choices: Vec<&str> = choices.iter().map(String::as_str).collect();
    ctx.setting(
        SETTING_ZOOM,
        Descriptor::choice("Interface zoom", &choices, "100%").description(
            "Makes the whole interface larger or smaller, in every window (Ctrl+Alt+= and Ctrl+Alt+-).",
        ),
    )?;
    ctx.step(
        StepSpec::new(
            STEP_ZOOM,
            "The interface zoom changes (`from`, `to`: factors; `to` is rounded to a step).",
        )
        .args(json!({ "from": 1.0, "to": 1.25 })),
    )?;
    ctx.command(
        CommandSpec::new(CMD_ZOOM_IN, "Zoom in the interface", "View")
            .description("Makes the whole interface one step larger.")
            .key("Ctrl+Alt+Equals")
            .key("Ctrl+Alt+Plus")
            .menu(MENU_VIEW, "7_zoom", 0),
        |call| {
            let to = scale::zoom_step(current(call.app), 1);
            set(call.app, to);
            Ok(json!({ "zoom": current(call.app) }))
        },
    )?
    .command(
        CommandSpec::new(CMD_ZOOM_OUT, "Zoom out the interface", "View")
            .description("Makes the whole interface one step smaller.")
            .key("Ctrl+Alt+Minus")
            .menu(MENU_VIEW, "7_zoom", 1),
        |call| {
            let to = scale::zoom_step(current(call.app), -1);
            set(call.app, to);
            Ok(json!({ "zoom": current(call.app) }))
        },
    )?
    .command(
        CommandSpec::new(CMD_ZOOM_RESET, "Reset the interface zoom", "View")
            .description("Gives the interface its normal size.")
            .key("Ctrl+Alt+0")
            .menu(MENU_VIEW, "7_zoom", 2),
        |call| {
            set(call.app, 1.0);
            Ok(json!({ "zoom": current(call.app) }))
        },
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn labels_round_trip() {
        for f in ZOOM_STEPS {
            assert_eq!(parse(&label(f)), Some((f * 100.0).round() / 100.0));
        }
        assert_eq!(parse("abc"), None);
    }
}
