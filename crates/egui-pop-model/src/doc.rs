//! Persisted documents (ADR 0008): every document is a JSON object with a
//! `"version"` (missing means 0) and a `"kind"`. A [`Schema`] knows the
//! current version and one migration per older one; loading migrates,
//! then deserializes. No I/O here: the persistence driver of
//! `egui-pop-core` reads and writes the text.
//!
//! ```
//! use egui_pop_model::doc::Schema;
//! use serde_json::json;
//!
//! // Version 2 renamed `colour` to `color`.
//! let schema = Schema::new("prefs", 2)
//!     .migration(0, |j| Ok(j))
//!     .migration(1, |mut j| {
//!         let c = j["colour"].take();
//!         j["color"] = c;
//!         Ok(j)
//!     });
//! let loaded = schema
//!     .load::<serde_json::Value>(r#"{"version": 1, "kind": "prefs", "colour": "red"}"#)
//!     .unwrap();
//! assert_eq!(loaded.value["color"], json!("red"));
//! assert_eq!(loaded.from_version, 1);
//! ```

use std::collections::BTreeMap;
use std::fmt;

use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::Value as Json;

/// A migration from one version to the next.
pub type Migration = fn(Json) -> Result<Json, String>;

/// Why a document could not be loaded.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DocError {
    /// Not JSON.
    Parse(String),
    NotAnObject,
    /// Another kind of document.
    WrongKind {
        expected: String,
        found: String,
    },
    /// Written by a newer version.
    TooNew {
        found: u32,
        supported: u32,
    },
    /// No way from this version to the next.
    MissingMigration {
        from: u32,
    },
    Migration {
        from: u32,
        error: String,
    },
    /// Migrated, but not the expected shape.
    Shape(String),
}

impl fmt::Display for DocError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DocError::Parse(e) => write!(f, "not JSON: {e}"),
            DocError::NotAnObject => write!(f, "not a JSON object"),
            DocError::WrongKind { expected, found } => {
                write!(f, "a `{found}` document, not a `{expected}` one")
            }
            DocError::TooNew { found, supported } => write!(
                f,
                "version {found} is newer than this application reads (up to {supported})"
            ),
            DocError::MissingMigration { from } => {
                write!(f, "no migration from version {from}")
            }
            DocError::Migration { from, error } => {
                write!(f, "migrating from version {from}: {error}")
            }
            DocError::Shape(e) => write!(f, "unexpected content: {e}"),
        }
    }
}

impl std::error::Error for DocError {}

/// A document loaded, and how.
#[derive(Clone, Debug, PartialEq)]
pub struct Loaded<T> {
    pub value: T,
    /// The version it was written at.
    pub from_version: u32,
    /// The migrations applied, as `"1 -> 2"`.
    pub migrated: Vec<String>,
}

/// A kind of document and its versions.
#[derive(Clone, Debug)]
pub struct Schema {
    pub kind: &'static str,
    pub current: u32,
    migrations: BTreeMap<u32, Migration>,
}

impl Schema {
    pub fn new(kind: &'static str, current: u32) -> Self {
        Self {
            kind,
            current,
            migrations: BTreeMap::new(),
        }
    }

    /// The migration from `from` to `from + 1`.
    pub fn migration(mut self, from: u32, f: Migration) -> Self {
        self.migrations.insert(from, f);
        self
    }

    /// The versions without a migration to the next: should be empty.
    pub fn gaps(&self) -> Vec<u32> {
        (0..self.current)
            .filter(|v| !self.migrations.contains_key(v))
            .collect()
    }

