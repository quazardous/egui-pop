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
//! A panel goes further than the picker's list, each part opt-in: a section
//! that folds ([`PlaceSection::foldable`]) or says what it lacks
//! ([`PlaceSection::empty`]); an item with a line shown under its name
//! ([`PlaceItem::line`]), a badge in a tone ([`PlaceItem::badge_tone`]) and
//! a toggle of the application's (a pin, [`PlaceToggle`]); and room under
//! an item or a section for the application's own content
//! ([`PlacesPanel::under_item`], [`PlacesPanel::under_section`]). What the
//! user did comes back in [`PlacesOutput`]: the application keeps the
//! state and acts.
//!
//! [`Files::set_system_places`]: super::Files::set_system_places
//! [`Files::place_providers`]: super::Files::place_providers
//! [`FilesPlugin::places`]: super::FilesPlugin::places

use std::fmt;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use egui::Ui;
use egui_pop_core::fold::{Fold, TreeRow};
use egui_pop_core::icon::{Size, Tone as IconTone};
use egui_pop_core::kit::{Kit, KitCx, Part, State, Tone};
use egui_pop_core::lock;
use egui_pop_core::strings::StrKey;
use egui_pop_core::theme::Tokens;
use egui_pop_core::ui::{UiPop as _, tone_color};
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
    /// The badge's tone (a warning ▲ on an account to reconnect); plain
    /// small text when none.
    pub badge_tone: Option<Tone>,
    /// A line shown under its name, always visible (a path, a card's free
    /// space, "needs reconnecting"): why in a word, on the item itself.
    /// Said as the item's description.
    pub line: Option<String>,
    /// A toggle of the application's at the item's end (a pin):
    /// [`PlacesOutput::toggled`] says it was clicked.
    pub toggle: Option<PlaceToggle>,
    /// The items nested under it (a folder's subfolders, a keyword's
    /// narrower ones), shown indented under it while it is open: a panel
    /// then draws its section as a tree ([`PlaceItem::children`]).
    pub children: Vec<PlaceItem>,
    /// Whether it is open the first time it is shown, when it has
    /// children (no by default).
    pub open_at_first: bool,
    /// Whether it is open, when the application keeps the state: the
    /// panel follows it ([`PlacesOutput::expanded`] says the user asked to
    /// change it). Kept in egui's memory when none.
    pub open: Option<bool>,
    /// Drawn muted (a folder of an offline volume): its name, line, badge
    /// and icon in the muted text role, said to be unavailable. It stays
    /// choosable, its detail still in its tip.
    pub dimmed: bool,
}

/// A toggle at the end of a [`PlaceItem`] (a pin for the favourites): an
/// icon on or off, named for assistive technology. The application owns
/// its state: a click is reported ([`PlacesOutput::toggled`]), and the
/// application gives the item again with the new state.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct PlaceToggle {
    /// An icon of the icon registry (`phosphor:push-pin`).
    pub icon: String,
    /// Whether it is on: drawn in the accent, said as pressed.
    pub on: bool,
    /// Its name, said and in its tip ("Add to favourites").
    pub name: String,
}

impl PlaceToggle {
    pub fn new(icon: impl Into<String>, on: bool, name: impl Into<String>) -> Self {
        Self {
            icon: icon.into(),
            on,
            name: name.into(),
        }
    }
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
            badge_tone: None,
            line: None,
            toggle: None,
            children: Vec::new(),
            open_at_first: false,
            open: None,
            dimmed: false,
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

    /// The badge's tone (see [`PlaceItem::badge_tone`]).
    #[must_use]
    pub fn badge_tone(mut self, tone: Tone) -> Self {
        self.badge_tone = Some(tone);
        self
    }

    /// A line under its name, always shown (see [`PlaceItem::line`]).
    #[must_use]
    pub fn line(mut self, line: impl Into<String>) -> Self {
        self.line = Some(line.into());
        self
    }

    /// A toggle at its end (see [`PlaceToggle`]).
    #[must_use]
    pub fn toggle(mut self, toggle: PlaceToggle) -> Self {
        self.toggle = Some(toggle);
        self
    }

