//! What a token file says about itself: its format stamp, the renames
//! a file written for an older contract goes through, and the warnings
//! a file loads with.
//!
//! The stamp sits in the root's `$extensions`:
//!
//! ```json
//! { "$extensions": { "org.egui-pop.format": { "version": 1, "contract": 1 } } }
//! ```
//!
//! A file without it is read as current, so a file written by hand needs
//! none. A file stamped newer than this egui-pop knows loads all the same,
//! best-effort, with a warning.
//!
//! ```
//! use egui_pop_tokens::{FormatStamp, TokenFile};
//!
//! let mut file = TokenFile::new("mine");
//! assert_eq!(FormatStamp::read(&file), (FormatStamp::CURRENT, vec![]));
//! file.stamp();
//! assert_eq!(FormatStamp::read(&file).0, FormatStamp::CURRENT);
//! ```

use std::fmt;

use serde_json::{Map, Value as Json, json};

use crate::TokenFile;
use crate::contract::{CONTRACT_VERSION, RENAMED, Renamed};
use crate::file::alias_target;

/// The root `$extensions` key of a token file's format stamp.
pub const FORMAT_EXTENSION: &str = "org.egui-pop.format";

/// The version of the file format egui-pop writes: how a token file is
/// laid out (DTCG, `$extends`, egui-pop's `$extensions`), apart from
/// which tokens it gives ([`CONTRACT_VERSION`]).
pub const FORMAT_VERSION: u64 = 1;

/// A token file's format stamp: the format and contract versions it was
/// written for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FormatStamp {
    /// The format's version ([`FORMAT_VERSION`]).
    pub version: u64,
    /// The contract's version ([`CONTRACT_VERSION`]).
    pub contract: u64,
}

impl FormatStamp {
    /// What this egui-pop writes, and what a file without a stamp is
    /// read as.
    pub const CURRENT: FormatStamp = FormatStamp {
        version: FORMAT_VERSION,
        contract: CONTRACT_VERSION,
    };

    /// The stamp as written under [`FORMAT_EXTENSION`].
    pub fn to_json(self) -> Json {
        json!({ "version": self.version, "contract": self.contract })
    }

    /// The stamp a file carries, [`FormatStamp::CURRENT`] when it has
    /// none, and what is wrong with it: a part that cannot be read
    /// counts as current, and a version newer than this egui-pop's is
    /// read best-effort; each says so in a warning.
    pub fn read(file: &TokenFile) -> (FormatStamp, Vec<Warning>) {
        let mut warnings = Vec::new();
        let mut stamp = FormatStamp::CURRENT;
        let Some(raw) = file
            .extensions
            .as_ref()
            .and_then(|x| x.get(FORMAT_EXTENSION))
        else {
            return (stamp, warnings);
        };
        let Some(obj) = raw.as_object() else {
            warnings.push(Warning::BadStamp {
                path: format!("$extensions.{FORMAT_EXTENSION}"),
                reason: format!("`{raw}` is not a map; read as current"),
            });
            return (stamp, warnings);
        };
        for (key, known, slot) in [
            ("version", FORMAT_VERSION, &mut stamp.version),
            ("contract", CONTRACT_VERSION, &mut stamp.contract),
        ] {
            let path = format!("$extensions.{FORMAT_EXTENSION}.{key}");
            match obj.get(key) {
                None => {}
                Some(v) => match v.as_u64().filter(|n| *n > 0) {
                    Some(n) => {
                        *slot = n;
                        if n > known {
                            warnings.push(Warning::Newer {
                                path,
                                found: n,
                                known,
                            });
                        }
                    }
                    None => warnings.push(Warning::BadStamp {
                        path,
                        reason: format!(
                            "`{v}` is not a version (a whole number from 1); read as current"
                        ),
                    }),
                },
            }
        }
        (stamp, warnings)
    }
}

