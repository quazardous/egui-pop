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

use egui::accesskit::Role;
use egui::{Key, Modifiers, Sense};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable as _;
use egui_pop::api::dialogs::{self, Confirm};
use egui_pop::api::steps::Before;
use egui_pop::api::workbench::STEP_FOCUS;
use egui_pop::api::{App, Ctx, DefaultPlugins, Plugin, PluginError, palettes};
use egui_pop::core::sections::{Section, Sections};
use egui_pop::core::tab::{self, ScopeKind, TabStop};
use egui_pop::core::{focus, regions};
use egui_pop::editors::terminal::{Sink, Spawn, Terminal, TerminalSource, TerminalState};
use egui_pop::model::commands::{CommandSpec, Origin};
use egui_pop::model::ids::{CommandId, PanelId, PluginId};
use egui_pop::model::layout::{Node, Placement};
use egui_pop::model::panels::{Area, PanelSpec};
use egui_pop::theme::ThemeSpec;
use egui_pop_gallery::pages::workbench::{NOTES, OUTLINE};
use egui_pop_gallery::{Gallery, Options};
use egui_pop_test::SharedGpu as _;
use egui_pop_test::themed;
use egui_pop_test::walk::{self, ESCAPE, End, F6, Press, SHIFT_F6, SHIFT_TAB, Stop, TAB};
use serde_json::{Value as Json, json};

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

#[test]
fn a_panel_with_only_text_holds_the_focus_itself_in_the_gallery() {
    let mut h = with_panels();
    h.state_mut().app.run(
        &CommandId::new("gallery.outline.focus").unwrap(),
        Json::Null,
        Origin::User,
    );
    let _ = h.run_ok();
    assert_eq!(h.state().app.handle().focused_panel(), Some(OUTLINE));
    assert_eq!(name(walk::focused(&h)), "Outline");
}

#[test]
fn the_menu_the_status_bar_and_nothing_leave_the_last_panel_focused() {
    let mut h = harness("overview");
    walk::reach(&mut h, TAB, "Pages", 40).expect("Tab reaches the side list");
    let side = h.state().app.handle().focused_panel();
    assert!(side.is_some());
    walk::reach(&mut h, SHIFT_TAB, "No job", 10).expect("back to the status bar");
    assert_eq!(h.state().app.handle().focused_panel(), side);
    walk::reach(&mut h, SHIFT_TAB, "File", 40).expect("back to the menu");
    assert_eq!(h.state().app.handle().focused_panel(), side);
    assert_eq!(walk::press(&mut h, ESCAPE), None);
    assert_eq!(h.state().app.handle().focused_panel(), side);
    // A menu opened from the keyboard leaves the keys to the panel too.
    walk::press(&mut h, ALT_F);
    assert_eq!(h.state().app.handle().focused_panel(), side);
}

// One focused panel (item 3), in a small application with a panel of each kind.

const IMAGE: PanelId = PanelId::from_static("demo.image");
const LIST: PanelId = PanelId::from_static("demo.list");
const EMPTY: PanelId = PanelId::from_static("demo.empty");
const TOOLS: PanelId = PanelId::from_static("demo.tools");
const SEARCH: PanelId = PanelId::from_static("demo.search");

fn command(id: &str) -> CommandId {
    CommandId::new(id).unwrap()
}

#[derive(Clone, Default)]
struct Seen {
    /// The runs of the command bound to `C` on the image.
    circles: Arc<Mutex<u32>>,
    /// The runs of the step `workbench.focus`.
    focus_steps: Arc<Mutex<u32>>,
    search: Arc<Mutex<String>>,
}

struct Panels(Seen);

impl Plugin for Panels {
    fn id(&self) -> PluginId {
        PluginId::from_static("demo")
    }

