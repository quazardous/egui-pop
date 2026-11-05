//! The shell's hooks for applications: the title zone above the menu
//! bar, no menu bar, an edge-to-edge main area, frameless areas, a
//! starting layout, the top bar's end zone, a wrapper around the whole
//! drawing, `on_exit` and the `app.exit` step, the window's place
//! restored.

use std::cell::Cell;
use std::rc::Rc;

use egui_kittest::Harness;
use egui_kittest::kittest::Queryable as _;
use egui_pop_api::app::{MENU_BAR_END, STEP_EXIT};
use egui_pop_api::arrangements;
use egui_pop_api::workbench::{PANEL_INSPECTOR, PANEL_MAIN, PANEL_SIDE_BAR};
use egui_pop_api::zones::{self, Block};
use egui_pop_api::{App, AppBuilder, Ctx, DefaultPlugins, Plugin, PluginError};
use egui_pop_model::commands::CommandSpec;
use egui_pop_model::ids::{BlockId, CommandId, PanelId, PluginId};
use egui_pop_model::layout::{Layout, Node};
use egui_pop_model::panels::{Area, PanelSpec};
use serde_json::json;

struct Main;

impl Plugin for Main {
    fn id(&self) -> PluginId {
        PluginId::from_static("main")
    }

    fn connect(&mut self, ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
        ctx.block(
            zones::MAIN,
            Block::add(BlockId::from_static("main.body"), 0, |ui, _| {
                ui.label("main body");
            }),
        )
        .block(
            zones::TITLE,
            Block::add(BlockId::from_static("main.title"), 0, |ui, _| {
                ui.label("my own title bar");
            }),
        );
        Ok(())
    }
}

fn harness(b: AppBuilder) -> Harness<'static, App> {
    let app = b.plugin(Main).build(&egui::Context::default()).unwrap();
    let mut h = Harness::builder()
        .with_size(egui::vec2(600.0, 400.0))
        .build_ui_state(|ui, app: &mut App| app.show_wrapped(ui), app);
    h.run();
    h
}

#[test]
fn the_title_zone_is_above_the_menu_bar() {
    let h = harness(App::builder());
    let title = h.get_by_label("my own title bar").rect();
    let file = h.get_by_label("File").rect();
    assert!(title.bottom() <= file.top() + 0.5, "{title:?} {file:?}");
}

#[test]
fn the_menu_bar_may_be_left_out() {
    let with = harness(App::builder());
    let main = with.get_by_label("main body").rect();
    let without = harness(App::builder().menu_bar(false));
    assert!(without.query_by_label("File").is_none());
    // The title zone stays, and the centre takes the bar's room.
    without.get_by_label("my own title bar");
    let higher = without.get_by_label("main body").rect();
    assert!(higher.top() < main.top(), "{higher:?} {main:?}");
}

#[test]
fn the_main_area_may_be_drawn_edge_to_edge() {
    let framed = harness(App::builder()).get_by_label("main body").rect();
    let edge = harness(App::builder().main_edge_to_edge(true))
        .get_by_label("main body")
        .rect();
    assert!(edge.left() < framed.left(), "{edge:?} {framed:?}");
}

const TREE: PanelId = PanelId::from_static("demo.tree");
const LOG: PanelId = PanelId::from_static("demo.log");
const VIEW: PanelId = PanelId::from_static("demo.view");

/// A tree on the start side and a log at the bottom, each saying where it
/// is drawn.
struct Areas;

impl Plugin for Areas {
    fn id(&self) -> PluginId {
        PluginId::from_static("areas")
    }

    fn connect(&mut self, ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
        ctx.panel(PanelSpec::new(TREE, "Tree", Area::StartSide), |ui, _| {
            ui.label("tree body");
        })?
        .panel(PanelSpec::new(LOG, "Log", Area::Bottom), |ui, _| {
            ui.label("log body");
        })?
        .panel(PanelSpec::new(VIEW, "View", Area::Center), |ui, _| {
            ui.label("view body");
        })?;
        Ok(())
    }
}

fn areas(b: AppBuilder) -> Harness<'static, App> {
    harness(b.plugin(Areas))
}

#[test]
fn side_and_bottom_areas_may_be_frameless() {
    let framed = areas(App::builder());
    let bare = areas(App::builder().frameless(&[Area::StartSide, Area::Bottom]));
    for label in ["tree body", "log body"] {
        let f = framed.get_by_label(label).rect();
        let b = bare.get_by_label(label).rect();
        assert!(b.left() < f.left(), "{label}: {b:?} {f:?}");
    }
    // The tree runs to its area's edge, where its tab starts.
    let tree = bare.get_by_label("tree body").rect();
    let tab = bare.get_by_label("Tree").rect();
    assert_eq!(tree.left(), tab.left());
    assert!(bare.state().handle().frameless(Area::Bottom));
    assert!(!bare.state().handle().frameless(Area::EndSide));
}

