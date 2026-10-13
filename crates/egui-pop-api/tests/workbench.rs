//! The workbench: panels in areas from their declarations, placement
//! rules, changes through steps a plugin may veto, tabs over egui_dock,
//! the layout saved and repaired, zones placed in areas.

use egui_kittest::Harness;
use egui_kittest::kittest::Queryable as _;
use egui_pop_api::steps::{Before, Outcome};
use egui_pop_api::workbench::{PANEL_INSPECTOR, PANEL_MAIN, PANEL_SIDE_BAR, STEP_MOVE};
use egui_pop_api::zones::{self, Block};
use egui_pop_api::{App, AppBuilder, Ctx, Plugin, PluginError};
use egui_pop_model::commands::Origin;
use egui_pop_model::ids::{BlockId, CommandId, PanelId, PluginId};
use egui_pop_model::layout::Node;
use egui_pop_model::panels::{Area, PanelSpec};
use serde_json::Value as Json;

const LOG: PanelId = PanelId::from_static("demo.log");
const TREE: PanelId = PanelId::from_static("demo.tree");
const NOTES: PanelId = PanelId::from_static("demo.notes");

struct Demo {
    veto: bool,
}

impl Plugin for Demo {
    fn id(&self) -> PluginId {
        PluginId::from_static("demo")
    }

    fn connect(&mut self, ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
        ctx.panel(
            PanelSpec::new(LOG, "Log", Area::Bottom)
                .only_in(&[Area::Bottom])
                .min_size(60.0),
            |ui, _| {
                ui.label("log body");
            },
        )?
        .panel(
            PanelSpec::new(TREE, "Tree", Area::StartSide).default_size(220.0),
            |ui, _| {
                ui.label("tree body");
            },
        )?
        .panel(
            PanelSpec::new(NOTES, "Notes", Area::Bottom).closed(),
            |ui, _| {
                ui.label("notes body");
            },
        )?;
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
        if self.veto {
            ctx.before(STEP_MOVE, |_, args| {
                if args["panel"] == "demo.tree" && args["area"] == "bottom" {
                    Before::Veto("the tree stays on a side".into())
                } else {
                    Before::Go
                }
            });
        }
        Ok(())
    }
}

fn builder(veto: bool) -> AppBuilder {
    App::builder().plugin(Demo { veto })
}

fn harness(builder: AppBuilder) -> Harness<'static, App> {
    let app = builder.build(&egui::Context::default()).unwrap();
    let mut h = Harness::builder()
        .with_size(egui::vec2(900.0, 600.0))
        .build_ui_state(|ui, app: &mut App| app.show(ui), app);
    h.run();
    h
}

fn area_of(h: &Harness<'_, App>, p: &PanelId) -> Option<Area> {
    h.state().handle().layout().area_of(p)
}

#[test]
fn panels_open_in_their_default_areas_and_zones_become_panels() {
    let h = harness(builder(false));
    assert_eq!(area_of(&h, &LOG), Some(Area::Bottom));
    assert_eq!(area_of(&h, &TREE), Some(Area::StartSide));
    assert_eq!(area_of(&h, &PANEL_MAIN), Some(Area::Center));
    // A zone with blocks is a panel of its area; one without, none.
    assert_eq!(area_of(&h, &PANEL_INSPECTOR), Some(Area::EndSide));
    assert!(h.query_by_label("inspector body").is_some());
    assert_eq!(area_of(&h, &PANEL_SIDE_BAR), None);
    // Closed by declaration.
    assert_eq!(area_of(&h, &NOTES), None);
    assert!(!h.state().handle().panel_visible(&NOTES));
    // Drawn: the main zone plainly, the panels' bodies.
    assert!(h.query_by_label("main body").is_some());
    assert!(h.query_by_label("log body").is_some());
    assert!(h.query_by_label("tree body").is_some());
    assert!(h.state().handle().diagnostics().is_empty());
}

#[test]
fn placement_rules_refuse_and_steps_may_veto() {
    let h = harness(builder(true));
    let app = h.state().handle();
    // The log stays in the bottom panel.
    let out = app.move_panel(&LOG, Area::StartSide).unwrap();
    assert!(
        matches!(out, Outcome::Done(Err(ref e)) if e.contains("stays out of")),
        "{out:?}"
    );
    assert_eq!(area_of(&h, &LOG), Some(Area::Bottom));
    // A plugin vetoes the tree going to the bottom.
    let out = app.move_panel(&TREE, Area::Bottom).unwrap();
    assert_eq!(out.vetoed(), Some("the tree stays on a side"));
    // Allowed.
    let out = app.move_panel(&TREE, Area::EndSide).unwrap();
    assert!(out.ok(), "{out:?}");
    assert_eq!(area_of(&h, &TREE), Some(Area::EndSide));
}