    fn connect(&mut self, ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
        let Seen {
            circles,
            focus_steps,
            search,
        } = self.0.clone();
        let mut size = String::new();
        ctx.panel(PanelSpec::new(IMAGE, "Image", Area::Center), |ui, _| {
            let _ = ui.button("Zoom");
            let (_, canvas) = ui.allocate_exact_size(egui::vec2(100.0, 100.0), Sense::click());
            egui_pop::core::a11y::name(&canvas, egui::WidgetType::Image, "Canvas");
            focus::prefer(ui.ctx(), canvas.id);
        })?
        .panel(PanelSpec::new(LIST, "List", Area::StartSide), |ui, _| {
            egui::ScrollArea::vertical()
                .max_height(60.0)
                .show(ui, |ui| {
                    for i in 0..30 {
                        let _ = ui.button(format!("Row {i}"));
                    }
                });
        })?
        .panel(PanelSpec::new(EMPTY, "Empty", Area::Bottom), |ui, _| {
            ui.label("Nothing to focus");
        })?
        .panel(
            PanelSpec::new(TOOLS, "Tools", Area::EndSide)
                .palette()
                .min_size(160.0),
            move |ui, _| {
                ui.label("Brush");
                ui.text_edit_singleline(&mut size);
            },
        )?
        .panel(
            PanelSpec::new(SEARCH, "Search", Area::Bottom),
            move |ui, _| {
                ui.text_edit_singleline(&mut *search.lock().unwrap());
            },
        )?;
        ctx.command(
            CommandSpec::new(CIRCLE, "Add a circle", "Demo")
                .when("focusedPanel == 'demo.image'")
                .key("C"),
            move |_| {
                *circles.lock().unwrap() += 1;
                Ok(Json::Null)
            },
        )?;
        ctx.command(
            CommandSpec::new(GO_SEARCH, "Go to the search", "Demo").key("T"),
            |call| {
                call.app.focus_panel(&SEARCH);
                Ok(Json::Null)
            },
        )?;
        ctx.before(STEP_FOCUS, move |_, _| {
            *focus_steps.lock().unwrap() += 1;
            Before::Go
        });
        Ok(())
    }
}

const CIRCLE: CommandId = CommandId::from_static("demo.circle");
const GO_SEARCH: CommandId = CommandId::from_static("demo.go_search");

fn panels() -> (Harness<'static, App>, Seen) {
    let seen = Seen::default();
    let app = App::builder()
        .plugins(DefaultPlugins)
        .plugin(Panels(seen.clone()))
        .build(&egui::Context::default())
        .unwrap();
    let mut h = Harness::builder()
        .with_size(egui::vec2(1000.0, 700.0))
        .build_ui_state(|ui, app: &mut App| app.show(ui), app);
    let _ = h.run_ok();
    (h, seen)
}

fn run(h: &mut Harness<'_, App>, id: &str) {
    h.state_mut().run(&command(id), Json::Null, Origin::User);
    let _ = h.run_ok();
}

fn panel(h: &Harness<'_, App>) -> Option<PanelId> {
    h.state().handle().focused_panel()
}

fn moves(h: &Harness<'_, App>) -> Vec<Json> {
    let events = h.state().handle().events_since(0);
    events
        .into_iter()
        .filter(|e| e.name == "workbench.focused")
        .map(|e| e.data["panel"].clone())
        .collect()
}

#[test]
fn a_when_clause_on_the_focused_panel_follows_tab() {
    let (mut h, seen) = panels();
    walk::press(&mut h, Press::key(Key::C));
    assert_eq!(*seen.circles.lock().unwrap(), 0, "no panel has the keys");
    run(&mut h, "demo.image.focus");
    walk::press(&mut h, Press::key(Key::C));
    assert_eq!(*seen.circles.lock().unwrap(), 1);
    reach_back_in(&mut h, "Row 0");
    assert_eq!(panel(&h), Some(LIST));
    walk::press(&mut h, Press::key(Key::C));
    assert_eq!(*seen.circles.lock().unwrap(), 1, "Tab took the keys away");
    walk::reach(&mut h, TAB, "Canvas", 80).expect("Tab goes back");
    assert_eq!(panel(&h), Some(IMAGE));
    walk::press(&mut h, Press::key(Key::C));
    assert_eq!(*seen.circles.lock().unwrap(), 2);
}

#[test]
fn a_panel_with_a_canvas_gives_it_the_focus_before_its_toolbar() {
    let (mut h, _) = panels();
    run(&mut h, "demo.image.focus");
    assert_eq!(name(walk::focused(&h)), "Canvas");
    assert_eq!(panel(&h), Some(IMAGE));
}