impl TokenFile {
    /// Stamps the file with [`FormatStamp::CURRENT`] under
    /// [`FORMAT_EXTENSION`], keeping the rest of its root `$extensions`.
    pub fn stamp(&mut self) {
        let mut map = match self.extensions.take() {
            Some(Json::Object(m)) => m,
            _ => Map::new(),
        };
        map.insert(FORMAT_EXTENSION.to_string(), FormatStamp::CURRENT.to_json());
        self.extensions = Some(Json::Object(map));
    }
}

/// Something a token file loaded with, short of failing: the file still
/// loads, and the warning says what was read otherwise than written.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum Warning {
    /// The file is stamped with a newer format or contract than this
    /// egui-pop knows: it is read best-effort.
    Newer {
        /// The stamp's part (`$extensions.org.egui-pop.format.contract`).
        path: String,
        found: u64,
        known: u64,
    },
    /// The stamp cannot be read: the file is read as current.
    BadStamp { path: String, reason: String },
    /// A token renamed since the contract the file was written for: read
    /// under its new name.
    Renamed {
        from: String,
        to: String,
        /// The contract version that dropped the former name.
        contract: u64,
    },
    /// A token the base theme does not have: `kept` as the file's own,
    /// or left out (the theme editor changes its base's tokens only).
    Unknown {
        path: String,
        base: String,
        kept: bool,
    },
    /// A token whose value does not load over the base theme: left out,
    /// the base theme's value kept.
    Invalid { path: String, reason: String },
    /// The base theme (`$extends`) is not known yet: the tokens were not
    /// checked; resolving the theme reports what is wrong.
    Unchecked { base: String },
    /// The file's compiled form could not be kept in the cache
    /// ([`crate::cache`]): it was read uncached, and is compiled again
    /// next time.
    NotCached { file: String, reason: String },
    /// A file of a theme package ([`crate::package`]) not read: a face
    /// missing, not a font or outside the package (the families naming
    /// it fall back), or a second token file.
    Font { file: String, reason: String },
    /// A font family a token names (`font.family.ui`) not found where
    /// the theme is loaded (a system face not installed on this
    /// machine): the theme still loads, and `fallback`, the next name
    /// of the list found, draws instead.
    FontMissing {
        path: String,
        family: String,
        fallback: String,
    },
}

impl Warning {
    /// The token, or the part of the stamp, the warning is about.
    pub fn path(&self) -> Option<&str> {
        match self {
            Warning::Newer { path, .. }
            | Warning::BadStamp { path, .. }
            | Warning::Unknown { path, .. }
            | Warning::Invalid { path, .. }
            | Warning::FontMissing { path, .. } => Some(path),
            Warning::Renamed { from, .. } => Some(from),
            Warning::Unchecked { .. } | Warning::NotCached { .. } | Warning::Font { .. } => None,
        }
    }

    /// Whether the token the warning is about was left out of the file.
    pub fn left_out(&self) -> bool {
        matches!(
            self,
            Warning::Invalid { .. } | Warning::Unknown { kept: false, .. }
        )
    }
}

impl fmt::Display for Warning {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Warning::Newer { path, found, known } => write!(
                f,
                "`{path}`: {found} is newer than this egui-pop's ({known}); read best-effort"
            ),
            Warning::BadStamp { path, reason } => write!(f, "`{path}`: {reason}"),
            Warning::Renamed { from, to, contract } => write!(
                f,
                "`{from}` was renamed `{to}` in contract {contract}; read as `{to}`, update your file"
            ),
            Warning::Unknown { path, base, kept } => {
                if *kept {
                    write!(
                        f,
                        "`{path}` is not a token of `{base}`; kept as the file's own (a typo, or a token renamed or removed?)"
                    )
                } else {
                    write!(f, "`{path}` is not a token of `{base}`; left out")
                }
            }
            Warning::Invalid { path, reason } => {
                write!(
                    f,
                    "`{path}` left out, the base theme's value kept: {reason}"
                )
            }
            Warning::Unchecked { base } => write!(
                f,
                "no theme `{base}` yet: the tokens were not checked against it"
            ),
            Warning::NotCached { file, reason } => {
                write!(f, "`{file}`: read uncached: {reason}")
            }
            Warning::Font { file, reason } => write!(f, "`{file}`: {reason}"),
            Warning::FontMissing {
                path,
                family,
                fallback,
            } => write!(
                f,
                "`{path}`: no font named `{family}` here; {fallback} draws instead"
            ),
        }
    }
}

