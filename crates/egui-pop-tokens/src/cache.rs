//! A cache of compiled files: what is read from a file at run time and
//! costly to compile (a YAML token file, say) is compiled once and kept,
//! never next to its source.
//!
//! The store is not specific to tokens. An entry is the compiled form of
//! one source file, made by one compiler; it is keyed by the source's
//! path, a hash of its contents and the compiler's version:
//!
//! - a **hit** (same path, same contents, same compiler version) gives
//!   the compiled bytes back;
//! - a **miss** or a **stale** entry (the source changed, or the compiler
//!   is another version) gives nothing: the caller compiles the source
//!   and puts the result, which replaces the stale entry.
//!
//! Nothing here is fatal (ADR 0003): an entry that cannot be read is a
//! miss, and one that cannot be written is an error the caller reports
//! while going on with what it compiled.
//!
//! The entries live under `<dir>/<compiler>/`, one file per source path.
//! The directory is the platform's cache directory under the
//! application's id ([`Cache::for_app`], [`platform_dir`]):
//!
//! | | Cache |
//! |---|---|
//! | Linux and BSD | `$XDG_CACHE_HOME/<app>/egui-pop` (`~/.cache/<app>/egui-pop`) |
//! | macOS | `~/Library/Caches/<app>/egui-pop` |
//! | Windows | `%LOCALAPPDATA%\<app>\cache\egui-pop` |
//!
//! egui-pop does not know the application's id, so the cache is off
//! until the application gives one: [`set_default`] at start-up for every
//! file read at run time ([`KitData::load`](crate::kit::KitData::load),
//! [`Themes::add_path`](crate::Themes::add_path)), or a [`Cache`] passed
//! where a reader takes one. [`Cache::at`] puts it elsewhere (a test's
//! temporary directory), [`Cache::off`] turns it off.
//!
//! ```
//! use egui_pop_tokens::cache::{Cache, Compiler};
//!
//! let dir = std::env::temp_dir().join(format!("egui-pop-cache-doc-{}", std::process::id()));
//! let cache = Cache::at(&dir);
//! let upper = Compiler { name: "upper", version: "1" };
//! let source = dir.join("notes.txt");
//! assert_eq!(cache.get(&upper, &source, b"hello"), None);
//! cache.put(&upper, &source, b"hello", b"HELLO").unwrap();
//! assert_eq!(cache.get(&upper, &source, b"hello").as_deref(), Some(&b"HELLO"[..]));
//! // The source changed: stale.
//! assert_eq!(cache.get(&upper, &source, b"hello!"), None);
//! # std::fs::remove_dir_all(&dir).ok();
//! ```

use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::RwLock;

/// What made an entry: its name (a folder under the cache, so letters,
/// digits, `-`, `_` and `.`) and its version. An entry made by another
/// version is stale. A compiler raises its version whenever what it
/// writes changes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Compiler<'a> {
    pub name: &'a str,
    pub version: &'a str,
}

/// Where compiled files are kept, or nowhere (off).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Cache {
    dir: Option<PathBuf>,
}

/// An entry that could not be written: the source was compiled all the
/// same, and is compiled again next time.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CacheError {
    /// The entry's file.
    pub entry: PathBuf,
    pub message: String,
}

impl fmt::Display for CacheError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "cannot write the cache entry `{}`: {}",
            self.entry.display(),
            self.message
        )
    }
}

impl std::error::Error for CacheError {}

/// The first line of an entry: the store's format, then the compiler and
/// the source it was made from.
const MAGIC: &str = "egui-pop-cache/1";

impl Cache {
    /// No cache: nothing is read or written.
    pub const fn off() -> Cache {
        Cache { dir: None }
    }

    /// A cache in a directory of one's choosing (created when an entry is
    /// first written).
    pub fn at(dir: impl Into<PathBuf>) -> Cache {
        Cache {
            dir: Some(dir.into()),
        }
    }

    /// The cache of an application, in the platform's cache directory
    /// under its id ([`platform_dir`]); off when the platform gives none.
    pub fn for_app(app_id: &str) -> Cache {
        Cache {
            dir: platform_dir(app_id),
        }
    }

    /// The cache's directory; none when it is off.
    pub fn dir(&self) -> Option<&Path> {
        self.dir.as_deref()
    }

    /// Whether entries are read and written.
    pub fn is_on(&self) -> bool {
        self.dir.is_some()
    }

