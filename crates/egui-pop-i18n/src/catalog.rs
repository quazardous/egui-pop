//! Looking text up: one Fluent bundle per locale and a fallback chain
//! ending in English.

use std::collections::{BTreeMap, BTreeSet};

use fluent_bundle::concurrent::FluentBundle;
use fluent_bundle::{FluentArgs, FluentResource};
use fluent_syntax::ast::Entry;
use unic_langid::LanguageIdentifier;

use crate::locale::{self, ENGLISH, PSEUDO};

type Bundle = FluentBundle<FluentResource>;

/// A key split into its message and attribute: `id` or `id.attr`.
pub fn split_key(key: &str) -> (&str, Option<&str>) {
    match key.split_once('.') {
        Some((id, attr)) => (id, Some(attr)),
        None => (key, None),
    }
}

/// The areas whose keys carry an owner's id right after them
/// (`command-<owner>-…`).
const AREAS: [&str; 6] = ["command", "panel", "menu", "setting", "plugin", "param"];

/// Whether a message belongs to `owner` (a plugin, by its stable id):
/// `<owner>-…` for its free messages, or `<area>-<owner>-…` for its
/// commands, panels, menus, settings and parameters.
///
/// ```
/// use egui_pop_i18n::catalog::owned_by;
///
/// assert!(owned_by("acme", "acme-greeting"));
/// assert!(owned_by("acme.denoise", "command-acme-denoise-run"));
/// assert!(!owned_by("acme", "acmex-greeting"));
/// assert!(!owned_by("acme", "panel-helios-acme"));
/// ```
pub fn owned_by(owner: &str, message: &str) -> bool {
    let owner = egui_pop_core::strings::slug(owner);
    let prefixed = |rest: &str| {
        rest.strip_prefix(owner.as_str())
            .is_some_and(|r| r.starts_with('-'))
    };
    prefixed(message)
        || AREAS.iter().any(|area| {
            message
                .strip_prefix(area)
                .and_then(|r| r.strip_prefix('-'))
                .is_some_and(prefixed)
        })
}

/// Catalogues per locale and the locale in use.
///
/// A lookup names a key (`id` or `id.attr`) and tries the current
/// locale, then its parents (`fr-CA`, then `fr`), then English; what
/// English lacks too is `None`, and the caller shows its own English
/// text or the key. `x-pseudo` reads English, accented and lengthened.
pub struct Catalog {
    bundles: BTreeMap<String, Bundle>,
    /// The message ids each locale defines.
    ids: BTreeMap<String, BTreeSet<String>>,
    /// The ids of each locale an application's catalogue may override
    /// (egui-pop's own, see [`Catalog::add_base`]).
    base: BTreeMap<String, BTreeSet<String>>,
    /// What an application's catalogue overrode: (locale, id).
    overridden: Vec<(String, String)>,
    locale: String,
    chain: Vec<String>,
}

impl std::fmt::Debug for Catalog {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Catalog")
            .field("locales", &self.bundles.keys().collect::<Vec<_>>())
            .field("locale", &self.locale)
            .finish()
    }
}

impl Catalog {
    /// No catalogue, in `locale`.
    pub fn new(locale: &str) -> Self {
        let mut c = Self {
            bundles: BTreeMap::new(),
            ids: BTreeMap::new(),
            base: BTreeMap::new(),
            overridden: Vec::new(),
            locale: String::new(),
            chain: Vec::new(),
        };
        c.set_locale(locale);
        c
    }

    /// Switches the locale; the next lookups use it. A tag naming no
    /// language selects English.
    pub fn set_locale(&mut self, tag: &str) {
        self.locale = locale::normalize(tag).unwrap_or_else(|| ENGLISH.into());
        self.rechain();
    }

    fn rechain(&mut self) {
        let available: Vec<&str> = self.bundles.keys().map(String::as_str).collect();
        self.chain = locale::chain(&self.locale, &available);
    }

    /// The current locale's tag.
    pub fn locale(&self) -> &str {
        &self.locale
    }

    /// The locales a lookup tries, in order.
    pub fn chain(&self) -> &[String] {
        &self.chain
    }

    /// The locales that have a catalogue.
    pub fn locales(&self) -> impl Iterator<Item = &str> {
        self.bundles.keys().map(String::as_str)
    }

