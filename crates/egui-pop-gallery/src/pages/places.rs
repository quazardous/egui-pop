//! The places panel page: a `PlacesPanel` as an import window's sources
//! column, with every opt-in part. Sections fold (Favourites, Folders,
//! Cards, Services, History), an empty one says what it lacks ("No
//! cards"), each item shows a line under its name (a path, an account's
//! state), an account's state mark has a tone (a warning !), a pin at
//! each item's end adds it to Favourites, the chosen account shows its
//! albums under it, and History's past imports sit under the section,
//! each with "Import again". History's fold is kept by the page, the
//! others by egui's memory. The playground shows what the panel last
//! reported.
//!
//! Beside it, a Library's sources on the same panel: Folders and Keywords
//! are trees (nested items, the disk open at first, Animals kept by the
//! page), the offline backup disk and its folders are dimmed, a folder,
//! an album and a keyword are chosen together (a selected set), and the
//! albums take a photo dragged from the chip above them (a hook on each
//! row, through the drag session).

use egui::Ui;
use egui_pop::api::bus::drag;
use egui_pop::api::files::places::{PlaceItem, PlaceSection, PlaceToggle, PlacesPanel};
use egui_pop::core::kit::Tone;
use egui_pop::core::ui::{Space, UiPop};
use egui_pop::core::widgets;

/// The pretend column's width.
const WIDTH: f32 = 260.0;

/// The page's state.
#[derive(Debug)]
pub struct State {
    /// The item chosen, by id.
    pub at: Option<String>,
    /// The items pinned to Favourites, by id.
    pub pinned: Vec<String>,
    /// Whether History is open: the page keeps it.
    pub history_open: bool,
    /// The album chosen under the account, by name.
    pub album: Option<String>,
    /// What the panel last reported.
    pub last: String,
    /// The Library's folder, album and keyword chosen, each its scope's.
    pub scopes: [Option<String>; 3],
    /// Whether the keyword Animals is open: the page keeps it.
    pub animals_open: bool,
    /// The photos dropped on each album, by album id.
    pub dropped: Vec<(String, String)>,
}

impl Default for State {
    fn default() -> Self {
        Self {
            at: Some("ana".into()),
            pinned: vec!["/home/ana/Pictures".into()],
            history_open: true,
            album: None,
            last: "Nothing yet".into(),
            scopes: [
                Some("disk/2026".into()),
                Some("album/portraits".into()),
                Some("Animals|Dog".into()),
            ],
            animals_open: true,
            dropped: Vec::new(),
        }
    }
}

/// The folders: a name and its path.
const FOLDERS: [(&str, &str); 2] = [("Pictures", "/home/ana/Pictures"), ("Scans", "/data/scans")];

/// The past imports History lists under its folders.
const IMPORTS: [&str; 2] = ["12 November, 214 photos", "3 November, 48 photos"];

/// The albums of the account Ana.
const ALBUMS: [&str; 2] = ["Summer", "Family"];

fn pin(state: &State, id: &str) -> PlaceToggle {
    let on = state.pinned.iter().any(|p| p == id);
    let name = if on {
        "Remove from favourites"
    } else {
        "Add to favourites"
    };
    PlaceToggle::new("phosphor:push-pin", on, name)
}

fn folder(state: &State, name: &str, path: &str) -> PlaceItem {
    let parent = std::path::Path::new(path)
        .parent()
        .map(|p| p.display().to_string())
        .unwrap_or_default();
    PlaceItem::folder(name, path)
        .icon("phosphor:folder")
        .line(parent)
        .toggle(pin(state, path))
}

fn accounts(state: &State) -> Vec<PlaceItem> {
    vec![
        PlaceItem::new("ana", "Ana's photos")
            .icon("phosphor:cloud")
            .line("ana@example.org")
            .toggle(pin(state, "ana")),
        PlaceItem::new("studio", "Studio")
            .icon("phosphor:cloud")
            .badge("!")
            .badge_tone(Tone::Warning)
            .line("needs reconnecting")
            .toggle(pin(state, "studio")),
    ]
}