    /// Items nested under it, after those it has: a branch of a tree
    /// (volume, folder, subfolders; `Animals`, `Dog`). A panel indents
    /// them by depth under a disclosure toggle named after the item ("Show
    /// or hide Animals"); Right opens it and Left folds it while the item
    /// has the keyboard, and Up and Down go through the rows shown. Each
    /// item is said at its level, a branch as expanded or not.
    ///
    /// Need (data): a tree of places in a side panel (folders and their subfolders, nested keywords).
    #[must_use]
    pub fn children(mut self, children: impl IntoIterator<Item = PlaceItem>) -> Self {
        self.children.extend(children);
        self
    }

    /// Whether a branch is open the first time it is shown; its state is
    /// then kept in egui's memory (see [`PlaceItem::open`]).
    #[must_use]
    pub fn open_at_first(mut self, open: bool) -> Self {
        self.open_at_first = open;
        self
    }

    /// A branch open or not as the application keeps it: the user's clicks
    /// and arrows come back in [`PlacesOutput::expanded`].
    #[must_use]
    pub fn open(mut self, open: bool) -> Self {
        self.open = Some(open);
        self
    }

    /// Drawn muted and said to be unavailable (see [`PlaceItem::dimmed`]):
    /// an offline volume's folders, still choosable.
    ///
    /// Need (state): a place shown out of reach but still choosable (a folder of an offline volume).
    #[must_use]
    pub fn dimmed(mut self, dimmed: bool) -> Self {
        self.dimmed = dimmed;
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
    /// Whether it folds, and whether it is open the first time it is
    /// shown; a fixed heading when none.
    pub fold: Option<bool>,
    /// Whether it is open, when the application keeps the state: the
    /// panel follows it ([`PlacesOutput::folded`] says the user asked to
    /// change it). Kept in egui's memory when none.
    pub open: Option<bool>,
    /// The line it shows when it has no items ("No cards"): a section
    /// with one is shown empty, one without is left out.
    pub empty: Option<String>,
    /// Shown even without items or an empty line: its content comes from
    /// [`PlacesPanel::under_section`].
    pub keep: bool,
}

impl PlaceSection {
    pub fn new(id: impl Into<String>, title: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            title: title.into(),
            items: Vec::new(),
            fold: None,
            open: None,
            empty: None,
            keep: false,
        }
    }

    /// Folds under its band (egui-pop's [`Fold`]), open at first when
    /// `open_at_first`; its state kept in egui's memory, unless the
    /// application keeps it ([`PlaceSection::open`]).
    #[must_use]
    pub fn foldable(mut self, open_at_first: bool) -> Self {
        self.fold = Some(open_at_first);
        self
    }

    /// Folds, open or not as the application keeps it: the user's clicks
    /// come back in [`PlacesOutput::folded`].
    #[must_use]
    pub fn open(mut self, open: bool) -> Self {
        self.fold.get_or_insert(open);
        self.open = Some(open);
        self
    }

    /// The line shown when it has no items; it is then shown empty.
    #[must_use]
    pub fn empty(mut self, line: impl Into<String>) -> Self {
        self.empty = Some(line.into());
        self
    }

    /// Shown even without items or an empty line (its content drawn by
    /// [`PlacesPanel::under_section`]).
    #[must_use]
    pub fn keep(mut self) -> Self {
        self.keep = true;
        self
    }