    /// Adds Fluent source to a locale's catalogue. Refused, with nothing
    /// added, when it does not parse or defines a message already there,
    /// unless that message came from a base catalogue
    /// ([`Catalog::add_base`]): the new text overrides it, and
    /// [`Catalog::overridden`] says so.
    pub fn add(&mut self, tag: &str, source: &str) -> Result<(), String> {
        self.add_with(tag, source, None, false)
    }

    /// Adds a base catalogue (egui-pop's own): an application's catalogue
    /// added after it may override its messages, with a warning.
    pub fn add_base(&mut self, tag: &str, source: &str) -> Result<(), String> {
        self.add_with(tag, source, None, false)?;
        let tag = locale::normalize(tag).unwrap_or_default();
        let resource = FluentResource::try_new(source.to_string()).map_err(|_| "parsed above")?;
        let ids = resource.entries().filter_map(|e| match e {
            Entry::Message(m) => Some(m.id.name.to_string()),
            _ => None,
        });
        self.base.entry(tag).or_default().extend(ids);
        Ok(())
    }

    /// The base messages an application's catalogue overrode: (locale,
    /// id), in order.
    pub fn overridden(&self) -> &[(String, String)] {
        &self.overridden
    }

    /// Adds Fluent source whose messages replace those already there (an
    /// override directory over the embedded catalogues).
    pub fn add_overriding(&mut self, tag: &str, source: &str) -> Result<(), String> {
        self.add_with(tag, source, None, true)
    }

    /// Adds a plugin's Fluent source: every message must belong to
    /// `owner` (see [`owned_by`]); a plugin translates its own commands,
    /// panels and settings, never another's.
    pub fn add_owned(&mut self, owner: &str, tag: &str, source: &str) -> Result<(), String> {
        self.add_with(tag, source, Some(owner), false)
    }

    fn add_with(
        &mut self,
        tag: &str,
        source: &str,
        owner: Option<&str>,
        overriding: bool,
    ) -> Result<(), String> {
        let tag = locale::normalize(tag)
            .filter(|t| t != PSEUDO)
            .ok_or_else(|| format!("`{tag}` names no language"))?;
        let resource = FluentResource::try_new(source.to_string()).map_err(|(_, errors)| {
            let errors: Vec<String> = errors.iter().map(ToString::to_string).collect();
            format!("does not parse: {}", errors.join("; "))
        })?;
        let fresh = !self.bundles.contains_key(&tag);
        let bundle = self
            .bundles
            .entry(tag.clone())
            .or_insert_with(|| new_bundle(&tag));
        let base = self.base.get(&tag);
        let mut overrides = Vec::new();
        for entry in resource.entries() {
            if let Entry::Message(m) = entry {
                let id = m.id.name;
                if let Some(owner) = owner.filter(|o| !owned_by(o, id)) {
                    return Err(format!("`{id}` does not belong to `{owner}`"));
                }
                if !overriding && bundle.has_message(id) {
                    if base.is_some_and(|b| b.contains(id)) {
                        overrides.push(id.to_string());
                    } else {
                        return Err(format!("`{id}` is already defined for {tag}"));
                    }
                }
            }
        }
        let overriding = overriding || !overrides.is_empty();
        if let Some(b) = self.base.get_mut(&tag) {
            for id in &overrides {
                b.remove(id);
            }
        }
        self.overridden
            .extend(overrides.into_iter().map(|id| (tag.clone(), id)));
        let ids: Vec<String> = resource
            .entries()
            .filter_map(|e| match e {
                Entry::Message(m) => Some(m.id.name.to_string()),
                _ => None,
            })
            .collect();
        if overriding {
            bundle.add_resource_overriding(resource);
        } else {
            bundle.add_resource(resource).map_err(|errors| {
                let errors: Vec<String> = errors.iter().map(ToString::to_string).collect();
                errors.join("; ")
            })?;
        }
        self.ids.entry(tag).or_default().extend(ids);
        if fresh {
            self.rechain();
        }
        Ok(())
    }

    /// The text of `key` along the chain, with the locale that had it.
    pub fn find(&self, key: &str, args: Option<&FluentArgs>) -> Option<(String, String)> {
        self.chain.iter().find_map(|tag| {
            let b = self.bundles.get(tag)?;
            format(b, key, args).map(|t| (tag.clone(), self.pseudo(t)))
        })
    }