#[test]
fn the_main_area_may_be_frameless() {
    let view = || Layout {
        areas: vec![(Area::Center, Node::tab(VIEW))],
        hidden: vec![PANEL_MAIN],
        ..Layout::default()
    };
    let edge = areas(App::builder().layout(view()).main_edge_to_edge(true))
        .get_by_label("view body")
        .rect();
    // No margin around the area, and none in its panels' bodies either.
    let bare = areas(App::builder().layout(view()).frameless(&[Area::Center]))
        .get_by_label("view body")
        .rect();
    assert!(bare.left() < edge.left(), "{bare:?} {edge:?}");
}

fn starting() -> Layout {
    Layout {
        areas: vec![
            (Area::EndSide, Node::tab(TREE)),
            (Area::Bottom, Node::tab(LOG)),
        ],
        hidden: vec![PANEL_SIDE_BAR, PANEL_INSPECTOR],
        ..Layout::default()
    }
}

#[test]
fn the_builder_gives_the_starting_layout() {
    let h = areas(
        App::builder()
            .plugins(DefaultPlugins)
            .layout(starting())
            .folded(&[Area::Bottom]),
    );
    let app = h.state().handle().clone();
    let layout = app.layout();
    assert_eq!(layout.area_of(&TREE), Some(Area::EndSide));
    assert_eq!(layout.area_of(&LOG), Some(Area::Bottom));
    assert_eq!(layout.folded, vec![Area::Bottom]);
    assert!(!app.panel_visible(&PANEL_SIDE_BAR));
    assert!(!app.panel_visible(&PANEL_INSPECTOR));
    // The main zone has its block: placed, as the layout does not hide it.
    assert!(app.panel_visible(&PANEL_MAIN));
    // Moved, then reset: the `Default` arrangement is the starting layout.
    app.move_panel(&TREE, Area::StartSide);
    assert_eq!(app.layout().area_of(&TREE), Some(Area::StartSide));
    assert_eq!(arrangements::current(&app), arrangements::DEFAULT);
    assert!(arrangements::reset(&app).ok());
    assert_eq!(app.layout().area_of(&TREE), Some(Area::EndSide));
    assert!(!app.panel_visible(&PANEL_SIDE_BAR));
}

#[test]
fn without_a_layout_panels_start_in_their_default_areas() {
    let h = areas(App::builder().plugins(DefaultPlugins));
    let app = h.state().handle().clone();
    assert_eq!(app.layout().area_of(&TREE), Some(Area::StartSide));
    assert!(arrangements::reset(&app).ok());
    assert_eq!(app.layout().area_of(&TREE), Some(Area::StartSide));
}

const CMD_PIN: CommandId = CommandId::from_static("demo.pin");

/// A widget in the top bar's end zone, beside a command at the bar's end.
struct BarEnd;

impl Plugin for BarEnd {
    fn id(&self) -> PluginId {
        PluginId::from_static("bar_end")
    }

    fn connect(&mut self, ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
        ctx.block(
            zones::TOP_BAR_END,
            Block::add(BlockId::from_static("bar_end.name"), 0, |ui, _| {
                ui.label("recording");
                ui.label("frame 12");
            }),
        );
        ctx.command(
            CommandSpec::new(CMD_PIN, "Pin", "Demo").menu(MENU_BAR_END, "1_demo", 0),
            |_| Ok(serde_json::Value::Null),
        )?;
        Ok(())
    }
}

#[test]
fn the_top_bars_end_takes_widgets_before_its_commands() {
    let h = harness(App::builder().plugin(BarEnd));
    let file = h.get_by_label("File").rect();
    let name = h.get_by_label("recording").rect();
    let frame = h.get_by_label("frame 12").rect();
    let pin = h.get_by_label("Pin").rect();
    // Left to right, after the menus, before the command, at the end.
    assert!(file.right() < name.left(), "{file:?} {name:?}");
    assert!(name.right() <= frame.left(), "{name:?} {frame:?}");
    assert!(frame.right() <= pin.left(), "{frame:?} {pin:?}");
    assert!(name.left() > 300.0, "at the bar's end: {name:?}");
    // On one line, the bar's.
    assert!((name.center().y - pin.center().y).abs() < 2.0);
}

