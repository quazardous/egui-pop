//! A panel of places on its own: an application's sections, named and
//! driven by the keyboard.

use egui::Key;
use egui::accesskit::{Role, Toggled};
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT as _, Queryable as _};
use egui_pop_api::files::places::{PlaceItem, PlaceSection, PlacesPanel};

#[derive(Default)]
struct Panel {
    sections: Vec<PlaceSection>,
    at: Option<String>,
    chosen: Vec<(String, String)>,
}

fn sections() -> Vec<PlaceSection> {
    vec![
        PlaceSection::new("cards", "Cards")
            .item(PlaceItem::folder("EOS_DIGITAL", "/media/EOS_DIGITAL").badge("214"))
            .item(PlaceItem::folder("SD_CARD", "/media/SD_CARD")),
        PlaceSection::new("services", "Services").item(
            PlaceItem::new("photos", "Photo service")
                .icon("phosphor:cloud")
                .detail("Signed in"),
        ),
        PlaceSection::new("empty", "Nothing"),
    ]
}

fn harness() -> Harness<'static, Panel> {
    let mut h = Harness::builder()
        .with_size(egui::vec2(240.0, 320.0))
        .build_ui_state(
            |ui, p: &mut Panel| {
                let out = PlacesPanel::new("test.places", &p.sections)
                    .selected(p.at.as_deref())
                    .show(ui);
                if let Some((section, item)) = out.chosen {
                    p.at = Some(item.id.clone());
                    p.chosen.push((section, item.id));
                }
            },
            Panel {
                sections: sections(),
                ..Panel::default()
            },
        );
    h.run();
    h
}

#[test]
fn sections_are_headings_and_items_buttons_named_with_their_badges() {
    let mut h = harness();
    assert_eq!(
        h.get_by_label("Cards").accesskit_node().role(),
        Role::Heading
    );
    assert_eq!(
        h.get_by_label("Services").accesskit_node().role(),
        Role::Heading
    );
    assert!(
        h.query_by_label("Nothing").is_none(),
        "an empty section is left out"
    );
    let card = h.get_by_label("EOS_DIGITAL, 214");
    assert_eq!(card.accesskit_node().role(), Role::Button);
    card.click();
    h.run();
    assert_eq!(
        h.state().chosen,
        [("cards".into(), "/media/EOS_DIGITAL".into())]
    );
    assert_eq!(
        h.get_by_label("EOS_DIGITAL, 214")
            .accesskit_node()
            .toggled(),
        Some(Toggled::True),
        "the item chosen is shown selected"
    );
    // An item that is no folder is the application's to act on.
    h.get_by_label("Photo service").click();
    h.run();
    assert_eq!(h.state().chosen[1], ("services".into(), "photos".into()));
}

#[test]
fn the_arrows_go_from_item_to_item_across_sections_and_enter_chooses() {
    let mut h = harness();
    h.get_by_label("EOS_DIGITAL, 214").focus();
    h.run();
    h.key_press(Key::ArrowDown);
    h.run();
    h.key_press(Key::ArrowDown);
    h.run();
    assert!(h.get_by_label("Photo service").is_focused());
    h.key_press(Key::Enter);
    h.run();
    assert_eq!(h.state().chosen, [("services".into(), "photos".into())]);
    // Back up, across the section's heading.
    h.key_press(Key::ArrowUp);
    h.run();
    assert!(h.get_by_label("SD_CARD").is_focused());
}

/// What the rich panel reported, frame by frame.
#[derive(Default)]
struct Rich {
    pinned: bool,
    open: bool,
    toggled: Vec<String>,
    folded: Vec<(String, bool)>,
    again: u32,
}

fn rich_sections(r: &Rich) -> Vec<PlaceSection> {
    use egui_pop_api::files::places::PlaceToggle;
    use egui_pop_core::kit::Tone;
    let name = if r.pinned {
        "Remove from favourites"
    } else {
        "Add to favourites"
    };
    vec![
        PlaceSection::new("cards", "Cards")
            .foldable(true)
            .empty("No cards"),
        PlaceSection::new("services", "Services")
            .foldable(true)
            .item(
                PlaceItem::new("studio", "Studio")
                    .badge("!")
                    .badge_tone(Tone::Warning)
                    .line("needs reconnecting")
                    .toggle(PlaceToggle::new("phosphor:push-pin", r.pinned, name)),
            ),
        PlaceSection::new("history", "History").open(r.open).keep(),
    ]
}

fn rich() -> Harness<'static, Rich> {
    let mut h = Harness::builder()
        .with_size(egui::vec2(260.0, 360.0))
        .build_ui_state(
            |ui, r: &mut Rich| {
                let sections = rich_sections(r);
                let mut again = false;
                let out = PlacesPanel::new("test.rich", &sections)
                    .under_item(|ui, _, item| {
                        if item.id == "studio" {
                            ui.label("Studio's albums");
                        }
                    })
                    .under_section(|ui, section| {
                        if section.id == "history" {
                            again = ui.button("Import again").clicked();
                        }
                    })
                    .show(ui);
                if again {
                    r.again += 1;
                }
                if let Some((_, item)) = out.toggled {
                    r.pinned = !r.pinned;
                    r.toggled.push(item.id);
                }
                if let Some((id, open)) = out.folded {
                    if id == "history" {
                        r.open = open;
                    }
                    r.folded.push((id, open));
                }
            },
            Rich {
                open: true,
                ..Rich::default()
            },
        );
    h.run();
    h
}

