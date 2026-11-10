//! Settings export and import: the file a person writes their settings
//! to and reads them back from, and the plan an import shows before it
//! applies (one row per setting: what it is now, what the file says, and
//! whether it would change, stay, or be left out and why).
//!
//! **The file** is a JSON document (ADR 0008) of kind `settings-export`,
//! version 1:
//!
//! ```json
//! {
//!   "kind": "settings-export",
//!   "version": 1,
//!   "app": "my-app",
//!   "selection": { "layer": "app", "place": "interface" },
//!   "settings": { "version": 3, "values": { "appearance.theme": "dark" } },
//!   "shortcuts": { "version": 3, "bindings": [{ "keys": "Ctrl+K", "command": "palette.open" }] },
//!   "leftOut": ["sync.token"]
//! }
//! ```
//!
//! - `settings.values` is by setting id; `settings.version` is the version
//!   of the `settings` document they were written at, so an import
//!   migrates them as the settings file is (a retired choice read as the
//!   one that replaced it).
//! - `shortcuts` (optional) is the person's keymap layer, with the
//!   `keymap` document's version.
//! - `leftOut` lists the sensitive settings that were not written
//!   ([`Descriptor::sensitive`]); their values are never in the file.
//! - `app` and `selection` say where it comes from and what was chosen;
//!   an import reads them for its words only.
//!
//! **The plan** ([`plan`]) judges each value against the setting this
//! application declares: a value that changes ([`Verdict::Change`]), one
//! equal to the current ([`Verdict::Same`]), a setting not declared here
//! ([`Verdict::Unknown`]), one the application retired
//! ([`Verdict::Obsolete`]), a value of the wrong type or out of range
//! ([`Verdict::Invalid`], never clamped), and a sensitive setting
//! ([`Verdict::Sensitive`], never read from a file).

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::{Value as Json, json};

use crate::doc::{self, Schema};
use crate::keymap::KeymapDoc;
use crate::values::Descriptor;

/// The document's kind.
pub const KIND: &str = "settings-export";
/// The document's version.
pub const VERSION: u32 = 1;
/// The key of the shortcuts' row in a plan.
pub const SHORTCUTS: &str = "shortcuts";

/// The `settings-export` document's schema.
pub fn schema() -> Schema {
    Schema::new(KIND, VERSION).migration(0, doc::same)
}

/// A settings export.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingsFile {
    /// The application that wrote it.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub app: String,
    /// What was chosen when it was written (`layer`, `place`).
    #[serde(default, skip_serializing_if = "Json::is_null")]
    pub selection: Json,
    #[serde(default)]
    pub settings: Values,
    /// The person's shortcuts, when they were exported.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shortcuts: Option<Shortcuts>,
    /// The sensitive settings left out.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub left_out: Vec<String>,
}

/// The settings' values, by id, at a `settings` document version.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Values {
    #[serde(default)]
    pub version: u32,
    #[serde(default)]
    pub values: BTreeMap<String, Json>,
}

/// A keymap layer, at a `keymap` document version.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Shortcuts {
    #[serde(default)]
    pub version: u32,
    #[serde(flatten)]
    pub keymap: KeymapDoc,
}

impl SettingsFile {
    /// A file of these values (by setting id) and shortcuts, at the
    /// current document versions.
    pub fn new(values: BTreeMap<String, Json>, shortcuts: Option<KeymapDoc>) -> SettingsFile {
        SettingsFile {
            settings: Values {
                version: crate::values::settings_schema().current,
                values,
            },
            shortcuts: shortcuts.map(|keymap| Shortcuts {
                version: crate::keymap_schema().current,
                keymap,
            }),
            ..SettingsFile::default()
        }
    }

    /// Its text.
    pub fn write(&self) -> Result<String, String> {
        schema().save(self).map_err(|e| e.to_string())
    }

    /// Reads a file's text: the document migrated, then its values and
    /// shortcuts brought to this version of their own documents.
    pub fn read(text: &str) -> Result<SettingsFile, String> {
        let json: Json =
            serde_json::from_str(text).map_err(|e| format!("not a settings file: {e}"))?;
        Self::from_json(json)
    }

