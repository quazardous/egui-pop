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

/// The Agents page (`developer/agents`).
/// The Agents page's title.
pub const AGENTS_SECTION: StrKey = StrKey::new("egui-pop-agent-agents-section", "Agents");
/// What the page is for.
pub const AGENTS_ABOUT: StrKey = StrKey::new(
    "egui-pop-agent-agents-about",
    "Programs on this computer paired to drive this app, and what each may do. Changes apply at once.",
);
/// The paired agents' group.
pub const AGENTS_PAIRED: StrKey = StrKey::new("egui-pop-agent-agents-paired", "Paired agents");
/// The defaults' group.
pub const AGENTS_DEFAULTS: StrKey = StrKey::new("egui-pop-agent-agents-defaults", "Defaults");
/// No agent paired.
pub const AGENTS_EMPTY: StrKey = StrKey::new(
    "egui-pop-agent-agents-empty",
    "No agent is paired. An agent asks to be paired when it connects, and you approve it then.",
);
/// A column: the agent's name.
pub const COL_NAME: StrKey = StrKey::new("egui-pop-agent-agents-col-name", "Name");
/// A column: the agent's id, in the journal.
pub const COL_ID: StrKey = StrKey::new("egui-pop-agent-agents-col-id", "Id");
/// A column: when it was paired.
pub const COL_PAIRED: StrKey = StrKey::new("egui-pop-agent-agents-col-paired", "Paired since");
/// A column: when it last asked something.
pub const COL_SEEN: StrKey = StrKey::new("egui-pop-agent-agents-col-seen", "Last seen");
/// A column: whether it is connected now.
pub const COL_CONNECTED: StrKey = StrKey::new("egui-pop-agent-agents-col-connected", "Connected");
/// A column and its field: the grant's preset.
pub const COL_ACCESS: StrKey = StrKey::new("egui-pop-agent-agents-col-access", "Access");
/// A column: the grant in words.
pub const COL_DETAILS: StrKey = StrKey::new("egui-pop-agent-agents-col-details", "What it may do");
/// A column: whether it reads private data.
pub const COL_PRIVATE: StrKey = StrKey::new("egui-pop-agent-agents-col-private", "Private data");
/// The private data switch's tooltip.
pub const PRIVATE_HELP: StrKey = StrKey::new(
    "egui-pop-agent-agents-private-help",
    "Places, people and file paths in what it reads; off withholds them. Applies at once.",
);
/// A column: whom a default applies to.
pub const COL_WHO: StrKey = StrKey::new("egui-pop-agent-agents-col-who", "Who");
/// The access field's tooltip.
pub const ACCESS_HELP: StrKey = StrKey::new(
    "egui-pop-agent-agents-access-help",
    "What it may do; applies at once.",
);
/// A preset.
pub const PRESET_READ_ONLY: StrKey =
    StrKey::new("egui-pop-agent-agents-preset-read-only", "Read only");
/// A preset.
pub const PRESET_EDITOR: StrKey = StrKey::new("egui-pop-agent-agents-preset-editor", "Editor");
/// A preset.
pub const PRESET_FULL: StrKey = StrKey::new("egui-pop-agent-agents-preset-full", "Full");
/// A grant that is none of the presets.
pub const PRESET_CUSTOM: StrKey =
    StrKey::new("egui-pop-agent-agents-preset-custom", "Set by the app");
/// The read-only preset in words.
pub const DETAIL_READ_ONLY: StrKey = StrKey::new(
    "egui-pop-agent-agents-detail-read-only",
    "Reads everything, changes nothing.",
);
/// The editor preset in words.
pub const DETAIL_EDITOR: StrKey = StrKey::new(
    "egui-pop-agent-agents-detail-editor",
    "Runs the commands offered to agents, destructive ones once you confirm; writes no value directly; no interface input.",
);
/// The full preset in words.
pub const DETAIL_FULL: StrKey = StrKey::new(
    "egui-pop-agent-agents-detail-full",
    "Everything the app lets agents do, interface input included.",
);
/// Another grant in words.
pub const DETAIL_CUSTOM: StrKey = StrKey::new(
    "egui-pop-agent-agents-detail-custom",
    "Commands: { $commands }. Values: { $values }. Interface input: { $ui }.",
);
/// Another read-only grant in words.
pub const DETAIL_CUSTOM_READ: StrKey = StrKey::new(
    "egui-pop-agent-agents-detail-custom-read",
    "Read only. Commands: { $commands }.",
);
/// No pattern.
pub const NONE: StrKey = StrKey::new("egui-pop-agent-agents-none", "none");
/// Yes.
pub const YES: StrKey = StrKey::new("egui-pop-agent-agents-yes", "Yes");
/// No.
pub const NO: StrKey = StrKey::new("egui-pop-agent-agents-no", "No");
/// Connected, with how many connections.
pub const CONNECTED_YES: StrKey =
    StrKey::new("egui-pop-agent-agents-connected-yes", "Yes ({ $count })");