#[test]
fn a_panel_with_nothing_to_focus_holds_the_focus_itself() {
    let (mut h, _) = panels();
    run(&mut h, "demo.empty.focus");
    let stop = walk::focused(&h).expect("the panel holds it");
    assert_eq!(stop.name, "Empty", "{stop}");
    assert!(stop.visible, "{stop}");
    assert_eq!(panel(&h), Some(EMPTY));
    // Tab goes on from there, and the panel is no stop of its own.
    let on = walk::press(&mut h, TAB).expect("Tab goes on");
    assert_ne!(on.name, "Empty", "{on}");
    let walked = walk::walk(&mut h, TAB, 200);
    let named = walked.stops.iter().filter(|s| s.name == "Empty").count();
    assert_eq!(named, 1, "its tab, the panel no more: {walked}");
}

#[test]
fn escape_from_a_panel_holding_the_focus_itself_keeps_the_panel() {
    let (mut h, _) = panels();
    run(&mut h, "demo.empty.focus");
    assert_eq!(walk::press(&mut h, ESCAPE), None);
    assert_eq!(panel(&h), Some(EMPTY));
}

#[test]
fn a_row_scrolled_out_of_its_panel_is_still_that_panels() {
    let (mut h, _) = panels();
    run(&mut h, "demo.list.focus");
    let row = walk::reach(&mut h, TAB, "Row 29", 80).expect("Tab gets there");
    assert!(row.visible, "the keyboard's focus comes into view: {row}");
    // The wheel can still take it away.
    h.event(egui::Event::PointerMoved(row.rect.center()));
    let _ = h.run_ok();
    for _ in 0..6 {
        h.event(egui::Event::MouseWheel {
            unit: egui::MouseWheelUnit::Line,
            delta: egui::vec2(0.0, 20.0),
            phase: egui::TouchPhase::Move,
            modifiers: Modifiers::NONE,
        });
        let _ = h.run_ok();
    }
    let stop = walk::focused(&h).expect("it keeps the focus");
    assert!(!stop.visible, "this test needs it out of view: {stop}");
    assert_eq!(panel(&h), Some(LIST));
}

#[test]
fn a_panel_focus_command_leaves_a_focus_already_inside_where_it_is() {
    let (mut h, _) = panels();
    run(&mut h, "demo.list.focus");
    assert_eq!(name(walk::focused(&h)), "Row 0");
    assert_eq!(name(walk::press(&mut h, TAB)), "Row 1");
    run(&mut h, "demo.list.focus");
    assert_eq!(name(walk::focused(&h)), "Row 1");
}

#[test]
fn each_change_of_panel_is_published_once_and_runs_no_step() {
    let (mut h, seen) = panels();
    run(&mut h, "demo.list.focus");
    assert_eq!(*seen.focus_steps.lock().unwrap(), 1);
    assert_eq!(
        moves(&h),
        [json!("demo.list")],
        "the command, then its widget"
    );
    // Within a panel, nothing to say.
    walk::press(&mut h, TAB);
    assert_eq!(moves(&h).len(), 1);
    // Tab to another panel: one, and no step to loop back through.
    walk::reach(&mut h, TAB, "Canvas", 80).expect("Tab gets there");
    assert_eq!(moves(&h), [json!("demo.list"), json!("demo.image")]);
    assert_eq!(*seen.focus_steps.lock().unwrap(), 1);
}

#[test]
fn a_press_moves_the_panel_and_the_focus_going_away_does_not_bring_it_back() {
    let (mut h, _) = panels();
    run(&mut h, "demo.list.focus");
    h.get_by_label("Nothing to focus").click();
    let _ = h.run_ok();
    assert_eq!(panel(&h), Some(EMPTY));
    assert_eq!(walk::focused(&h), None);
    let _ = h.run_ok();
    assert_eq!(panel(&h), Some(EMPTY));
}

#[test]
fn a_floating_palette_only_borrows_the_keys() {
    let (mut h, _) = panels();
    run(&mut h, "demo.image.focus");
    // A press on its background: the keys stay on the image.
    h.get_by_label("Brush").click();
    let _ = h.run_ok();
    assert_eq!(panel(&h), Some(IMAGE));
    // Its field takes the keyboard, and gives it back.
    h.get_by_role(Role::TextInput).click();
    let _ = h.run_ok();
    assert_eq!(panel(&h), Some(TOOLS));
    assert_eq!(walk::press(&mut h, ESCAPE), None);
    assert_eq!(panel(&h), Some(IMAGE));
    // The same when it is given the keyboard, then closed.
    run(&mut h, "demo.tools.focus");
    assert_eq!(panel(&h), Some(TOOLS));
    assert!(walk::focused(&h).is_some());
    h.state().handle().set_panel_visible(&TOOLS, false);
    let _ = h.run_ok();
    assert_eq!(panel(&h), Some(IMAGE));
}

