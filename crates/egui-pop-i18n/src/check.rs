//! Checks a directory of catalogues (`<dir>/<locale>/*.ftl`): per
//! locale, coverage, missing and orphan keys, and keys whose English
//! changed since they were translated; across them, the faults that must
//! fail a build.
//!
//! A key is one translatable unit: a message's value (`id`) or one of its
//! attributes (`id.attr`).
//!
//! **Faults** (a build fails): a file that does not parse, a key defined
//! twice in a locale, no English catalogue. Coverage, missing, orphan and
//! changed keys are a report: a translation lags behind English, and that
//! is normal. (Keys used by the code but absent in English are found by
//! the extractor, [`crate::extract`].)
//!
//! **Changed English.** Each translated locale keeps a sidecar,
//! `<dir>/<locale>/english.hashes`: one line per translated key, `key
//! hash`, the hash of the English text the translation was made from. A
//! key whose English hash differs now is reported as changed; a
//! translated key with no recorded hash is reported as unstamped.
//! [`stamp`] records the hashes of newly translated keys, or of every one
//! once the changed translations were reviewed. A sidecar rather than
//! comments in the `.ftl` files, so translation tools that rewrite
//! comments cannot lose it.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::fmt::Write as _;
use std::io;
use std::path::Path;

use fluent_syntax::ast::{Entry, Identifier, Message, Pattern, Resource};

use crate::locale::ENGLISH;

/// The sidecar holding, per translated key, the hash of its English.
pub const HASHES: &str = "english.hashes";

/// One catalogue file.
#[derive(Clone, Debug)]
pub struct File {
    pub locale: String,
    pub name: String,
    pub source: String,
}

/// The catalogues of a directory: its `.ftl` files and each locale's hashes.
#[derive(Clone, Debug, Default)]
pub struct Catalogs {
    pub files: Vec<File>,
    pub hashes: BTreeMap<String, BTreeMap<String, String>>,
}

impl Catalogs {
    /// Reads `dir/<locale>/*.ftl` and `dir/<locale>/english.hashes`.
    pub fn read(dir: &Path) -> io::Result<Self> {
        let mut out = Self::default();
        let mut locales: Vec<_> = std::fs::read_dir(dir)?
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| p.is_dir())
            .collect();
        locales.sort();
        for ldir in locales {
            let locale = ldir
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or_default();
            let mut files: Vec<_> = std::fs::read_dir(&ldir)?
                .filter_map(|e| e.ok().map(|e| e.path()))
                .filter(|p| p.extension().is_some_and(|e| e == "ftl"))
                .collect();
            files.sort();
            for f in files {
                out.files.push(File {
                    locale: locale.to_string(),
                    name: f
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or_default()
                        .into(),
                    source: std::fs::read_to_string(&f)?,
                });
            }
            if let Ok(text) = std::fs::read_to_string(ldir.join(HASHES)) {
                out.hashes.insert(locale.to_string(), parse_hashes(&text));
            }
        }
        Ok(out)
    }

    fn locales(&self) -> BTreeSet<&str> {
        self.files.iter().map(|f| f.locale.as_str()).collect()
    }
}

/// What must fail a build.
#[derive(Clone, Debug, PartialEq)]
pub enum Fault {
    Unparsable { file: String, errors: Vec<String> },
    Duplicate { locale: String, key: String },
    NoEnglish,
}

impl fmt::Display for Fault {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Fault::Unparsable { file, errors } => {
                write!(f, "{file} does not parse: {}", errors.join("; "))
            }
            Fault::Duplicate { locale, key } => write!(f, "{locale}: `{key}` is defined twice"),
            Fault::NoEnglish => write!(f, "no English catalogue (en/*.ftl)"),
        }
    }
}

/// One translated locale against English.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct LocaleReport {
    pub locale: String,
    /// English's keys.
    pub english: usize,
    /// English's keys this locale translates.
    pub translated: usize,
    /// In English, not translated.
    pub missing: Vec<String>,
    /// Translated, no longer in English.
    pub orphans: Vec<String>,
    /// Translated from an English text that has changed since.
    pub changed: Vec<String>,
    /// Translated, with no English hash recorded.
    pub unstamped: Vec<String>,
}

