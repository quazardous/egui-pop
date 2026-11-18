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
//! Beside them, an application adds **sections of its own** (a photo
//! import's cards, services and history): a [`PlaceProvider`] registered on
//! the files service ([`Files::place_providers`], [`FilesPlugin::places`])
//! gives [`PlaceSection`]s of [`PlaceItem`]s. The in-app picker lists them
//! under its own places, and a [`PlacesPanel`] draws all of them anywhere
//! (an import window's sources column). An item is a name, an icon, a
//! detail, a badge and, for a folder, its path; egui-pop knows nothing more
//! of it: an item without a path (an account of a service) is the
//! application's to act on.
//!
//! [`Files::set_system_places`]: super::Files::set_system_places
//! [`Files::place_providers`]: super::Files::place_providers
//! [`FilesPlugin::places`]: super::FilesPlugin::places

use std::fmt;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use egui::Ui;
use egui_pop_core::lock;
use egui_pop_core::strings::StrKey;
use egui_pop_core::theme::Tokens;
use egui_pop_core::{a11y, widgets};

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

/// One place of a [`PlaceSection`]: a folder, a volume, or an application's
/// own (a card, an account of a service). Made with [`PlaceItem::new`] or
/// [`PlaceItem::folder`] and its builders.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct PlaceItem {
    /// Its id, unique in its section: what a panel says was chosen.
    pub id: String,
    /// Its name, shown and said.
    pub name: String,
    /// The folder it opens, if it is one: the picker lists only those.
    pub path: Option<PathBuf>,
    /// An icon of the icon registry (`phosphor:sd-card`), before its name.
    pub icon: Option<String>,
    /// A line about it, in its tip (a card's free space); its path when none.
    pub detail: Option<String>,
    /// A short mark after its name (a count, "new"), said with it.
    pub badge: Option<String>,
}

impl PlaceItem {
    /// An item that is no folder: the application acts on it.
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            path: None,
            icon: None,
            detail: None,
            badge: None,
        }
    }

    /// A folder, its id its path.
    pub fn folder(name: impl Into<String>, path: impl Into<PathBuf>) -> Self {
        let path = path.into();
        Self {
            path: Some(path.clone()),
            ..Self::new(path.display().to_string(), name)
        }
    }

    #[must_use]
    pub fn path(mut self, path: impl Into<PathBuf>) -> Self {
        self.path = Some(path.into());
        self
    }

    #[must_use]
    pub fn icon(mut self, icon: impl Into<String>) -> Self {
        self.icon = Some(icon.into());
        self
    }

    #[must_use]
    pub fn detail(mut self, detail: impl Into<String>) -> Self {
        self.detail = Some(detail.into());
        self
    }

    #[must_use]
    pub fn badge(mut self, badge: impl Into<String>) -> Self {
        self.badge = Some(badge.into());
        self
    }

    /// What a screen reader says of it: its name, then its badge.
    pub fn said(&self) -> String {
        match &self.badge {
            Some(b) => format!("{}, {b}", self.name),
            None => self.name.clone(),
        }
    }
}

/// A titled list of places: the system's folders, the devices, or an
/// application's ("Cards", "Services").
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct PlaceSection {
    /// Its id: unique among the sections shown.
    pub id: String,
    pub title: String,
    pub items: Vec<PlaceItem>,
}

impl PlaceSection {
    pub fn new(id: impl Into<String>, title: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            title: title.into(),
            items: Vec::new(),
        }
    }

    /// Adds an item, after those it has.
    #[must_use]
    pub fn item(mut self, item: PlaceItem) -> Self {
        self.items.push(item);
        self
    }

    /// Adds items, after those it has.
    #[must_use]
    pub fn items(mut self, items: impl IntoIterator<Item = PlaceItem>) -> Self {
        self.items.extend(items);
        self
    }
}