    /// As [`read`](Self::read), from JSON.
    pub fn from_json(json: Json) -> Result<SettingsFile, String> {
        if json.get("kind").and_then(Json::as_str) != Some(KIND) {
            return Err(format!(
                "not a settings file: its kind is not `{KIND}` (another document, or not exported by Settings)"
            ));
        }
        let up = schema().upgrade(json).map_err(|e| e.to_string())?;
        let mut file: SettingsFile =
            serde_json::from_value(up.value).map_err(|e| format!("not a settings file: {e}"))?;
        // The values as the `settings` document's migrations read them:
        // by store path.
        let doc = json!({
            "kind": "settings",
            "version": file.settings.version,
            "values": file
                .settings
                .values
                .iter()
                .map(|(k, v)| (format!("settings/{k}"), v.clone()))
                .collect::<serde_json::Map<_, _>>(),
        });
        let up = crate::values::settings_schema()
            .upgrade(doc)
            .map_err(|e| format!("its settings: {e}"))?;
        file.settings = Values {
            version: crate::values::settings_schema().current,
            values: up.value["values"]
                .as_object()
                .map(|o| {
                    o.iter()
                        .map(|(k, v)| {
                            (
                                k.strip_prefix("settings/").unwrap_or(k).to_string(),
                                v.clone(),
                            )
                        })
                        .collect()
                })
                .unwrap_or_default(),
        };
        if let Some(s) = file.shortcuts.take() {
            let mut json = serde_json::to_value(&s.keymap).map_err(|e| e.to_string())?;
            json["kind"] = json!("keymap");
            json["version"] = json!(s.version);
            let up = crate::keymap_schema()
                .upgrade(json)
                .map_err(|e| format!("its shortcuts: {e}"))?;
            let keymap: KeymapDoc =
                serde_json::from_value(up.value).map_err(|e| format!("its shortcuts: {e}"))?;
            file.shortcuts = Some(Shortcuts {
                version: crate::keymap_schema().current,
                keymap,
            });
        }
        Ok(file)
    }
}

/// Splits values to export: the sensitive ones are left out, by id.
pub fn without_secrets(
    values: BTreeMap<String, Json>,
    sensitive: &dyn Fn(&str) -> bool,
) -> (BTreeMap<String, Json>, Vec<String>) {
    let mut left_out = Vec::new();
    let kept = values
        .into_iter()
        .filter(|(k, _)| {
            let secret = sensitive(k);
            if secret {
                left_out.push(k.clone());
            }
            !secret
        })
        .collect();
    (kept, left_out)
}

/// What an import does with one value.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Verdict {
    /// It changes the setting.
    Change,
    /// It is the value already set.
    Same,
    /// No such setting here (a plugin not loaded, another application).
    Unknown,
    /// A setting the application retired.
    Obsolete,
    /// The wrong type, or out of range: never applied.
    Invalid,
    /// A secret: never read from a file.
    Sensitive,
}

impl Verdict {
    /// Whether it may be applied.
    pub fn applies(self) -> bool {
        self == Verdict::Change
    }

    /// The tone its row is shown with.
    pub fn tone(self) -> &'static str {
        match self {
            Verdict::Change => "info",
            Verdict::Same => "success",
            Verdict::Unknown | Verdict::Obsolete | Verdict::Sensitive => "warning",
            Verdict::Invalid => "danger",
        }
    }
}

/// One row of an import's plan.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Row {
    /// The setting's id, or [`SHORTCUTS`].
    pub key: String,
    /// Its label (the id when unknown).
    pub title: String,
    pub verdict: Verdict,
    /// Its value now, in the scope imported into (`null` when unknown).
    pub current: Json,
    /// What the file says (after the file's migrations).
    pub imported: Json,
    /// Why it is left out, or what it does.
    pub reason: String,
    /// The tone its row is shown with.
    pub tone: String,
}

