//! Every overlay of the gallery opened and closed appears in and leaves
//! `windows.list` (ADR 0029, slice 6): the palette, the menu bar's and a
//! card's menus, a popup menu, a modal, the strips' popover, a hover card,
//! a flyout, a toolbar's overflow, a palette's menu and the node graph's
//! menu, each a popup of the main window while open, and none left once
//! closed; no window invariant broken.

use egui_kittest::Harness;
use egui_kittest::kittest::Queryable as _;
use egui_pop::api::bus::{WindowKind, WindowRecord};
use egui_pop::theme::Density;
use egui_pop_gallery::Gallery;

use super::{harness, options};

fn popups(h: &Harness<'_, Gallery>) -> Vec<WindowRecord> {
    h.state()
        .app
        .handle()
        .windows_list()
        .into_iter()
        .filter(|w| w.kind == WindowKind::Popup)
        .collect()
}

/// Opens an overlay with `open`, finds it listed as a popup of the main
/// window, closes it with `close`, finds none left.
fn opened_and_closed(
    h: &mut Harness<'_, Gallery>,
    what: &str,
    open: impl FnOnce(&mut Harness<'_, Gallery>),
    close: impl FnOnce(&mut Harness<'_, Gallery>),
) {
    assert!(popups(h).is_empty(), "{what}: before: {:?}", popups(h));
    open(h);
    h.run();
    let open = popups(h);
    assert!(!open.is_empty(), "{what}: no popup listed");
    for p in &open {
        assert_eq!(p.parent.as_deref(), Some("main"), "{what}: {p:?}");
        assert!(!p.owner.is_empty(), "{what}: {p:?}");
    }
    close(h);
    h.run();
    h.run();
    assert!(popups(h).is_empty(), "{what}: left: {:?}", popups(h));
    let broken = h.state().app.handle().broken_invariants();
    assert!(broken.is_empty(), "{what}: {broken:?}");
}

fn escape(h: &mut Harness<'_, Gallery>) {
    h.key_press(egui::Key::Escape);
}

#[test]
fn every_overlay_opened_and_closed_appears_in_and_leaves_the_windows_list() {
    let mut h = harness(options("pop-dark", Density::Comfortable, "menus"));
    h.run();
    opened_and_closed(
        &mut h,
        "the palette",
        |h| {
            h.key_press_modifiers(
                egui::Modifiers::COMMAND | egui::Modifiers::SHIFT,
                egui::Key::P,
            );
            h.run();
        },
        escape,
    );
    opened_and_closed(
        &mut h,
        "the menu bar's View menu",
        |h| h.get_by_label("View").click(),
        escape,
    );
    opened_and_closed(
        &mut h,
        "a card's context menu",
        |h| h.get_by_label("Card with a context menu").click_secondary(),
        escape,
    );
    let app = h.state().app.handle().clone();
    opened_and_closed(
        &mut h,
        "a popup menu",
        |h| {
            assert!(egui_pop::api::menus::popup_at(
                &app,
                &egui_pop::api::app::MENU_VIEW,
                egui::pos2(200.0, 200.0)
            ));
            h.run();
        },
        escape,
    );

    let mut h = harness(options("pop-dark", Density::Comfortable, "inputs"));
    h.run();
    opened_and_closed(
        &mut h,
        "a modal",
        |h| {
            h.get_by_label("Open the form in a modal").scroll_to_me();
            h.run();
            h.get_by_label("Open the form in a modal").click();
        },
        escape,
    );

    let mut h = harness(options("pop-dark", Density::Comfortable, "strips"));
    h.run();
    opened_and_closed(
        &mut h,
        "the strips' popover",
        |h| {
            h.get_by_label("Open the problems popover").click();
            h.run();
        },
        escape,
    );
    opened_and_closed(
        &mut h,
        "a hover card",
        |h| {
            h.get_by_label("Show the node's card").click();
            h.run();
        },
        escape,
    );

    let mut h = harness(options("pop-dark", Density::Comfortable, "workbench"));
    h.run();
    h.get_by_label("Open the demonstration panels").click();
    h.run();
    h.get_by_label("Start side: fold").click();
    h.run();
    h.run();
    let app = h.state().app.handle().clone();
    let outline = egui_pop::model::ids::PanelId::new("gallery.outline").unwrap();
    opened_and_closed(
        &mut h,
        "a flyout",
        |h| {
            assert!(app.open_flyout(&outline, true).unwrap().ok());
            h.run();
        },
        escape,
    );

    let mut h = harness(options("pop-dark", Density::Comfortable, "toolbars"));
    h.run();
    opened_and_closed(
        &mut h,
        "a toolbar's overflow",
        |h| {
            h.get_by_label("More tools: Narrow").click();
            h.run();
        },
        escape,
    );

    let mut h = harness(options("pop-dark", Density::Comfortable, "palettes"));
    h.run();
    h.get_by_label("Open the palettes").click();
    h.run();
    h.run();
    // The navigator stacked under the toolbox: their group's band has a menu.
    let app = h.state().app.handle().clone();
    let toolbox = egui_pop::model::ids::PanelId::from_static("gallery.toolbox");
    let navigator = egui_pop::model::ids::PanelId::from_static("gallery.navigator");
    let _ = app.change_workbench(egui_pop::api::workbench::Change::Group(
        vec![navigator],
        toolbox,
        egui_pop::model::layout::Join::Below,
    ));
    h.run();
    h.run();
    opened_and_closed(
        &mut h,
        "a palette group's menu",
        |h| {
            h.get_by_label("Toolbox · Navigator menu").click();
            h.run();
        },
        escape,
    );

    let mut h = harness(options("pop-dark", Density::Comfortable, "node-graph"));
    h.run();
    opened_and_closed(
        &mut h,
        "the node graph's menu",
        |h| {
            let graph = h.get_by_label("Processing graph").rect();
            let at = graph.left_bottom() + egui::vec2(12.0, -12.0);
            h.hover_at(at);
            h.run();
            h.event(egui::Event::PointerButton {
                pos: at,
                button: egui::PointerButton::Secondary,
                pressed: true,
                modifiers: egui::Modifiers::NONE,
            });
            h.event(egui::Event::PointerButton {
                pos: at,
                button: egui::PointerButton::Secondary,
                pressed: false,
                modifiers: egui::Modifiers::NONE,
            });
            h.run();
        },
        escape,
    );
}
