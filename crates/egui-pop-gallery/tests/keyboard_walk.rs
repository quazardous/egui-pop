//! The keyboard survey's walk (`docs/keyboard-survey.md`): the gallery by
//! real key presses alone, after a start the test may set up by pointer.
//!
//! Behaviour that holds is pinned. A gap the walk confirmed is an ignored
//! test asserting what the survey asks for, its reason naming the survey's
//! item: `cargo test -p egui-pop-gallery --test keyboard_walk -- --ignored`
//! lists what is still open, and the change closing an item removes the
//! `ignore`. "Check" numbers are the survey's walk, "item" numbers its
//! ranked list.

use std::cell::Cell;
use std::sync::{Arc, Mutex};

use egui::{Key, Modifiers};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable as _;
use egui_pop::api::dialogs::{self, Confirm};
use egui_pop::api::{App, Ctx, DefaultPlugins, Plugin, PluginError, palettes};
use egui_pop::core::sections::{Section, Sections};
use egui_pop::core::tab::{self, ScopeKind, TabStop};
use egui_pop::editors::terminal::{Sink, Spawn, Terminal, TerminalSource, TerminalState};
use egui_pop::model::commands::{CommandSpec, Origin};
use egui_pop::model::ids::{CommandId, PluginId};
use egui_pop::model::layout::Node;
use egui_pop::model::panels::Area;
use egui_pop::theme::ThemeSpec;
use egui_pop_gallery::pages::workbench::{NOTES, OUTLINE};
use egui_pop_gallery::{Gallery, Options};
use egui_pop_test::SharedGpu as _;
use egui_pop_test::themed;
use egui_pop_test::walk::{self, ESCAPE, End, F6, Press, SHIFT_F6, SHIFT_TAB, Stop, TAB};
use serde_json::Value as Json;

const ENTER: Press = Press::key(Key::Enter);
const SPACE: Press = Press::key(Key::Space);
const UP: Press = Press::key(Key::ArrowUp);
const DOWN: Press = Press::key(Key::ArrowDown);
const RIGHT: Press = Press::key(Key::ArrowRight);
const ALT_F: Press = Press {
    modifiers: Modifiers::ALT,
    key: Key::F,
};
/// Ctrl as a desktop sends it: `command` follows it off macOS.
const CTRL: Modifiers = Modifiers {
    ctrl: true,
    command: true,
    alt: false,
    shift: false,
    mac_cmd: false,
};

fn harness(page: &str) -> Harness<'static, Gallery> {
    let options = Options {
        page: page.to_string(),
        ..Options::default()
    };
    let mut h = Harness::builder()
        .with_size(egui::vec2(1100.0, 720.0))
        .shared_wgpu()
        .build_eframe(move |cc| {
            egui_pop::core::motion::freeze(&cc.egui_ctx, 0.0);
            Gallery::from_creation_context(cc, options).expect("the gallery builds")
        });
    let _ = h.run_ok();
    h
}

/// Goes to a stop by keys. A page's stops come last in the window's order,
/// so Shift+Tab gets there sooner.
#[track_caller]
fn reach_back(h: &mut Harness<'_, Gallery>, name: &str) -> Stop {
    walk::reach(h, SHIFT_TAB, name, 60).unwrap_or_else(|| panic!("Shift+Tab reaches {name}"))
}

/// Opens the workbench page's panels, by keys.
fn with_panels() -> Harness<'static, Gallery> {
    let mut h = harness("workbench");
    reach_back(&mut h, "Open the demonstration panels");
    walk::press(&mut h, ENTER);
    h
}

fn name(stop: Option<Stop>) -> String {
    stop.map_or_else(|| "nothing".to_string(), |s| s.name)
}

/// A rectangle's pixels in an image.
fn crop(image: &image::RgbaImage, r: egui::Rect) -> image::RgbaImage {
    let (x, y) = (r.min.x as u32, r.min.y as u32);
    image::imageops::crop_imm(image, x, y, r.width() as u32, r.height() as u32).to_image()
}

// Tab from nothing through the window (check 1).

#[test]
fn tab_goes_round_the_window_with_the_status_bar_before_the_panels() {
    let mut h = harness("overview");
    let walked = walk::walk(&mut h, TAB, 200);
    assert_eq!(walked.end, End::Looped { at: 0 }, "{walked}");
    assert_eq!(walked.names()[..3], ["File", "Edit", "View"], "{walked}");
    let at = |name: &str| walked.position(name).expect(name);
    assert!(at("Help") < at("Theme"), "{walked}");
    assert!(at("Reduced motion") < at("No job"), "{walked}");
    assert!(at("No job") < at("Pages"), "{walked}");
    // The next panels' lines come after the side list, then the inspector.
    assert!(at("Pages") < at("Token paths"), "{walked}");
}