    /// The file holding a compiler's entry for a source (none when the
    /// cache is off). A relative source path is taken from the current
    /// directory.
    pub fn entry_path(&self, compiler: &Compiler<'_>, source: &Path) -> Option<PathBuf> {
        let dir = self.dir.as_ref()?;
        let source = std::path::absolute(source).unwrap_or_else(|_| source.to_path_buf());
        let key = hash(source.as_os_str().as_encoded_bytes());
        Some(
            dir.join(folder_name(compiler.name))
                .join(format!("{key:032x}")),
        )
    }

    /// The compiled form of a source, if the cache holds one made from
    /// these contents by this version of the compiler. Anything else (no
    /// entry, an unreadable one, a stale one) is none.
    pub fn get(&self, compiler: &Compiler<'_>, source: &Path, contents: &[u8]) -> Option<Vec<u8>> {
        let path = self.entry_path(compiler, source)?;
        let mut bytes = std::fs::read(path).ok()?;
        let end = bytes.iter().position(|b| *b == b'\n')?;
        if bytes[..end] != *header(compiler, contents).as_bytes() {
            return None;
        }
        bytes.drain(..=end);
        Some(bytes)
    }

    /// Keeps the compiled form of a source, replacing what the cache held
    /// for it. Written to a temporary file then renamed, so a reader never
    /// sees half an entry. Off, it does nothing.
    pub fn put(
        &self,
        compiler: &Compiler<'_>,
        source: &Path,
        contents: &[u8],
        compiled: &[u8],
    ) -> Result<(), CacheError> {
        let Some(path) = self.entry_path(compiler, source) else {
            return Ok(());
        };
        let failed = |e: std::io::Error| CacheError {
            entry: path.clone(),
            message: e.to_string(),
        };
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(failed)?;
        }
        let mut bytes = header(compiler, contents).into_bytes();
        bytes.push(b'\n');
        bytes.extend_from_slice(compiled);
        let tmp = path.with_extension(format!("tmp{}", std::process::id()));
        std::fs::write(&tmp, &bytes).map_err(failed)?;
        std::fs::rename(&tmp, &path).map_err(|e| {
            std::fs::remove_file(&tmp).ok();
            failed(e)
        })
    }
}

fn header(compiler: &Compiler<'_>, contents: &[u8]) -> String {
    format!(
        "{MAGIC} {} {} {:032x} {}",
        folder_name(compiler.name),
        compiler.version.replace(char::is_whitespace, "_"),
        hash(contents),
        contents.len()
    )
}

/// A compiler's name as a folder: anything but letters, digits, `-`, `_`
/// and `.` becomes `_`.
fn folder_name(name: &str) -> String {
    let clean: String = name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.') {
                c
            } else {
                '_'
            }
        })
        .collect();
    match clean.trim_matches('.') {
        "" => "_".into(),
        _ => clean,
    }
}

/// FNV-1a, 128 bits: stable across builds and platforms, so an entry
/// written by one build is found by the next (a cache, not a defence:
/// whoever can write the source can write the cache).
fn hash(bytes: &[u8]) -> u128 {
    const OFFSET: u128 = 0x6c62_272e_07bb_0142_62b8_2175_6295_c58d;
    const PRIME: u128 = 0x0000_0000_0100_0000_0000_0000_0000_013b;
    bytes
        .iter()
        .fold(OFFSET, |h, b| (h ^ u128::from(*b)).wrapping_mul(PRIME))
}

/// The cache directory for an application, read from the environment
/// `env` gives (see the module); none when the variables it needs are
/// missing.
///
/// ```
/// let dir = egui_pop_tokens::cache::platform_dir_with("demo", |name| match name {
///     "XDG_CACHE_HOME" => Some("/cache".into()),
///     _ => None,
/// });
/// if cfg!(all(unix, not(target_os = "macos"))) {
///     assert_eq!(dir, Some("/cache/demo/egui-pop".into()));
/// }
/// ```
pub fn platform_dir_with(app_id: &str, env: impl Fn(&str) -> Option<String>) -> Option<PathBuf> {
    let absolute = |v: Option<String>| v.map(PathBuf::from).filter(|p| p.is_absolute());
    let base = if cfg!(windows) {
        absolute(env("LOCALAPPDATA"))?.join(app_id).join("cache")
    } else if cfg!(target_os = "macos") {
        absolute(env("HOME"))?.join("Library/Caches").join(app_id)
    } else {
        match absolute(env("XDG_CACHE_HOME")) {
            Some(p) => p,
            None => absolute(env("HOME"))?.join(".cache"),
        }
        .join(app_id)
    };
    Some(base.join("egui-pop"))
}