/// The sections, from the page's state.
fn sections(state: &State) -> Vec<PlaceSection> {
    let folders: Vec<PlaceItem> = FOLDERS.iter().map(|(n, p)| folder(state, n, p)).collect();
    let all: Vec<PlaceItem> = folders.iter().cloned().chain(accounts(state)).collect();
    let favourites = all.iter().filter(|i| state.pinned.contains(&i.id)).cloned();
    vec![
        PlaceSection::new("favourites", "Favourites")
            .foldable(true)
            .empty("No favourites")
            .items(favourites),
        PlaceSection::new("history", "History")
            .open(state.history_open)
            .keep()
            .item(
                folder(state, "Pictures", "/home/ana/Pictures")
                    .line("Last import 2026-11-12 09:30"),
            ),
        PlaceSection::new("folders", "Folders")
            .foldable(true)
            .items(folders),
        PlaceSection::new("cards", "Cards and cameras")
            .foldable(true)
            .empty("No cards"),
        PlaceSection::new("services", "Services")
            .foldable(true)
            .items(accounts(state)),
    ]
}

/// A photo dragged from the page's chip onto an album.
#[derive(Debug)]
struct Photo(&'static str);

/// The Library's scopes, in the order of [`State::scopes`].
const SCOPES: [&str; 3] = ["folders", "albums", "keywords"];

/// The Library's sources: two trees, a dimmed volume, albums to drop on.
fn library_sections(state: &State) -> Vec<PlaceSection> {
    let count = |id: &str| state.dropped.iter().filter(|(a, _)| a == id).count();
    let album = |id: &str, name: &str| {
        let item = PlaceItem::new(id, name).icon("phosphor:images");
        match count(id) {
            0 => item,
            n => item.badge(format!("+{n}")),
        }
    };
    vec![
        PlaceSection::new("folders", "Folders")
            .item(
                PlaceItem::new("disk", "Photos disk")
                    .icon("phosphor:hard-drives")
                    .open_at_first(true)
                    .children([
                        PlaceItem::new("disk/2025", "2025"),
                        PlaceItem::new("disk/2026", "2026")
                            .open_at_first(true)
                            .children([PlaceItem::new("disk/2026/trip", "Summer trip")]),
                    ]),
            )
            .item(
                PlaceItem::new("backup", "Backup disk")
                    .icon("phosphor:hard-drives")
                    .line("offline, last seen 3 days ago")
                    .dimmed(true)
                    .open_at_first(true)
                    .children([PlaceItem::new("backup/scans", "Old scans").dimmed(true)]),
            ),
        PlaceSection::new("albums", "Albums")
            .item(album("album/portraits", "Portraits"))
            .item(album("album/harbour", "Harbour")),
        PlaceSection::new("keywords", "Keywords").item(
            PlaceItem::new("Animals", "Animals")
                .open(state.animals_open)
                .children([
                    PlaceItem::new("Animals|Cat", "Cat"),
                    PlaceItem::new("Animals|Dog", "Dog"),
                ]),
        ),
    ]
}

/// The Library's column: a photo to drag, then its sources.
fn library(ui: &mut Ui, state: &mut State) {
    ui.muted("A Library's sources: trees, a dimmed offline disk, one choice per scope.");
    let chip = ui.add(
        egui::Button::new("Photo harbour.jpg, drag it onto an album").sense(egui::Sense::drag()),
    );
    if chip.drag_started() {
        drag::start(ui.ctx(), "photos", Photo("harbour.jpg"));
    }
    let sections = library_sections(state);
    let set: Vec<&str> = state.scopes.iter().flatten().map(String::as_str).collect();
    let mut dropped = None;
    let out = PlacesPanel::new("gallery.places.library", &sections)
        .selected_set(&set)
        .on_row(|ui, section, item, r| {
            // The albums take a photo: the page's own behaviour, on the row.
            if section.id != "albums" {
                return;
            }
            match drag::target::<Photo>(ui.ctx(), &item.id, r.rect, |_| None) {
                drag::Seen::Over { .. } | drag::Seen::Unknown(_) => {
                    let accent = ui.visuals().selection.stroke.color;
                    ui.painter().rect_stroke(
                        r.rect,
                        2.0,
                        egui::Stroke::new(2.0, accent),
                        egui::StrokeKind::Inside,
                    );
                }
                drag::Seen::Dropped(photo) => dropped = Some((item.id.clone(), photo.0)),
                _ => {}
            }
        })
        .show(ui);
    if let Some((album, photo)) = dropped {
        state.last = format!("dropped {photo} on {album}");
        state.dropped.push((album, photo.to_owned()));
    }
    if let Some((section, item)) = out.chosen
        && let Some(n) = SCOPES.iter().position(|s| *s == section)
    {
        state.last = format!("scoped {section} to {}", item.name);
        state.scopes[n] = Some(item.id);
    }
    if let Some((id, open)) = out.expanded {
        if id == "Animals" {
            state.animals_open = open;
        }
        state.last = format!("{} {id}", if open { "opened" } else { "folded" });
    }
}

pub fn playground(ui: &mut Ui, state: &mut State) {
    ui.label(format!("Last: {}", state.last));
    ui.label(format!("Favourites: {}", state.pinned.join(", ")));
    if ui.button("Toggle History").clicked() {
        state.history_open = !state.history_open;
    }
}

pub fn show(ui: &mut Ui, state: &mut State) {
    ui.muted(
        "An import window's sources: sections fold, items show why in a word, pins add favourites.",
    );
    ui.space_role(Space::Group);
    let sections = sections(state);
    let mut again = None;
    let mut album = None;
    ui.horizontal_top(|ui| {
        ui.allocate_ui(egui::vec2(WIDTH, ui.available_height()), |ui| {
            ui.set_width(WIDTH);
            let chosen_album = state.album.clone();
            let at = state.at.clone();
            let out = PlacesPanel::new("gallery.places.sources", &sections)
                .selected(if chosen_album.is_some() {
                    None
                } else {
                    at.as_deref()
                })
                .under_item(|ui, _, item| {
                    // The chosen account's albums, under it.
                    if item.id != "ana" || at.as_deref() != Some("ana") {
                        return;
                    }
                    ui.indent("albums", |ui| {
                        for name in ALBUMS {
                            let on = chosen_album.as_deref() == Some(name);
                            if widgets::selectable_label(ui, on, name).clicked() {
                                album = Some(name);
                            }
                        }
                    });
                })
                .under_section(|ui, section| {
                    if section.id != "history" {
                        return;
                    }
                    for (n, import) in IMPORTS.iter().enumerate() {
                        ui.horizontal(|ui| {
                            ui.meta(*import);
                            if ui.button("Import again").clicked() {
                                again = Some(n);
                            }
                        });
                    }
                })
                .show(ui);
            if let Some((section, item)) = out.chosen {
                state.last = format!("chose {} in {section}", item.name);
                state.at = Some(item.id);
                state.album = None;
            }
            if let Some((_, item)) = out.toggled {
                if let Some(n) = state.pinned.iter().position(|p| *p == item.id) {
                    state.pinned.remove(n);
                    state.last = format!("unpinned {}", item.name);
                } else {
                    state.pinned.push(item.id.clone());
                    state.last = format!("pinned {}", item.name);
                }
            }
            if let Some((section, open)) = out.folded {
                if section == "history" {
                    state.history_open = open;
                }
                state.last = format!("{} {section}", if open { "opened" } else { "folded" });
            }
        });
        ui.space_role(Space::Group);
        ui.allocate_ui(egui::vec2(WIDTH, ui.available_height()), |ui| {
            ui.set_width(WIDTH);
            ui.vertical(|ui| library(ui, state));
        });
    });
    if let Some(name) = album {
        state.album = Some(name.to_owned());
        state.last = format!("chose the album {name}");
    }
    if let Some(n) = again {
        state.last = format!("import again: {}", IMPORTS[n]);
    }
}