#[test]
fn shift_tab_goes_round_the_other_way() {
    let mut h = harness("overview");
    let forth = walk::walk(&mut h, TAB, 200);
    let back = walk::walk(&mut h, SHIFT_TAB, 200);
    assert_eq!(back.end, End::Looped { at: 0 });
    assert_eq!(back.stops.len(), forth.stops.len());
    assert_eq!(back.stops.last().map(|s| s.id), Some(forth.stops[0].id));
}

// The one thing to settle first: which panel the keys obey (item 3).

#[test]
#[ignore = "keyboard survey item 3: Tab into another panel leaves focusedPanel on the old one"]
fn tab_into_another_panel_moves_the_focused_panel() {
    let mut h = with_panels();
    h.get_by_label("Sections of the document.").click();
    let _ = h.run_ok();
    assert_eq!(h.state().app.handle().focused_panel(), Some(OUTLINE));
    assert!(walk::focused(&h).is_none(), "a click on a label takes none");
    reach_back(&mut h, "Clear the notes…");
    assert_eq!(h.state().app.handle().focused_panel(), Some(NOTES));
}

#[test]
#[ignore = "keyboard survey item 3: a panel's focus command moves no widget focus"]
fn a_panel_focus_command_gives_it_the_keyboard() {
    let mut h = with_panels();
    h.state_mut().app.run(
        &CommandId::new("gallery.notes.focus").unwrap(),
        Json::Null,
        Origin::User,
    );
    let _ = h.run_ok();
    assert_eq!(h.state().app.handle().focused_panel(), Some(NOTES));
    assert_eq!(name(walk::focused(&h)), "Clear the notes…");
}

// Stops that show nothing (check 2, item 8).

/// Every stop of the walk on this page without a name: side lines, frames.
fn nameless(page: &str) -> Vec<Stop> {
    let mut h = harness(page);
    let walked = walk::walk(&mut h, SHIFT_TAB, 200);
    let out = walked.stops.into_iter();
    out.filter(|s| s.name.is_empty()).collect()
}

#[test]
#[ignore = "keyboard survey items 8 and 9: a side area's line is a stop with no role and no name"]
fn every_stop_says_what_it_is() {
    let lines = nameless("overview");
    assert_eq!(lines, Vec::<Stop>::new());
}

#[test]
#[ignore = "keyboard survey items 8 and 9: a side area's line shows nothing when focused"]
fn a_focused_side_areas_line_shows_it() {
    let mut h = harness("overview");
    let at_rest = h.render().unwrap();
    let walked = walk::walk(&mut h, SHIFT_TAB, 200);
    let line = walked.stops.iter().find(|s| s.name.is_empty()).unwrap();
    let mut h = harness("overview");
    while walk::focused(&h).is_none_or(|s| s.id != line.id) {
        walk::press(&mut h, SHIFT_TAB);
    }
    let rect = line.rect.intersect(h.ctx.content_rect());
    assert_ne!(crop(&at_rest, rect), crop(&h.render().unwrap(), rect));
}

#[test]
#[ignore = "keyboard survey item 8: the frame's caption is a stop showing nothing"]
fn a_frames_caption_is_no_stop() {
    // The frame page draws the frame's own title bar as its preview.
    let caption = nameless("frame");
    assert!(
        !caption.iter().any(|s| s.rect.width() > 100.0),
        "{caption:?}"
    );
}

#[test]
#[ignore = "keyboard survey item 8: a flyout's edge is a stop showing nothing"]
fn a_flyouts_edge_is_no_stop() {
    let mut h = harness("workbench");
    reach_back(&mut h, "Start side: fold");
    walk::press(&mut h, ENTER);
    // The folded strip's icon, by keys.
    walk::reach(&mut h, TAB, "Side bar", 60).expect("Tab reaches the strip's icon");
    walk::press(&mut h, ENTER);
    let flyout = h.state().app.handle().flyout().expect("the flyout is open");
    let rect = flyout.1.expect("drawn");
    let walked = walk::walk(&mut h, TAB, 200);
    let edge = walked
        .stops
        .iter()
        .find(|s| s.name.is_empty() && (s.rect.center().x - rect.max.x).abs() < 10.0);
    assert!(edge.is_none(), "{edge:?}");
}

#[test]
#[ignore = "keyboard survey item 8: an open menu's frame is a stop showing nothing"]
fn a_menus_frame_is_no_stop() {
    let mut h = harness("overview");
    walk::press(&mut h, ALT_F);
    let walked = walk::walk(&mut h, TAB, 3);
    assert_ne!(walked.stops[1].name, "", "{walked}");
}

