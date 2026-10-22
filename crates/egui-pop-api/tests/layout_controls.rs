//! The layout, as VS Code arranges it: each area shown or hidden by its
//! toggle and its key, through the fold step; the primary side bar on
//! the left or the right, each change a setting undone as one step; the
//! title bar's controls (their icons following the areas) and the
//! layout menu (arrangements applied and saved, the options).

use egui::accesskit::Toggled;
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT as _, Queryable as _};
use egui_pop_api::arrangements;
use egui_pop_api::dialogs::{Answer, DIALOGS};
use egui_pop_api::frame::FramePlugin;
use egui_pop_api::layout_controls::{
    self, CMD_TOGGLE_PANEL, CMD_TOGGLE_PRIMARY_SIDE_BAR, CMD_TOGGLE_SECONDARY_SIDE_BAR,
    MENU_LAYOUT, SETTING_SIDE_BAR_POSITION, SideBarPosition, TOGGLES,
};
use egui_pop_api::menus;
use egui_pop_api::strips::{self, StripsPlugin};
use egui_pop_api::workbench::{self, PanelAlignment, STEP_FOLD};
use egui_pop_api::zones::{self, Block};
use egui_pop_api::{App, AppHandle, Ctx, DefaultPlugins, Plugin, PluginError};
use egui_pop_core::icon::Icon;
use egui_pop_model::commands::Origin;
use egui_pop_model::ids::{BlockId, PanelId, PluginId};
use egui_pop_model::layout::{Layout, Node};
use egui_pop_model::panels::{Area, PanelSpec};
use egui_pop_model::values::{Change, Set, Value};
use serde_json::Value as Json;

const LOG: PanelId = PanelId::from_static("demo.log");
const TREE: PanelId = PanelId::from_static("demo.tree");

struct Demo;

impl Plugin for Demo {
    fn id(&self) -> PluginId {
        PluginId::from_static("demo")
    }

    fn connect(&mut self, ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
        ctx.panel(PanelSpec::new(LOG, "Log", Area::Bottom), |ui, _| {
            ui.label("log body");
        })?
        .panel(PanelSpec::new(TREE, "Tree", Area::StartSide), |ui, _| {
            ui.label("tree body");
        })?;
        ctx.block(
            zones::MAIN,
            Block::add(BlockId::from_static("demo.main"), 0, |ui, _| {
                ui.label("main body");
            }),
        )
        .block(
            zones::INSPECTOR,
            Block::add(BlockId::from_static("demo.inspector"), 0, |ui, _| {
                ui.label("inspector body");
            }),
        );
        // A built-in arrangement: the tree at the end, no log.
        ctx.arrangement(
            "Focus",
            Layout {
                areas: vec![(Area::EndSide, Node::tab(TREE))],
                hidden: vec![LOG],
                ..Layout::default()
            },
        );
        Ok(())
    }
}

fn harness() -> Harness<'static, App> {
    let app = App::builder()
        .plugins(DefaultPlugins)
        .plugin(FramePlugin::own().name("Demo").layout_controls(true))
        .plugin(StripsPlugin::default())
        .plugin(Demo)
        .build(&egui::Context::default())
        .unwrap();
    let mut h = Harness::builder()
        .with_size(egui::vec2(1100.0, 700.0))
        .build_ui_state(|ui, app: &mut App| app.show(ui), app);
    h.run();
    h
}

fn handle(h: &Harness<'_, App>) -> AppHandle {
    h.state().handle().clone()
}

/// Where an area was drawn last.
fn rect(h: &Harness<'_, App>, area: &str) -> egui::Rect {
    let snap = h.state().handle().debug_snapshot();
    let v: Vec<f32> = snap["workbench"]["rects"][area]
        .as_array()
        .unwrap_or_else(|| panic!("{area} drawn: {}", snap["workbench"]["rects"]))
        .iter()
        .map(|x| x.as_f64().unwrap() as f32)
        .collect();
    egui::Rect::from_min_max(egui::pos2(v[0], v[1]), egui::pos2(v[2], v[3]))
}

/// Writes a setting as the user does.
fn set(h: &mut Harness<'_, App>, path: &str, value: &str) {
    egui_pop_core::lock(&handle(h).store())
        .set(Set::new(
            path,
            Change::To(Value::Choice(value.into())),
            Origin::User,
        ))
        .unwrap();
    h.run();
    h.run();
}

#[test]
fn the_primary_side_bar_lies_left_or_right_as_the_setting_says() {
    let mut h = harness();
    let app = handle(&h);
    assert_eq!(
        layout_controls::side_bar_position(&app),
        SideBarPosition::Left
    );
    let (start, end) = (rect(&h, "start_side"), rect(&h, "end_side"));
    assert!(start.right() <= end.left(), "{start:?} {end:?}");
    set(
        &mut h,
        &format!("settings/{SETTING_SIDE_BAR_POSITION}"),
        "right",
    );
    assert_eq!(
        layout_controls::side_bar_position(&app),
        SideBarPosition::Right
    );
    let (start, end) = (rect(&h, "start_side"), rect(&h, "end_side"));
    assert!(end.right() <= start.left(), "{start:?} {end:?}");
    // The panels stay in their areas: only the sides change.
    assert_eq!(app.layout().area_of(&TREE), Some(Area::StartSide));
    // One undo step puts it back.
    assert!(app.undo(&Origin::User).ok());
    h.run();
    h.run();
    assert_eq!(
        layout_controls::side_bar_position(&app),
        SideBarPosition::Left
    );
    let (start, end) = (rect(&h, "start_side"), rect(&h, "end_side"));
    assert!(start.right() <= end.left());
}