/// An application's sections of places ([`PlaceProviders::register`]).
/// Asked each time a picker or a panel is drawn: it answers from what it
/// holds (a device watch's list, a history kept in memory), never reading a
/// disk or a network there.
pub trait PlaceProvider: fmt::Debug + Send + Sync {
    /// Its id: registering another with this id replaces it.
    fn id(&self) -> &str;

    /// Its sections now, in the order shown.
    fn sections(&self) -> Vec<PlaceSection>;
}

/// The place providers declared, as a service of [`Files`](super::Files).
#[derive(Clone, Debug, Default)]
pub struct PlaceProviders(Arc<Mutex<Vec<Arc<dyn PlaceProvider>>>>);

impl PlaceProviders {
    /// Adds a provider, or replaces the one with its id; its sections come
    /// after those of the ones before it. A plugin registering one takes it
    /// back in its `disconnect`.
    pub fn register(&self, provider: Arc<dyn PlaceProvider>) {
        let mut list = lock(&self.0);
        list.retain(|p| p.id() != provider.id());
        list.push(provider);
    }

    /// Takes a provider away; says whether it was there.
    pub fn unregister(&self, id: &str) -> bool {
        let mut list = lock(&self.0);
        let before = list.len();
        list.retain(|p| p.id() != id);
        list.len() != before
    }

    /// The ids of the providers, in their order.
    pub fn ids(&self) -> Vec<String> {
        lock(&self.0).iter().map(|p| p.id().to_string()).collect()
    }

    /// Every provider's sections, in order; a section without items is
    /// left out. A provider that panics gives none, and the log says so.
    pub fn sections(&self) -> Vec<PlaceSection> {
        let list = lock(&self.0).clone();
        let mut out = Vec::new();
        for provider in list {
            match catch_unwind(AssertUnwindSafe(|| provider.sections())) {
                Ok(sections) => out.extend(sections.into_iter().filter(|s| !s.items.is_empty())),
                Err(_) => {
                    log::warn!(target: "egui_pop", "files: the place provider {} panicked", provider.id());
                }
            }
        }
        out
    }
}

/// A section's title, said as a heading.
pub(crate) fn section_title(ui: &mut Ui, title: &str) -> egui::Response {
    let r = ui.weak(title);
    a11y::name(&r, egui::Role::Heading, title);
    r
}

/// An item's row: its icon, its name and its badge, `selected` or not, its
/// detail (else its path) in its tip; named by [`PlaceItem::said`].
pub(crate) fn place_row(ui: &mut Ui, item: &PlaceItem, selected: bool) -> egui::Response {
    let icon = item
        .icon
        .as_deref()
        .map(|name| egui_pop_core::icon::registry::resolve(name, "files.places"));
    let row = |ui: &mut Ui| match &item.badge {
        Some(badge) => ui.add(
            widgets::SelectableRow::new(
                selected,
                (
                    item.name.as_str(),
                    egui::RichText::new(badge.as_str()).small().weak(),
                ),
            )
            .name(item.said()),
        ),
        None => widgets::selectable_label(ui, selected, item.name.as_str()),
    };
    let r = match icon {
        Some(icon) => {
            ui.horizontal(|ui| {
                let side = ui.spacing().interact_size.y;
                let (rect, _) =
                    ui.allocate_exact_size(egui::vec2(side, side), egui::Sense::hover());
                let colour = ui.visuals().text_color();
                icon.unwrap_or(egui_pop_core::icon::PLACEHOLDER).paint(
                    ui.painter(),
                    rect.shrink(side * 0.2),
                    colour,
                );
                row(ui)
            })
            .inner
        }
        None => row(ui),
    };
    let tip = item
        .detail
        .clone()
        .or_else(|| item.path.as_ref().map(|p| p.display().to_string()));
    match tip {
        Some(tip) => r.on_hover_text(tip),
        None => r,
    }
}