// Dock separators (check 3).

#[test]
fn a_dock_separator_is_a_stop_and_moves_with_shift_or_ctrl_and_an_arrow() {
    let mut h = with_panels();
    let app = h.state().app.handle().clone();
    let mut layout = app.layout();
    for (area, node) in &mut layout.areas {
        if *area == Area::StartSide {
            let Node::Tabs { panels, .. } = node.clone() else {
                panic!("two tabs on the start side: {node:?}");
            };
            *node = Node::Split {
                vertical: true,
                ratio: 0.5,
                a: Box::new(Node::tab(panels[0].clone())),
                b: Box::new(Node::tab(panels[1].clone())),
            };
        }
    }
    assert_eq!(app.set_layout(layout), Vec::<String>::new());
    let _ = h.run_ok();
    let line = |s: &Stop| s.name.is_empty() && s.rect.height() < 10.0 && s.rect.width() < 200.0;
    let separator = (0..200)
        .find_map(|_| walk::press(&mut h, TAB).filter(line))
        .expect("Tab reaches the separator");
    let press = |h: &mut Harness<'_, Gallery>, modifiers, key| {
        walk::press(h, Press { modifiers, key }).expect("it keeps the focus")
    };
    let y = |s: Stop| s.rect.min.y;
    let moved = press(&mut h, Modifiers::SHIFT, Key::ArrowDown);
    assert_eq!(y(moved), separator.rect.min.y + 16.0);
    let moved = press(&mut h, CTRL, Key::ArrowUp);
    assert_eq!(y(moved), separator.rect.min.y);
    // Across the line, it does not move; a plain arrow leaves for the cone.
    let moved = press(&mut h, CTRL, Key::ArrowRight);
    assert_eq!(moved.id, separator.id);
    assert_eq!(y(moved), separator.rect.min.y);
    assert_ne!(walk::press(&mut h, DOWN).map(|s| s.id), Some(separator.id));
}

#[test]
#[ignore = "keyboard survey item 9: no key moves a side area's line"]
fn a_side_areas_line_moves_with_the_arrows() {
    let mut h = harness("overview");
    let line = (0..200)
        .find_map(|_| walk::press(&mut h, SHIFT_TAB).filter(|s| s.name.is_empty()))
        .expect("the line is a stop");
    let moved = walk::press(&mut h, Press::key(Key::ArrowLeft)).expect("the focus stays");
    assert_eq!(moved.id, line.id);
    assert_ne!(moved.rect, line.rect);
}

// Where the focus goes after it is lost (checks 4, 5, 11; item 6).

#[test]
#[ignore = "keyboard survey item 6: after Escape, Tab restarts at the window's first stop"]
fn after_escape_tab_goes_on_from_the_place() {
    let mut h = harness("dialogs");
    reach_back(&mut h, "Pick a side");
    assert_eq!(walk::press(&mut h, ESCAPE), None);
    assert_ne!(name(walk::press(&mut h, TAB)), "File");
}

#[test]
#[ignore = "keyboard survey item 6: a focused tab that goes away leaves nothing focused, Tab restarts at the top"]
fn a_focused_tab_that_goes_away_hands_the_focus_on() {
    let mut h = with_panels();
    reach_back(&mut h, "Notes");
    h.state_mut().app.run(
        &CommandId::new("gallery.notes.toggle").unwrap(),
        Json::Null,
        Origin::User,
    );
    let _ = h.run_ok();
    assert!(walk::focused(&h).is_some());
}

#[test]
#[ignore = "keyboard survey item 6: a button changing the panels loses the focus it holds"]
fn a_button_rearranging_the_panels_keeps_the_focus() {
    let mut h = harness("workbench");
    reach_back(&mut h, "Open the demonstration panels");
    assert_eq!(
        name(walk::press(&mut h, ENTER)),
        "Open the demonstration panels"
    );
}

fn rows<'a>() -> Harness<'a> {
    let mut h = themed(&ThemeSpec::default(), egui::vec2(400.0, 300.0), |ui| {
        let _ = ui.button("Before");
        Sections::new("walk.rows").show(ui, |stack| {
            stack.rows(Section::new("log", "Log").count(500), 500, |ui, range| {
                for i in range {
                    let _ = ui.button(format!("Row {}", i + 1));
                }
            });
        });
        let _ = ui.button("After");
    });
    let _ = h.run_ok();
    h
}

