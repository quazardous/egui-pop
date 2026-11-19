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

/// A destructive call's confirmation: its title.
pub const CONFIRM_TITLE: StrKey = StrKey::new(
    "egui-pop-agent-confirm-title",
    "Let “{ $agent }” run “{ $what }”?",
);
/// What it means, with the call's arguments.
pub const CONFIRM_DETAIL: StrKey = StrKey::new(
    "egui-pop-agent-confirm-detail",
    "An agent asks for something that cannot easily be undone. Arguments: { $args }. Allow it only if you asked for it.",
);
/// Its button.
pub const CONFIRM_VERB: StrKey = StrKey::new("egui-pop-agent-confirm-verb", "Run");

/// Every key.
pub const ALL: &[StrKey] = &[
    PAIR_TITLE,
    PAIR_DETAIL,
    PAIR_VERB,
    CONFIRM_TITLE,
    CONFIRM_DETAIL,
    CONFIRM_VERB,
];