#[test]
fn the_key_that_focuses_a_panel_is_not_typed_into_it() {
    let (mut h, seen) = panels();
    h.event(egui::Event::Key {
        key: Key::T,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: Modifiers::NONE,
    });
    h.event(egui::Event::Text("t".to_string()));
    let _ = h.run_ok();
    assert_eq!(panel(&h), Some(SEARCH));
    let stop = walk::focused(&h).expect("the field has the keyboard");
    assert_eq!(stop.role, Role::TextInput, "{stop}");
    assert_eq!(*seen.search.lock().unwrap(), "");
}

#[test]
fn behind_a_modal_a_panel_focus_command_moves_only_the_logical_focus() {
    let (mut h, _) = panels();
    let d = h.state().handle().service(&dialogs::DIALOGS).unwrap();
    d.open(Confirm::new("A question?", "Accept", CIRCLE));
    let _ = h.run_ok();
    let before = name(walk::focused(&h));
    assert_eq!(before, "Accept");
    run(&mut h, "demo.list.focus");
    assert_eq!(panel(&h), Some(LIST));
    assert_eq!(name(walk::focused(&h)), before);
}

#[test]
fn an_agent_does_not_take_the_keyboard_from_someone_typing() {
    let (mut h, _) = panels();
    run(&mut h, "demo.search.focus");
    let field = walk::focused(&h).expect("the field has the keyboard");
    assert_eq!(field.role, Role::TextInput, "{field}");
    let agent = Origin::Agent("a1".to_string());
    h.state_mut()
        .run(&command("demo.list.focus"), Json::Null, agent);
    let _ = h.run_ok();
    assert_eq!(panel(&h), Some(LIST));
    assert_eq!(walk::focused(&h).map(|s| s.id), Some(field.id));
}

