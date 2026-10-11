//! The strings egui-pop itself shows (Cancel, Retry, accessibility
//! descriptions), by key with English fallbacks. An application installs
//! a [`Strings`] provider (a translation plugin, for instance); without
//! one, the fallbacks are used.
//!
//! ```
//! use std::sync::Arc;
//! use egui_pop_core::strings::{self, MapStrings, RETRY};
//!
//! let ctx = egui::Context::default();
//! assert_eq!(strings::tr(&ctx, &RETRY), "Retry");
//! strings::install(&ctx, Arc::new(MapStrings::new("fr").with(RETRY.key, "Réessayer")));
//! assert_eq!(strings::tr(&ctx, &RETRY), "Réessayer");
//! ```

use std::collections::BTreeMap;
use std::sync::Arc;

use egui::{Context, Id};

/// A string's key and its English fallback. `{name}` placeholders are
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

pub const OK: StrKey = StrKey::new("egui-pop.ok", "OK");
pub const CANCEL: StrKey = StrKey::new("egui-pop.cancel", "Cancel");
pub const CLOSE: StrKey = StrKey::new("egui-pop.close", "Close");
pub const RESET: StrKey = StrKey::new("egui-pop.reset", "Reset");
pub const RETRY: StrKey = StrKey::new("egui-pop.retry", "Retry");
pub const FAULT_TITLE: StrKey = StrKey::new("egui-pop.fault.title", "{name} stopped working");
pub const FAULT_BODY: StrKey = StrKey::new(
    "egui-pop.fault.body",
    "The rest of the application is fine. Retry, or report the message below.",
);
pub const HELD_BY: StrKey = StrKey::new("egui-pop.value.held-by", "Being changed by {origin}");
pub const READ_ONLY: StrKey = StrKey::new("egui-pop.value.read-only", "Read only");

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
];

/// Gives the text of a key in the current language, if it has one.
pub trait Strings: Send + Sync {
    /// The text for a key; `None` falls back to English.
    fn get(&self, key: &str) -> Option<String>;

    /// The language, as a BCP 47 tag.
    fn language(&self) -> &str {
        "en"
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

/// The text of a key.
pub fn tr(ctx: &Context, key: &StrKey) -> String {
    provider(ctx)
        .get(key.key)
        .unwrap_or_else(|| key.fallback.to_string())
}

/// The text of a key with its `{name}` placeholders filled.
pub fn tr_args(ctx: &Context, key: &StrKey, args: &[(&str, &str)]) -> String {
    let mut text = tr(ctx, key);
    for (name, value) in args {
        text = text.replace(&format!("{{{name}}}"), value);
    }
    text
}
