//! The strings egui-pop shows (Cancel, Retry, accessibility
//! descriptions, the titles of what plugins declare), by key with English
//! fallbacks. An application installs a [`Strings`] provider (the
//! `egui-pop-i18n` plugin, for instance); without one, the fallbacks are
//! used.
//!
//! Keys are Fluent message ids, with an optional attribute
//! (`egui-pop-retry`, `command-app-quit.title`), so a provider can read
//! them from `.ftl` catalogues as they are. Placeholders are written as
//! Fluent writes them, `{ $name }`, and filled by [`tr_args`].
//!
//! What plugins declare (commands, panels, menus, settings, plugins) is
//! translated through keys derived from its stable id, never from its
//! English, so rewording English keeps the translations:
//!
//! | What | Key |
//! |---|---|
//! | a command's title, description | `command-<id>.title`, `.description` |
//! | a command's category | `category-<english>` (categories have no id) |
//! | a panel's title | `panel-<id>.title` |
//! | a submenu's label | `menu-<id>.label` |
//! | a setting's label, description | `setting-<id>.label`, `.description` |
//! | one value of a choice setting | `setting-<id>-choice-<value>` |
//! | a plugin's title (a settings section) | `plugin-<id>.title` |
//!
//! where every id is written with [`slug`] (a category's English in
//! lower case).
//!
//! ```
//! use std::sync::Arc;
//! use egui_pop_core::strings::{self, MapStrings, RETRY};
//!
//! let ctx = egui::Context::default();
//! assert_eq!(strings::tr(&ctx, &RETRY), "Retry");
//! strings::install(&ctx, Arc::new(MapStrings::new("fr").with(RETRY.key, "Réessayer")));
//! assert_eq!(strings::tr(&ctx, &RETRY), "Réessayer");
//! assert_eq!(strings::command_title(&ctx, "app.quit", "Quit"), "Quit");
//! ```

use std::collections::BTreeMap;
use std::sync::Arc;

use egui::{Context, Id};

/// A string's key and its English fallback. `{ $name }` placeholders are
/// filled by [`tr_args`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StrKey {
    pub key: &'static str,
    pub fallback: &'static str,
}

impl StrKey {
    pub const fn new(key: &'static str, fallback: &'static str) -> Self {
        Self { key, fallback }
    }
}

pub const OK: StrKey = StrKey::new("egui-pop-ok", "OK");
pub const CANCEL: StrKey = StrKey::new("egui-pop-cancel", "Cancel");
pub const CLOSE: StrKey = StrKey::new("egui-pop-close", "Close");
pub const RESET: StrKey = StrKey::new("egui-pop-reset", "Reset");
pub const RETRY: StrKey = StrKey::new("egui-pop-retry", "Retry");
pub const FAULT_TITLE: StrKey = StrKey::new("egui-pop-fault-title", "{ $name } stopped working");
pub const FAULT_BODY: StrKey = StrKey::new(
    "egui-pop-fault-body",
    "The rest of the application is fine. Retry, or report the message below.",
);
pub const HELD_BY: StrKey = StrKey::new("egui-pop-value-held-by", "Being changed by { $origin }");
pub const READ_ONLY: StrKey = StrKey::new("egui-pop-value-read-only", "Read only");
/// A bound widget's tooltip when its path holds no value.
pub const NO_VALUE: StrKey = StrKey::new("egui-pop-value-missing", "no value at `{ $path }`");
/// The window frame's buttons (the close button says [`CLOSE`]).
pub const MINIMIZE: StrKey = StrKey::new("egui-pop-frame-minimize", "Minimize");
pub const MAXIMIZE: StrKey = StrKey::new("egui-pop-frame-maximize", "Maximize");
pub const RESTORE: StrKey = StrKey::new("egui-pop-frame-restore", "Restore");

