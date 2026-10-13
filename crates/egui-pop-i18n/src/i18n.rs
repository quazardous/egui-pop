//! [`I18n`]: the catalogues an application shows, shared between the
//! plugin, the interface's strings and the lookup helpers.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::Path;
use std::sync::{Arc, Mutex, PoisonError, RwLock};

use fluent_bundle::{FluentArgs, FluentValue};
use serde_json::{Map, Value as Json, json};

use crate::catalog::Catalog;
use crate::locale::{self, ENGLISH};

/// One catalogue file built into the application: its locale, a name
/// for messages, and its Fluent source (usually `include_str!`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Catalogue {
    pub locale: &'static str,
    pub name: &'static str,
    pub source: &'static str,
    /// A base catalogue (egui-pop's own): one added after it may override
    /// its messages, with a warning, instead of being refused.
    pub base: bool,
}

impl Catalogue {
    pub const fn new(locale: &'static str, name: &'static str, source: &'static str) -> Self {
        Self {
            locale,
            name,
            source,
            base: false,
        }
    }

    /// As a base catalogue (see [`Catalogue::base`]).
    pub const fn as_base(mut self) -> Self {
        self.base = true;
        self
    }
}

/// egui-pop's own catalogues: the interface's strings and what its
/// plugins declare, in English and French.
pub const EGUI_POP: &[Catalogue] = &[
    Catalogue::new(
        "en",
        "egui-pop.ftl",
        include_str!("../locales/en/egui-pop.ftl"),
    )
    .as_base(),
    Catalogue::new(
        "fr",
        "egui-pop.ftl",
        include_str!("../locales/fr/egui-pop.ftl"),
    )
    .as_base(),
];

/// A lookup waiting for the `i18n.resolve` step, or its answer.
#[derive(Debug, Default)]
pub(crate) struct Resolve {
    /// Some plugin hooks the step: lookups go through it.
    pub hooked: bool,
    /// What the step answered, by lookup.
    pub answers: HashMap<String, Option<String>>,
    /// Lookups asked since the last frame, by their arguments.
    pub pending: BTreeMap<String, Json>,
}

#[derive(Debug)]
struct Inner {
    catalog: RwLock<Catalog>,
    /// Keys looked up and not found in the locale's own language.
    missing: Mutex<BTreeMap<String, BTreeSet<String>>>,
    resolve: Mutex<Resolve>,
    /// Bumped whenever the text shown may change (locale, catalogues).
    generation: Mutex<u64>,
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

/// The application's catalogues and its locale. Cheap to clone: clones
/// share them.
///
/// ```
/// use egui_pop_i18n::{I18n, args};
///
/// let i18n = I18n::new("fr");
/// i18n.add("fr", "files = { $count ->\n    [one] { $count } fichier\n   *[other] { $count } fichiers\n}\n").unwrap();
/// i18n.add("en", "files = { $count ->\n    [one] { $count } file\n   *[other] { $count } files\n}\n").unwrap();
/// assert_eq!(i18n.t("files", &args![count = 1]), "1 fichier");
/// assert_eq!(i18n.t("files", &args![count = 1500]), "1\u{202f}500 fichiers");
/// i18n.set_locale("en");
/// assert_eq!(i18n.t("files", &args![count = 0]), "0 files");
/// // Missing everywhere: the key itself.
/// assert_eq!(i18n.t("nowhere", &[]), "nowhere");
/// ```
#[derive(Clone, Debug)]
pub struct I18n {
    inner: Arc<Inner>,
}

impl Default for I18n {
    fn default() -> Self {
        Self::new(ENGLISH)
    }
}

/// Arguments of a lookup, by name.
pub type Args<'a> = [(&'a str, FluentValue<'a>)];

/// Builds lookup arguments: `args![count = 3, name = "Ada"]`.
#[macro_export]
macro_rules! args {
    ($($name:ident = $value:expr),* $(,)?) => {
        [$((stringify!($name), $crate::FluentValue::from($value))),*]
    };
}

fn fluent_args<'a>(args: &'a Args<'a>) -> Option<FluentArgs<'a>> {
    if args.is_empty() {
        return None;
    }
    let mut out = FluentArgs::new();
    for (k, v) in args {
        out.set(*k, v.clone());
    }
    Some(out)
}

/// Arguments as JSON, for the `i18n.resolve` step.
fn args_json(args: &Args<'_>) -> Json {
    let mut m = Map::new();
    for (k, v) in args {
        let v = match v {
            FluentValue::Number(n) => json!(n.value),
            FluentValue::String(s) => json!(s),
            _ => Json::Null,
        };
        m.insert((*k).to_string(), v);
    }
    Json::Object(m)
}

/// JSON arguments back to Fluent's.
pub(crate) fn json_args(args: &Json) -> Vec<(String, FluentValue<'static>)> {
    args.as_object()
        .map(|m| {
            m.iter()
                .map(|(k, v)| {
                    let v = match v {
                        Json::Number(n) => {
                            crate::format::fluent_number(n.as_f64().unwrap_or_default())
                        }
                        Json::String(s) => FluentValue::from(s.clone()),
                        other => FluentValue::from(other.to_string()),
                    };
                    (k.clone(), v)
                })
                .collect()
        })
        .unwrap_or_default()
}

impl I18n {
    /// No catalogue, in `locale`.
    pub fn new(locale: &str) -> Self {
        Self {
            inner: Arc::new(Inner {
                catalog: RwLock::new(Catalog::new(locale)),
                missing: Mutex::new(BTreeMap::new()),
                resolve: Mutex::new(Resolve::default()),
                generation: Mutex::new(0),
            }),
        }
    }