#[test]
fn an_empty_section_says_what_it_lacks_and_a_kept_one_shows_the_apps_content() {
    let mut h = rich();
    assert!(h.query_by_label("No cards").is_some());
    assert!(h.query_by_label("Studio's albums").is_some());
    h.get_by_label("Import again").click();
    h.run();
    assert_eq!(h.state().again, 1);
}

#[test]
fn an_item_shows_its_line_said_as_its_description_and_its_badge_with_its_name() {
    let h = rich();
    let item = h.get_by_label("Studio, !");
    assert_eq!(item.accesskit_node().role(), Role::Button);
    assert_eq!(
        item.accesskit_node().data().description(),
        Some("needs reconnecting")
    );
}

#[test]
fn an_items_toggle_is_named_pressed_or_not_and_reported() {
    let mut h = rich();
    let pin = h.get_by_label("Add to favourites");
    assert_eq!(pin.accesskit_node().role(), Role::Button);
    assert_eq!(pin.accesskit_node().toggled(), Some(Toggled::False));
    pin.click();
    h.run();
    h.run();
    assert_eq!(h.state().toggled, ["studio"]);
    assert_eq!(
        h.get_by_label("Remove from favourites")
            .accesskit_node()
            .toggled(),
        Some(Toggled::True)
    );
}

#[test]
fn sections_fold_by_their_band_and_the_apps_state_is_followed() {
    let mut h = rich();
    let expanded = |h: &Harness<'_, Rich>, label: &str| {
        h.get_by_label(label).accesskit_node().data().is_expanded()
    };
    assert_eq!(expanded(&h, "Cards"), Some(true));
    h.get_by_label("Cards").click();
    h.run();
    assert_eq!(expanded(&h, "Cards"), Some(false));
    assert!(h.query_by_label("No cards").is_none());
    assert_eq!(h.state().folded, [("cards".into(), false)]);
    // History's state is the application's: folded by a click through
    // the panel, then opened by the application alone.
    h.get_by_label("History").click();
    h.run();
    assert!(!h.state().open);
    assert!(h.query_by_label("Import again").is_none());
    h.state_mut().open = true;
    h.run();
    h.run();
    assert_eq!(expanded(&h, "History"), Some(true));
    assert!(h.query_by_label("Import again").is_some());
}

/// What the Library's panel reported, frame by frame.
#[derive(Default)]
struct Library {
    /// The app-kept state of "Animals".
    animals_open: bool,
    expanded: Vec<(String, bool)>,
    chosen: Vec<String>,
    /// Every row the hook saw last frame: its id and rect.
    rows: Vec<(String, egui::Rect)>,
    /// The ids drawn selected through the set.
    set: Vec<&'static str>,
}

fn library_sections(l: &Library) -> Vec<PlaceSection> {
    vec![
        PlaceSection::new("folders", "Folders")
            .item(
                PlaceItem::new("disk", "Photos disk")
                    .open_at_first(true)
                    .children([PlaceItem::new("2026", "2026")
                        .children([PlaceItem::new("2026/summer", "Summer trip")])]),
            )
            .item(
                PlaceItem::new("backup", "Backup disk")
                    .line("offline, last seen 3 days ago")
                    .dimmed(true)
                    .children([PlaceItem::new("backup/old", "Old scans").dimmed(true)]),
            ),
        PlaceSection::new("albums", "Albums")
            .item(PlaceItem::new("album/family", "Family"))
            .item(PlaceItem::new("album/travel", "Travel")),
        PlaceSection::new("keywords", "Keywords").item(
            PlaceItem::new("Animals", "Animals")
                .open(l.animals_open)
                .children([PlaceItem::new("Animals|Dog", "Dog")]),
        ),
    ]
}

