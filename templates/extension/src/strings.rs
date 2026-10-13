//! The extension's text: Fluent keys with the English shown when no
//! catalogue has them (no `I18nPlugin`, or a locale without the key).
//! The catalogues are `locales/<locale>/tally.ftl`; a test checks that
//! each defines every key here.

use egui_pop::core::strings::{self, StrKey};
use egui_pop::egui::Context;

/// The minus button's accessible name.
pub const DECREASE: StrKey = StrKey::new("tally-decrease", "Decrease { $name }");
/// The plus button's accessible name.
pub const INCREASE: StrKey = StrKey::new("tally-increase", "Increase { $name }");
/// The count, as shown and as a screen reader says it.
pub const VALUE: StrKey = StrKey::new("tally-value", "{ $name }: { $count }");
/// Drawn instead of a tally when the plugin is not connected.
pub const MISSING: StrKey = StrKey::new("tally-missing", "The tally plugin is not connected.");

/// Every key, for the catalogue check.
pub const ALL: &[StrKey] = &[DECREASE, INCREASE, VALUE, MISSING];

/// A key's text in the interface's language.
pub fn tr(ctx: &Context, key: &StrKey, args: &[(&str, &str)]) -> String {
    strings::tr_args(ctx, key, args)
}

/// [`MISSING`] in English, for errors returned to callers.
pub fn missing_english() -> &'static str {
    MISSING.fallback
}