    /// egui-pop's own catalogues ([`EGUI_POP`]), in `locale`.
    pub fn with_egui_pop(locale: &str) -> Self {
        let i = Self::new(locale);
        i.add_catalogues(EGUI_POP)
            .expect("egui-pop's own catalogues load");
        i
    }

    fn read<R>(&self, f: impl FnOnce(&Catalog) -> R) -> R {
        f(&self
            .inner
            .catalog
            .read()
            .unwrap_or_else(PoisonError::into_inner))
    }

    fn write<R>(&self, f: impl FnOnce(&mut Catalog) -> R) -> R {
        let r = f(&mut self
            .inner
            .catalog
            .write()
            .unwrap_or_else(PoisonError::into_inner));
        *lock(&self.inner.generation) += 1;
        lock(&self.inner.resolve).answers.clear();
        r
    }

    /// Adds Fluent source to a locale. Refused, with nothing added, when
    /// it does not parse or defines a message already there.
    pub fn add(&self, locale: &str, source: &str) -> Result<(), String> {
        self.write(|c| c.add(locale, source))
    }

    /// Adds a plugin's Fluent source: every message must belong to
    /// `owner` (see [`crate::catalog::owned_by`]).
    pub fn add_owned(&self, owner: &str, locale: &str, source: &str) -> Result<(), String> {
        self.write(|c| c.add_owned(owner, locale, source))
    }

    /// Adds catalogues built into the application.
    pub fn add_catalogues(&self, catalogues: &[Catalogue]) -> Result<(), String> {
        for c in catalogues {
            self.write(|cat| {
                if c.base {
                    cat.add_base(c.locale, c.source)
                } else {
                    cat.add(c.locale, c.source)
                }
            })
            .map_err(|e| format!("{}/{}: {e}", c.locale, c.name))?;
        }
        Ok(())
    }

    /// egui-pop's messages an application's catalogue overrode: (locale,
    /// id). The plugin publishes each as an `i18n.overridden` warning.
    pub fn overridden(&self) -> Vec<(String, String)> {
        self.read(|c| c.overridden().to_vec())
    }

