//! The persistence driver (ADR 0008): documents saved atomically with one
//! backup, loaded through their schema's migrations, falling back to the
//! backup when the main file is missing or unreadable.
//!
//! ```
//! use egui_pop_core::persist::Persist;
//! use egui_pop_model::values::{SettingsDoc, settings_schema};
//!
//! let dir = std::env::temp_dir().join(format!("egui-pop-doc-{}", std::process::id()));
//! let persist = Persist::new(&dir);
//! persist.save("settings", &settings_schema(), &SettingsDoc::default()).unwrap();
//! let back = persist.load::<SettingsDoc>("settings", &settings_schema()).unwrap().unwrap();
//! assert!(back.value.values.is_empty());
//! # std::fs::remove_dir_all(&dir).unwrap();
//! ```
//!
//! A [`Writer`] writes documents a short while after their last change
//! (debounced, about a second), so what the user changed survives a
//! crash and nothing waits for the exit:
//!
//! ```
//! use egui_pop_core::persist::{Persist, Writer};
//!
//! let dir = std::env::temp_dir().join(format!("egui-pop-writer-{}", std::process::id()));
//! let mut w = Writer::new(Persist::new(&dir), 1.0);
//! assert!(w.offer("view", "{\"a\": 1}".into(), 10.0));
//! assert!(w.offer("view", "{\"a\": 2}".into(), 10.5)); // the deadline moves
//! assert!(w.tick(11.0).is_empty());
//! assert_eq!(w.tick(11.5).len(), 1);
//! assert!(!w.offer("view", "{\"a\": 2}".into(), 12.0)); // written already
//! # std::fs::remove_dir_all(&dir).unwrap();
//! ```

use std::collections::BTreeMap;
use std::fmt;
use std::fs;
use std::hash::{Hash as _, Hasher as _};
use std::io::{self, Write as _};
use std::path::{Path, PathBuf};

use egui_pop_model::doc::{DocError, Schema};
use serde::Serialize;
use serde::de::DeserializeOwned;

/// Which file a document came from.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Source {
    Main,
    Backup,
}

/// A document loaded from disk.
#[derive(Clone, Debug, PartialEq)]
pub struct Read<T> {
    pub value: T,
    pub source: Source,
    pub from_version: u32,
    /// What happened on the way: migrations, a fallback to the backup.
    pub notes: Vec<String>,
}

/// Why a document could not be read or written.
#[derive(Debug)]
pub enum PersistError {
    Io {
        path: PathBuf,
        error: io::Error,
    },
    /// Neither file could be read as a document.
    Doc {
        path: PathBuf,
        error: DocError,
        backup: Option<DocError>,
    },
}

impl fmt::Display for PersistError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PersistError::Io { path, error } => write!(f, "{}: {error}", path.display()),
            PersistError::Doc {
                path,
                error,
                backup,
            } => {
                write!(f, "{}: {error}", path.display())?;
                if let Some(b) = backup {
                    write!(f, " (backup: {b})")?;
                }
                Ok(())
            }
        }
    }
}

impl std::error::Error for PersistError {}

/// Documents in a directory: `<name>.json`, its backup `<name>.json.bak`
/// and, while writing, `<name>.json.tmp`.
#[derive(Clone, Debug)]
pub struct Persist {
    dir: PathBuf,
}

fn io_err(path: &Path) -> impl FnOnce(io::Error) -> PersistError + '_ {
    move |error| PersistError::Io {
        path: path.to_path_buf(),
        error,
    }
}

impl Persist {
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        Self { dir: dir.into() }
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// The path of a document.
    pub fn path(&self, name: &str) -> PathBuf {
        self.dir.join(format!("{name}.json"))
    }

    fn backup(&self, name: &str) -> PathBuf {
        self.dir.join(format!("{name}.json.bak"))
    }

    /// Writes a document's text atomically, keeping the previous one as
    /// the backup.
    /// A name may hold `/` (`workspaces/photos/layout`): its folders are
    /// made.
    pub fn save_text(&self, name: &str, text: &str) -> Result<(), PersistError> {
        let path = self.path(name);
        let dir = path.parent().unwrap_or(&self.dir).to_path_buf();
        fs::create_dir_all(&dir).map_err(io_err(&dir))?;
        let tmp = self.dir.join(format!("{name}.json.tmp"));
        {
            let mut f = fs::File::create(&tmp).map_err(io_err(&tmp))?;
            f.write_all(text.as_bytes()).map_err(io_err(&tmp))?;
            f.sync_all().map_err(io_err(&tmp))?;
        }
        if path.exists() {
            fs::rename(&path, self.backup(name)).map_err(io_err(&path))?;
        }
        fs::rename(&tmp, &path).map_err(io_err(&path))?;
        Ok(())
    }