/// Whether each area shows, by the toggles' order.
fn shown(app: &AppHandle) -> [bool; 3] {
    TOGGLES.map(|(area, _, _)| layout_controls::area_shown(app, area))
}

#[test]
fn each_toggle_folds_its_area_and_shows_it_again_through_the_fold_step() {
    let mut h = harness();
    let app = handle(&h);
    assert_eq!(shown(&app), [true; 3]);
    for (i, (area, command, key)) in TOGGLES.into_iter().enumerate() {
        assert_eq!(app.when_context().get(key), "true", "{key}");
        app.run(command.clone(), Json::Null);
        h.run();
        h.run();
        let mut expected = [true; 3];
        expected[i] = false;
        assert_eq!(shown(&app), expected, "{command} hides only its area");
        assert!(app.layout().folded.contains(&area), "folded, not closed");
        assert_eq!(app.when_context().get(key), "false", "{key}");
        let trace = app.step_traces().pop().unwrap();
        assert_eq!(trace.step, STEP_FOLD);
        assert_eq!(trace.args["folded"], true);
        app.run(command.clone(), Json::Null);
        h.run();
        h.run();
        assert_eq!(shown(&app), [true; 3], "{command} shows it again");
        assert_eq!(app.step_traces().pop().unwrap().args["folded"], false);
    }
    assert_eq!(h.query_all_by_label("log body").count(), 1);
}

#[test]
fn a_toggle_brings_back_a_closed_panel_when_its_area_has_nothing_left() {
    let mut h = harness();
    let app = handle(&h);
    app.set_panel_visible(&LOG, false);
    h.run();
    assert!(!layout_controls::area_shown(&app, Area::Bottom));
    app.run(CMD_TOGGLE_PANEL, Json::Null);
    h.run();
    h.run();
    assert!(app.panel_visible(&LOG));
    assert!(layout_controls::area_shown(&app, Area::Bottom));
}

#[test]
fn ctrl_b_ctrl_j_and_ctrl_alt_b_toggle_the_areas() {
    let mut h = harness();
    let app = handle(&h);
    assert_eq!(app.keys_for(&CMD_TOGGLE_PRIMARY_SIDE_BAR), ["Ctrl+B"]);
    assert_eq!(app.keys_for(&CMD_TOGGLE_PANEL), ["Ctrl+J"]);
    assert_eq!(app.keys_for(&CMD_TOGGLE_SECONDARY_SIDE_BAR), ["Ctrl+Alt+B"]);
    // VS Code's Ctrl+B wins: the strips' own fold command has no key.
    assert!(app.keys_for(&strips::CMD_TOGGLE_SIDE_BAR).is_empty());
    let keys = [
        (egui::Modifiers::COMMAND, egui::Key::B),
        (egui::Modifiers::COMMAND, egui::Key::J),
        (
            egui::Modifiers::COMMAND | egui::Modifiers::ALT,
            egui::Key::B,
        ),
    ];
    for (i, (modifiers, key)) in keys.into_iter().enumerate() {
        h.key_press_modifiers(modifiers, key);
        h.run();
        h.run();
        let mut expected = [true; 3];
        expected[i] = false;
        assert_eq!(shown(&app), expected, "{modifiers:?} {key:?}");
        h.key_press_modifiers(modifiers, key);
        h.run();
        h.run();
        assert_eq!(shown(&app), [true; 3], "{modifiers:?} {key:?} again");
    }
}

/// A toggle's name in the title bar: its command's title.
fn toggle_name(app: &AppHandle, area: Area) -> String {
    let command = layout_controls::toggle_command(area).unwrap();
    app.command(&command).unwrap().title
}

fn toggled(h: &Harness<'_, App>, name: &str) -> Option<Toggled> {
    h.get_by_label(name).accesskit_node().toggled()
}