#[test]
#[ignore = "keyboard survey item 6: a windowed row scrolled away loses the focus, Tab restarts at the top"]
fn a_windowed_row_scrolled_away_keeps_the_place() {
    let mut h = rows();
    let row = walk::reach(&mut h, TAB, "Row 3", 10).expect("Tab reaches it");
    h.event(egui::Event::PointerMoved(row.rect.center()));
    let _ = h.run_ok();
    for _ in 0..6 {
        h.event(egui::Event::MouseWheel {
            unit: egui::MouseWheelUnit::Line,
            delta: egui::vec2(0.0, -20.0),
            phase: egui::TouchPhase::Move,
            modifiers: Modifiers::NONE,
        });
        let _ = h.run_ok();
    }
    assert!(h.query_by_label("Row 3").is_none(), "scrolled away");
    assert_ne!(name(walk::press(&mut h, TAB)), "Before");
}

// Focus out of view (checks 1, 10, 11; item 7).

#[test]
fn the_side_list_scrolls_to_the_focus() {
    let mut h = harness("overview");
    let walked = walk::walk(&mut h, TAB, 200);
    assert!(walked.stops.iter().all(|s| s.visible), "{walked}");
}

#[test]
#[ignore = "keyboard survey item 7: Tab goes past the fold of a long form, it does not scroll"]
fn a_long_form_scrolls_to_the_focus() {
    let mut h = harness("settings");
    let walked = walk::walk(&mut h, SHIFT_TAB, 200);
    assert!(walked.stops.iter().all(|s| s.visible), "{walked}");
}

#[test]
#[ignore = "keyboard survey item 7: Tab leaves windowed rows after those drawn; the others cannot be reached"]
fn tab_goes_through_windowed_rows_past_those_drawn() {
    let mut h = rows();
    walk::reach(&mut h, TAB, "Row 40", 100).expect("Tab reaches a row below the window");
}

// F6 (check 6, item 5).

#[test]
fn f6_goes_between_the_inputs_pages_scopes_and_back() {
    let mut h = harness("inputs");
    assert_eq!(name(walk::press(&mut h, F6)), "Street");
    assert_eq!(name(walk::press(&mut h, F6)), "Tags");
    assert_eq!(name(walk::press(&mut h, F6)), "Street");
    assert_eq!(name(walk::press(&mut h, SHIFT_F6)), "Tags");
    assert_eq!(name(walk::press(&mut h, SHIFT_F6)), "Street");
}

#[test]
#[ignore = "keyboard survey item 5: no shell panel opens a scope, F6 does nothing outside the inputs page"]
fn f6_goes_to_a_region_on_every_page() {
    for page in ["overview", "workbench", "menus", "settings"] {
        let mut h = harness(page);
        assert!(walk::press(&mut h, F6).is_some(), "{page}");
        assert!(walk::press(&mut h, SHIFT_F6).is_some(), "{page}");
    }
}

// The gallery's side list (checks 7 and 8; items 10 and 16).

#[test]
fn up_and_down_go_through_the_side_list_and_enter_opens_a_page_keeping_the_focus() {
    let mut h = harness("overview");
    walk::reach(&mut h, TAB, "Pages", 40).expect("Tab reaches the list");
    // The list holds the focus as one; its cursor starts on the page shown.
    assert_eq!(name(walk::press(&mut h, DOWN)), "Pages");
    assert_eq!(name(walk::press(&mut h, DOWN)), "Pages");
    assert_eq!(name(walk::press(&mut h, UP)), "Pages");
    assert_eq!(h.state().page().as_str(), "overview");
    assert_eq!(name(walk::press(&mut h, ENTER)), "Pages");
    assert_eq!(h.state().page().as_str(), "tokens");
}

#[test]
fn a_plain_arrow_stays_in_its_region() {
    let mut h = harness("overview");
    walk::reach(&mut h, TAB, "Pages", 40).expect("Tab reaches the list");
    let stop = walk::press(&mut h, RIGHT).expect("the focus stays");
    assert_eq!(stop.name, "Pages", "{stop}");
}

#[test]
fn the_side_list_is_one_stop() {
    let mut h = harness("overview");
    let titles: Vec<String> = h
        .state()
        .app
        .handle()
        .stories()
        .into_iter()
        .map(|s| s.title)
        .collect();
    let walked = walk::walk(&mut h, TAB, 200);
    let rows = walked.stops.iter().filter(|s| titles.contains(&s.name));
    assert!(rows.count() <= 1, "{walked}");
}

// Enter and Space on a plain button (check 9, items 4 and 14).

