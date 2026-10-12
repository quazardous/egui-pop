//! The text the platform services show themselves, by key with English
//! fallbacks, translated by the installed `egui_pop_core::strings`
//! provider (egui-pop's catalogues carry them). What their plugins
//! declare (settings, commands, plugin titles) is translated where it is
//! drawn, through keys derived from its id. [`ALL`] lists every key.

use egui_pop_core::strings::StrKey;

/// The toast offering a crash report from the last session.
pub const CRASH_OFFER: StrKey = StrKey::new(
    "egui-pop-crash-offer",
    "{ $app } closed unexpectedly last time. A report was saved on this computer; nothing was sent.",
);
/// Its button.
pub const CRASH_SHOW: StrKey = StrKey::new("egui-pop-crash-show", "Show the report");

/// Every key.
pub const ALL: &[StrKey] = &[CRASH_OFFER, CRASH_SHOW];