#[test]
fn panel_commands_show_hide_and_focus() {
    let mut h = harness(builder(false));
    let run = |h: &mut Harness<'_, App>, id: &str| {
        h.state_mut()
            .run(&CommandId::new(id).unwrap(), Json::Null, Origin::User);
        h.run();
    };
    run(&mut h, "demo.notes.toggle");
    assert_eq!(area_of(&h, &NOTES), Some(Area::Bottom));
    // Two panels in the bottom area: tabs.
    assert!(h.query_by_label("Notes").is_some());
    assert!(h.query_by_label("notes body").is_some());
    h.get_by_label("Log").click();
    h.run();
    assert!(h.query_by_label("log body").is_some());
    assert!(h.query_by_label("notes body").is_none());
    run(&mut h, "demo.notes.focus");
    assert!(h.query_by_label("notes body").is_some());
    assert_eq!(h.state().handle().focused_panel(), Some(NOTES));
    run(&mut h, "demo.notes.toggle");
    assert_eq!(area_of(&h, &NOTES), None);
    assert!(h.state().handle().layout().hidden.contains(&NOTES));
}

#[test]
fn the_layout_is_saved_and_repaired_at_the_next_start() {
    let dir = std::env::temp_dir().join(format!("egui-pop-workbench-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    {
        let h = harness(builder(false).persist(&dir));
        h.state().handle().move_panel(&TREE, Area::EndSide);
        h.state().save().unwrap();
    }
    let h = harness(builder(false).persist(&dir));
    assert_eq!(area_of(&h, &TREE), Some(Area::EndSide));
    // A panel gone since: dropped, the rest kept.
    let h = harness(App::builder().persist(&dir));
    assert_eq!(area_of(&h, &TREE), None);
    assert!(h.state().handle().layout().areas.is_empty());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_plugin_gone_takes_its_panels_and_coming_back_puts_them_back() {
    let mut h = harness(builder(false));
    h.state_mut().disconnect(&PluginId::from_static("demo"));
    h.state_mut().rebuild();
    h.run();
    assert_eq!(area_of(&h, &LOG), None);
    assert!(h.query_by_label("log body").is_none());
    h.state_mut()
        .connect(&PluginId::from_static("demo"))
        .unwrap();
    h.state_mut().rebuild();
    h.run();
    assert_eq!(area_of(&h, &LOG), Some(Area::Bottom));
    // The dock projects the layout's tree.
    let layout = h.state().handle().layout();
    let bottom = layout
        .areas
        .iter()
        .find(|(a, _)| *a == Area::Bottom)
        .unwrap();
    assert!(matches!(&bottom.1, Node::Tabs { panels, .. } if panels.contains(&LOG)));
}

#[test]
fn the_drop_hint_is_published_with_its_rect_and_name() {
    let mut h = harness(builder(false));
    let app = h.state().handle().clone();
    assert!(app.drop_hint().is_none());
    // Drag the Log tab (bottom only) over the start side bar.
    let from = h.get_by_label("Log").rect().center();
    let to = h.get_by_label("tree body").rect().center();
    let pointer = |h: &mut Harness<'_, App>, e: egui::Event| {
        h.input_mut().events.push(e);
        h.step();
    };
    pointer(&mut h, egui::Event::PointerMoved(from));
    pointer(
        &mut h,
        egui::Event::PointerButton {
            pos: from,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: egui::Modifiers::NONE,
        },
    );
    for i in 1..=8 {
        let at = from + (to - from) * (i as f32 / 8.0);
        pointer(&mut h, egui::Event::PointerMoved(at));
    }
    h.step();
    let hint = app.drop_hint().expect("a hint while dragging");
    assert_eq!(hint.panel, LOG);
    assert_eq!(hint.area, "start_side");
    assert!(hint.refusal.is_some(), "the log stays in the bottom panel");
    assert!(hint.rect.contains(to));
    let rects = app.overlay_rects();
    let shown = rects
        .iter()
        .find(|o| o.kind == egui_pop_api::app::OverlayKind::DropHint)
        .expect("among the overlays");
    assert_eq!(shown.id, "drop:demo.log:start_side");
    assert_eq!(shown.rect, hint.rect);
    let snap = app.debug_snapshot();
    assert_eq!(
        snap["workbench"]["drop_hint"]["name"],
        "drop:demo.log:start_side"
    );
    assert!(snap["workbench"]["rects"]["bottom"].is_array());
    pointer(
        &mut h,
        egui::Event::PointerButton {
            pos: to,
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: egui::Modifiers::NONE,
        },
    );
    h.run();
    assert!(app.drop_hint().is_none());
    assert_eq!(area_of(&h, &LOG), Some(Area::Bottom), "refused");
}