#[test]
fn a_detached_panel_takes_the_focus_and_the_keys() {
    let (mut h, _) = panels();
    run(&mut h, "demo.image.focus");
    h.state().handle().detach_panel(&LIST);
    let _ = h.run_ok();
    assert_eq!(panel(&h), Some(IMAGE));
    h.get_by_label("Row 1").click();
    let _ = h.run_ok();
    assert_eq!(panel(&h), Some(LIST), "a press in its window");
    run(&mut h, "demo.image.focus");
    assert_eq!(name(walk::focused(&h)), "Canvas");
    run(&mut h, "demo.list.focus");
    assert_eq!(name(walk::focused(&h)), "Row 0");
    assert_eq!(panel(&h), Some(LIST));
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
#[ignore = "keyboard survey item 9: a side area's line is a stop with no role and no name"]
fn every_stop_says_what_it_is() {
    let lines = nameless("overview");
    assert_eq!(lines, Vec::<Stop>::new());
}

#[test]
#[ignore = "keyboard survey item 9: a side area's line shows nothing when focused"]
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
fn a_frames_caption_is_no_stop() {
    // The frame page draws the frame's own title bar as its preview.
    let caption = nameless("frame");
    assert!(
        !caption.iter().any(|s| s.rect.width() > 100.0),
        "{caption:?}"
    );
}

#[test]
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
fn a_side_areas_edge_lying_over_the_centre_is_no_stop() {
    let (mut h, _) = panels();
    let app = h.state().handle().clone();
    let placed = app.place_area(Area::StartSide, Placement::Over).unwrap();
    assert!(placed.ok(), "{placed:?}");
    let _ = h.run_ok();
    let rect = app.area_rect(Area::StartSide).expect("drawn");
    // Nothing focused, Tab would hide the palettes.
    run(&mut h, "demo.list.focus");
    let walked = walk::walk(&mut h, TAB, 200);
    assert!(matches!(walked.end, End::Looped { .. }), "{walked}");
    let edge = walked
        .stops
        .iter()
        .find(|s| s.name.is_empty() && (s.rect.center().x - rect.max.x).abs() < 10.0);
    assert!(edge.is_none(), "{walked}");
}

#[test]
fn a_menus_frame_is_no_stop() {
    let mut h = harness("overview");
    walk::press(&mut h, ALT_F);
    let walked = walk::walk(&mut h, TAB, 3);
    assert_ne!(walked.stops[1].name, "", "{walked}");
    // Nor on the way back: the first item, then the menu's button.
    let settings = walked.stops[1].name.clone();
    assert_eq!(name(walk::press(&mut h, SHIFT_TAB)), settings, "{walked}");
    assert_eq!(name(walk::press(&mut h, SHIFT_TAB)), "File", "{walked}");
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
fn after_escape_tab_goes_on_from_the_place() {
    let mut h = harness("dialogs");
    reach_back(&mut h, "Pick a side");
    assert_eq!(walk::press(&mut h, ESCAPE), None);
    assert_ne!(name(walk::press(&mut h, TAB)), "File");
}

#[test]
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
fn a_button_rearranging_the_panels_keeps_the_focus() {
    let mut h = harness("workbench");
    reach_back(&mut h, "Open the demonstration panels");
    assert_eq!(
        name(walk::press(&mut h, ENTER)),
        "Open the demonstration panels"
    );
}

#[test]
fn a_button_closing_the_panels_keeps_the_focus() {
    let mut h = with_panels();
    reach_back(&mut h, "Close them");
    assert_eq!(name(walk::press(&mut h, ENTER)), "Close them");
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
    focus::install(&h.ctx);
    let _ = h.run_ok();
    h
}

#[test]
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

// Focus into view (checks 1, 10, 11; item 7). Plain areas, nesting and the
// pointer are in `crates/egui-pop-test/tests/focus_into_view.rs`.

#[test]
fn the_side_list_scrolls_to_the_focus() {
    let mut h = harness("overview");
    let walked = walk::walk(&mut h, TAB, 200);
    assert!(walked.stops.iter().all(|s| s.visible), "{walked}");
}

#[test]
fn a_long_form_scrolls_to_the_focus() {
    let mut h = harness("settings");
    let walked = walk::walk(&mut h, SHIFT_TAB, 200);
    assert!(walked.stops.iter().all(|s| s.visible), "{walked}");
}

#[test]
fn every_stop_of_the_first_page_comes_into_view_both_ways() {
    let mut h = harness("overview");
    let forth = walk::walk(&mut h, TAB, 200);
    assert_eq!(forth.end, End::Looped { at: 0 }, "{forth}");
    assert!(forth.stops.iter().all(|s| s.visible), "{forth}");
    let back = walk::walk(&mut h, SHIFT_TAB, 200);
    assert_eq!(back.stops.len(), forth.stops.len(), "{back}");
    assert!(back.stops.iter().all(|s| s.visible), "{back}");
}

#[test]
fn tab_goes_through_windowed_rows_past_those_drawn() {
    let mut h = rows();
    walk::reach(&mut h, TAB, "Row 40", 100).expect("Tab reaches a row below the window");
}

// F6 (check 6, item 5).

#[test]
fn f6_goes_between_the_inputs_pages_scopes_and_on() {
    let mut h = harness("inputs");
    walk::reach(&mut h, F6, "Street", 6).expect("F6 reaches the page's first scope");
    assert_eq!(name(walk::press(&mut h, F6)), "Tags");
    assert_eq!(name(walk::press(&mut h, SHIFT_F6)), "Street");
    assert_eq!(name(walk::press(&mut h, F6)), "Tags");
    // Their turn over, F6 goes on: they keep no one in.
    assert_ne!(name(walk::press(&mut h, F6)), "Street");
}

#[test]
fn f6_goes_to_a_region_on_every_page() {
    for page in ["overview", "workbench", "menus", "settings"] {
        let mut h = harness(page);
        assert!(walk::press(&mut h, F6).is_some(), "{page}");
        assert!(walk::press(&mut h, SHIFT_F6).is_some(), "{page}");
    }
}

// F6 through the shell (item 5).

/// The names F6 goes through once round, from where the focus is.
fn f6_round(h: &mut Harness<'_, App>, key: Press) -> Vec<String> {
    let walked = walk::walk(h, key, 12);
    assert!(matches!(walked.end, End::Looped { at: 0 }), "{walked}");
    walked.stops.into_iter().map(|s| s.name).collect()
}

#[test]
fn f6_goes_through_the_regions_in_order_and_shift_f6_back() {
    let (mut h, _) = panels();
    let round = f6_round(&mut h, F6);
    // The side area, the bottom one (a panel of text in front, holding the
    // focus itself), the centre by what it prefers, the palette's unnamed field.
    assert_eq!(round, ["Row 0", "Empty", "Canvas", ""]);
    let (mut h, _) = panels();
    assert_eq!(f6_round(&mut h, SHIFT_F6), ["", "Canvas", "Empty", "Row 0"]);
}

#[test]
fn a_region_remembers_its_last_focus() {
    let (mut h, _) = panels();
    assert_eq!(name(walk::press(&mut h, F6)), "Row 0");
    assert_eq!(name(walk::press(&mut h, TAB)), "Row 1");
    assert_eq!(name(walk::press(&mut h, F6)), "Empty");
    assert_eq!(name(walk::press(&mut h, SHIFT_F6)), "Row 1");
}

#[test]
fn f6_after_escape_goes_on_from_the_place() {
    let (mut h, _) = panels();
    run(&mut h, "demo.search.focus");
    assert_eq!(walk::press(&mut h, ESCAPE), None);
    assert_eq!(name(walk::press(&mut h, F6)), "Canvas");
}

#[test]
fn f6_into_a_panel_of_text_alone_gives_it_the_keys() {
    let (mut h, _) = panels();
    run(&mut h, "demo.empty.focus");
    let own = walk::focused(&h).expect("the panel holds the focus");
    assert_eq!(own.name, "Empty");
    assert_eq!(panel(&h), Some(EMPTY));
    assert_eq!(name(walk::press(&mut h, F6)), "Canvas");
    assert_eq!(panel(&h), Some(IMAGE));
    assert_eq!(name(walk::press(&mut h, SHIFT_F6)), "Empty");
    assert_eq!(panel(&h), Some(EMPTY));
}

#[test]
fn f6_is_the_way_in_where_tab_hides_the_palettes() {
    let (mut h, _) = panels();
    // Nothing focused: Tab hides them, and they keep their rule.
    assert_eq!(walk::press(&mut h, TAB), None);
    assert!(palettes::hidden(h.state().handle()));
    assert_eq!(walk::press(&mut h, TAB), None);
    assert!(!palettes::hidden(h.state().handle()));
    // F6 goes in, to a palette as to the rest, and Tab then moves.
    let field = walk::press(&mut h, SHIFT_F6).expect("F6 goes in");
    assert_eq!(field.role, Role::TextInput, "{field}");
    assert_eq!(panel(&h), Some(TOOLS));
    assert!(walk::press(&mut h, TAB).is_some());
    assert!(!palettes::hidden(h.state().handle()));
    // Hidden, a palette is no region.
    let (mut h, _) = panels();
    walk::press(&mut h, TAB);
    assert!(palettes::hidden(h.state().handle()));
    assert_eq!(f6_round(&mut h, F6), ["Row 0", "Empty", "Canvas"]);
}

#[test]
fn f6_does_not_change_what_tab_goes_through() {
    let (mut h, _) = panels();
    run(&mut h, "demo.list.focus");
    assert_eq!(name(walk::press(&mut h, TAB)), "Row 1");
    assert_eq!(name(walk::press(&mut h, SHIFT_TAB)), "Row 0");
}

#[test]
fn f6_stays_behind_a_modal() {
    let (mut h, _) = panels();
    let d = h.state().handle().service(&dialogs::DIALOGS).unwrap();
    d.open(Confirm::new("A question?", "Accept", CIRCLE));
    let _ = h.run_ok();
    assert_eq!(name(walk::focused(&h)), "Accept");
    assert_eq!(name(walk::press(&mut h, F6)), "Accept");
    assert_eq!(name(walk::press(&mut h, SHIFT_F6)), "Accept");
}

#[test]
fn f6_leaves_nothing_open_behind_it() {
    // A popover keeps the focus, for Escape first.
    let mut h = popover();
    assert_eq!(name(walk::press(&mut h, TAB)), "Dismiss all");
    assert_eq!(name(walk::press(&mut h, F6)), "Dismiss all");
    assert!(h.query_by_label("Dismiss all").is_some());
    // So does an open menu.
    let mut h = harness("overview");
    walk::press(&mut h, ALT_F);
    let _ = h.run_ok();
    assert!(egui::Popup::is_any_open(&h.ctx));
    assert_eq!(walk::press(&mut h, F6), None);
    assert!(egui::Popup::is_any_open(&h.ctx));
}

#[test]
fn an_open_flyout_is_a_region() {
    let mut h = harness("workbench");
    reach_back(&mut h, "Start side: fold");
    walk::press(&mut h, ENTER);
    walk::reach(&mut h, TAB, "Side bar", 60).expect("Tab reaches the strip's icon");
    walk::press(&mut h, ENTER);
    assert!(h.state().app.handle().flyout().is_some());
    let inside = |h: &Harness<'_, Gallery>| {
        let rect = h.state().app.handle().flyout().and_then(|f| f.1).unwrap();
        walk::focused(h).is_some_and(|s| rect.contains(s.rect.center()))
    };
    let found = (0..12).any(|_| {
        walk::press(&mut h, F6);
        inside(&h)
    });
    assert!(found, "F6 reaches the flyout");
    // It goes on from there, the flyout still open.
    walk::press(&mut h, F6);
    assert!(!inside(&h));
    assert!(h.state().app.handle().flyout().is_some());
}

#[test]
fn f6_in_zen_mode_has_the_centre_alone() {
    let (mut h, _) = panels();
    h.state().handle().set_zen(true);
    let _ = h.run_ok();
    assert_eq!(f6_round(&mut h, F6), ["Canvas"]);
}

#[test]
fn regions_go_by_rank_passing_over_one_with_nothing_to_focus() {
    let mut h = themed(&ThemeSpec::default(), egui::vec2(400.0, 300.0), |ui| {
        focus::region(ui, &focus::Region::new("last", 2), |ui| {
            let _ = ui.button("Last");
        });
        focus::region(ui, &focus::Region::new("text", 1), |ui| {
            ui.label("Only text");
        });
        focus::region(ui, &focus::Region::new("first", 0), |ui| {
            let _ = ui.button("First");
        });
        let f6 = ui.input(|i| i.key_pressed(Key::F6));
        if f6 && let Some(hop) = regions::next(ui.ctx(), false) {
            ui.memory_mut(|m| m.request_focus(hop.target.expect("no region holds it")));
        }
    });
    let _ = h.run_ok();
    let walked = walk::walk(&mut h, F6, 6);
    assert_eq!(walked.names(), ["First", "Last"], "{walked}");
}

#[test]
fn f6_is_a_binding_like_any_other() {
    let (mut h, _) = panels();
    let handle = h.state().handle().clone();
    assert_eq!(
        handle.keys_for(&egui_pop::api::workbench::CMD_NEXT_REGION),
        ["F6"]
    );
    assert_eq!(
        handle.keys_for(&egui_pop::api::workbench::CMD_PREVIOUS_REGION),
        ["Shift+F6"]
    );
    // An agent's run moves it too, unless someone is typing.
    let agent = Origin::Agent("a1".to_string());
    h.state_mut().run(
        &egui_pop::api::workbench::CMD_NEXT_REGION,
        Json::Null,
        agent.clone(),
    );
    let _ = h.run_ok();
    assert_eq!(name(walk::focused(&h)), "Row 0");
    run(&mut h, "demo.search.focus");
    let field = walk::focused(&h).expect("the field has the keyboard");
    assert_eq!(field.role, Role::TextInput, "{field}");
    h.state_mut().run(
        &egui_pop::api::workbench::CMD_NEXT_REGION,
        Json::Null,
        agent,
    );
    let _ = h.run_ok();
    assert_eq!(walk::focused(&h).map(|s| s.id), Some(field.id));
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
    assert!(walked.stops[1].name.starts_with("Settings"), "{walked}");
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

/// Goes to a stop of the small harness, by keys.
#[track_caller]
fn reach_back_in(h: &mut Harness<'_, App>, name: &str) -> Stop {
    walk::reach(h, TAB, name, 60).unwrap_or_else(|| panic!("Tab reaches {name}"))
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
fn escape_from_a_dialog_gives_the_focus_back_to_its_button() {
    let mut h = harness("dialogs");
    reach_back(&mut h, "Confirm an export");
    walk::press(&mut h, SPACE);
    assert_eq!(name(walk::press(&mut h, ESCAPE)), "Confirm an export");
}

#[test]
fn escape_from_the_palette_gives_the_focus_back() {
    let mut h = harness("dialogs");
    reach_back(&mut h, "Confirm an export");
    assert_eq!(
        name(walk::press(&mut h, Press::key(Key::F1))),
        "Command palette"
    );
    assert_eq!(name(walk::press(&mut h, ESCAPE)), "Confirm an export");
}

#[test]
fn answering_a_dialog_gives_the_focus_back_too() {
    let mut h = harness("dialogs");
    reach_back(&mut h, "Confirm an export");
    walk::press(&mut h, ENTER);
    // Accepted by Enter.
    assert_eq!(name(walk::press(&mut h, ENTER)), "Confirm an export");
    assert_ne!(sent(&h), "");
    // Cancelled by its button, with Tab and Space.
    walk::press(&mut h, ENTER);
    assert_eq!(name(walk::press(&mut h, TAB)), "Cancel");
    assert_eq!(name(walk::press(&mut h, SPACE)), "Confirm an export");
}

#[test]
fn a_held_enter_accepting_a_dialog_does_not_click_the_button_behind_it() {
    let mut h = harness("dialogs");
    reach_back(&mut h, "Confirm an export");
    walk::press(&mut h, ENTER);
    assert_eq!(name(walk::hold(&mut h, ENTER)), "Confirm an export");
    assert!(h.query_by_label("Export the page?").is_none());
    for _ in 0..3 {
        walk::hold(&mut h, ENTER);
        assert!(h.query_by_label("Export the page?").is_none());
    }
    walk::release(&mut h, ENTER);
    // Released, the button works again.
    assert_eq!(name(walk::press(&mut h, ENTER)), "Export");
}

#[test]
fn a_queue_gives_the_focus_back_to_what_held_it_before_the_first() {
    let mut h = harness("dialogs");
    reach_back(&mut h, "Confirm an export");
    let d = h
        .state()
        .app
        .handle()
        .service(&dialogs::DIALOGS)
        .expect("default plugin");
    // The button opens one, and a second waits behind it.
    walk::press(&mut h, ENTER);
    d.open(Confirm::new(
        "Second?",
        "Accept",
        CommandId::from_static("none"),
    ));
    walk::press(&mut h, ESCAPE);
    assert!(h.query_by_label("Second?").is_some());
    assert_eq!(name(walk::focused(&h)), "Accept");
    assert_eq!(name(walk::press(&mut h, ESCAPE)), "Confirm an export");
}

#[test]
fn a_dialog_opened_from_the_palette_gives_the_focus_back_to_what_held_it_before_the_palette() {
    let (mut h, runs) = asking();
    reach_back_in(&mut h, "File");
    walk::press(&mut h, Press::key(Key::F1));
    h.event(egui::Event::Text("Ask a question".to_string()));
    let _ = h.run_ok();
    walk::press(&mut h, ENTER);
    assert!(h.query_by_label("A question?").is_some());
    assert_eq!(name(walk::press(&mut h, ENTER)), "File");
    assert_eq!(*runs.lock().unwrap(), ["answered"]);
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

struct ShellPanel(TerminalState);

const SHELL_PANEL: PanelId = PanelId::from_static("shell.panel");

impl Plugin for ShellPanel {
    fn id(&self) -> PluginId {
        PluginId::from_static("shell")
    }

    fn connect(&mut self, ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
        let state = self.0.clone();
        ctx.panel(
            PanelSpec::new(SHELL_PANEL, "Shell", Area::Center),
            move |ui, _| {
                let _ = Terminal::new().label("Shell").show(ui, &state);
            },
        )?;
        ctx.panel(PanelSpec::new(LIST, "List", Area::StartSide), |ui, _| {
            let _ = ui.button("Row");
        })?;
        Ok(())
    }
}

#[test]
fn a_focused_terminal_in_a_panel_keeps_f6() {
    let recorder = Recorder::default();
    let state = TerminalState::new("walk.shell", recorder.clone());
    let app = App::builder()
        .plugins(DefaultPlugins)
        .plugin(ShellPanel(state))
        .build(&egui::Context::default())
        .unwrap();
    let mut h = Harness::builder()
        .with_size(egui::vec2(1000.0, 700.0))
        .build_ui_state(|ui, app: &mut App| app.show(ui), app);
    let _ = h.run_ok();
    // F6 goes in; there it is the program's, and Shift+Escape the way out.
    assert_eq!(name(walk::press(&mut h, F6)), "Row");
    assert_eq!(name(walk::press(&mut h, F6)), "Shell");
    assert_eq!(name(walk::press(&mut h, F6)), "Shell");
    assert_eq!(recorder.0.lock().unwrap().as_slice(), b"\x1b[17~");
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