/// Never seen.
pub const NEVER: StrKey = StrKey::new("egui-pop-agent-agents-never", "Never");
/// The unpaired default.
pub const DEFAULT_UNPAIRED: StrKey = StrKey::new(
    "egui-pop-agent-agents-default-unpaired",
    "Clients that are not paired",
);
/// The paired default.
pub const DEFAULT_PAIRED: StrKey = StrKey::new(
    "egui-pop-agent-agents-default-paired",
    "Paired agents without their own access",
);
/// The revoke button.
pub const REVOKE: StrKey = StrKey::new("egui-pop-agent-agents-revoke", "Revoke");
/// The revoke confirmation's title.
pub const REVOKE_TITLE: StrKey = StrKey::new(
    "egui-pop-agent-agents-revoke-title",
    "Revoke this agent's pairing?",
);
/// What revoking does.
pub const REVOKE_TEXT: StrKey = StrKey::new(
    "egui-pop-agent-agents-revoke-text",
    "Its token stops working and its open connections close now. To drive this app again, it must be paired again, with your approval.",
);

/// Every key.
pub const ALL: &[StrKey] = &[
    PAIR_TITLE,
    PAIR_DETAIL,
    PAIR_VERB,
    CONFIRM_TITLE,
    CONFIRM_DETAIL,
    CONFIRM_VERB,
    AGENTS_SECTION,
    AGENTS_ABOUT,
    AGENTS_PAIRED,
    AGENTS_DEFAULTS,
    AGENTS_EMPTY,
    COL_NAME,
    COL_ID,
    COL_PAIRED,
    COL_SEEN,
    COL_CONNECTED,
    COL_ACCESS,
    COL_DETAILS,
    COL_WHO,
    ACCESS_HELP,
    COL_PRIVATE,
    PRIVATE_HELP,
    PRESET_READ_ONLY,
    PRESET_EDITOR,
    PRESET_FULL,
    PRESET_CUSTOM,
    DETAIL_READ_ONLY,
    DETAIL_EDITOR,
    DETAIL_FULL,
    DETAIL_CUSTOM,
    DETAIL_CUSTOM_READ,
    NONE,
    YES,
    NO,
    CONNECTED_YES,
    NEVER,
    DEFAULT_UNPAIRED,
    DEFAULT_PAIRED,
    REVOKE,
    REVOKE_TITLE,
    REVOKE_TEXT,
];

/// The Agents page's texts, as its manifest declares them (translated
/// through `Admin::texts`).
pub const PAGE: &[StrKey] = &[
    AGENTS_SECTION,
    AGENTS_ABOUT,
    AGENTS_PAIRED,
    AGENTS_DEFAULTS,
    AGENTS_EMPTY,
    COL_NAME,
    COL_ID,
    COL_PAIRED,
    COL_SEEN,
    COL_CONNECTED,
    COL_ACCESS,
    COL_DETAILS,
    COL_WHO,
    ACCESS_HELP,
    COL_PRIVATE,
    PRIVATE_HELP,
    PRESET_READ_ONLY,
    PRESET_EDITOR,
    PRESET_FULL,
    PRESET_CUSTOM,
    DETAIL_READ_ONLY,
    DETAIL_EDITOR,
    DETAIL_FULL,
    DETAIL_CUSTOM,
    DETAIL_CUSTOM_READ,
    NONE,
    YES,
    NO,
    CONNECTED_YES,
    NEVER,
    DEFAULT_UNPAIRED,
    DEFAULT_PAIRED,
    REVOKE,
    REVOKE_TITLE,
    REVOKE_TEXT,
];