fn library(set: Vec<&'static str>) -> Harness<'static, Library> {
    let mut h = Harness::builder()
        .with_size(egui::vec2(260.0, 480.0))
        .build_ui_state(
            |ui, l: &mut Library| {
                let sections = library_sections(l);
                let set = l.set.clone();
                let mut rows = Vec::new();
                let out = PlacesPanel::new("test.library", &sections)
                    .selected_set(&set)
                    .on_row(|_, _, item, r| rows.push((item.id.clone(), r.rect)))
                    .show(ui);
                l.rows = rows;
                if let Some((id, open)) = out.expanded {
                    if id == "Animals" {
                        l.animals_open = open;
                    }
                    l.expanded.push((id, open));
                }
                if let Some((_, item)) = out.chosen {
                    l.chosen.push(item.id);
                }
            },
            Library {
                set,
                ..Library::default()
            },
        );
    h.run();
    h
}

#[test]
fn nested_items_fold_by_keyboard_named_at_their_level() {
    let mut h = library(Vec::new());
    let expanded = |h: &Harness<'_, Library>, label: &str| {
        h.get_by_label(label).accesskit_node().data().is_expanded()
    };
    let level = |h: &Harness<'_, Library>, label: &str| {
        h.get_by_label(label).accesskit_node().data().level()
    };
    // Open at first: the disk shows its year, the year is folded.
    assert_eq!(expanded(&h, "Photos disk"), Some(true));
    assert_eq!(level(&h, "Photos disk"), Some(1));
    assert_eq!(level(&h, "2026"), Some(2));
    assert_eq!(expanded(&h, "2026"), Some(false));
    assert!(h.query_by_label("Summer trip").is_none());
    // The toggles are named after their rows.
    assert_eq!(
        h.get_by_label("Show or hide 2026").accesskit_node().role(),
        Role::DisclosureTriangle
    );
    // Right opens the row that has the keyboard, Left folds it.
    h.get_by_label("2026").focus();
    h.run();
    h.key_press(Key::ArrowRight);
    h.run();
    h.run();
    assert_eq!(expanded(&h, "2026"), Some(true));
    assert_eq!(level(&h, "Summer trip"), Some(3));
    assert_eq!(h.state().expanded, [("2026".into(), true)]);
    // Down goes through the rows shown, into the nested one.
    h.key_press(Key::ArrowDown);
    h.run();
    let focused = |h: &Harness<'_, Library>, label: &str| h.get_by_label(label).is_focused();
    // The keyboard reaches the child, through its parent's toggle at most.
    if !focused(&h, "Summer trip") {
        h.key_press(Key::ArrowDown);
        h.run();
    }
    assert!(focused(&h, "Summer trip"));
    h.key_press(Key::Enter);
    h.run();
    assert_eq!(h.state().chosen, ["2026/summer"]);
    h.get_by_label("2026").focus();
    h.run();
    h.key_press(Key::ArrowLeft);
    h.run();
    h.run();
    assert_eq!(expanded(&h, "2026"), Some(false));
    assert!(h.query_by_label("Summer trip").is_none());
    // An app-kept branch follows the application.
    assert_eq!(expanded(&h, "Animals"), Some(false));
    h.state_mut().animals_open = true;
    h.run();
    h.run();
    assert_eq!(expanded(&h, "Animals"), Some(true));
    assert!(h.query_by_label("Dog").is_some());
    h.get_by_label("Show or hide Animals").click();
    h.run();
    h.run();
    assert!(!h.state().animals_open);
    assert!(h.query_by_label("Dog").is_none());
}

#[test]
fn a_dimmed_item_is_muted_said_unavailable_and_still_chosen() {
    let mut h = library(Vec::new());
    let backup = h.get_by_label("Backup disk");
    assert_eq!(
        backup.accesskit_node().data().description(),
        Some("Unavailable, offline, last seen 3 days ago")
    );
    // Its name is painted in the muted text role, a plain one's not.
    let muted = egui_pop_core::theme::Tokens::of(&h.ctx)
        .text_role
        .muted
        .color;
    let colour_of = |h: &Harness<'_, Library>, text: &str| {
        h.output()
            .shapes
            .iter()
            .find_map(|s| match &s.shape {
                egui::Shape::Text(t) if t.galley.text() == text => {
                    Some(t.galley.job.sections[0].format.color)
                }
                _ => None,
            })
            .unwrap_or_else(|| panic!("{text} painted"))
    };
    assert_eq!(colour_of(&h, "Backup disk"), muted);
    h.get_by_label("Family").click();
    h.run();
    assert_ne!(colour_of(&h, "Family"), muted);
    h.get_by_label("Backup disk").click();
    h.run();
    assert_eq!(h.state().chosen, ["album/family", "backup"]);
}

#[test]
fn the_row_hook_sees_every_row_shown_with_its_rect() {
    let h = library(Vec::new());
    let ids: Vec<&str> = h.state().rows.iter().map(|(id, _)| id.as_str()).collect();
    assert_eq!(
        ids,
        [
            "disk",
            "2026",
            "backup",
            "album/family",
            "album/travel",
            "Animals"
        ]
    );
    for (id, rect) in &h.state().rows {
        let label = match id.as_str() {
            "disk" => "Photos disk",
            "backup" => "Backup disk",
            "album/family" => "Family",
            "album/travel" => "Travel",
            other => other,
        };
        let node = h.get_by_label(label).rect();
        assert!(
            rect.contains_rect(node) || node.contains_rect(*rect),
            "{id}: {rect:?} against {node:?}"
        );
    }
}

#[test]
fn a_selected_set_selects_one_item_in_each_of_several_sections() {
    let h = library(vec!["2026", "album/travel"]);
    let toggled = |label: &str| h.get_by_label(label).accesskit_node().toggled();
    assert_eq!(toggled("2026"), Some(Toggled::True));
    assert_eq!(toggled("Travel"), Some(Toggled::True));
    assert_eq!(toggled("Family"), Some(Toggled::False));
    assert_eq!(toggled("Photos disk"), Some(Toggled::False));
}