#[test]
#[ignore = "keyboard survey item 14: Ctrl+Enter clicks a focused plain button"]
fn ctrl_enter_does_not_click_a_plain_button() {
    let clicks = Cell::new(0);
    let mut h = themed(&ThemeSpec::default(), egui::vec2(200.0, 80.0), |ui| {
        if ui.button("Go").clicked() {
            clicks.set(clicks.get() + 1);
        }
    });
    let _ = h.run_ok();
    walk::reach(&mut h, TAB, "Go", 4).expect("Tab reaches the button");
    let key = Key::Enter;
    walk::press(
        &mut h,
        Press {
            modifiers: CTRL,
            key,
        },
    );
    drop(h);
    assert_eq!(clicks.get(), 0);
}

#[test]
#[ignore = "keyboard survey item 14: a held Space clicks at every repeat"]
fn a_held_space_clicks_once() {
    let clicks = Cell::new(0);
    let mut h = themed(&ThemeSpec::default(), egui::vec2(200.0, 80.0), |ui| {
        if ui.button("Go").clicked() {
            clicks.set(clicks.get() + 1);
        }
    });
    let _ = h.run_ok();
    walk::press(&mut h, TAB);
    for repeat in [false, true, true, true] {
        h.event(egui::Event::Key {
            key: Key::Space,
            physical_key: None,
            pressed: true,
            repeat,
            modifiers: Modifiers::NONE,
        });
        let _ = h.run_ok();
    }
    drop(h);
    assert_eq!(clicks.get(), 1);
}

/// The button at rest, focused and pressed, as rendered.
fn rendered_states() -> [image::RgbaImage; 3] {
    let mut h = harness("palettes");
    let stop = reach_back(&mut h, "Open the palettes");
    let focused = h.render().unwrap();
    walk::press(&mut h, ESCAPE);
    let rest = h.render().unwrap();
    h.hover_at(stop.rect.center());
    h.drag_at(stop.rect.center());
    let _ = h.run_ok();
    let pressed = h.render().unwrap();
    // The left of it: the harness draws its pointer from the middle.
    let mut left = stop.rect;
    left.set_width(stop.rect.width() / 3.0);
    [&rest, &focused, &pressed].map(|image| crop(image, left))
}

#[test]
fn a_focused_plain_button_is_seen() {
    let [rest, focused, _] = rendered_states();
    assert_ne!(rest, focused);
}

#[test]
#[ignore = "keyboard survey item 4: a focused plain button looks pressed"]
fn a_focused_plain_button_does_not_look_pressed() {
    let [_, focused, pressed] = rendered_states();
    assert_ne!(focused, pressed);
}

// Menus (checks 12, 13, 14; items 11 and 15).

#[test]
fn alt_and_a_letter_opens_a_menu_tab_goes_in_and_escape_closes_it() {
    let mut h = harness("overview");
    assert_eq!(walk::press(&mut h, ALT_F), None, "opens with no focus");
    assert!(egui_pop_test::a11y_tree(&h).contains("Quit"));
    let walked = walk::walk(&mut h, TAB, 3);
    assert_eq!(walked.stops[0].name, "File", "{walked}");
    assert!(walked.stops[2].name.starts_with("Settings"), "{walked}");
    assert_eq!(walk::press(&mut h, ESCAPE), None);
    assert!(!egui_pop_test::a11y_tree(&h).contains("Quit"));
    assert_eq!(name(walk::press(&mut h, TAB)), "File");
}

#[test]
#[ignore = "keyboard survey item 11: arrows do nothing in a menu opened by Alt and a letter"]
fn down_enters_a_menu_opened_by_its_letter() {
    let mut h = harness("overview");
    walk::press(&mut h, ALT_F);
    assert!(walk::press(&mut h, DOWN).is_some());
}

#[test]
#[ignore = "keyboard survey item 11: arrows leave a menu opened from its button, for what lies behind it"]
fn arrows_stay_in_a_menu_opened_from_its_button() {
    let mut h = harness("overview");
    assert_eq!(name(walk::press(&mut h, TAB)), "File");
    walk::press(&mut h, ENTER);
    assert!(name(walk::press(&mut h, DOWN)).starts_with("Settings"));
    assert!(name(walk::press(&mut h, DOWN)).starts_with("Quit"));
}

#[test]
#[ignore = "keyboard survey item 15: Tab with a menu open and nothing focused hides the palettes behind it"]
fn tab_with_a_menu_open_leaves_the_palettes() {
    let mut h = harness("palettes");
    reach_back(&mut h, "Open the palettes");
    walk::press(&mut h, ENTER);
    assert_eq!(walk::press(&mut h, ESCAPE), None);
    walk::press(&mut h, ALT_F);
    assert!(egui_pop_test::a11y_tree(&h).contains("Quit"));
    walk::press(&mut h, TAB);
    assert!(!palettes::hidden(h.state().app.handle()));
}