/// What a plan needs of a setting this application declares.
#[derive(Clone, Debug)]
pub struct Declared {
    pub title: String,
    pub descriptor: Descriptor,
    /// Its value now, in the scope imported into.
    pub current: Json,
}

/// Judges each value of a file against the settings declared here
/// (`declared`) and those retired (`obsolete`, with what replaced them).
/// The rows are in the file's order (by id); the sensitive settings the
/// file left out come last.
pub fn plan(
    file: &SettingsFile,
    declared: &dyn Fn(&str) -> Option<Declared>,
    obsolete: &dyn Fn(&str) -> Option<String>,
) -> Vec<Row> {
    let mut rows = Vec::new();
    let row =
        |key: &str, title: &str, verdict, current: Json, imported: &Json, reason: String| Row {
            key: key.into(),
            title: title.into(),
            verdict,
            current,
            imported: imported.clone(),
            reason,
            tone: verdict.tone().into(),
        };
    for (key, imported) in &file.settings.values {
        if let Some(note) = obsolete(key) {
            let reason = if note.is_empty() {
                "obsolete: this setting was retired".to_string()
            } else {
                format!("obsolete: {note}")
            };
            rows.push(row(
                key,
                key,
                Verdict::Obsolete,
                Json::Null,
                imported,
                reason,
            ));
            continue;
        }
        let Some(d) = declared(key) else {
            rows.push(row(
                key,
                key,
                Verdict::Unknown,
                Json::Null,
                imported,
                "unknown: no such setting here".into(),
            ));
            continue;
        };
        if d.descriptor.sensitive {
            rows.push(row(
                key,
                &d.title,
                Verdict::Sensitive,
                Json::Null,
                &Json::Null,
                "sensitive: never read from a file".into(),
            ));
            continue;
        }
        match d.descriptor.accept(imported) {
            Err(reason) => rows.push(row(
                key,
                &d.title,
                Verdict::Invalid,
                d.current,
                imported,
                format!("invalid: {reason}"),
            )),
            Ok(v) if v.to_json() == d.current => rows.push(row(
                key,
                &d.title,
                Verdict::Same,
                d.current,
                imported,
                "already set".into(),
            )),
            Ok(v) => rows.push(row(
                key,
                &d.title,
                Verdict::Change,
                d.current,
                &v.to_json(),
                "changes".into(),
            )),
        }
    }
    for key in &file.left_out {
        let title = declared(key).map_or_else(|| key.clone(), |d| d.title);
        rows.push(row(
            key,
            &title,
            Verdict::Sensitive,
            Json::Null,
            &Json::Null,
            "sensitive: left out of the file when it was written".into(),
        ));
    }
    rows
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::values::Descriptor;

    fn declared(key: &str) -> Option<Declared> {
        let (title, descriptor, current) = match key {
            "volume" => (
                "Volume",
                Descriptor::float("Volume", 0.0, 1.0, 0.5),
                json!(0.5),
            ),
            "count" => ("Count", Descriptor::int("Count", 1, 9, 3), json!(3)),
            "theme" => (
                "Theme",
                Descriptor::choice("Theme", &["dark", "light"], "dark"),
                json!("dark"),
            ),
            "token" => (
                "Token",
                Descriptor::text("Token", "").sensitive(),
                json!("s3cret"),
            ),
            "files.view" => (
                "View",
                Descriptor::choice("View", &["details", "thumbnails"], "details"),
                json!("details"),
            ),
            _ => return None,
        };
        Some(Declared {
            title: title.into(),
            descriptor,
            current,
        })
    }

    fn obsolete(key: &str) -> Option<String> {
        (key == "old.zoom").then(|| "replaced by Interface zoom".into())
    }

    #[test]
    fn a_file_round_trips_and_its_values_are_migrated() {
        let file = SettingsFile::new(
            [("theme".to_string(), json!("light"))].into(),
            Some(KeymapDoc::default()),
        );
        let text = file.write().unwrap();
        assert!(text.contains("\"kind\": \"settings-export\""));
        assert_eq!(SettingsFile::read(&text).unwrap(), file);
        // Written at settings version 2: the file browser's views read
        // as its modes.
        let old = json!({
            "kind": "settings-export",
            "version": 1,
            "settings": { "version": 2, "values": { "files.view": "grid" } },
        });
        let read = SettingsFile::from_json(old).unwrap();
        assert_eq!(read.settings.values["files.view"], json!("thumbnails"));
        assert!(SettingsFile::read("{\"kind\": \"keymap\"}").is_err());
        assert!(SettingsFile::read("not json").is_err());
        let newer = json!({ "kind": "settings-export", "version": 9 });
        assert!(SettingsFile::from_json(newer).is_err());
    }

    #[test]
    fn secrets_are_left_out_and_listed() {
        let values: BTreeMap<String, Json> = [
            ("theme".to_string(), json!("light")),
            ("token".to_string(), json!("s3cret")),
        ]
        .into();
        let (kept, left) = without_secrets(values, &|k| {
            declared(k).is_some_and(|d| d.descriptor.sensitive)
        });
        assert_eq!(kept.keys().collect::<Vec<_>>(), ["theme"]);
        assert_eq!(left, ["token"]);
    }

    #[test]
    fn the_plan_says_what_changes_and_why_the_rest_is_left_out() {
        let mut file = SettingsFile::new(
            [
                ("volume".to_string(), json!(0.8)),
                ("count".to_string(), json!(3)),
                ("theme".to_string(), json!("sepia")),
                ("token".to_string(), json!("stolen")),
                ("nowhere".to_string(), json!(1)),
                ("old.zoom".to_string(), json!(2)),
            ]
            .into(),
            None,
        );
        file.left_out = vec!["token".into()];
        let rows = plan(&file, &declared, &obsolete);
        let said: Vec<(&str, Verdict)> = rows.iter().map(|r| (r.key.as_str(), r.verdict)).collect();
        assert_eq!(
            said,
            [
                ("count", Verdict::Same),
                ("nowhere", Verdict::Unknown),
                ("old.zoom", Verdict::Obsolete),
                ("theme", Verdict::Invalid),
                ("token", Verdict::Sensitive),
                ("volume", Verdict::Change),
                ("token", Verdict::Sensitive),
            ]
        );
        let volume = rows.iter().find(|r| r.key == "volume").unwrap();
        assert_eq!(
            (&volume.current, &volume.imported),
            (&json!(0.5), &json!(0.8))
        );
        let theme = rows.iter().find(|r| r.key == "theme").unwrap();
        assert_eq!(theme.reason, "invalid: `sepia` is not one of dark, light");
        assert_eq!(theme.tone, "danger");
        assert_eq!(rows[2].reason, "obsolete: replaced by Interface zoom");
        assert!(
            rows.iter()
                .filter(|r| r.key == "token")
                .all(|r| r.imported.is_null() && r.current.is_null()),
            "a secret's value is never shown"
        );
    }

    #[test]
    fn values_out_of_range_or_of_the_wrong_type_are_refused_not_clamped() {
        let d = Descriptor::int("Count", 1, 9, 3);
        assert_eq!(d.accept(&json!(4)), Ok(crate::values::Value::Int(4)));
        assert_eq!(d.accept(&json!(12)).unwrap_err(), "12 is above 9");
        assert_eq!(
            d.accept(&json!(2.5)).unwrap_err(),
            "2.5 is not a whole number"
        );
        assert!(d.accept(&json!("4")).is_err());
        let f = Descriptor::floats("Colour", 3, 0.0, 1.0, &[0.0, 0.0, 0.0]);
        assert!(f.accept(&json!([0.1, 0.2, 0.3])).is_ok());
        assert_eq!(
            f.accept(&json!([0.1, 0.2, 0.3, 0.4])).unwrap_err(),
            "4 values, at most 3"
        );
        assert_eq!(f.accept(&json!([0.1, 2.0])).unwrap_err(), "2 is above 1");
        let t = Descriptor::text("Name", "").capacity(3);
        assert_eq!(t.accept(&json!("abcd")).unwrap_err(), "4 bytes, at most 3");
    }
}