    /// Brings a document's JSON to the current version.
    pub fn upgrade(&self, mut json: Json) -> Result<Loaded<Json>, DocError> {
        let obj = json.as_object().ok_or(DocError::NotAnObject)?;
        if let Some(kind) = obj.get("kind").and_then(Json::as_str)
            && kind != self.kind
        {
            return Err(DocError::WrongKind {
                expected: self.kind.into(),
                found: kind.into(),
            });
        }
        let from = obj
            .get("version")
            .and_then(Json::as_u64)
            .map_or(0, |v| v.min(u32::MAX as u64) as u32);
        if from > self.current {
            return Err(DocError::TooNew {
                found: from,
                supported: self.current,
            });
        }
        let mut migrated = Vec::new();
        for v in from..self.current {
            let m = self
                .migrations
                .get(&v)
                .ok_or(DocError::MissingMigration { from: v })?;
            json = m(json).map_err(|error| DocError::Migration { from: v, error })?;
            migrated.push(format!("{v} -> {}", v + 1));
        }
        if let Some(obj) = json.as_object_mut() {
            obj.insert("version".into(), self.current.into());
            obj.insert("kind".into(), self.kind.into());
        }
        Ok(Loaded {
            value: json,
            from_version: from,
            migrated,
        })
    }

    /// Parses, migrates and deserializes a document.
    pub fn load<T: DeserializeOwned>(&self, text: &str) -> Result<Loaded<T>, DocError> {
        let json: Json = serde_json::from_str(text).map_err(|e| DocError::Parse(e.to_string()))?;
        let up = self.upgrade(json)?;
        let value = serde_json::from_value(up.value).map_err(|e| DocError::Shape(e.to_string()))?;
        Ok(Loaded {
            value,
            from_version: up.from_version,
            migrated: up.migrated,
        })
    }

    /// The text of a document at the current version.
    pub fn save<T: Serialize>(&self, value: &T) -> Result<String, DocError> {
        let mut json = serde_json::to_value(value).map_err(|e| DocError::Shape(e.to_string()))?;
        let obj = json.as_object_mut().ok_or(DocError::NotAnObject)?;
        let mut out = serde_json::Map::new();
        out.insert("kind".into(), self.kind.into());
        out.insert("version".into(), self.current.into());
        for (k, v) in std::mem::take(obj) {
            if k != "kind" && k != "version" {
                out.insert(k, v);
            }
        }
        serde_json::to_string_pretty(&Json::Object(out)).map_err(|e| DocError::Shape(e.to_string()))
    }
}

/// Unversioned documents (version 0) have the shape of version 1.
pub(crate) fn same(json: Json) -> Result<Json, String> {
    Ok(json)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn documents_migrate_and_refuse_what_they_cannot_read() {
        let s = Schema::new("x", 2)
            .migration(0, same)
            .migration(1, |mut j| {
                j["b"] = j["a"].take();
                Ok(j)
            });
        assert!(s.gaps().is_empty());
        let l = s.load::<Json>(r#"{"a": 1}"#).unwrap();
        assert_eq!(l.from_version, 0);
        assert_eq!(l.migrated, ["0 -> 1", "1 -> 2"]);
        assert_eq!(l.value["b"], json!(1));
        assert_eq!(l.value["version"], json!(2));
        assert!(matches!(
            s.load::<Json>(r#"{"version": 3}"#),
            Err(DocError::TooNew { found: 3, .. })
        ));
        assert!(matches!(
            s.load::<Json>(r#"{"kind": "y"}"#),
            Err(DocError::WrongKind { .. })
        ));
        assert!(matches!(s.load::<Json>("[]"), Err(DocError::NotAnObject)));
        assert!(matches!(s.load::<Json>("{"), Err(DocError::Parse(_))));
        let gap = Schema::new("x", 2).migration(1, same);
        assert_eq!(gap.gaps(), [0]);
        assert_eq!(
            gap.load::<Json>("{}").unwrap_err(),
            DocError::MissingMigration { from: 0 }
        );
        let text = s.save(&json!({"b": 2, "version": 9})).unwrap();
        let back: Json = serde_json::from_str(&text).unwrap();
        assert_eq!(back, json!({"kind": "x", "version": 2, "b": 2}));
    }
}