#[test]
fn the_title_bar_toggles_round_trip_and_their_icons_follow() {
    let mut h = harness();
    let app = handle(&h);
    let icons = [
        (Area::StartSide, Icon::LayoutLeft, Icon::LayoutLeftOff),
        (Area::Bottom, Icon::LayoutBottom, Icon::LayoutBottomOff),
        (Area::EndSide, Icon::LayoutRight, Icon::LayoutRightOff),
    ];
    assert!(h.query_by_label("Customize layout").is_some());
    for (area, filled, outlined) in icons {
        let name = toggle_name(&app, area);
        assert_eq!(toggled(&h, &name), Some(Toggled::True), "{name}");
        assert_eq!(layout_controls::toggle_icon(&app, area), filled);
        h.get_by_label(&name).click();
        h.run();
        h.run();
        assert!(app.layout().folded.contains(&area), "{name} folds");
        assert_eq!(toggled(&h, &name), Some(Toggled::False), "{name}");
        assert_eq!(layout_controls::toggle_icon(&app, area), outlined);
        h.get_by_label(&name).click();
        h.run();
        h.run();
        assert!(!app.layout().folded.contains(&area), "{name} unfolds");
        assert_eq!(toggled(&h, &name), Some(Toggled::True), "{name}");
        assert_eq!(layout_controls::toggle_icon(&app, area), filled);
    }
}

/// Opens the layout menu from the title bar's button.
fn open_menu(h: &mut Harness<'_, App>) {
    h.get_by_label("Customize layout").click();
    h.run();
    assert_eq!(menus::popup_open(&handle(h)), Some(MENU_LAYOUT));
}

/// Chooses an entry of the layout menu.
fn choose(h: &mut Harness<'_, App>, label: &str) {
    open_menu(h);
    h.get_by_label(label).click();
    h.run();
    h.run();
}

#[test]
fn the_layout_menu_applies_a_built_in_arrangement_and_saves_the_current_one() {
    let mut h = harness();
    let app = handle(&h);
    assert_eq!(arrangements::current(&app), arrangements::DEFAULT);
    // Built in: the application's template.
    choose(&mut h, "Focus");
    assert_eq!(arrangements::current(&app), "Focus");
    assert_eq!(app.layout().area_of(&TREE), Some(Area::EndSide));
    assert!(!app.panel_visible(&LOG));
    // Changed, then saved under a name asked in a dialog.
    app.move_panel(&TREE, Area::StartSide);
    h.run();
    choose(&mut h, "Save the current arrangement as…");
    let dialogs = app.service(&DIALOGS).unwrap();
    assert!(dialogs.current().is_some(), "a name is asked");
    let mut values = serde_json::Map::new();
    values.insert("name".into(), Json::from("Mine"));
    dialogs
        .answer(Answer::Accept { values }, Origin::User)
        .unwrap();
    h.run();
    h.run();
    assert!(arrangements::saved(&app).contains_key("Mine"));
    assert_eq!(arrangements::current(&app), "Mine");
    // The menu lists it, ticked; the other arrangement switches back.
    open_menu(&mut h);
    assert_eq!(toggled(&h, "Mine"), Some(Toggled::True));
    h.get_by_label("Focus").click();
    h.run();
    h.run();
    assert_eq!(app.layout().area_of(&TREE), Some(Area::EndSide));
    choose(&mut h, "Mine");
    assert_eq!(app.layout().area_of(&TREE), Some(Area::StartSide));
}

#[test]
fn each_layout_option_changes_the_layout_and_is_undone_in_one_step() {
    let mut h = harness();
    let app = handle(&h);
    // The defaults: the side bars as they were, the panel under the
    // editors.
    let (start, end, bottom) = (
        rect(&h, "start_side"),
        rect(&h, "end_side"),
        rect(&h, "bottom"),
    );
    assert!(start.right() <= end.left());
    assert!(bottom.left() >= start.right() - 1.0 && bottom.right() <= end.left() + 1.0);
    open_menu(&mut h);
    assert_eq!(
        toggled(&h, "Primary side bar on the left"),
        Some(Toggled::True)
    );
    assert_eq!(toggled(&h, "Panel under the editors"), Some(Toggled::True));
    h.key_press(egui::Key::Escape);
    h.run();

    choose(&mut h, "Primary side bar on the right");
    assert_eq!(
        layout_controls::side_bar_position(&app),
        SideBarPosition::Right
    );
    let (start, end) = (rect(&h, "start_side"), rect(&h, "end_side"));
    assert!(end.right() <= start.left(), "{start:?} {end:?}");
    // The toggles' icons follow their side.
    assert_eq!(
        layout_controls::toggle_icon(&app, Area::StartSide),
        Icon::LayoutRight
    );
    assert_eq!(
        layout_controls::toggle_icon(&app, Area::EndSide),
        Icon::LayoutLeft
    );

    choose(&mut h, "Panel across the full width");
    assert_eq!(workbench::panel_alignment(&app), PanelAlignment::Justify);
    let (start, end, bottom) = (
        rect(&h, "start_side"),
        rect(&h, "end_side"),
        rect(&h, "bottom"),
    );
    assert!(bottom.left() <= end.left() + 1.0 && bottom.right() >= start.right() - 1.0);
    assert!(start.bottom() <= bottom.top() + 1.0, "under both side bars");

    // Each was one undo step.
    assert!(app.undo(&Origin::User).ok());
    h.run();
    assert_eq!(workbench::panel_alignment(&app), PanelAlignment::Center);
    assert!(app.undo(&Origin::User).ok());
    h.run();
    assert_eq!(
        layout_controls::side_bar_position(&app),
        SideBarPosition::Left
    );
}