/// Every string egui-pop core shows, for catalogues and tests.
pub const ALL: &[StrKey] = &[
    OK,
    CANCEL,
    CLOSE,
    RESET,
    RETRY,
    FAULT_TITLE,
    FAULT_BODY,
    HELD_BY,
    READ_ONLY,
    NO_VALUE,
    MINIMIZE,
    MAXIMIZE,
    RESTORE,
];

/// Gives the text of a key in the current language, if it has one.
pub trait Strings: Send + Sync {
    /// The text for a key; `None` falls back to English.
    fn get(&self, key: &str) -> Option<String>;

    /// The text for a key with its arguments placed; `None` falls back
    /// to English. By default [`Strings::get`] with its `{ $name }`
    /// placeholders filled; a provider formatting its own (plurals,
    /// numbers) overrides it.
    fn get_args(&self, key: &str, args: &[(&str, &str)]) -> Option<String> {
        self.get(key).map(|t| fill(&t, args))
    }

    /// The language, as a BCP 47 tag.
    fn language(&self) -> &str {
        "en"
    }

    /// A number as the language writes it, with exactly `decimals`
    /// digits after the separator and its thousands grouped; `None`
    /// writes it as Rust does (`1234.50`). A provider that knows the
    /// locale's separators overrides it.
    fn number(&self, value: f64, decimals: usize) -> Option<String> {
        let _ = (value, decimals);
        None
    }
}

/// No translations: every string is its English fallback.
#[derive(Clone, Copy, Debug, Default)]
pub struct English;

impl Strings for English {
    fn get(&self, _key: &str) -> Option<String> {
        None
    }
}

/// Translations from a map, for tests and small applications.
#[derive(Clone, Debug, Default)]
pub struct MapStrings {
    language: String,
    map: BTreeMap<String, String>,
}

impl MapStrings {
    pub fn new(language: &str) -> Self {
        Self {
            language: language.into(),
            map: BTreeMap::new(),
        }
    }

    pub fn with(mut self, key: &str, text: &str) -> Self {
        self.map.insert(key.into(), text.into());
        self
    }
}

impl Strings for MapStrings {
    fn get(&self, key: &str) -> Option<String> {
        self.map.get(key).cloned()
    }

    fn language(&self) -> &str {
        &self.language
    }
}

fn id() -> Id {
    Id::new("egui_pop_core::strings")
}

/// Installs a provider for a context.
pub fn install(ctx: &Context, strings: Arc<dyn Strings>) {
    ctx.data_mut(|d| d.insert_temp(id(), strings));
}

/// The installed provider (English when none is).
pub fn provider(ctx: &Context) -> Arc<dyn Strings> {
    ctx.data(|d| d.get_temp::<Arc<dyn Strings>>(id()))
        .unwrap_or_else(|| Arc::new(English))
}

/// A number in the interface's language, with exactly `decimals` digits
/// after the separator (slider values, readouts):
/// [`Strings::number`], else as Rust writes it.
///
/// ```
/// assert_eq!(egui_pop_core::strings::number(&egui::Context::default(), 1234.5, 2), "1234.50");
/// ```
pub fn number(ctx: &Context, value: f64, decimals: usize) -> String {
    provider(ctx)
        .number(value, decimals)
        .unwrap_or_else(|| format!("{value:.decimals$}"))
}

/// The English shown when a provider has nothing: the fallback, else
/// the key itself.
fn english(key: &str, fallback: &str) -> String {
    if fallback.is_empty() {
        key.to_string()
    } else {
        fallback.to_string()
    }
}

/// The text of a key.
pub fn tr(ctx: &Context, key: &StrKey) -> String {
    lookup(ctx, key.key, key.fallback)
}

/// The text of a key with its `{ $name }` placeholders filled.
pub fn tr_args(ctx: &Context, key: &StrKey, args: &[(&str, &str)]) -> String {
    lookup_args(ctx, key.key, key.fallback, args)
}

/// The text of any key, with the English to show without a translation
/// (the key itself when `fallback` is empty).
pub fn lookup(ctx: &Context, key: &str, fallback: &str) -> String {
    provider(ctx)
        .get(key)
        .unwrap_or_else(|| english(key, fallback))
}