    /// Whether a panel shows it: it has items, an empty line or is kept.
    pub fn shown(&self) -> bool {
        !self.items.is_empty() || self.empty.is_some() || self.keep
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
    /// left out, unless it has an empty line or is kept
    /// ([`PlaceSection::shown`]). A provider that panics gives none, and the log says so.
    pub fn sections(&self) -> Vec<PlaceSection> {
        let list = lock(&self.0).clone();
        let mut out = Vec::new();
        for provider in list {
            match catch_unwind(AssertUnwindSafe(|| provider.sections())) {
                Ok(sections) => out.extend(sections.into_iter().filter(PlaceSection::shown)),
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
    /// The item whose toggle was clicked ([`PlaceItem::toggle`]), its
    /// section's id with it: the application flips its state.
    pub toggled: Option<(String, PlaceItem)>,
    /// The foldable section the user folded or opened, by id, and whether
    /// it is open now.
    pub folded: Option<(String, bool)>,
    /// The nested item ([`PlaceItem::children`]) the user opened or
    /// folded, by id, and whether it is open now.
    pub expanded: Option<(String, bool)>,
}

/// What draws an item's context menu ([`PlacesPanel::menu`]), or the
/// application's content under an item ([`PlacesPanel::under_item`]).
type PlaceMenu<'a> = Box<dyn FnMut(&mut Ui, &PlaceSection, &PlaceItem) + 'a>;

/// What draws the application's content under a section's items
/// ([`PlacesPanel::under_section`]).
type SectionSlot<'a> = Box<dyn FnMut(&mut Ui, &PlaceSection) + 'a>;

/// What sees each item's row ([`PlacesPanel::on_row`]).
type RowHook<'a> = Box<dyn FnMut(&mut Ui, &PlaceSection, &PlaceItem, &egui::Response) + 'a>;

/// A side panel of places: sections, each a heading over its items, as the
/// in-app picker lists them (an import window's sources column). One stop
/// for Tab; Up and Down go from item to item across the sections (through
/// an item's toggle, a fold's band and the application's content, in the
/// order drawn), Enter or Space chooses. Each item is a button named by its
/// name and badge, described by its line, selected or not; each title a
/// heading, or a fold's band when the section folds. What a chosen item
/// does is the caller's: [`PlacesOutput::chosen`].
///
/// Opt-in parts (see the module): folding sections, an empty section's
/// line, an item's visible line, toned badge and toggle, the
/// application's content under an item or a section, nested items
/// ([`PlaceItem::children`]), dimmed items ([`PlaceItem::dimmed`]), a hook
/// on each row ([`PlacesPanel::on_row`]) and a selection in several
/// sections ([`PlacesPanel::selected_set`]). A panel using none of them
/// looks as it always did.
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
    selected_set: &'a [&'a str],
    menu: Option<PlaceMenu<'a>>,
    under_item: Option<PlaceMenu<'a>>,
    under_section: Option<SectionSlot<'a>>,
    on_row: Option<RowHook<'a>>,
}

impl fmt::Debug for PlacesPanel<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PlacesPanel")
            .field("name", &self.name)
            .field("sections", &self.sections.len())
            .field("selected", &self.selected)
            .field("selected_set", &self.selected_set)
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
            selected_set: &[],
            menu: None,
            under_item: None,
            under_section: None,
            on_row: None,
        }
    }

    /// The item shown selected, by id: where the caller is.
    #[must_use]
    pub fn selected(mut self, id: Option<&'a str>) -> Self {
        self.selected = id;
        self
    }

    /// The items shown selected, by id, beside [`PlacesPanel::selected`]:
    /// one in each section where the application's scopes add up (a
    /// folder, an album and a keyword chosen together).
    ///
    /// Need (data): a side panel of places with one selection in each section (scopes chosen together).
    #[must_use]
    pub fn selected_set(mut self, ids: &'a [&'a str]) -> Self {
        self.selected_set = ids;
        self
    }

    /// A hook called with each item's row and its response, after the
    /// panel's own handling: the application attaches what the panel
    /// knows nothing of (a drop target or a drag source through the drag
    /// session, `bus::drag`; its own context menu; a tip). Called for
    /// nested items too.
    ///
    /// Need (dialog): a side panel of places whose rows are drop targets or drag sources.
    #[must_use]
    pub fn on_row(
        mut self,
        hook: impl FnMut(&mut Ui, &PlaceSection, &PlaceItem, &egui::Response) + 'a,
    ) -> Self {
        self.on_row = Some(Box::new(hook));
        self
    }

    /// A context menu on each item, drawn by `menu` (rename, eject, forget).
    #[must_use]
    pub fn menu(mut self, menu: impl FnMut(&mut Ui, &PlaceSection, &PlaceItem) + 'a) -> Self {
        self.menu = Some(Box::new(menu));
        self
    }

    /// The application's content under each item, drawn by `add` in the
    /// room below it (an account's albums under the account): `add` draws
    /// nothing for an item without any. Its widgets join the panel's
    /// keyboard order after the item.
    #[must_use]
    pub fn under_item(mut self, add: impl FnMut(&mut Ui, &PlaceSection, &PlaceItem) + 'a) -> Self {
        self.under_item = Some(Box::new(add));
        self
    }

    /// The application's content under each section's items, drawn by
    /// `add` (a history's past imports, each with "Import again"), inside
    /// the section's fold when it folds. Its widgets join the panel's
    /// keyboard order after the section's items.
    #[must_use]
    pub fn under_section(mut self, add: impl FnMut(&mut Ui, &PlaceSection) + 'a) -> Self {
        self.under_section = Some(Box::new(add));
        self
    }

    pub fn show(mut self, ui: &mut Ui) -> PlacesOutput {
        let mut out = PlacesOutput::default();
        let t = Tokens::of(ui.ctx());
        let sections = self.sections;
        let name = self.name.clone();
        egui_pop_core::tab::roving(ui, &name, egui_pop_core::tab::Axis::Vertical, |ui| {
            ui.vertical(|ui| {
                for (n, section) in sections.iter().filter(|s| s.shown()).enumerate() {
                    if n > 0 {
                        ui.add_space(t.space.s);
                    }
                    match section.fold {
                        None => {
                            section_title(ui, &section.title);
                            self.section_body(ui, section, &mut out);
                        }
                        Some(at_first) => {
                            let open_wanted = section.open;
                            let fold = Fold::new((&name, &section.id), section.title.as_str())
                                .default_open(open_wanted.unwrap_or(at_first))
                                .show(ui, |ui| self.section_body(ui, section, &mut out));
                            if fold.toggled {
                                out.folded = Some((section.id.clone(), fold.open));
                            } else if let Some(want) = open_wanted
                                && want != fold.open
                            {
                                // The application keeps the state: follow it.
                                egui_pop_core::fold::set_open(ui.ctx(), fold.id, want);
                            }
                        }
                    }
                }
            });
        });
        out
    }

    /// A section's items, its empty line, and the application's content.
    fn section_body(&mut self, ui: &mut Ui, section: &PlaceSection, out: &mut PlacesOutput) {
        if section.items.is_empty()
            && let Some(empty) = &section.empty
        {
            ui.meta(empty.as_str());
        }
        // A section with nested items is drawn as a tree, every row at its
        // depth; one without is drawn as it always was.
        let tree = section.items.iter().any(|i| !i.children.is_empty());
        for item in &section.items {
            self.item(ui, section, item, tree.then_some(0), out);
        }
        if let Some(add) = self.under_section.as_mut() {
            ui.push_id(("under", &section.id), |ui| add(ui, section));
        }
    }

    /// Whether an item is drawn selected.
    fn is_selected(&self, id: &str) -> bool {
        self.selected == Some(id) || self.selected_set.contains(&id)
    }

    /// An item's row, at `depth` in a tree section (`None` in a flat
    /// one), its menu, hook and content, then its children while open.
    fn item(
        &mut self,
        ui: &mut Ui,
        section: &PlaceSection,
        item: &PlaceItem,
        depth: Option<usize>,
        out: &mut PlacesOutput,
    ) {
        let on = self.is_selected(&item.id);
        let ctx = ui.ctx().clone();
        let open_id = ui.make_persistent_id(("place.open", &section.id, &item.id));
        ui.push_id((&section.id, &item.id), |ui| {
            let (r, toggled, open) = match depth {
                None => {
                    let (r, toggled) = item_row(ui, item, on);
                    (r, toggled, false)
                }
                Some(depth) => {
                    let branch = !item.children.is_empty();
                    if let Some(want) = item.open
                        && branch
                        && egui_pop_core::fold::is_open(&ctx, open_id) != Some(want)
                    {
                        // The application keeps the state: follow it.
                        egui_pop_core::fold::set_open(&ctx, open_id, want);
                    }
                    let shown = TreeRow::new(open_id, item.name.clone())
                        .depth(depth)
                        .leaf(!branch)
                        .default_open(item.open.unwrap_or(item.open_at_first))
                        .show(ui, |ui| item_row(ui, item, on));
                    let (r, toggled) = shown.row.inner.clone();
                    let mut open = shown.open;
                    let mut expanded = shown.toggled;
                    shown.keep(&ctx);
                    // Right opens and Left folds a branch from its row too,
                    // as a tree's arrows do.
                    if branch && r.has_focus() {
                        ui.memory_mut(|m| {
                            m.set_focus_lock_filter(
                                r.id,
                                egui::EventFilter {
                                    horizontal_arrows: true,
                                    ..Default::default()
                                },
                            );
                        });
                        let key = if open {
                            egui::Key::ArrowLeft
                        } else {
                            egui::Key::ArrowRight
                        };
                        if ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, key)) {
                            open = !open;
                            expanded = true;
                            egui_pop_core::fold::set_open(&ctx, open_id, open);
                        }
                    }
                    ctx.accesskit_node_builder(r.id, |node| {
                        node.set_level(depth + 1);
                        if branch {
                            node.set_expanded(open);
                        }
                    });
                    if expanded {
                        out.expanded = Some((item.id.clone(), open));
                    }
                    (r, toggled, open)
                }
            };
            // Enter and Space click the item that has the keyboard.
            if r.clicked() {
                out.chosen = Some((section.id.clone(), item.clone()));
            }
            if toggled {
                out.toggled = Some((section.id.clone(), item.clone()));
            }
            if let Some(menu) = self.menu.as_mut() {
                egui_pop_core::pointer::claim::context_menu(&r, |ui| {
                    menu(ui, section, item);
                });
            }
            if let Some(hook) = self.on_row.as_mut() {
                hook(ui, section, item, &r);
            }
            if let Some(add) = self.under_item.as_mut() {
                add(ui, section, item);
            }
            if open && let Some(depth) = depth {
                for child in &item.children {
                    self.item(ui, section, child, Some(depth + 1), out);
                }
            }
        });
    }
}

