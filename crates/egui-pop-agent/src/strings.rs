//! The text the agent socket shows itself, by key with English
//! fallbacks, translated by the installed `egui_pop_core::strings`
//! provider (egui-pop's catalogues carry them). What its plugin declares
//! (its setting, its command) is translated where it is drawn, through
//! keys derived from its id. [`ALL`] lists every key.

use egui_pop_core::strings::StrKey;

/// The pairing request's title.
pub const PAIR_TITLE: StrKey =
    StrKey::new("egui-pop-agent-pair-title", "Pair the agent “{ $name }”?");
/// What pairing it means.
pub const PAIR_DETAIL: StrKey = StrKey::new(
    "egui-pop-agent-pair-detail",
    "A program on this computer asks to drive this app as “{ $id }”, within what the app allows agents. Pair it only if you started it.",
);
/// Its button.
pub const PAIR_VERB: StrKey = StrKey::new("egui-pop-agent-pair-verb", "Pair");

/// Every key.
pub const ALL: &[StrKey] = &[PAIR_TITLE, PAIR_DETAIL, PAIR_VERB];