// A status popover (check 15, item 22).

/// The strips page with a problem, its status item focused and open.
fn popover() -> Harness<'static, Gallery> {
    let mut h = harness("strips");
    h.get_by_label("Add an error").click();
    let _ = h.run_ok();
    walk::reach(&mut h, TAB, "Problems", 40).expect("Tab reaches the status item");
    walk::press(&mut h, ENTER);
    let _ = h.run_ok();
    assert!(h.query_by_label("Dismiss all").is_some());
    h
}

#[test]
fn tab_goes_into_a_popover_and_escape_goes_back_to_its_item() {
    let mut h = popover();
    assert_eq!(name(walk::press(&mut h, TAB)), "Dismiss all");
    assert_eq!(name(walk::press(&mut h, ESCAPE)), "Problems");
    assert!(h.query_by_label("Dismiss all").is_none());
}

#[test]
#[ignore = "keyboard survey item 22: Tab past a popover's last stop goes behind it, the popover stays open"]
fn tab_past_a_popovers_end_does_not_leave_it_open_behind() {
    let mut h = popover();
    let inside = ["Dismiss all", "Add a warning"];
    for _ in 0..3 {
        let at = name(walk::press(&mut h, TAB));
        assert!(
            inside.contains(&at.as_str()) || h.query_by_label("Dismiss all").is_none(),
            "{at}"
        );
    }
}

// Dialogs (check 16, items 20 and 21).

#[test]
fn a_dialog_starts_on_its_action_or_on_cancel_for_a_danger_and_tab_wraps_in_it() {
    let mut h = harness("dialogs");
    reach_back(&mut h, "Confirm an export");
    assert_eq!(name(walk::press(&mut h, SPACE)), "Export");
    let walked = walk::walk(&mut h, TAB, 5);
    assert_eq!(walked.names(), ["Cancel", "Export"], "{walked}");
    assert_eq!(walked.end, End::Looped { at: 0 });
    // It ended back on Cancel; F6 stays in.
    assert_eq!(name(walk::press(&mut h, F6)), "Cancel");
    let mut h = harness("dialogs");
    reach_back(&mut h, "Delete three images");
    assert_eq!(name(walk::press(&mut h, ENTER)), "Cancel");
    let walked = walk::walk(&mut h, SHIFT_TAB, 5);
    assert_eq!(walked.names(), ["Delete 3 images", "Cancel"], "{walked}");
}

/// What the dialogs page's commands ran: one per accepted dialog.
fn sent(h: &Harness<'_, Gallery>) -> String {
    h.state()
        .app
        .handle()
        .context()
        .get("galleryDialogSent")
        .to_string()
}

#[test]
fn enter_on_a_button_opening_a_dialog_leaves_it_open() {
    let mut h = harness("dialogs");
    reach_back(&mut h, "Confirm an export");
    assert_eq!(name(walk::press(&mut h, ENTER)), "Export");
    assert_eq!(sent(&h), "", "accepted by the key that opened it");
    assert!(h.query_by_label("Export the page?").is_some());
    // A fresh press answers it.
    walk::press(&mut h, ENTER);
    assert_ne!(sent(&h), "");
    assert!(h.query_by_label("Export the page?").is_none());
}

#[test]
fn every_dialog_of_the_page_stays_open_on_the_key_that_opened_it() {
    for (button, title) in [
        ("Confirm an export", "Export the page?"),
        ("Delete three images", "Delete 3 images?"),
        ("Export settings", "Quality"),
        ("Pick a side", "Which side?"),
        ("Show a notice", "Saved"),
        ("Confirm, with a close button", "Share the page?"),
    ] {
        for key in [ENTER, SPACE] {
            let mut h = harness("dialogs");
            reach_back(&mut h, button);
            walk::hold(&mut h, key);
            assert!(h.query_by_label(title).is_some(), "{button}, {key:?}");
            // Held, whatever it now has the focus on.
            for _ in 0..3 {
                walk::hold(&mut h, key);
            }
            walk::release(&mut h, key);
            assert!(h.query_by_label(title).is_some(), "{button}, {key:?} held");
            assert_eq!(sent(&h), "", "{button}, {key:?}");
        }
    }
}

