//! The user's text settings, owned by egui-pop, on the settings
//! window's Fonts page (under Appearance): the text size
//! (`appearance.text_size`, a percentage among [`TEXT_SIZES`], so the
//! font atlas holds few sizes), applied through the theme's size scale
//! ([`ThemeSpec::text_size`]; only text grows: the interface zoom,
//! `zoom`, scales everything), and a family for interface text and one
//! for numbers and code (`appearance.font_ui`, `appearance.font_mono`;
//! empty keeps the theme's), put first in the theme's lists
//! ([`ThemeSpec::font_ui`]).
//!
//! These are the user's, not the theme's: changing theme keeps them.

use egui_pop_core::fonts::Role;
use egui_pop_core::lock;
use egui_pop_core::theme::ThemeSpec;
use egui_pop_core::tokens::{TEXT_SIZES, text_size_step};
use egui_pop_model::commands::Origin;
use egui_pop_model::ids::{OwnerId, SettingId};
use egui_pop_model::values::{Change, Descriptor, Set, Value};

use crate::app::AppHandle;
use crate::ctx::{Ctx, CtxError};

/// The setting: the text size as a percentage among the steps
/// (`"115%"`).
pub const SETTING_TEXT_SIZE: SettingId = SettingId::from_static("appearance.text_size");

/// The setting naming the interface text's family (empty: the
/// theme's).
pub const SETTING_FONT_UI: SettingId = SettingId::from_static("appearance.font_ui");

/// The setting naming the family of numbers and code (empty: the
/// theme's).
pub const SETTING_FONT_MONO: SettingId = SettingId::from_static("appearance.font_mono");

/// The settings shown on the settings window's Fonts page.
pub const FONT_SETTINGS: [SettingId; 3] = [SETTING_TEXT_SIZE, SETTING_FONT_UI, SETTING_FONT_MONO];

/// The setting of a role's family.
pub fn font_setting(role: Role) -> SettingId {
    match role {
        Role::Ui => SETTING_FONT_UI,
        Role::Mono => SETTING_FONT_MONO,
    }
}

/// A step's label: `"115%"`.
pub fn label(factor: f32) -> String {
    format!("{}%", (factor * 100.0).round() as i32)
}

/// The factor of a label (`"115%"` → 1.15), rounded to a step.
pub fn parse(label: &str) -> Option<f32> {
    label
        .trim()
        .trim_end_matches('%')
        .parse::<f32>()
        .ok()
        .filter(|p| p.is_finite() && *p > 0.0)
        .map(|p| text_size_step(p / 100.0))
}

fn read_choice(app: &AppHandle, id: &SettingId) -> Option<String> {
    lock(&app.store())
        .get(&format!("settings/{id}"))
        .and_then(|e| e.value.as_choice().map(str::to_string))
}

fn write(app: &AppHandle, id: &SettingId, value: Value) {
    let set = Set::new(&format!("settings/{id}"), Change::To(value), Origin::User)
        .by(&OwnerId::FRAMEWORK);
    if let Err(e) = lock(&app.store()).set(set) {
        log::warn!(target: "egui_pop", "{id}: {e}");
    }
}

/// The text size the setting asks for, a factor (1: the theme's).
pub fn text_size(app: &AppHandle) -> f32 {
    read_choice(app, &SETTING_TEXT_SIZE)
        .and_then(|c| parse(&c))
        .unwrap_or(1.0)
}

/// Asks for a text size, rounded to a step; applied before the next
/// frame (the theme is installed again).
pub fn set_text_size(app: &AppHandle, factor: f32) {
    write(
        app,
        &SETTING_TEXT_SIZE,
        Value::Choice(label(text_size_step(factor))),
    );
}

/// The family the user chose for a role, `None` for the theme's.
pub fn font(app: &AppHandle, role: Role) -> Option<String> {
    lock(&app.store())
        .get(&format!("settings/{}", font_setting(role)))
        .and_then(|e| e.value.as_text().map(|t| t.trim().to_string()))
        .filter(|t| !t.is_empty())
}

/// Chooses a family for a role (`None`: the theme's); applied before
/// the next frame.
pub fn set_font(app: &AppHandle, role: Role, family: Option<&str>) {
    write(
        app,
        &font_setting(role),
        Value::Text(family.unwrap_or_default().trim().to_string()),
    );
}

/// What the settings asked for when last seen; the spec changes when
/// they move.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct Seen {
    text_size: Option<f32>,
    fonts: Option<(Option<String>, Option<String>)>,
}

/// Writes the settings into the spec when they moved since `seen`. Seen
/// for the first time, a setting at its default leaves the
/// application's spec alone.
pub(crate) fn apply(app: &AppHandle, seen: &mut Seen, spec: &mut ThemeSpec) -> bool {
    let mut changed = false;
    let size = text_size(app);
    if seen.text_size != Some(size) {
        if seen.text_size.is_some() || (size - 1.0).abs() > f32::EPSILON {
            changed |= (spec.text_size - size).abs() > f32::EPSILON;
            spec.text_size = size;
        }
        seen.text_size = Some(size);
    }
    let fonts = (font(app, Role::Ui), font(app, Role::Mono));
    if seen.fonts.as_ref() != Some(&fonts) {
        if seen.fonts.is_some() || fonts.0.is_some() || fonts.1.is_some() {
            changed |= spec.font_ui != fonts.0 || spec.font_mono != fonts.1;
            spec.font_ui.clone_from(&fonts.0);
            spec.font_mono.clone_from(&fonts.1);
        }
        seen.fonts = Some(fonts);
    }
    changed
}

/// The framework's registrations.
pub(crate) fn register(ctx: &mut Ctx<'_>) -> Result<(), CtxError> {
    let choices: Vec<String> = TEXT_SIZES.iter().map(|f| label(*f)).collect();
    let choices: Vec<&str> = choices.iter().map(String::as_str).collect();
    ctx.setting(
        SETTING_TEXT_SIZE,
        Descriptor::choice("Text size", &choices, "100%").description(
            "Makes the text larger or smaller, without the spacing (the interface zoom scales everything).",
        ),
    )?;
    ctx.setting(
        SETTING_FONT_UI,
        Descriptor::text("Interface font", "").description(
            "A family drawn first for the interface's text, the theme's own behind it. Empty keeps the theme's.",
        ),
    )?;
    ctx.setting(
        SETTING_FONT_MONO,
        Descriptor::text("Monospace font", "").description(
            "A family drawn first for numbers and code, the theme's own behind it. Empty keeps the theme's.",
        ),
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn labels_round_trip_and_snap_to_a_step() {
        for f in TEXT_SIZES {
            assert_eq!(parse(&label(f)), Some(f));
        }
        assert_eq!(parse("120%"), Some(1.15));
        assert_eq!(parse("abc"), None);
        assert_eq!(parse("-5%"), None);
    }
}
