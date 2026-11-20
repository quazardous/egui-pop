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