/// [`lookup`] with arguments.
pub fn lookup_args(ctx: &Context, key: &str, fallback: &str, args: &[(&str, &str)]) -> String {
    provider(ctx)
        .get_args(key, args)
        .unwrap_or_else(|| fill(&english(key, fallback), args))
}

/// Fills `{ $name }` (and `{$name}`) placeholders.
pub fn fill(text: &str, args: &[(&str, &str)]) -> String {
    let mut text = text.to_string();
    for (name, value) in args {
        text = text
            .replace(&format!("{{ ${name} }}"), value)
            .replace(&format!("{{${name}}}"), value);
    }
    text
}

/// A stable id written as part of a Fluent identifier: ASCII letters,
/// digits and `-` are kept, every other character becomes `-`
/// (`app.quit` gives `app-quit`). Two ids that differ only by those
/// characters share a key.
pub fn slug(id: &str) -> String {
    id.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' {
                c
            } else {
                '-'
            }
        })
        .collect()
}

/// The kinds of declarations translated by derived keys.
pub mod kind {
    pub const COMMAND: &str = "command";
    pub const CATEGORY: &str = "category";
    pub const PANEL: &str = "panel";
    pub const MENU: &str = "menu";
    pub const SETTING: &str = "setting";
    pub const PLUGIN: &str = "plugin";
}

/// The key of a declaration's text: `<kind>-<slug(id)>`, then
/// `.<attr>` when given.
pub fn declared_key(kind: &str, id: &str, attr: Option<&str>) -> String {
    match attr {
        Some(a) => format!("{kind}-{}.{a}", slug(id)),
        None => format!("{kind}-{}", slug(id)),
    }
}

/// A declaration's text in the current language, else its English.
pub fn declared(ctx: &Context, kind: &str, id: &str, attr: Option<&str>, english: &str) -> String {
    lookup(ctx, &declared_key(kind, id, attr), english)
}

/// A command derived from a panel (`<panel>.focus`, `<panel>.toggle`):
/// its title and description are shared texts with a `{ $panel }`
/// placeholder, filled with the panel's title in the interface's
/// language, so a catalogue translates them once for every panel. A
/// translation under the command's own keys still wins.
#[derive(Clone, Debug, PartialEq)]
pub struct PanelCommand {
    pub title: StrKey,
    pub description: StrKey,
    /// The panel's id.
    pub panel: String,
    /// The panel's English title.
    pub panel_title: String,
}

/// The commands derived from panels, by command id.
pub type PanelCommands = Arc<BTreeMap<String, PanelCommand>>;

fn panel_commands_id() -> Id {
    Id::new("egui_pop_core::strings::panel_commands")
}

/// Installs the commands derived from panels for a context (see
/// [`PanelCommand`]); the application does it for each context it is
/// drawn in.
pub fn set_panel_commands(ctx: &Context, commands: PanelCommands) {
    ctx.data_mut(|d| d.insert_temp(panel_commands_id(), commands));
}

/// The commands derived from panels installed for a context.
pub fn panel_commands(ctx: &Context) -> Option<PanelCommands> {
    ctx.data(|d| d.get_temp::<PanelCommands>(panel_commands_id()))
}

/// The panel a command is derived from, with its shared texts.
pub fn panel_command(ctx: &Context, command: &str) -> Option<PanelCommand> {
    panel_commands(ctx).and_then(|m| m.get(command).cloned())
}

/// A panel command's text: its own key when translated, else the shared
/// text, `{ $panel }` filled.
fn panel_command_text(
    ctx: &Context,
    id: &str,
    attr: &str,
    shared: &StrKey,
    p: &PanelCommand,
) -> String {
    let panel = panel_title(ctx, &p.panel, &p.panel_title);
    let args = [("panel", panel.as_str())];
    provider(ctx)
        .get_args(&declared_key(kind::COMMAND, id, Some(attr)), &args)
        .unwrap_or_else(|| tr_args(ctx, shared, &args))
}