#[test]
fn holding_the_key_that_opened_a_dialog_does_not_answer_it_until_released() {
    for key in [ENTER, SPACE] {
        let mut h = harness("dialogs");
        reach_back(&mut h, "Confirm an export");
        assert_eq!(name(walk::hold(&mut h, key)), "Export");
        // The repeats, with the focus now on the action.
        for _ in 0..5 {
            assert_eq!(name(walk::hold(&mut h, key)), "Export");
            assert!(h.query_by_label("Export the page?").is_some(), "{key:?}");
        }
        assert_eq!(sent(&h), "", "{key:?}");
        walk::release(&mut h, key);
        assert!(h.query_by_label("Export the page?").is_some(), "{key:?}");
        // Released, the same key answers.
        walk::press(&mut h, key);
        assert_ne!(sent(&h), "", "{key:?}");
    }
}

#[test]
fn a_danger_held_open_does_not_go_to_its_cancel() {
    let mut h = harness("dialogs");
    reach_back(&mut h, "Delete three images");
    assert_eq!(name(walk::hold(&mut h, ENTER)), "Cancel");
    walk::hold(&mut h, ENTER);
    assert!(h.query_by_label("Delete 3 images?").is_some());
    walk::release(&mut h, ENTER);
    assert!(h.query_by_label("Delete 3 images?").is_some());
}

#[test]
fn another_key_is_not_held_back_by_the_one_that_opened_the_dialog() {
    let mut h = harness("dialogs");
    reach_back(&mut h, "Confirm an export");
    walk::hold(&mut h, ENTER);
    // Enter still down: Tab goes on, Space clicks what is focused.
    assert_eq!(name(walk::press(&mut h, TAB)), "Cancel");
    walk::press(&mut h, SPACE);
    assert!(h.query_by_label("Export the page?").is_none());
    assert_eq!(sent(&h), "");
    walk::release(&mut h, ENTER);
}

const ASK: CommandId = CommandId::from_static("doc.ask");
const ANSWERED: CommandId = CommandId::from_static("doc.answered");

