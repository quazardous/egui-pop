//! The layout, as VS Code arranges it: the primary side bar on the left
//! or the right, each change a setting undone as one step.

use egui_kittest::Harness;
use egui_pop_api::frame::FramePlugin;
use egui_pop_api::layout_controls::{self, SETTING_SIDE_BAR_POSITION, SideBarPosition};
use egui_pop_api::zones::{self, Block};
use egui_pop_api::{App, AppHandle, Ctx, DefaultPlugins, Plugin, PluginError};
use egui_pop_model::commands::Origin;
use egui_pop_model::ids::{BlockId, PanelId, PluginId};
use egui_pop_model::panels::{Area, PanelSpec};
use egui_pop_model::values::{Change, Set, Value};

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
        Ok(())
    }
}

fn harness() -> Harness<'static, App> {
    let app = App::builder()
        .plugins(DefaultPlugins)
        .plugin(FramePlugin::own().name("Demo"))
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