    /// Reads `dir/<locale>/*.ftl` over what is loaded: their messages
    /// replace those of the same id (translators try their work without
    /// rebuilding). Returns how many files were read; a missing
    /// directory reads none.
    pub fn add_dir(&self, dir: &Path) -> Result<usize, String> {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return Ok(0);
        };
        let mut locales: Vec<_> = entries
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| p.is_dir())
            .collect();
        locales.sort();
        let mut n = 0;
        for ldir in locales {
            let tag = ldir
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or_default()
                .to_string();
            let mut files: Vec<_> = std::fs::read_dir(&ldir)
                .map_err(|e| format!("{}: {e}", ldir.display()))?
                .filter_map(|e| e.ok().map(|e| e.path()))
                .filter(|p| p.extension().is_some_and(|e| e == "ftl"))
                .collect();
            files.sort();
            for f in files {
                let source =
                    std::fs::read_to_string(&f).map_err(|e| format!("{}: {e}", f.display()))?;
                self.write(|c| c.add_overriding(&tag, &source))
                    .map_err(|e| format!("{}: {e}", f.display()))?;
                n += 1;
            }
        }
        Ok(n)
    }

    /// Switches the locale (`fr`, `fr_FR.UTF-8`, `x-pseudo`); a tag
    /// naming no language selects English.
    pub fn set_locale(&self, tag: &str) {
        self.write(|c| c.set_locale(tag));
    }

    /// The current locale.
    pub fn locale(&self) -> String {
        self.read(|c| c.locale().to_string())
    }

    /// The locales a lookup tries now, in order.
    pub fn chain(&self) -> Vec<String> {
        self.read(|c| c.chain().to_vec())
    }

    /// The locales that have a catalogue.
    pub fn locales(&self) -> Vec<String> {
        self.read(|c| c.locales().map(String::from).collect())
    }

    /// Every key a locale's catalogue defines.
    pub fn keys(&self, locale: &str) -> BTreeSet<String> {
        self.read(|c| c.keys(locale))
    }

    /// Changes whenever the text shown may change: a locale switched, a
    /// catalogue added. Caches of translated text compare it.
    pub fn generation(&self) -> u64 {
        *lock(&self.inner.generation)
    }

    /// The text of `key` in the current locale, along its chain and
    /// English; `None` when no catalogue has it. Goes through the
    /// `i18n.resolve` step when a plugin hooks it, and counts a key the
    /// locale's own language lacks as missing.
    pub fn get(&self, key: &str, args: &Args<'_>) -> Option<String> {
        let fargs = fluent_args(args);
        let (locale, text, translated) = self.read(|c| {
            (
                c.locale().to_string(),
                c.get(key, fargs.as_ref()),
                c.translated(key),
            )
        });
        if !translated {
            lock(&self.inner.missing)
                .entry(locale.clone())
                .or_default()
                .insert(key.to_string());
        }
        let mut r = lock(&self.inner.resolve);
        if !r.hooked {
            return text;
        }
        let asked = json!({ "key": key, "locale": locale, "args": args_json(args) });
        let id = asked.to_string();
        if let Some(answer) = r.answers.get(&id) {
            return answer.clone();
        }
        r.pending.insert(id, asked);
        text
    }

    /// The text of `key`, else `english` (a Fluent pattern when there
    /// are arguments), else the key itself.
    pub fn text(&self, key: &str, english: &str, args: &Args<'_>) -> String {
        match self.get(key, args) {
            Some(t) => t,
            None => {
                let fargs = fluent_args(args);
                self.read(|c| c.text(key, english, fargs.as_ref()))
            }
        }
    }

    /// The text of `key`, else the key itself.
    pub fn t(&self, key: &str, args: &Args<'_>) -> String {
        self.text(key, "", args)
    }

    /// The text of `key` in one locale, without the step: what the
    /// `i18n.resolve` step does by default.
    pub fn get_in(&self, locale: &str, key: &str, args: &Args<'_>) -> Option<String> {
        let fargs = fluent_args(args);
        self.read(|c| c.get_in(locale, key, fargs.as_ref()))
    }

    /// Keys looked up and missing in each locale's own language (they
    /// showed English or the key), since the last [`I18n::clear_missing`].
    pub fn missing(&self) -> BTreeMap<String, BTreeSet<String>> {
        lock(&self.inner.missing).clone()
    }

    /// Forgets the missing keys seen so far.
    pub fn clear_missing(&self) {
        lock(&self.inner.missing).clear();
    }

    pub(crate) fn resolve(&self) -> std::sync::MutexGuard<'_, Resolve> {
        lock(&self.inner.resolve)
    }

    /// Whether lookups go through the `i18n.resolve` step.
    pub(crate) fn set_hooked(&self, hooked: bool) {
        let mut r = self.resolve();
        if r.hooked != hooked {
            r.hooked = hooked;
            r.answers.clear();
            r.pending.clear();
            drop(r);
            *lock(&self.inner.generation) += 1;
        }
    }

    /// Answers from the step, then the text shown changes.
    pub(crate) fn answer(&self, answers: Vec<(String, Option<String>)>) {
        if answers.is_empty() {
            return;
        }
        self.resolve().answers.extend(answers);
        *lock(&self.inner.generation) += 1;
    }

    /// The locales that have a catalogue (English always), each with
    /// its own name: what the language setting offers.
    pub fn offered(&self) -> Vec<(String, String)> {
        let mut out: Vec<(String, String)> = self
            .locales()
            .into_iter()
            .map(|l| {
                let name = locale::autonym(&l);
                (l, name)
            })
            .collect();
        if !out.iter().any(|(l, _)| l == ENGLISH) {
            out.insert(0, (ENGLISH.into(), locale::autonym(ENGLISH)));
        }
        out
    }
}