    /// Saves a document at its schema's current version.
    pub fn save<T: Serialize>(
        &self,
        name: &str,
        schema: &Schema,
        value: &T,
    ) -> Result<(), PersistError> {
        let path = self.path(name);
        let text = schema.save(value).map_err(|error| PersistError::Doc {
            path,
            error,
            backup: None,
        })?;
        self.save_text(name, &text)
    }

    /// Loads a document: `None` when there is none (neither file exists).
    pub fn load<T: DeserializeOwned>(
        &self,
        name: &str,
        schema: &Schema,
    ) -> Result<Option<Read<T>>, PersistError> {
        let path = self.path(name);
        let backup = self.backup(name);
        let read = |p: &Path| -> Option<Result<String, io::Error>> {
            match fs::read_to_string(p) {
                Err(e) if e.kind() == io::ErrorKind::NotFound => None,
                r => Some(r),
            }
        };
        let main_error = match read(&path) {
            None => None,
            Some(Err(error)) => return Err(PersistError::Io { path, error }),
            Some(Ok(text)) => match schema.load::<T>(&text) {
                Ok(l) => {
                    return Ok(Some(Read {
                        value: l.value,
                        source: Source::Main,
                        from_version: l.from_version,
                        notes: l.migrated.iter().map(|m| format!("migrated {m}")).collect(),
                    }));
                }
                Err(e) => Some(e),
            },
        };
        match read(&backup) {
            None => match main_error {
                None => Ok(None),
                Some(error) => Err(PersistError::Doc {
                    path,
                    error,
                    backup: None,
                }),
            },
            Some(Err(error)) => Err(PersistError::Io {
                path: backup,
                error,
            }),
            Some(Ok(text)) => match schema.load::<T>(&text) {
                Ok(l) => {
                    let mut notes = vec![match &main_error {
                        Some(e) => {
                            format!("{} is unreadable ({e}): the backup is used", path.display())
                        }
                        None => format!("{} is missing: the backup is used", path.display()),
                    }];
                    notes.extend(l.migrated.iter().map(|m| format!("migrated {m}")));
                    for n in &notes {
                        log::warn!(target: "egui_pop", "{n}");
                    }
                    Ok(Some(Read {
                        value: l.value,
                        source: Source::Backup,
                        from_version: l.from_version,
                        notes,
                    }))
                }
                Err(b) => Err(PersistError::Doc {
                    path,
                    error: main_error.unwrap_or(DocError::Parse("missing".into())),
                    backup: Some(b),
                }),
            },
        }
    }
}

/// How long a [`Writer`] waits after a document's last change, by
/// default, in seconds.
pub const DEBOUNCE: f64 = 1.0;

/// How long, at most, a document changing all the time waits, as a
/// multiple of the delay.
const LONGEST: f64 = 5.0;

/// One document waiting to be written.
#[derive(Clone, Debug, Default)]
struct Slot {
    /// The text to write, when it differs from what was written.
    pending: Option<String>,
    /// When to write it.
    due: f64,
    /// When the first unwritten change came (bounds the wait).
    since: f64,
    /// A fingerprint of what was written last.
    written: Option<u64>,
}

fn fingerprint(text: &str) -> u64 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    text.hash(&mut h);
    h.finish()
}

/// Writes documents a short while after their last change: each offer of
/// a new text moves its deadline (`delay` seconds later, at most five
/// delays after the first unwritten change); an offer of what was
/// written last does nothing. Times are the caller's clock, in seconds
/// (`motion::now` in an application). Writes are atomic
/// ([`Persist::save_text`]).
#[derive(Clone, Debug)]
pub struct Writer {
    persist: Persist,
    delay: f64,
    docs: BTreeMap<String, Slot>,
}

impl Writer {
    pub fn new(persist: Persist, delay: f64) -> Self {
        Self {
            persist,
            delay: delay.max(0.0),
            docs: BTreeMap::new(),
        }
    }

    /// Where it writes.
    pub fn persist(&self) -> &Persist {
        &self.persist
    }

    /// Offers a document's text: scheduled when it differs from what was
    /// written last (or waits already). Whether it is to be written.
    pub fn offer(&mut self, name: &str, text: String, now: f64) -> bool {
        let print = fingerprint(&text);
        let slot = self.docs.entry(name.to_string()).or_default();
        if slot.pending.is_none() && slot.written == Some(print) {
            return false;
        }
        if slot.pending.as_deref() == Some(text.as_str()) {
            return true;
        }
        if slot.written == Some(print) {
            // Back to what is on disk: nothing to write.
            slot.pending = None;
            return false;
        }
        if slot.pending.is_none() {
            slot.since = now;
        }
        slot.pending = Some(text);
        slot.due = (now + self.delay).min(slot.since + self.delay * LONGEST);
        true
    }