    /// The text of `key` along the chain, English included.
    pub fn get(&self, key: &str, args: Option<&FluentArgs>) -> Option<String> {
        self.find(key, args).map(|(_, t)| t)
    }

    /// The text of `key` in one locale's chain (not the current one).
    pub fn get_in(&self, locale: &str, key: &str, args: Option<&FluentArgs>) -> Option<String> {
        let tag = locale::normalize(locale).unwrap_or_else(|| ENGLISH.into());
        let available: Vec<&str> = self.locales().collect();
        let text = locale::chain(&tag, &available)
            .iter()
            .filter_map(|t| self.bundles.get(t))
            .find_map(|b| format(b, key, args))?;
        Some(if tag == PSEUDO { pseudo(&text) } else { text })
    }

    /// Whether the current locale's own language has `key`: English's
    /// text, shown for a missing translation, does not count (unless the
    /// locale is English).
    pub fn translated(&self, key: &str) -> bool {
        let english = locale::language(&self.locale) == ENGLISH;
        self.chain
            .iter()
            .filter(|t| english || t.as_str() != ENGLISH)
            .filter_map(|t| self.bundles.get(t))
            .any(|b| format(b, key, None).is_some())
    }

    /// The text of `key`, else `english` (read as a Fluent pattern when
    /// there are arguments: `{ $count -> [one] … *[other] … }`), else the
    /// key itself.
    pub fn text(&self, key: &str, english: &str, args: Option<&FluentArgs>) -> String {
        if let Some(t) = self.get(key, args) {
            return t;
        }
        let text = if english.is_empty() {
            key.to_string()
        } else {
            english_text(english, args)
        };
        self.pseudo(text)
    }

    fn pseudo(&self, text: String) -> String {
        if self.locale == PSEUDO {
            pseudo(&text)
        } else {
            text
        }
    }

    /// Every key a locale's catalogue defines (`id`, `id.attr`).
    pub fn keys(&self, locale: &str) -> BTreeSet<String> {
        let mut out = BTreeSet::new();
        let Some(b) = self.bundles.get(locale) else {
            return out;
        };
        for id in self.ids.get(locale).into_iter().flatten() {
            if let Some(m) = b.get_message(id) {
                if m.value().is_some() {
                    out.insert(id.clone());
                }
                for a in m.attributes() {
                    out.insert(format!("{id}.{}", a.id()));
                }
            }
        }
        out
    }
}

fn new_bundle(tag: &str) -> Bundle {
    let id: LanguageIdentifier = tag.parse().unwrap_or_default();
    let mut b = FluentBundle::new_concurrent(vec![id]);
    b.set_use_isolating(false);
    b.set_formatter(Some(crate::format::fluent_formatter));
    // NUMBER is Fluent's own; the formatter writes it for the locale.
    let _ = b.add_builtins();
    let _ = b.add_function(
        "DATETIME",
        crate::format::datetime_function(tag.to_string()),
    );
    b
}

/// `key` formatted by one bundle, if it has it.
fn format(bundle: &Bundle, key: &str, args: Option<&FluentArgs>) -> Option<String> {
    let (id, attr) = split_key(key);
    let message = bundle.get_message(id)?;
    let pattern = match attr {
        Some(a) => message.get_attribute(a)?.value(),
        None => message.value()?,
    };
    let mut errors = Vec::new();
    Some(
        bundle
            .format_pattern(pattern, args, &mut errors)
            .into_owned(),
    )
}

fn pseudo(text: &str) -> String {
    fluent_pseudo::transform_dom(text, false, true, true).into_owned()
}

/// English written in the code, with its arguments placed.
pub fn english_text(english: &str, args: Option<&FluentArgs>) -> String {
    let Some(args) = args else {
        return english.to_string();
    };
    let source = format!("m = {}", english.replace('\n', "\n    "));
    let Ok(resource) = FluentResource::try_new(source) else {
        return english.to_string();
    };
    let mut bundle = new_bundle(ENGLISH);
    if bundle.add_resource(resource).is_err() {
        return english.to_string();
    }
    format(&bundle, "m", Some(args)).unwrap_or_else(|| english.to_string())
}