/// What a [`PlacesPanel`] did this frame.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct PlacesOutput {
    /// The item chosen: a click, or Enter or Space on it; its section's id
    /// with it.
    pub chosen: Option<(String, PlaceItem)>,
}

/// What draws an item's context menu ([`PlacesPanel::menu`]).
type PlaceMenu<'a> = Box<dyn FnMut(&mut Ui, &PlaceSection, &PlaceItem) + 'a>;

/// A side panel of places: sections, each a heading over its items, as the
/// in-app picker lists them (an import window's sources column). One stop
/// for Tab; Up and Down go from item to item across the sections, Enter or
/// Space chooses. Each item is a button named by its name and badge,
/// selected or not; each title a heading. What a chosen
/// item does is the caller's: [`PlacesOutput::chosen`].
///
/// ```
/// use egui_pop_api::files::places::{PlaceItem, PlaceSection, PlacesPanel};
///
/// fn sources(ui: &mut egui::Ui, at: &mut Option<String>) {
///     let sections = [PlaceSection::new("cards", "Cards")
///         .item(PlaceItem::folder("EOS_DIGITAL", "/media/EOS_DIGITAL").badge("214"))];
///     let out = PlacesPanel::new("import.sources", &sections)
///         .selected(at.as_deref())
///         .show(ui);
///     if let Some((_, item)) = out.chosen {
///         *at = Some(item.id);
///     }
/// }
/// # let _ = sources;
/// ```
///
/// Need (dialog): a side panel of places, with sections of an application's.
pub struct PlacesPanel<'a> {
    name: String,
    sections: &'a [PlaceSection],
    selected: Option<&'a str>,
    menu: Option<PlaceMenu<'a>>,
}

impl fmt::Debug for PlacesPanel<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PlacesPanel")
            .field("name", &self.name)
            .field("sections", &self.sections.len())
            .field("selected", &self.selected)
            .finish_non_exhaustive()
    }
}

impl<'a> PlacesPanel<'a> {
    /// A panel named `name` (its roving group's, unique in the window) of
    /// these sections.
    pub fn new(name: impl Into<String>, sections: &'a [PlaceSection]) -> Self {
        Self {
            name: name.into(),
            sections,
            selected: None,
            menu: None,
        }
    }

    /// The item shown selected, by id: where the caller is.
    #[must_use]
    pub fn selected(mut self, id: Option<&'a str>) -> Self {
        self.selected = id;
        self
    }

    /// A context menu on each item, drawn by `menu` (rename, eject, forget).
    #[must_use]
    pub fn menu(mut self, menu: impl FnMut(&mut Ui, &PlaceSection, &PlaceItem) + 'a) -> Self {
        self.menu = Some(Box::new(menu));
        self
    }

    pub fn show(mut self, ui: &mut Ui) -> PlacesOutput {
        let mut out = PlacesOutput::default();
        let t = Tokens::of(ui.ctx());
        let sections = self.sections;
        let selected = self.selected;
        let menu = &mut self.menu;
        egui_pop_core::tab::roving(ui, &self.name, egui_pop_core::tab::Axis::Vertical, |ui| {
            ui.vertical(|ui| {
                for (n, section) in sections.iter().filter(|s| !s.items.is_empty()).enumerate() {
                    if n > 0 {
                        ui.add_space(t.space.s);
                    }
                    section_title(ui, &section.title);
                    for item in &section.items {
                        let on = selected == Some(item.id.as_str());
                        let r = ui
                            .push_id((&section.id, &item.id), |ui| place_row(ui, item, on))
                            .inner;
                        // Enter and Space click the item that has the keyboard.
                        if r.clicked() {
                            out.chosen = Some((section.id.clone(), item.clone()));
                        }
                        if let Some(menu) = menu.as_mut() {
                            egui_pop_core::pointer::claim::context_menu(&r, |ui| {
                                menu(ui, section, item);
                            });
                        }
                    }
                }
            });
        });
        out
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