/// A command's title.
pub fn command_title(ctx: &Context, id: &str, english: &str) -> String {
    if let Some(p) = panel_command(ctx, id) {
        return panel_command_text(ctx, id, "title", &p.title, &p);
    }
    declared(ctx, kind::COMMAND, id, Some("title"), english)
}

/// A command's description (empty stays empty).
pub fn command_description(ctx: &Context, id: &str, english: &str) -> String {
    if english.is_empty() {
        return String::new();
    }
    if let Some(p) = panel_command(ctx, id) {
        return panel_command_text(ctx, id, "description", &p.description, &p);
    }
    declared(ctx, kind::COMMAND, id, Some("description"), english)
}

/// The key of a command category, derived from its English (categories
/// have no id).
pub fn category_key(english: &str) -> String {
    declared_key(kind::CATEGORY, &english.to_lowercase(), None)
}

/// A command category.
pub fn category(ctx: &Context, english: &str) -> String {
    lookup(ctx, &category_key(english), english)
}

/// A panel's title.
pub fn panel_title(ctx: &Context, id: &str, english: &str) -> String {
    declared(ctx, kind::PANEL, id, Some("title"), english)
}

/// A submenu's label.
pub fn menu_label(ctx: &Context, id: &str, english: &str) -> String {
    declared(ctx, kind::MENU, id, Some("label"), english)
}

/// A setting's label.
pub fn setting_label(ctx: &Context, id: &str, english: &str) -> String {
    declared(ctx, kind::SETTING, id, Some("label"), english)
}

/// A setting's description (empty stays empty).
pub fn setting_description(ctx: &Context, id: &str, english: &str) -> String {
    if english.is_empty() {
        return String::new();
    }
    declared(ctx, kind::SETTING, id, Some("description"), english)
}

/// The key of one value of a choice setting.
pub fn setting_choice_key(id: &str, value: &str) -> String {
    format!(
        "{}-choice-{}",
        declared_key(kind::SETTING, id, None),
        slug(value)
    )
}

/// One value of a choice setting, as shown (the value itself without a
/// translation).
pub fn setting_choice(ctx: &Context, id: &str, value: &str) -> String {
    lookup(ctx, &setting_choice_key(id, value), value)
}

/// A plugin's title (the settings form's section).
pub fn plugin_title(ctx: &Context, id: &str, english: &str) -> String {
    declared(ctx, kind::PLUGIN, id, Some("title"), english)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn derived_keys_are_fluent_ids() {
        assert_eq!(
            declared_key(kind::COMMAND, "app.quit", Some("title")),
            "command-app-quit.title"
        );
        assert_eq!(
            setting_choice_key("appearance.focus", "hover"),
            "setting-appearance-focus-choice-hover"
        );
        assert_eq!(category_key("View"), "category-view");
        assert_eq!(slug("a b/é_c"), "a-b---c");
    }

    #[test]
    fn every_key_is_a_unique_fluent_id() {
        let mut seen = std::collections::BTreeSet::new();
        for k in ALL {
            assert!(seen.insert(k.key), "`{}` twice", k.key);
            assert!(k.key.starts_with("egui-pop-"), "{}", k.key);
            assert!(
                k.key
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-'),
                "{}",
                k.key
            );
        }
    }

    #[test]
    fn a_missing_text_shows_english_then_the_key() {
        let ctx = Context::default();
        assert_eq!(lookup(&ctx, "nowhere", ""), "nowhere");
        assert_eq!(lookup(&ctx, "nowhere", "Here"), "Here");
        assert_eq!(
            lookup_args(&ctx, "k", "Hi { $who }, {$who}", &[("who", "Ada")]),
            "Hi Ada, Ada"
        );
        assert_eq!(category(&ctx, "View"), "View");
        assert_eq!(command_description(&ctx, "x", ""), "");
    }
}
