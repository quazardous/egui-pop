//! The system's places the in-app picker lists beside home: its folders
//! (Desktop, Documents, Downloads, Pictures, Music, Videos) and the volumes
//! mounted, removable cards among them, the first place of a photo import.
//!
//! [`system_places`] reads them from the system: on Linux and the BSDs the
//! XDG user directories (`user-dirs.dirs`) and the mount points under
//! `/run/media/<user>`, `/media` and `/mnt`; on macOS the folders in home
//! and `/Volumes`; on Windows the folders in the user's profile and the
//! drives. They are read when a picker opens, not each frame. An
//! application gives its own with [`Files::set_system_places`] (a demo, a
//! sandbox, tests).
//!
//! [`Files::set_system_places`]: super::Files::set_system_places

use std::path::{Path, PathBuf};

use egui_pop_core::strings::StrKey;

use crate::strings as txt;

/// A folder of the system's, by what it is for.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SystemFolder {
    Desktop,
    Documents,
    Downloads,
    Pictures,
    Music,
    Videos,
}

impl SystemFolder {
    /// In the order the picker lists them.
    pub const ALL: [SystemFolder; 6] = [
        SystemFolder::Desktop,
        SystemFolder::Documents,
        SystemFolder::Downloads,
        SystemFolder::Pictures,
        SystemFolder::Music,
        SystemFolder::Videos,
    ];

    /// Its name in the picker, translated.
    pub fn title(self) -> &'static StrKey {
        match self {
            SystemFolder::Desktop => &txt::FILES_DESKTOP,
            SystemFolder::Documents => &txt::FILES_DOCUMENTS,
            SystemFolder::Downloads => &txt::FILES_DOWNLOADS,
            SystemFolder::Pictures => &txt::FILES_PICTURES,
            SystemFolder::Music => &txt::FILES_MUSIC,
            SystemFolder::Videos => &txt::FILES_VIDEOS,
        }
    }

    /// Its key in `user-dirs.dirs` (`XDG_<key>_DIR`).
    fn xdg_key(self) -> &'static str {
        match self {
            SystemFolder::Desktop => "DESKTOP",
            SystemFolder::Documents => "DOCUMENTS",
            SystemFolder::Downloads => "DOWNLOAD",
            SystemFolder::Pictures => "PICTURES",
            SystemFolder::Music => "MUSIC",
            SystemFolder::Videos => "VIDEOS",
        }
    }

    /// Its usual folder in home, where the system names none (macOS,
    /// Windows, Linux without `user-dirs.dirs`).
    fn in_home(self) -> &'static str {
        match self {
            SystemFolder::Desktop => "Desktop",
            SystemFolder::Documents => "Documents",
            SystemFolder::Downloads => "Downloads",
            SystemFolder::Pictures => "Pictures",
            SystemFolder::Music => "Music",
            SystemFolder::Videos if cfg!(target_os = "macos") => "Movies",
            SystemFolder::Videos => "Videos",
        }
    }
}

/// A volume mounted: its name and where it is.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Volume {
    pub name: String,
    pub path: PathBuf,
}

/// The system's places, as the picker lists them.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SystemPlaces {
    /// The system's folders that exist, home's own excluded.
    pub folders: Vec<(SystemFolder, PathBuf)>,
    /// The volumes mounted, the system's own excluded.
    pub volumes: Vec<Volume>,
}

/// The system's places, read from the system (see the module).
pub fn system_places() -> SystemPlaces {
    let Some(home) = super::home() else {
        return SystemPlaces {
            folders: Vec::new(),
            volumes: volumes(),
        };
    };
    let named = if cfg!(all(unix, not(target_os = "macos"))) {
        let config = std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .filter(|p| p.is_absolute())
            .unwrap_or_else(|| home.join(".config"));
        std::fs::read_to_string(config.join("user-dirs.dirs"))
            .map(|text| xdg_user_dirs(&text, &home))
            .ok()
    } else {
        None
    };
    let folders = named
        .unwrap_or_else(|| {
            SystemFolder::ALL
                .iter()
                .map(|f| (*f, home.join(f.in_home())))
                .collect()
        })
        .into_iter()
        .filter(|(_, p)| p != &home && p.is_dir())
        .collect();
    SystemPlaces {
        folders,
        volumes: volumes(),
    }
}

