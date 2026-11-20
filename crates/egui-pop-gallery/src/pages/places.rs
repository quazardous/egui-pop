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

use egui::Ui;
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
}

impl Default for State {
    fn default() -> Self {
        Self {
            at: Some("ana".into()),
            pinned: vec!["/home/ana/Pictures".into()],
            history_open: true,
            album: None,
            last: "Nothing yet".into(),
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
    if let Some(name) = album {
        state.album = Some(name.to_owned());
        state.last = format!("chose the album {name}");
    }
    if let Some(n) = again {
        state.last = format!("import again: {}", IMPORTS[n]);
    }
}
