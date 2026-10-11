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

use std::fmt;
use std::fs;
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
    pub fn save_text(&self, name: &str, text: &str) -> Result<(), PersistError> {
        fs::create_dir_all(&self.dir).map_err(io_err(&self.dir))?;
        let path = self.path(name);
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