/// The folders `user-dirs.dirs` names (`XDG_PICTURES_DIR="$HOME/Pictures"`),
/// in the picker's order; one set to home itself means none, as the
/// specification says. Whether they exist is not checked.
pub fn xdg_user_dirs(text: &str, home: &Path) -> Vec<(SystemFolder, PathBuf)> {
    let value = |key: &str| {
        text.lines().find_map(|line| {
            let rest = line.trim().strip_prefix("XDG_")?.strip_prefix(key)?;
            let quoted = rest.strip_prefix("_DIR=")?.trim();
            let v = quoted.strip_prefix('"')?.strip_suffix('"')?;
            let path = if let Some(r) = v.strip_prefix("$HOME") {
                let r = r.trim_start_matches('/');
                if r.is_empty() {
                    home.to_path_buf()
                } else {
                    home.join(r)
                }
            } else if v.starts_with('/') {
                PathBuf::from(v)
            } else {
                return None;
            };
            Some(path)
        })
    };
    SystemFolder::ALL
        .iter()
        .filter_map(|f| value(f.xdg_key()).map(|p| (*f, p)))
        .filter(|(_, p)| p != home)
        .collect()
}

/// The volumes mounted (see the module), by name.
fn volumes() -> Vec<Volume> {
    let mut out = if cfg!(target_os = "macos") {
        mac_volumes()
    } else if cfg!(windows) {
        drives()
    } else if cfg!(unix) {
        unix_volumes()
    } else {
        Vec::new()
    };
    // `/media/<user>/x` is under `/media` too: once each.
    let mut seen = std::collections::HashSet::new();
    out.retain(|v| seen.insert(v.path.clone()));
    out.sort_by_key(|v| v.name.to_lowercase());
    out
}

fn named(path: PathBuf) -> Volume {
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string());
    Volume { name, path }
}

fn subfolders(dir: &Path) -> Vec<PathBuf> {
    std::fs::read_dir(dir)
        .map(|r| {
            r.filter_map(Result::ok)
                .map(|e| e.path())
                .filter(|p| p.is_dir())
                .collect()
        })
        .unwrap_or_default()
}

/// Mount points under `/run/media/<user>`, `/media/<user>`, `/media` and
/// `/mnt`: a folder on another device than its parent's.
#[cfg(unix)]
fn unix_volumes() -> Vec<Volume> {
    use std::os::unix::fs::MetadataExt as _;
    let user = std::env::var("USER").unwrap_or_default();
    let mut bases = Vec::new();
    if !user.is_empty() {
        bases.push(Path::new("/run/media").join(&user));
        bases.push(Path::new("/media").join(&user));
    }
    bases.push(PathBuf::from("/media"));
    bases.push(PathBuf::from("/mnt"));
    let mut out = Vec::new();
    for base in bases {
        let Ok(parent) = std::fs::metadata(&base) else {
            continue;
        };
        for p in subfolders(&base) {
            if std::fs::metadata(&p).is_ok_and(|m| m.dev() != parent.dev()) {
                out.push(named(p));
            }
        }
    }
    out
}

#[cfg(not(unix))]
fn unix_volumes() -> Vec<Volume> {
    Vec::new()
}

/// `/Volumes`, but the system's own volume, a link to `/`.
fn mac_volumes() -> Vec<Volume> {
    subfolders(Path::new("/Volumes"))
        .into_iter()
        .filter(|p| !std::fs::read_link(p).is_ok_and(|to| to == Path::new("/")))
        .map(named)
        .collect()
}

/// The drives from `C:` (`A:` and `B:` are floppies, slow to ask).
fn drives() -> Vec<Volume> {
    (b'C'..=b'Z')
        .map(|l| PathBuf::from(format!("{}:\\", l as char)))
        .filter(|p| p.is_dir())
        .map(|path| Volume {
            name: path.display().to_string().trim_end_matches('\\').to_owned(),
            path,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn user_dirs_read_home_relative_and_absolute_and_skip_home_itself() {
        let home = Path::new("/home/ana");
        let text = r#"
# This file is written by xdg-user-dirs-update
XDG_DESKTOP_DIR="$HOME/Bureau"
XDG_DOWNLOAD_DIR="$HOME/Téléchargements"
XDG_TEMPLATES_DIR="$HOME/Modèles"
XDG_MUSIC_DIR="$HOME"
XDG_PICTURES_DIR="/data/photos"
XDG_VIDEOS_DIR=relative/not/allowed
"#;
        assert_eq!(
            xdg_user_dirs(text, home),
            vec![
                (SystemFolder::Desktop, home.join("Bureau")),
                (SystemFolder::Downloads, home.join("Téléchargements")),
                (SystemFolder::Pictures, PathBuf::from("/data/photos")),
            ]
        );
    }
}
