//! A submenu item in a menu drawn by hand (`menus::submenu`), on the
//! menus page: the source row's context menu and its "Import
//! automatically" submenu, opened by the pointer resting on it and by the
//! keyboard, a row chosen in it, Escape closing one level, and what
//! assistive technology reads.

use egui::accesskit::{HasPopup, Role};
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT as _, Queryable as _};
use egui_pop::theme::Density;
use egui_pop_gallery::Gallery;

use super::{context_key, harness, options};

const ROW: &str = "Card reader (import automatically: off)";
const ITEM: &str = "Import automatically";

/// The menus page with the source row's context menu open.
fn menu_open() -> Harness<'static, Gallery> {
    let mut h = harness(options("pop-dark", Density::Comfortable, "menus"));
    h.run();
    // The row ends the page: brought into view first.
    h.get_by_label(ROW).scroll_to_me();
    h.run();
    h.get_by_label(ROW).click_secondary();
    h.run();
    assert!(h.query_by_label("Import now").is_some(), "the menu opened");
    h
}

fn submenu_open(h: &Harness<'_, Gallery>) -> bool {
    h.query_by_label("On plug-in").is_some()
}

fn expanded(h: &Harness<'_, Gallery>) -> Option<bool> {
    h.get_by_label(ITEM).accesskit_node().data().is_expanded()
}

#[test]
fn the_pointer_resting_on_a_hand_drawn_submenu_item_opens_it_and_a_row_is_chosen() {
    let mut h = menu_open();
    h.get_by_label(ITEM).hover();
    // Crossing the item opens nothing at once...
    h.step();
    assert!(!submenu_open(&h), "opened before the pointer rested");
    // ...resting on it does (`motion.submenu.delay`).
    h.run();
    h.run();
    assert!(submenu_open(&h), "the submenu opened on hover");
    assert_eq!(expanded(&h), Some(true));
    h.get_by_label("On plug-in").click();
    h.run();
    assert_eq!(context_key(&h, "galleryAuto"), "plug");
    // Choosing closes every level.
    assert!(h.query_by_label("Import now").is_none(), "the menu closed");
    assert!(!submenu_open(&h));
}

#[test]
fn the_keyboard_opens_a_hand_drawn_submenu_and_escape_closes_one_level() {
    let mut h = menu_open();
    h.get_by_label(ITEM).focus();
    h.run();
    assert!(!submenu_open(&h), "focus alone opens nothing");
    // Right opens it, the focus on its first row.
    h.key_press(egui::Key::ArrowRight);
    h.run();
    assert!(submenu_open(&h), "Right opened the submenu");
    assert!(h.get_by_label("Off").is_focused(), "focus on the first row");
    // Escape closes the submenu alone, back on its item.
    h.key_press(egui::Key::Escape);
    h.run();
    assert!(!submenu_open(&h), "Escape closed the submenu");
    assert!(h.query_by_label("Import now").is_some(), "the menu stays");
    assert!(h.get_by_label(ITEM).is_focused(), "focus back on the item");
    assert_eq!(expanded(&h), Some(false));
    // Enter opens it again; Left closes it, back on the item.
    h.key_press(egui::Key::Enter);
    h.run();
    assert!(submenu_open(&h), "Enter opened the submenu");
    h.key_press(egui::Key::ArrowLeft);
    h.run();
    assert!(!submenu_open(&h), "Left closed the submenu");
    assert!(h.get_by_label(ITEM).is_focused(), "focus back on the item");
    // Down then Enter chooses a row by the keyboard.
    h.key_press(egui::Key::ArrowRight);
    h.run();
    h.key_press(egui::Key::ArrowDown);
    h.run();
    assert!(h.get_by_label("On plug-in").is_focused());
    h.key_press(egui::Key::Enter);
    h.run();
    assert_eq!(context_key(&h, "galleryAuto"), "plug");
    assert!(h.query_by_label("Import now").is_none(), "the menu closed");
    // A second Escape closes the menu once the submenu is gone.
    let mut h = menu_open();
    h.get_by_label(ITEM).focus();
    h.run();
    h.key_press(egui::Key::ArrowRight);
    h.run();
    h.key_press(egui::Key::Escape);
    h.run();
    assert!(h.query_by_label("Import now").is_some());
    h.key_press(egui::Key::Escape);
    h.run();
    assert!(h.query_by_label("Import now").is_none(), "the menu closed");
}

#[test]
fn a_hand_drawn_submenu_item_reads_as_a_menu_item_with_a_menu() {
    let h = menu_open();
    let item = h.get_by_label(ITEM);
    let node = item.accesskit_node();
    assert_eq!(node.role(), Role::MenuItem);
    assert_eq!(node.has_popup(), Some(HasPopup::Menu));
    assert_eq!(node.data().is_expanded(), Some(false));
    egui_pop_test::assert_all_named(&h);
}