    /// Offers a document at its schema's current version.
    pub fn offer_doc<T: Serialize>(
        &mut self,
        name: &str,
        schema: &Schema,
        value: &T,
        now: f64,
    ) -> Result<bool, PersistError> {
        let text = schema.save(value).map_err(|error| PersistError::Doc {
            path: self.persist.path(name),
            error,
            backup: None,
        })?;
        Ok(self.offer(name, text, now))
    }

    /// Records that a document's text is on disk (loaded, or written by
    /// someone else): offering it again writes nothing.
    pub fn written(&mut self, name: &str, text: &str) {
        let slot = self.docs.entry(name.to_string()).or_default();
        slot.written = Some(fingerprint(text));
        slot.pending = None;
    }

    /// The documents waiting, by name.
    pub fn pending(&self) -> Vec<&str> {
        self.docs
            .iter()
            .filter(|(_, s)| s.pending.is_some())
            .map(|(n, _)| n.as_str())
            .collect()
    }

    /// When the next write is due, if one waits.
    pub fn next_due(&self) -> Option<f64> {
        self.docs
            .values()
            .filter(|s| s.pending.is_some())
            .map(|s| s.due)
            .reduce(f64::min)
    }

    /// Writes the documents whose deadline passed; what was written, and
    /// how it went. A failed write stays pending.
    pub fn tick(&mut self, now: f64) -> Vec<(String, Result<(), PersistError>)> {
        let due: Vec<String> = self
            .docs
            .iter()
            .filter(|(_, s)| s.pending.is_some() && s.due <= now)
            .map(|(n, _)| n.clone())
            .collect();
        due.into_iter().map(|n| self.write(n, now)).collect()
    }

    /// Writes every waiting document now.
    pub fn flush(&mut self) -> Vec<(String, Result<(), PersistError>)> {
        let due: Vec<String> = self.pending().into_iter().map(str::to_string).collect();
        due.into_iter()
            .map(|n| self.write(n, f64::INFINITY))
            .collect()
    }

    fn write(&mut self, name: String, now: f64) -> (String, Result<(), PersistError>) {
        let Some(slot) = self.docs.get_mut(&name) else {
            return (name, Ok(()));
        };
        let Some(text) = slot.pending.take() else {
            return (name, Ok(()));
        };
        let out = self.persist.save_text(&name, &text);
        match &out {
            Ok(()) => slot.written = Some(fingerprint(&text)),
            Err(e) => {
                log::warn!(target: "egui_pop", "writing `{name}`: {e}");
                // Tried again after another delay.
                slot.pending = Some(text);
                if now.is_finite() {
                    slot.due = now + self.delay;
                }
            }
        }
        (name, out)
    }
}

#[cfg(test)]
mod writer_tests {
    use super::*;

    fn dir(tag: &str) -> PathBuf {
        std::env::temp_dir().join(format!("egui-pop-writer-{tag}-{}", std::process::id()))
    }

    #[test]
    fn a_document_is_written_once_its_changes_settle() {
        let d = dir("settle");
        let mut w = Writer::new(Persist::new(&d), 1.0);
        assert!(w.next_due().is_none());
        assert!(w.offer("a", "1".into(), 0.0));
        assert!(w.offer("a", "2".into(), 0.8));
        assert_eq!(w.next_due(), Some(1.8));
        assert!(w.tick(1.0).is_empty());
        let wrote = w.tick(1.8);
        assert_eq!(wrote.len(), 1);
        assert!(wrote[0].1.is_ok());
        assert_eq!(fs::read_to_string(d.join("a.json")).unwrap(), "2");
        // The same text again: nothing; back and forth before the
        // deadline: nothing either.
        assert!(!w.offer("a", "2".into(), 2.0));
        assert!(w.offer("a", "3".into(), 2.0));
        assert!(!w.offer("a", "2".into(), 2.1));
        assert!(w.pending().is_empty());
        fs::remove_dir_all(&d).unwrap();
    }

    #[test]
    fn a_document_changing_all_the_time_is_still_written() {
        let d = dir("busy");
        let mut w = Writer::new(Persist::new(&d), 1.0);
        let mut wrote = 0;
        for i in 0..100 {
            let now = i as f64 * 0.1;
            w.offer("b", format!("{i}"), now);
            wrote += w.tick(now).len();
        }
        // Ten seconds of changes every tenth of a second: written every
        // five seconds at least.
        assert!(wrote >= 1, "{wrote}");
        assert!(w.flush().len() == 1 || w.pending().is_empty());
        assert!(w.pending().is_empty());
        w.written("c", "x");
        assert!(!w.offer("c", "x".into(), 0.0));
        fs::remove_dir_all(&d).unwrap();
    }
}