/// An item of a panel: the picker's row ([`place_row`]) when it has no
/// line and no toggle, as panels always drew it; else a row of one fixed
/// height with its line under its name and its toggle at its end. Says
/// whether its toggle was clicked.
fn item_row(ui: &mut Ui, item: &PlaceItem, selected: bool) -> (egui::Response, bool) {
    if item.line.is_none() && item.toggle.is_none() && !item.dimmed {
        return (place_row(ui, item, selected), false);
    }
    let ctx = ui.ctx().clone();
    let t = Tokens::of(&ctx);
    let kit = Kit::of(&ctx);
    let component = &kit.selectable_row;
    let body = egui::TextStyle::Body.resolve(ui.style());
    let small = egui::FontId::proportional(t.text_role.meta.size.max(4.0));
    let room = component.border_room(ctx.pixels_per_point());
    let pad = ui.spacing().button_padding.max(egui::Vec2::splat(room));
    let gap = ui.spacing().item_spacing.x;
    let line_h = if item.line.is_some() {
        ui.fonts_mut(|f| f.row_height(&small))
    } else {
        0.0
    };
    let height = (ui.fonts_mut(|f| f.row_height(&body)) + line_h + 2.0 * pad.y)
        .max(ui.spacing().interact_size.y);
    let toggle_side = Size::Inline.points(&t) + 2.0 * room;
    let width = ui.available_width().max(toggle_side * 3.0);
    let (rect, r) = ui.allocate_exact_size(egui::vec2(width, height), egui::Sense::click());
    let enabled = ui.is_enabled();
    let said = item.said();
    r.widget_info(|| egui::WidgetInfo::selected(egui::Role::Button, enabled, selected, &said));
    let description = match (&item.line, item.dimmed) {
        (Some(line), false) => Some(line.clone()),
        (line, true) => {
            let word = egui_pop_core::strings::tr(&ctx, &txt::FILES_PLACE_DIMMED);
            Some(match line {
                Some(line) => format!("{word}, {line}"),
                None => word,
            })
        }
        (None, false) => None,
    };
    if let Some(description) = description {
        ctx.accesskit_node_builder(r.id, |node| node.set_description(description));
    }
    if let Some(cursor) = ui.visuals().interact_cursor
        && r.hovered()
    {
        ctx.set_cursor_icon(cursor);
    }
    // The toggle's room at the end, outside the row's own click; a little
    // below the row's middle, so that the keyboard, which goes by place,
    // reaches it after its item.
    let icon_side = Size::Inline.points(&t);
    let toggle_rect = item.toggle.as_ref().map(|_| {
        egui::Rect::from_center_size(
            egui::pos2(
                rect.right() - pad.x - toggle_side / 2.0,
                rect.center().y + (pad.y / 2.0).max(1.0),
            ),
            egui::Vec2::splat(icon_side),
        )
    });
    if ui.is_rect_visible(rect) {
        // The chosen row is never `hover`: the choice stays under the
        // pointer, as a selectable row's.
        let state = State::from_response(&r)
            .with(State::SELECTED, selected)
            .with(State::DISABLED, !enabled);
        let state = if selected {
            state.without(State::HOVER)
        } else {
            state
        };
        let cx = KitCx {
            painter: ui.painter(),
            ppp: ctx.pixels_per_point(),
        };
        component.paint(
            &cx,
            &Part {
                rect,
                room: rect,
                state,
                label: None,
            },
        );
        let muted = item.dimmed && !selected;
        let text = if muted {
            t.text_role.muted.color
        } else {
            ui.visuals()
                .override_text_color
                .unwrap_or(component.style(state).text)
        };
        let painter = ui.painter_at(rect);
        let mut left = rect.left() + pad.x;
        let right = toggle_rect.map_or(rect.right() - pad.x, |_| {
            rect.right() - pad.x - toggle_side - gap
        });
        if let Some(name) = &item.icon {
            let side = ui.spacing().interact_size.y;
            let at = egui::Rect::from_min_size(
                egui::pos2(left, rect.center().y - side / 2.0),
                egui::Vec2::splat(side),
            );
            egui_pop_core::icon::registry::resolve(name, "files.places")
                .unwrap_or(egui_pop_core::icon::PLACEHOLDER)
                .paint(&painter, at.shrink(side * 0.2), text);
            left = at.right() + gap;
        }
        let one_line = |s: &str, font: egui::FontId, colour: egui::Color32, max: f32| {
            let mut job = egui::text::LayoutJob::simple_singleline(s.to_owned(), font, colour);
            job.wrap = egui::text::TextWrapping::truncate_at_width(max.max(0.0));
            ui.fonts_mut(|f| f.layout_job(job))
        };
        let top = rect.top() + pad.y;
        let name = one_line(&item.name, body.clone(), text, right - left);
        let name_h = name.size().y;
        let name_w = name.size().x;
        painter.galley(egui::pos2(left, top), name, text);
        let badge_colour = |tone: Option<Tone>| match tone {
            _ if muted => text,
            Some(tone) => tone_color(&t, tone),
            None if selected => text,
            None => t.text_role.meta.color,
        };
        let line_colour = if selected || muted {
            text
        } else {
            t.text_role.meta.color
        };
        match &item.line {
            // The badge leads the line, as a state mark.
            Some(line) => {
                let y = top + name_h;
                let mut x = left;
                if let Some(badge) = &item.badge {
                    let g = one_line(
                        badge,
                        small.clone(),
                        badge_colour(item.badge_tone),
                        right - x,
                    );
                    let w = g.size().x;
                    painter.galley(egui::pos2(x, y), g, text);
                    x += w + gap;
                }
                let g = one_line(line, small.clone(), line_colour, right - x);
                painter.galley(egui::pos2(x, y), g, text);
            }
            None => {
                if let Some(badge) = &item.badge {
                    let x = left + name_w + gap;
                    let g = one_line(
                        badge,
                        small.clone(),
                        badge_colour(item.badge_tone),
                        right - x,
                    );
                    let y = top + (name_h - g.size().y) / 2.0;
                    painter.galley(egui::pos2(x, y), g, text);
                }
            }
        }
    }
    let r = match item
        .detail
        .clone()
        .or_else(|| item.path.as_ref().map(|p| p.display().to_string()))
    {
        Some(tip) => r.on_hover_text(tip),
        None => r,
    };
    let mut toggled = false;
    if let (Some(toggle), Some(at)) = (&item.toggle, toggle_rect) {
        let icon = egui_pop_core::icon::registry::resolve(&toggle.icon, "files.places")
            .unwrap_or(egui_pop_core::icon::PLACEHOLDER);
        let tone = if toggle.on {
            IconTone::Accent
        } else {
            IconTone::Muted
        };
        let b = ui.put(
            at,
            icon.size(Size::Inline)
                .tone(tone)
                .label(&toggle.name)
                .selected(toggle.on),
        );
        toggled = b.clicked();
    }
    (r, toggled)
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