/// The cache directory for an application (see the module).
pub fn platform_dir(app_id: &str) -> Option<PathBuf> {
    platform_dir_with(app_id, |name| std::env::var(name).ok())
}

static DEFAULT: RwLock<Option<Cache>> = RwLock::new(None);

/// Sets the cache the readers taking none use, for the whole process
/// (an application calls it once at start-up, with
/// [`Cache::for_app`]).
pub fn set_default(cache: Cache) {
    *DEFAULT.write().unwrap_or_else(|e| e.into_inner()) = Some(cache);
}

/// The cache the readers taking none use: what [`set_default`] set, off
/// before.
pub fn default() -> Cache {
    DEFAULT
        .read()
        .unwrap_or_else(|e| e.into_inner())
        .clone()
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    const C1: Compiler<'static> = Compiler {
        name: "test",
        version: "1",
    };

    fn temp(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("egui-pop-cache-{name}-{}", std::process::id()));
        std::fs::remove_dir_all(&dir).ok();
        dir
    }

    #[test]
    fn a_hit_needs_the_same_path_contents_and_version() {
        let dir = temp("key");
        let cache = Cache::at(&dir);
        let a = dir.join("a.yaml");
        cache.put(&C1, &a, b"x", b"compiled").unwrap();
        assert_eq!(cache.get(&C1, &a, b"x").as_deref(), Some(&b"compiled"[..]));
        assert_eq!(cache.get(&C1, &a, b"y"), None, "changed contents");
        assert_eq!(
            cache.get(&C1, &dir.join("b.yaml"), b"x"),
            None,
            "other path"
        );
        let c2 = Compiler { version: "2", ..C1 };
        assert_eq!(cache.get(&c2, &a, b"x"), None, "other version");
        // A new entry replaces the stale one: one file per source.
        cache.put(&c2, &a, b"x", b"two").unwrap();
        assert_eq!(cache.get(&c2, &a, b"x").as_deref(), Some(&b"two"[..]));
        assert_eq!(cache.get(&C1, &a, b"x"), None);
        assert_eq!(std::fs::read_dir(dir.join("test")).unwrap().count(), 1);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn off_reads_and_writes_nothing() {
        let cache = Cache::off();
        assert!(!cache.is_on());
        assert_eq!(cache.put(&C1, Path::new("/a"), b"x", b"y"), Ok(()));
        assert_eq!(cache.get(&C1, Path::new("/a"), b"x"), None);
    }

    #[test]
    fn an_unwritable_cache_says_so() {
        let dir = temp("unwritable");
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("file");
        std::fs::write(&file, "").unwrap();
        // Under a file: no folder can be made there.
        let cache = Cache::at(file.join("cache"));
        let err = cache.put(&C1, &dir.join("a"), b"x", b"y").unwrap_err();
        assert!(err.to_string().contains("cannot write"), "{err}");
        assert_eq!(cache.get(&C1, &dir.join("a"), b"x"), None);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_damaged_entry_is_a_miss() {
        let dir = temp("damaged");
        let cache = Cache::at(&dir);
        let a = dir.join("a");
        cache.put(&C1, &a, b"x", b"y").unwrap();
        std::fs::write(cache.entry_path(&C1, &a).unwrap(), "garbage").unwrap();
        assert_eq!(cache.get(&C1, &a, b"x"), None);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn compiler_names_are_folder_names() {
        assert_eq!(folder_name("tokens"), "tokens");
        assert_eq!(folder_name("../x y"), ".._x_y");
        assert_eq!(folder_name(".."), "_");
    }

    #[test]
    fn the_platform_dir_is_under_the_app_id() {
        let dir = platform_dir_with("org.example.App", |name| match name {
            "HOME" => Some("/home/u".into()),
            "LOCALAPPDATA" => Some("C:\\Users\\u\\AppData\\Local".into()),
            _ => None,
        });
        let dir = dir.expect("a home is enough");
        assert!(dir.ends_with("egui-pop"));
        assert!(dir.to_string_lossy().contains("org.example.App"));
        assert_eq!(platform_dir_with("a", |_| None), None);
        assert_eq!(
            platform_dir_with("a", |_| Some("relative".into())),
            None,
            "a relative directory is not trusted"
        );
    }
}