/// The interface's strings (`egui_pop_core::strings`) read from an
/// [`I18n`], in one locale: installed in the egui context by the plugin,
/// again whenever the locale changes.
#[derive(Debug)]
pub struct Provider {
    i18n: I18n,
    language: String,
}

impl Provider {
    pub fn new(i18n: I18n) -> Self {
        let language = i18n.locale();
        Self { i18n, language }
    }
}

impl egui_pop_core::strings::Strings for Provider {
    fn get(&self, key: &str) -> Option<String> {
        self.i18n.get(key, &[])
    }

    fn get_args(&self, key: &str, args: &[(&str, &str)]) -> Option<String> {
        let args: Vec<(&str, FluentValue<'_>)> = args
            .iter()
            .map(|(k, v)| {
                let v = match v.parse::<f64>() {
                    Ok(n) => crate::format::fluent_number(n),
                    Err(_) => FluentValue::from(*v),
                };
                (*k, v)
            })
            .collect();
        self.i18n.get(key, &args)
    }

    fn language(&self) -> &str {
        &self.language
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FR: &str = r"
panel-graph =
    .title = Graphe
images-selected =
    { $count ->
        [one] { $count } image sélectionnée
       *[other] { $count } images sélectionnées
    }
";

    fn i18n() -> I18n {
        let i = I18n::new("fr-CA");
        i.add("fr", FR).unwrap();
        i.add("fr-CA", "panel-graph =\n    .title = Graphe (CA)\n")
            .unwrap();
        i.add("en", "panel-graph =\n    .title = Graph\nonly-en = Only\n")
            .unwrap();
        i
    }

    #[test]
    fn the_chain_falls_back_to_the_parent_then_english_then_the_key() {
        let i = i18n();
        assert_eq!(i.t("panel-graph.title", &[]), "Graphe (CA)");
        i.set_locale("fr");
        assert_eq!(i.t("panel-graph.title", &[]), "Graphe");
        assert_eq!(i.t("only-en", &[]), "Only");
        assert_eq!(i.text("nowhere", "Code English", &[]), "Code English");
        assert_eq!(i.t("nowhere", &[]), "nowhere");
        i.set_locale("de");
        assert_eq!(i.t("panel-graph.title", &[]), "Graph");
        let missing = i.missing();
        assert!(missing["fr"].contains("only-en"), "{missing:?}");
        assert!(missing["fr"].contains("nowhere"));
        assert!(!missing["fr"].contains("panel-graph.title"));
        assert!(missing["de"].contains("panel-graph.title"));
    }

    #[test]
    fn plurals_follow_the_locale() {
        let i = i18n();
        i.set_locale("fr");
        let english = "{ $count ->\n    [one] { $count } image selected\n   *[other] { $count } images selected\n}";
        assert_eq!(
            i.text("images-selected", english, &args![count = 0]),
            "0 image sélectionnée"
        );
        assert_eq!(
            i.text("images-selected", english, &args![count = 2]),
            "2 images sélectionnées"
        );
        i.set_locale("en");
        assert_eq!(
            i.text("images-selected", english, &args![count = 1]),
            "1 image selected"
        );
        assert_eq!(
            i.text("images-selected", english, &args![count = 0]),
            "0 images selected"
        );
    }

    #[test]
    fn an_applications_catalogue_overrides_egui_pops_and_says_so() {
        static APP: &[Catalogue] = &[Catalogue::new(
            "fr",
            "app.ftl",
            "egui-pop-cancel = Laisser tomber\napp-hello = Salut\n",
        )];
        let i = I18n::with_egui_pop("fr");
        assert!(i.overridden().is_empty());
        i.add_catalogues(APP).unwrap();
        assert_eq!(i.t("egui-pop-cancel", &[]), "Laisser tomber");
        assert_eq!(i.t("app-hello", &[]), "Salut");
        assert_eq!(
            i.overridden(),
            vec![("fr".to_string(), "egui-pop-cancel".to_string())]
        );
        // The application's own keys, twice: still refused.
        let e = i.add("fr", "app-hello = Encore\n").unwrap_err();
        assert!(e.contains("already defined"), "{e}");
        let e = i.add("fr", "egui-pop-cancel = Encore\n").unwrap_err();
        assert!(e.contains("already defined"), "{e}");
    }

    #[test]
    fn a_plugin_adds_only_its_own_messages() {
        let i = i18n();
        i.add_owned("acme", "fr", "command-acme-run =\n    .title = Lancer\n")
            .unwrap();
        let e = i
            .add_owned("acme", "fr", "panel-graph =\n    .title = Pirate\n")
            .unwrap_err();
        assert!(e.contains("does not belong"), "{e}");
        let e = i
            .add("fr", "panel-graph =\n    .title = Encore\n")
            .unwrap_err();
        assert!(e.contains("already defined"), "{e}");
        assert!(i.add("fr", "broken = {").is_err());
    }

    #[test]
    fn an_override_directory_replaces_messages() {
        let dir = std::env::temp_dir().join(format!("egui-pop-i18n-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("fr")).unwrap();
        std::fs::write(
            dir.join("fr/x.ftl"),
            "panel-graph =\n    .title = Graphique\n",
        )
        .unwrap();
        let i = i18n();
        i.set_locale("fr");
        assert_eq!(i.add_dir(&dir).unwrap(), 1);
        assert_eq!(i.t("panel-graph.title", &[]), "Graphique");
        assert_eq!(i.add_dir(&dir.join("none")).unwrap(), 0);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn the_pseudo_locale_accents_and_lengthens_english() {
        let i = i18n();
        i.set_locale("x-pseudo");
        let english = "Show all commands";
        let pseudo = i.text("command-x.title", english, &[]);
        assert_ne!(pseudo, english);
        assert!(!pseudo.is_ascii());
        assert!(pseudo.chars().count() > english.chars().count());
        assert!(!i.t("panel-graph.title", &[]).contains("Graphe"));
    }

    #[test]
    fn egui_pops_catalogues_load() {
        let i = I18n::with_egui_pop("fr");
        assert!(i.locales().contains(&"fr".to_string()));
        assert_eq!(i.t("egui-pop-retry", &[]), "Réessayer");
    }

    #[test]
    fn the_provider_formats_numbers_given_as_text() {
        use egui_pop_core::strings::Strings as _;
        let i = I18n::new("fr");
        i.add("fr", "n = { $n } éléments\n").unwrap();
        let p = Provider::new(i);
        assert_eq!(
            p.get_args("n", &[("n", "1234")]).unwrap(),
            "1\u{202f}234 éléments"
        );
        assert_eq!(p.language(), "fr");
    }
}