impl LocaleReport {
    /// The share of English's keys translated, in percent.
    pub fn coverage(&self) -> f64 {
        if self.english == 0 {
            100.0
        } else {
            100.0 * self.translated as f64 / self.english as f64
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Report {
    pub english: usize,
    pub locales: Vec<LocaleReport>,
    pub faults: Vec<Fault>,
}

impl fmt::Display for Report {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "en: {} keys", self.english)?;
        for l in &self.locales {
            writeln!(
                f,
                "{}: {:.1}% ({}/{}), {} missing, {} orphan, {} changed, {} unstamped",
                l.locale,
                l.coverage(),
                l.translated,
                l.english,
                l.missing.len(),
                l.orphans.len(),
                l.changed.len(),
                l.unstamped.len()
            )?;
            for (what, keys) in [
                ("orphan", &l.orphans),
                ("changed", &l.changed),
                ("unstamped", &l.unstamped),
            ] {
                for k in keys {
                    writeln!(f, "  {what}: {k}")?;
                }
            }
        }
        for fault in &self.faults {
            writeln!(f, "fault: {fault}")?;
        }
        Ok(())
    }
}

/// The keys of a source with the canonical text of each, or why it does
/// not parse.
fn units(source: &str) -> Result<Vec<(String, String)>, Vec<String>> {
    let resource = fluent_syntax::parser::parse(source).map_err(|(_, errors)| {
        errors
            .iter()
            .map(|e| {
                let line = source[..e.pos.start.min(source.len())]
                    .matches('\n')
                    .count()
                    + 1;
                format!("line {line}: {}", e.kind)
            })
            .collect::<Vec<_>>()
    })?;
    let mut out = Vec::new();
    for entry in &resource.body {
        if let Entry::Message(m) = entry {
            if let Some(v) = &m.value {
                out.push((m.id.name.to_string(), canonical(v)));
            }
            for a in &m.attributes {
                out.push((format!("{}.{}", m.id.name, a.id.name), canonical(&a.value)));
            }
        }
    }
    Ok(out)
}

/// A pattern as Fluent's serializer writes it: the same text, whatever
/// its layout in the file.
fn canonical(pattern: &Pattern<&str>) -> String {
    let resource = Resource {
        body: vec![Entry::Message(Message {
            id: Identifier { name: "m" },
            value: Some(pattern.clone()),
            attributes: Vec::new(),
            comment: None,
        })],
    };
    fluent_syntax::serializer::serialize(&resource)
}

/// The hash recorded for an English text: 64-bit FNV-1a, in hex. Stable
/// across Rust versions and platforms, unlike the standard hasher.
pub fn hash(text: &str) -> String {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in text.bytes() {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    format!("{h:016x}")
}

fn parse_hashes(text: &str) -> BTreeMap<String, String> {
    text.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .filter_map(|l| {
            let (k, h) = l.split_once(char::is_whitespace)?;
            Some((k.to_string(), h.trim().to_string()))
        })
        .collect()
}

/// The sidecar's text for a locale.
pub fn write_hashes(hashes: &BTreeMap<String, String>) -> String {
    let mut out = String::from(
        "# The hash of the English text each key was translated from: a key\n\
         # whose English changed since is reported by the catalogue check.\n",
    );
    for (k, h) in hashes {
        let _ = writeln!(out, "{k} {h}");
    }
    out
}

/// Every locale's keys, with their canonical text; faults found on the way.
fn keys(
    catalogs: &Catalogs,
    faults: &mut Vec<Fault>,
) -> BTreeMap<String, BTreeMap<String, String>> {
    let mut out: BTreeMap<String, BTreeMap<String, String>> = BTreeMap::new();
    for file in &catalogs.files {
        let locale = out.entry(file.locale.clone()).or_default();
        match units(&file.source) {
            Err(errors) => faults.push(Fault::Unparsable {
                file: format!("{}/{}", file.locale, file.name),
                errors,
            }),
            Ok(units) => {
                for (key, text) in units {
                    if locale.insert(key.clone(), text).is_some() {
                        let fault = Fault::Duplicate {
                            locale: file.locale.clone(),
                            key,
                        };
                        if !faults.contains(&fault) {
                            faults.push(fault);
                        }
                    }
                }
            }
        }
    }
    out
}

/// Checks every locale against English.
pub fn check(catalogs: &Catalogs) -> Report {
    let mut faults = Vec::new();
    let keys = keys(catalogs, &mut faults);
    let Some(english) = keys.get(ENGLISH) else {
        faults.push(Fault::NoEnglish);
        return Report {
            faults,
            ..Report::default()
        };
    };
    let none = BTreeMap::new();
    let locales = catalogs
        .locales()
        .into_iter()
        .filter(|l| *l != ENGLISH)
        .map(|locale| {
            let theirs = &keys[locale];
            let hashes = catalogs.hashes.get(locale).unwrap_or(&none);
            let mut r = LocaleReport {
                locale: locale.to_string(),
                english: english.len(),
                ..LocaleReport::default()
            };
            for (key, text) in english {
                if !theirs.contains_key(key) {
                    r.missing.push(key.clone());
                    continue;
                }
                r.translated += 1;
                match hashes.get(key) {
                    None => r.unstamped.push(key.clone()),
                    Some(h) if *h != hash(text) => r.changed.push(key.clone()),
                    Some(_) => {}
                }
            }
            r.orphans = theirs
                .keys()
                .filter(|k| !english.contains_key(*k))
                .cloned()
                .collect();
            r
        })
        .collect();
    Report {
        english: english.len(),
        locales,
        faults,
    }
}

/// A locale's sidecar brought up to date: the hash of the current English
/// for each translated key that has none (every translated key with
/// `all`), keys no longer translated or no longer in English dropped.
pub fn stamp(
    catalogs: &Catalogs,
    locale: &str,
    all: bool,
) -> Result<BTreeMap<String, String>, String> {
    let mut faults = Vec::new();
    let keys = keys(catalogs, &mut faults);
    if let Some(f) = faults.first() {
        return Err(f.to_string());
    }
    let english = keys
        .get(ENGLISH)
        .ok_or_else(|| Fault::NoEnglish.to_string())?;
    let theirs = keys
        .get(locale)
        .ok_or_else(|| format!("no catalogue for `{locale}`"))?;
    let old = catalogs.hashes.get(locale).cloned().unwrap_or_default();
    Ok(theirs
        .keys()
        .filter_map(|k| {
            let current = hash(english.get(k)?);
            let h = match old.get(k) {
                Some(h) if !all => h.clone(),
                _ => current,
            };
            Some((k.clone(), h))
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file(locale: &str, name: &str, source: &str) -> File {
        File {
            locale: locale.into(),
            name: name.into(),
            source: source.into(),
        }
    }

    const EN: &str = "\
panel-a =
    .title = Graph
panel-b =
    .title = Queue
panel-c =
    .title = Log
";

    fn catalogs(fr: &str) -> Catalogs {
        Catalogs {
            files: vec![file("en", "panels.ftl", EN), file("fr", "panels.ftl", fr)],
            hashes: BTreeMap::new(),
        }
    }

    #[test]
    fn missing_orphan_and_changed_keys_are_reported() {
        let mut c = catalogs(
            "panel-a =\n    .title = Graphe\npanel-b =\n    .title = File\npanel-z =\n    .title = Vieux\n",
        );
        let stamped = stamp(&c, "fr", false).unwrap();
        assert_eq!(stamped.len(), 2, "{stamped:?}");
        c.hashes.insert("fr".into(), stamped);
        // English rewords the queue's title after its translation.
        c.files[0].source = EN.replace("Queue", "Jobs");
        let r = check(&c);
        assert!(r.faults.is_empty(), "{r}");
        let fr = &r.locales[0];
        assert_eq!(fr.english, 3);
        assert_eq!(fr.translated, 2);
        assert_eq!(fr.missing, ["panel-c.title"]);
        assert_eq!(fr.orphans, ["panel-z.title"]);
        assert_eq!(fr.changed, ["panel-b.title"]);
        assert!(fr.unstamped.is_empty());
        assert!((fr.coverage() - 66.6).abs() < 0.1);
        // Restamping everything accepts the reviewed translations.
        let all = stamp(&c, "fr", true).unwrap();
        c.hashes.insert("fr".into(), all);
        assert!(check(&c).locales[0].changed.is_empty());
    }

    #[test]
    fn unstamped_translations_are_listed() {
        let r = check(&catalogs("panel-a =\n    .title = Graphe\n"));
        assert_eq!(r.locales[0].unstamped, ["panel-a.title"]);
    }

    #[test]
    fn unparsable_files_and_duplicates_are_faults() {
        let mut c = catalogs("panel-a =\n    .title = Graphe\npanel-a =\n    .title = Encore\n");
        c.files
            .push(file("fr", "broken.ftl", "ok = fine\nbad = {\n"));
        let r = check(&c);
        assert_eq!(r.faults.len(), 2, "{r}");
        assert!(matches!(&r.faults[0], Fault::Duplicate { key, .. } if key == "panel-a.title"));
        assert!(matches!(&r.faults[1], Fault::Unparsable { file, .. } if file == "fr/broken.ftl"));
        let r = check(&Catalogs {
            files: vec![file("fr", "a.ftl", "a = b\n")],
            ..Catalogs::default()
        });
        assert_eq!(r.faults, [Fault::NoEnglish]);
    }

    #[test]
    fn the_hash_ignores_layout() {
        let a = units("m =\n    .title = Graph\n").unwrap();
        let b = units("m =\n    .title =\n        Graph\n").unwrap();
        assert_eq!(hash(&a[0].1), hash(&b[0].1));
        assert_eq!(hash(""), "cbf29ce484222325");
    }

    #[test]
    fn egui_pops_catalogues_have_no_fault() {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("locales");
        let r = check(&Catalogs::read(&dir).unwrap());
        assert!(r.faults.is_empty(), "{r}");
    }
}
