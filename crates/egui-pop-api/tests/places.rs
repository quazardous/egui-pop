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
