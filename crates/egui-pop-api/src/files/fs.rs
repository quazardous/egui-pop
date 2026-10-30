//! Where the in-app picker reads: the system's file system ([`StdFs`],
//! the default), or a folder held in memory ([`MemoryFs`]), for the web
//! build, which has no file system, and for tests.
//!
//! The picker and the preview providers read through a [`SharedFs`]:
//! listing a folder, the facts of a path, the first bytes of a file. The
//! picker's New folder is the one thing it writes: [`StdFs`] makes the
//! folder, and a [`MemoryFs`] refuses (it is read-only; the picker says
//! so).
//!
//! ```
//! use std::path::Path;
//! use egui_pop_api::files::fs::{FileSystem, MemoryFs};
//!
//! let fs = MemoryFs::new()
//!     .file("/demo/notes.txt", b"Hello".to_vec())
//!     .dir("/demo/Photos");
//! let names: Vec<String> = fs.read_dir(Path::new("/demo")).unwrap().into_iter().map(|e| e.name).collect();
//! assert_eq!(names, ["Photos", "notes.txt"]);
//! assert_eq!(fs.metadata(Path::new("/demo/notes.txt")).unwrap().len, 5);
//! assert_eq!(fs.read(Path::new("/demo/notes.txt"), 2).unwrap(), b"He");
//! ```

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::io::{self, Read as _};
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;
use std::time::SystemTime;

/// One entry of a folder, as a file system lists it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DirItem {
    pub name: String,
    pub is_dir: bool,
}

/// The facts of a path.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Meta {
    pub is_dir: bool,
    /// Its size in bytes (0 for a folder).
    pub len: u64,
    /// When it last changed, if the file system says.
    pub modified: Option<SystemTime>,
}

/// What the picker and the preview providers read from.
pub trait FileSystem: Send + Sync + fmt::Debug {
    /// A folder's entries, in no particular order.
    fn read_dir(&self, dir: &Path) -> io::Result<Vec<DirItem>>;

    /// The facts of a path.
    fn metadata(&self, path: &Path) -> io::Result<Meta>;

    /// At most `limit` bytes from the start of a file.
    fn read(&self, path: &Path, limit: usize) -> io::Result<Vec<u8>>;

    /// Whether it can make folders ([`FileSystem::create_dir`]): the
    /// picker's New folder is disabled, with the reason, when it cannot.
    fn can_create_dirs(&self) -> bool {
        false
    }

    /// Makes a folder, whose parent exists. By default it cannot.
    fn create_dir(&self, _path: &Path) -> io::Result<()> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "this file system cannot make folders",
        ))
    }

    /// Whether its paths are the system's own, so that other programs
    /// (a system thumbnailer) can open them.
    fn is_native(&self) -> bool {
        false
    }

    fn is_dir(&self, path: &Path) -> bool {
        self.metadata(path).is_ok_and(|m| m.is_dir)
    }

    fn is_file(&self, path: &Path) -> bool {
        self.metadata(path).is_ok_and(|m| !m.is_dir)
    }
}

/// A file system shared by the picker, its preview and the workers.
pub type SharedFs = Arc<dyn FileSystem>;

/// The system's file system.
pub fn native() -> SharedFs {
    Arc::new(StdFs)
}

/// The system's file system, through `std::fs`.
#[derive(Clone, Copy, Debug, Default)]
pub struct StdFs;

impl FileSystem for StdFs {
    fn read_dir(&self, dir: &Path) -> io::Result<Vec<DirItem>> {
        Ok(std::fs::read_dir(dir)?
            .filter_map(Result::ok)
            .map(|e| DirItem {
                name: e.file_name().to_string_lossy().into_owned(),
                // Through links, as `Path::is_dir` does.
                is_dir: e.path().is_dir(),
            })
            .collect())
    }

    fn metadata(&self, path: &Path) -> io::Result<Meta> {
        let m = std::fs::metadata(path)?;
        Ok(Meta {
            is_dir: m.is_dir(),
            len: if m.is_dir() { 0 } else { m.len() },
            modified: m.modified().ok(),
        })
    }

    fn read(&self, path: &Path, limit: usize) -> io::Result<Vec<u8>> {
        let mut out = Vec::new();
        std::fs::File::open(path)?
            .take(limit as u64)
            .read_to_end(&mut out)?;
        Ok(out)
    }

    fn can_create_dirs(&self) -> bool {
        // The web has no file system to write to.
        !cfg!(target_arch = "wasm32")
    }

    fn create_dir(&self, path: &Path) -> io::Result<()> {
        std::fs::create_dir(path)
    }

    fn is_native(&self) -> bool {
        true
    }
}