/// A command opening a dialog, on a key of its own and in the palette.
struct Asking(Arc<Mutex<Vec<&'static str>>>);

impl Plugin for Asking {
    fn id(&self) -> PluginId {
        PluginId::from_static("doc")
    }

    fn connect(&mut self, ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
        let runs = self.0.clone();
        ctx.command(
            CommandSpec::new(ASK, "Ask a question", "Doc").key("Ctrl+Enter"),
            |call| {
                let d = call.app.service(&dialogs::DIALOGS).expect("default plugin");
                d.open(Confirm::new("A question?", "Accept", ANSWERED));
                Ok(Json::Null)
            },
        )?;
        ctx.command(CommandSpec::new(ANSWERED, "Answered", "Doc"), move |_| {
            runs.lock().unwrap().push("answered");
            Ok(Json::Null)
        })?;
        Ok(())
    }
}

fn asking() -> (Harness<'static, App>, Arc<Mutex<Vec<&'static str>>>) {
    let runs = Arc::new(Mutex::new(Vec::new()));
    let app = App::builder()
        .plugins(DefaultPlugins)
        .plugin(Asking(runs.clone()))
        .build(&egui::Context::default())
        .unwrap();
    let mut h = Harness::builder()
        .with_size(egui::vec2(800.0, 600.0))
        .build_ui_state(|ui, app: &mut App| app.show(ui), app);
    let _ = h.run_ok();
    (h, runs)
}

const CTRL_ENTER: Press = Press {
    modifiers: CTRL,
    key: Key::Enter,
};

#[test]
fn a_key_binding_held_does_not_answer_the_dialog_it_opens() {
    let (mut h, runs) = asking();
    walk::hold(&mut h, CTRL_ENTER);
    assert!(h.query_by_label("A question?").is_some());
    for _ in 0..3 {
        walk::hold(&mut h, CTRL_ENTER);
    }
    walk::release(&mut h, CTRL_ENTER);
    assert!(h.query_by_label("A question?").is_some());
    assert!(runs.lock().unwrap().is_empty());
    walk::press(&mut h, ENTER);
    assert_eq!(*runs.lock().unwrap(), ["answered"]);
}

#[test]
fn enter_in_the_palette_does_not_answer_the_dialog_its_command_opens() {
    let (mut h, runs) = asking();
    walk::press(&mut h, Press::key(Key::F1));
    h.event(egui::Event::Text("Ask a question".to_string()));
    let _ = h.run_ok();
    walk::hold(&mut h, ENTER);
    assert!(h.query_by_label("A question?").is_some());
    walk::hold(&mut h, ENTER);
    walk::hold(&mut h, ENTER);
    assert!(h.query_by_label("A question?").is_some());
    assert!(runs.lock().unwrap().is_empty());
    walk::release(&mut h, ENTER);
    walk::press(&mut h, ENTER);
    assert_eq!(*runs.lock().unwrap(), ["answered"]);
}

#[test]
fn a_held_enter_that_answers_a_dialog_does_not_answer_the_next_in_the_queue() {
    let (mut h, runs) = asking();
    let d = h.state().handle().service(&dialogs::DIALOGS).unwrap();
    d.open(Confirm::new("First?", "Accept", ANSWERED));
    d.open(Confirm::new("Second?", "Accept", ANSWERED));
    let _ = h.run_ok();
    walk::hold(&mut h, ENTER);
    assert_eq!(*runs.lock().unwrap(), ["answered"]);
    assert!(h.query_by_label("Second?").is_some());
    walk::hold(&mut h, ENTER);
    walk::hold(&mut h, ENTER);
    assert_eq!(
        *runs.lock().unwrap(),
        ["answered"],
        "the same key still down"
    );
    assert!(h.query_by_label("Second?").is_some());
    walk::release(&mut h, ENTER);
    walk::press(&mut h, ENTER);
    assert_eq!(*runs.lock().unwrap(), ["answered", "answered"]);
}

#[test]
#[ignore = "keyboard survey item 21: a dialog closed by Escape leaves nothing focused"]
fn escape_from_a_dialog_gives_the_focus_back_to_its_button() {
    let mut h = harness("dialogs");
    reach_back(&mut h, "Confirm an export");
    walk::press(&mut h, SPACE);
    assert_eq!(name(walk::press(&mut h, ESCAPE)), "Confirm an export");
}

#[test]
#[ignore = "keyboard survey item 21: the command palette closed by Escape leaves nothing focused"]
fn escape_from_the_palette_gives_the_focus_back() {
    let mut h = harness("dialogs");
    reach_back(&mut h, "Confirm an export");
    assert_eq!(
        name(walk::press(&mut h, Press::key(Key::F1))),
        "Command palette"
    );
    assert_eq!(name(walk::press(&mut h, ESCAPE)), "Confirm an export");
}

// Keys (checks 17 and 18).

#[test]
fn a_rebound_key_keeps_its_clause() {
    // Its clause holds in the notes' own window only.
    let mut h = harness("workbench");
    let command = CommandId::new("gallery.notes.ask_clear").unwrap();
    h.state()
        .app
        .handle()
        .rebind(command, Some("Ctrl+Shift+J".into()));
    let _ = h.run_ok();
    let modifiers = CTRL | Modifiers::SHIFT;
    let key = Key::J;
    walk::press(&mut h, Press { modifiers, key });
    assert!(h.query_by_label("Clear the notes?").is_none());
}

/// A terminal source recording what the program would read.
#[derive(Clone, Default)]
struct Recorder(Arc<Mutex<Vec<u8>>>);

impl TerminalSource for Recorder {
    fn name(&self) -> String {
        "recorder".into()
    }

    fn writable(&self) -> bool {
        true
    }

    fn start(&mut self, _spawn: &Spawn, _sink: Sink) -> Result<Json, String> {
        Ok(Json::Null)
    }

    fn write(&mut self, bytes: &[u8]) -> Result<(), String> {
        self.0.lock().unwrap().extend_from_slice(bytes);
        Ok(())
    }
}

#[test]
fn a_focused_terminal_in_a_scope_keeps_f6() {
    let recorder = Recorder::default();
    let state = TerminalState::new("walk.terminal", recorder.clone());
    let mut text = String::new();
    let mut h = themed(&ThemeSpec::default(), egui::vec2(600.0, 400.0), |ui| {
        egui_pop::core::focus::install(ui.ctx());
        tab::scope(ui, "first", ScopeKind::Panel, |ui| {
            let field = ui.text_edit_singleline(&mut text);
            tab::stop(ui, &field, TabStop::new());
            let _ = Terminal::new()
                .label("Shell")
                .height(150.0)
                .show(ui, &state);
        });
        tab::scope(ui, "second", ScopeKind::Panel, |ui| {
            let button = ui.button("There");
            tab::stop(ui, &button, TabStop::new());
        });
    });
    let _ = h.run_ok();
    // From the field, F6 goes to the other scope and back.
    walk::press(&mut h, TAB);
    assert_eq!(name(walk::press(&mut h, F6)), "There");
    assert_ne!(name(walk::press(&mut h, F6)), "There");
    // The terminal keeps it: the program reads it, the focus stays.
    walk::reach(&mut h, TAB, "Shell", 6).expect("Tab reaches the terminal");
    assert_eq!(name(walk::press(&mut h, F6)), "Shell");
    assert_eq!(recorder.0.lock().unwrap().as_slice(), b"\x1b[17~");
}