impl TokenFile {
    /// Brings a file to the current contract: reads its stamp
    /// ([`FormatStamp::read`]) and moves the tokens renamed since the
    /// contract it was written for to their new names ([`RENAMED`]), the
    /// aliases naming them too. What was read otherwise than written
    /// comes back as warnings. The stamp itself is left as it was.
    pub fn upgrade(&mut self) -> Vec<Warning> {
        prepare_with(self, RENAMED)
    }
}

fn prepare_with(file: &mut TokenFile, renames: &[Renamed]) -> Vec<Warning> {
    let (stamp, mut warnings) = FormatStamp::read(file);
    // A file that does not say its contract is current: nothing to move.
    let stated = file
        .extensions
        .as_ref()
        .and_then(|x| x.get(FORMAT_EXTENSION))
        .and_then(|x| x.get("contract"))
        .and_then(Json::as_u64)
        .is_some_and(|n| n > 0);
    if stated {
        migrate(file, stamp.contract, renames, &mut warnings);
    }
    warnings
}

/// Renames the tokens of a file written for `contract`, and the aliases
/// naming them, in the order of the table (so that a chain of renames is
/// followed). A new name the file gives itself wins over the former one.
fn migrate(file: &mut TokenFile, contract: u64, renames: &[Renamed], warnings: &mut Vec<Warning>) {
    let mut order: Vec<&Renamed> = renames.iter().filter(|r| r.contract > contract).collect();
    order.sort_by_key(|r| r.contract);
    for r in order {
        let mut met = false;
        if let Some(token) = file.tokens.remove(r.from) {
            met = true;
            file.tokens.entry(r.to.to_string()).or_insert(token);
        }
        for token in file.tokens.values_mut() {
            if alias_target(&token.value) == Some(r.from) {
                token.value = json!(format!("{{{}}}", r.to));
                met = true;
            }
        }
        if met {
            warnings.push(Warning::Renamed {
                from: r.from.to_string(),
                to: r.to.to_string(),
                contract: r.contract,
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file(text: &str) -> TokenFile {
        TokenFile::parse("t", text).unwrap()
    }

    #[test]
    fn a_missing_stamp_is_current_and_a_stamp_reads_back() {
        let mut f = file(r##"{ "$extensions": { "org.example": 1 } }"##);
        assert_eq!(FormatStamp::read(&f), (FormatStamp::CURRENT, vec![]));
        f.stamp();
        let back = file(&f.to_json_pretty());
        assert_eq!(FormatStamp::read(&back), (FormatStamp::CURRENT, vec![]));
        // Other extensions stay.
        assert_eq!(back.extensions.unwrap()["org.example"], json!(1));
    }

    #[test]
    fn a_newer_or_unreadable_stamp_is_a_warning() {
        let f = file(
            r#"{ "$extensions": { "org.egui-pop.format": { "version": 9, "contract": 7 } } }"#,
        );
        let (stamp, warnings) = FormatStamp::read(&f);
        assert_eq!(
            stamp,
            FormatStamp {
                version: 9,
                contract: 7
            }
        );
        assert_eq!(warnings.len(), 2, "{warnings:?}");
        assert!(matches!(warnings[1], Warning::Newer { found: 7, .. }));
        assert!(warnings[1].to_string().contains("read best-effort"));
        let f = file(r#"{ "$extensions": { "org.egui-pop.format": { "contract": "0.1" } } }"#);
        let (stamp, warnings) = FormatStamp::read(&f);
        assert_eq!(stamp, FormatStamp::CURRENT);
        assert!(matches!(&warnings[..], [Warning::BadStamp { .. }]));
        let f = file(r#"{ "$extensions": { "org.egui-pop.format": 1 } }"#);
        assert!(matches!(
            &FormatStamp::read(&f).1[..],
            [Warning::BadStamp { .. }]
        ));
    }

    #[test]
    fn tokens_renamed_since_the_files_contract_are_read_under_their_new_names() {
        // A fake table: the contract has no rename yet.
        let renames = [
            Renamed {
                from: "panel.fill",
                to: "surface.panel",
                contract: 2,
                since: "0.2",
            },
            Renamed {
                from: "old.b",
                to: "mid.b",
                contract: 2,
                since: "0.2",
            },
            Renamed {
                from: "mid.b",
                to: "new.b",
                contract: 3,
                since: "0.4",
            },
        ];
        let text = r##"{
          "$extensions": { "org.egui-pop.format": { "version": 1, "contract": 1 } },
          "panel": { "fill": { "$type": "color", "$value": "#101010" } },
          "old": { "b": { "$type": "color", "$value": "#202020" } },
          "x": { "$type": "color", "$value": "{panel.fill}" }
        }"##;
        let mut f = file(text);
        let warnings = prepare_with(&mut f, &renames);
        assert!(!f.tokens.contains_key("panel.fill"));
        assert_eq!(f.tokens["surface.panel"].value, json!("#101010"));
        assert_eq!(f.tokens["x"].value, json!("{surface.panel}"));
        // A chain of renames is followed.
        assert_eq!(f.tokens["new.b"].value, json!("#202020"));
        assert_eq!(warnings.len(), 3, "{warnings:?}");
        assert!(
            warnings[0].to_string().contains(
                "renamed `surface.panel` in contract 2; read as `surface.panel`, update your file"
            ),
            "{}",
            warnings[0]
        );
        // A file written for the contract that renamed them is left alone
        // (a contract newer than the real one, said so).
        let mut f = file(&text.replace("\"contract\": 1", "\"contract\": 2"));
        let warnings = prepare_with(&mut f, &renames);
        assert!(
            warnings.iter().all(|w| matches!(w, Warning::Newer { .. })),
            "{warnings:?}"
        );
        assert!(f.tokens.contains_key("panel.fill"));
        assert!(f.tokens.contains_key("old.b"));
        // So is a file without a stamp: it is current.
        let mut f = file(r##"{ "panel": { "fill": { "$type": "color", "$value": "#101010" } } }"##);
        assert!(prepare_with(&mut f, &renames).is_empty());
        assert!(f.tokens.contains_key("panel.fill"));
    }

    #[test]
    fn renames_name_a_contract_that_dropped_a_name() {
        for r in RENAMED {
            // The first contract dropped nothing.
            assert!(r.contract >= 2, "{} → {}", r.from, r.to);
            assert!(r.contract <= CONTRACT_VERSION, "{} → {}", r.from, r.to);
            assert!(crate::contract::required(r.from).is_none(), "{}", r.from);
        }
    }

    #[test]
    fn a_rename_entry_stays_two_minor_releases() {
        let r = Renamed {
            from: "a",
            to: "b",
            contract: 2,
            since: "0.3",
        };
        for (version, expired) in [
            ("0.3.0", false),
            ("0.4.1", false),
            ("0.5.0-rc.1", false),
            ("0.6.0", true),
            // A major counts as one more minor release.
            ("1.0.0", false),
            ("1.1.0", false),
            ("1.2.0", true),
        ] {
            assert_eq!(r.expired_at(version), Some(expired), "{version}");
        }
        assert_eq!(r.expired_at("0.2.0"), None, "older than the entry");
        assert_eq!(r.expired_at("one"), None);
    }

    /// Fails once an entry has outlived its window: the release raising
    /// this crate's version past it drops the entry (docs/tokens.md, "A
    /// file's version").
    #[test]
    fn no_rename_entry_outlives_its_window() {
        let now = env!("CARGO_PKG_VERSION");
        for r in RENAMED {
            match r.expired_at(now) {
                Some(false) => {}
                Some(true) => panic!(
                    "`{}` → `{}` was renamed in {} and is past its window in {now}: drop it from RENAMED",
                    r.from, r.to, r.since
                ),
                None => panic!(
                    "`{}` → `{}`: `since` ({}) is not a release at or before {now}",
                    r.from, r.to, r.since
                ),
            }
        }
    }
}