/// A folder tree held in memory: files with their bytes and their
/// modification time, and folders (a file's folders exist with it).
#[derive(Clone, Default)]
pub struct MemoryFs {
    files: BTreeMap<PathBuf, (Arc<[u8]>, Option<SystemTime>)>,
    dirs: BTreeSet<PathBuf>,
}

impl fmt::Debug for MemoryFs {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("MemoryFs")
            .field("files", &self.files.len())
            .field("dirs", &self.dirs.len())
            .finish()
    }
}

/// A path without `.` and with `..` applied, so that `a/../b` is `b`.
fn normal(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for c in path.components() {
        match c {
            Component::CurDir => {}
            Component::ParentDir => {
                out.pop();
            }
            c => out.push(c),
        }
    }
    out
}

impl MemoryFs {
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds a folder (and the folders above it).
    pub fn dir(mut self, path: impl AsRef<Path>) -> Self {
        self.add_dir(&normal(path.as_ref()));
        self
    }

    /// Adds a file (and its folders), with no modification time.
    pub fn file(self, path: impl AsRef<Path>, bytes: Vec<u8>) -> Self {
        self.file_at(path, bytes, None)
    }

    /// Adds a file (and its folders) changed at `modified`.
    pub fn file_at(
        mut self,
        path: impl AsRef<Path>,
        bytes: Vec<u8>,
        modified: Option<SystemTime>,
    ) -> Self {
        let path = normal(path.as_ref());
        if let Some(parent) = path.parent() {
            self.add_dir(parent);
        }
        self.files.insert(path, (bytes.into(), modified));
        self
    }

    fn add_dir(&mut self, dir: &Path) {
        for a in dir.ancestors() {
            if a.as_os_str().is_empty() || !self.dirs.insert(a.to_path_buf()) {
                break;
            }
        }
    }
}

impl FileSystem for MemoryFs {
    fn read_dir(&self, dir: &Path) -> io::Result<Vec<DirItem>> {
        let dir = normal(dir);
        if !self.dirs.contains(&dir) {
            return Err(io::Error::new(io::ErrorKind::NotFound, "no such folder"));
        }
        let child = |p: &PathBuf| {
            (p.parent() == Some(dir.as_path()))
                .then(|| p.file_name().map(|n| n.to_string_lossy().into_owned()))
                .flatten()
        };
        let dirs = self
            .dirs
            .iter()
            .filter_map(|d| child(d).map(|name| DirItem { name, is_dir: true }));
        let files = self.files.keys().filter_map(|f| {
            child(f).map(|name| DirItem {
                name,
                is_dir: false,
            })
        });
        Ok(dirs.chain(files).collect())
    }

    fn metadata(&self, path: &Path) -> io::Result<Meta> {
        let path = normal(path);
        if let Some((bytes, modified)) = self.files.get(&path) {
            return Ok(Meta {
                is_dir: false,
                len: bytes.len() as u64,
                modified: *modified,
            });
        }
        if self.dirs.contains(&path) {
            return Ok(Meta {
                is_dir: true,
                len: 0,
                modified: None,
            });
        }
        Err(io::Error::new(io::ErrorKind::NotFound, "no such file"))
    }

    fn read(&self, path: &Path, limit: usize) -> io::Result<Vec<u8>> {
        let (bytes, _) = self
            .files
            .get(&normal(path))
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "no such file"))?;
        Ok(bytes[..bytes.len().min(limit)].to_vec())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_memory_folder_lists_reads_and_resolves() {
        let fs = MemoryFs::new()
            .file("/r/a/x.txt", b"abc".to_vec())
            .dir("/r/b/c");
        let mut names: Vec<_> = fs
            .read_dir(Path::new("/r"))
            .unwrap()
            .into_iter()
            .map(|e| (e.name, e.is_dir))
            .collect();
        names.sort();
        assert_eq!(names, [("a".into(), true), ("b".into(), true)]);
        assert!(fs.is_dir(Path::new("/r/b/../a")));
        assert!(fs.is_file(Path::new("/r/a/./x.txt")));
        assert_eq!(fs.read(Path::new("/r/a/x.txt"), 99).unwrap(), b"abc");
        assert!(fs.read_dir(Path::new("/r/a/x.txt")).is_err());
        assert!(fs.metadata(Path::new("/nope")).is_err());
        assert!(!fs.is_native());
        // Read-only: the default of the trait.
        assert!(!fs.can_create_dirs());
        assert_eq!(
            fs.create_dir(Path::new("/r/new")).unwrap_err().kind(),
            io::ErrorKind::Unsupported
        );
        assert!(!fs.is_dir(Path::new("/r/new")));
    }
}