#[test]
fn the_top_bars_end_shows_without_the_menu_bar() {
    let h = harness(App::builder().menu_bar(false).plugin(BarEnd));
    assert!(h.query_by_label("File").is_none());
    let name = h.get_by_label("recording").rect();
    assert!(name.left() > 300.0, "at the bar's end: {name:?}");
}

#[test]
fn a_wrapper_draws_around_the_application() {
    let h = harness(App::builder().wrap(|ui, _app, inner| {
        egui::Panel::bottom("wrapper").show(ui, |ui| {
            ui.label("drawn around");
        });
        inner(ui);
    }));
    assert!(h.query_by_label("drawn around").is_some());
    assert!(h.query_by_label("main body").is_some());
}

#[test]
fn exiting_runs_on_exit_then_the_exit_step() {
    let ran = Rc::new(Cell::new(false));
    let r = ran.clone();
    let mut app = App::builder()
        .on_exit(move |_| r.set(true))
        .plugin(Main)
        .build(&egui::Context::default())
        .unwrap();
    let out = app.exit();
    assert!(ran.get());
    assert_eq!(out.result(), Some(&Ok(json!({ "saved": true }))));
    let t = app.handle().step_traces().pop().unwrap();
    assert_eq!(t.step, STEP_EXIT);
}

#[test]
fn the_windows_place_is_restored_from_the_settings_directory() {
    let dir = std::env::temp_dir().join(format!("egui-pop-restore-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let app = App::builder()
        .plugin(egui_pop_api::window::WindowPlugin)
        .persist(&dir)
        .build(&egui::Context::default())
        .unwrap();
    egui_pop_core::lock(&app.handle().store())
        .set(
            egui_pop_model::values::Set::new(
                egui_pop_api::window::RECT,
                egui_pop_model::values::Change::To(egui_pop_model::values::Value::Floats(vec![
                    10.0, 20.0, 900.0, 700.0,
                ])),
                egui_pop_model::commands::Origin::User,
            )
            .force(),
        )
        .unwrap();
    app.save().unwrap();
    let mut options = egui_pop_api::eframe::NativeOptions::default();
    let state = egui_pop_api::window::restore(&dir, &mut options).expect("kept");
    assert_eq!(state.size, egui::vec2(900.0, 700.0));
    assert_eq!(options.viewport.inner_size, Some(egui::vec2(900.0, 700.0)));
    assert!(egui_pop_api::window::restore(dir.join("none"), &mut options).is_none());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn saving_does_not_wait_for_the_exit_and_a_vetoed_exit_writes_nothing() {
    use egui_pop_core::motion;
    use egui_pop_model::values::{Change, Set, Value};
    let dir = std::env::temp_dir().join(format!("egui-pop-autosave-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    struct Stay;
    impl Plugin for Stay {
        fn id(&self) -> PluginId {
            PluginId::from_static("stay")
        }
        fn connect(&mut self, ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
            ctx.before(STEP_EXIT, |_, _| {
                egui_pop_api::steps::Before::Veto("not now".into())
            });
            Ok(())
        }
    }
    let ctx = egui::Context::default();
    motion::freeze(&ctx, 0.0);
    let mut app = App::builder()
        .plugin(Main)
        .plugin(Stay)
        .persist(&dir)
        .build(&ctx)
        .unwrap();
    // Nothing changed: nothing written.
    app.logic(&ctx);
    assert!(!dir.join("settings.json").exists());
    egui_pop_core::lock(&app.handle().store())
        .set(Set::new(
            &format!("settings/{}", egui_pop_api::SETTING_FOCUS),
            Change::To(Value::Choice("hover".into())),
            egui_pop_model::commands::Origin::User,
        ))
        .unwrap();
    // A second after the change, the debounced writer has written it.
    for t in [0.0, 0.5] {
        motion::freeze(&ctx, t);
        app.logic(&ctx);
        assert!(!dir.join("settings.json").exists(), "at {t}");
    }
    motion::freeze(&ctx, 1.5);
    app.logic(&ctx);
    let text = std::fs::read_to_string(dir.join("settings.json")).unwrap();
    assert!(text.contains("hover"), "{text}");
    // The exit is vetoed: it writes nothing more.
    std::fs::remove_file(dir.join("settings.json")).unwrap();
    let out = app.exit();
    assert_eq!(out.vetoed(), Some("not now"));
    assert!(!dir.join("settings.json").exists());
    let _ = std::fs::remove_dir_all(&dir);
}
