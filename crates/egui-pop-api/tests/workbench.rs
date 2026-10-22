//! The workbench: panels in areas from their declarations, placement
//! rules, changes through steps a plugin may veto, tabs over egui_dock,
//! the layout saved and repaired, zones placed in areas.

use egui_kittest::Harness;
use egui_kittest::kittest::Queryable as _;
use egui_pop_api::steps::{Before, Outcome};
use egui_pop_api::workbench::{PANEL_INSPECTOR, PANEL_MAIN, PANEL_SIDE_BAR, STEP_MOVE, STEP_PLACE};
use egui_pop_api::zones::{self, Block};
use egui_pop_api::{App, AppBuilder, Ctx, Plugin, PluginError};
use egui_pop_model::commands::Origin;
use egui_pop_model::ids::{BlockId, CommandId, PanelId, PluginId};
use egui_pop_model::layout::{Node, Placement};
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
    assert!(hint.preview.is_none(), "no ghost tab where it may not go");
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
    assert!(snap["workbench"]["drop_hint"]["preview"].is_null());
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

#[test]
fn a_tab_dragged_over_another_area_shows_its_ghost_at_the_end_of_its_tab_bar() {
    let mut h = harness(builder(false));
    let app = h.state().handle().clone();
    // Drag the Tree tab (start side) over the bottom area, which holds
    // the Log tab.
    let from = h.get_by_label("Tree").rect().center();
    let log = h.get_by_label("Log").rect();
    let to = h.get_by_label("log body").rect().center();
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
    assert_eq!((hint.panel.clone(), hint.area), (TREE, "bottom"));
    assert!(hint.refusal.is_none());
    let ghost = hint.preview.expect("the tab's ghost where it would land");
    assert_eq!(ghost.area, "bottom");
    assert_eq!(ghost.index, 1, "after the Log tab");
    assert!(ghost.rect.left() >= log.right() - 0.5, "after the last tab");
    assert!((ghost.rect.top() - log.top()).abs() < 0.5, "in the tab bar");
    assert!((ghost.rect.height() - log.height()).abs() < 0.5);
    assert!(hint.rect.contains_rect(ghost.rect));
    let snap = app.debug_snapshot();
    let preview = &snap["workbench"]["drop_hint"]["preview"];
    assert_eq!(preview["area"], "bottom");
    assert_eq!(preview["index"], 1);
    assert_eq!(
        preview["rect"][0].as_f64(),
        Some(f64::from(ghost.rect.min.x))
    );
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
    assert_eq!(area_of(&h, &TREE), Some(Area::Bottom), "dropped there");
    let bottom = app
        .layout()
        .areas
        .into_iter()
        .find(|(a, _)| *a == Area::Bottom)
        .unwrap();
    assert!(
        matches!(&bottom.1, Node::Tabs { panels, .. } if panels.last() == Some(&TREE)),
        "where the ghost was: last"
    );
}

/// A host keeping one layout per workspace, or none.
struct Workspaces {
    inner: egui_pop_api::host::LocalHost,
    document: std::sync::Arc<std::sync::Mutex<Option<String>>>,
}

impl egui_pop_api::host::Host for Workspaces {
    fn dispatch(&mut self, d: &egui_pop_api::host::Dispatch<'_>) -> egui_pop_api::host::Reply {
        self.inner.dispatch(d)
    }
    fn journal(&self) -> Option<&egui_pop_api::host::Journal> {
        self.inner.journal()
    }
    fn journal_mut(&mut self) -> Option<&mut egui_pop_api::host::Journal> {
        self.inner.journal_mut()
    }
    fn store(&self) -> egui_pop_core::binding::SharedStore {
        self.inner.store()
    }
    fn set_waker(&mut self, waker: egui_pop_api::host::Waker) {
        self.inner.set_waker(waker);
    }
    fn layout_document(&self) -> Option<String> {
        self.document.lock().unwrap().clone()
    }
}

#[test]
fn the_host_names_the_layouts_document_per_workspace_or_keeps_it() {
    use egui_pop_api::host::Host as _;
    let dir = std::env::temp_dir().join(format!("egui-pop-workspaces-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let document = std::sync::Arc::new(std::sync::Mutex::new(Some(
        "workspaces/photos/layout".to_string(),
    )));
    let host = || Workspaces {
        inner: egui_pop_api::host::LocalHost::default(),
        document: document.clone(),
    };
    let mut h = harness(builder(false).persist(&dir).host(host()));
    h.state().handle().move_panel(&TREE, Area::EndSide);
    h.state().save().unwrap();
    assert!(dir.join("workspaces/photos/layout.json").exists());
    assert!(!dir.join("layout.json").exists(), "no global layout");
    let trace = h.state().handle().step_traces().pop().unwrap();
    assert_eq!(trace.step, egui_pop_api::app::STEP_LAYOUT_SAVE);
    // Another workspace: its own document, the tree back where declared.
    *document.lock().unwrap() = Some("workspaces/maps/layout".into());
    h.state().handle().move_panel(&TREE, Area::StartSide);
    h.state().save().unwrap();
    // Back to the first: loaded from its document.
    *document.lock().unwrap() = Some("workspaces/photos/layout".into());
    let out = h.state_mut().load_layout().unwrap();
    assert!(out.ok(), "{out:?}");
    h.run();
    assert_eq!(area_of(&h, &TREE), Some(Area::EndSide));
    // The next start reads the workspace the host names.
    let h = harness(builder(false).persist(&dir).host(host()));
    assert_eq!(area_of(&h, &TREE), Some(Area::EndSide));
    // A host keeping the layout itself: nothing loaded or written.
    *document.lock().unwrap() = None;
    let _ = std::fs::remove_dir_all(&dir);
    let mut h = harness(builder(false).persist(&dir).host(host()));
    assert!(h.state_mut().load_layout().is_none());
    assert!(h.state().save_layout().is_none());
    h.state().save().unwrap();
    assert!(!dir.join("layout.json").exists());
    assert!(host().layout_document().is_none());
    // The local host opts out the same way.
    assert!(
        egui_pop_api::host::LocalHost::default()
            .with_layout_document(None)
            .layout_document()
            .is_none()
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_bottom_panel_lines_up_with_the_side_bars_as_the_setting_says() {
    use egui_pop_api::workbench::{PanelAlignment, SETTING_PANEL_ALIGNMENT};
    let set = |h: &mut Harness<'_, App>, a: PanelAlignment| {
        egui_pop_core::lock(&h.state().handle().store())
            .set(egui_pop_model::values::Set::new(
                &format!("settings/{SETTING_PANEL_ALIGNMENT}"),
                egui_pop_model::values::Change::To(egui_pop_model::values::Value::Choice(
                    a.as_str().into(),
                )),
                Origin::User,
            ))
            .unwrap();
        h.run();
        h.run();
    };
    let rects = |h: &Harness<'_, App>| {
        let snap = h.state().handle().debug_snapshot();
        let r = |a: &str| {
            let v: Vec<f32> = snap["workbench"]["rects"][a]
                .as_array()
                .unwrap()
                .iter()
                .map(|x| x.as_f64().unwrap() as f32)
                .collect();
            egui::Rect::from_min_max(egui::pos2(v[0], v[1]), egui::pos2(v[2], v[3]))
        };
        (r("start_side"), r("end_side"), r("bottom"))
    };
    let mut h = harness(builder(false));
    assert_eq!(
        egui_pop_api::workbench::panel_alignment(h.state().handle()),
        PanelAlignment::Center,
        "the default lines it up under the editors, as VS Code"
    );
    let (start, end, bottom) = rects(&h);
    assert!(bottom.left() >= start.right() - 1.0 && bottom.right() <= end.left() + 1.0);
    assert!(start.bottom() > bottom.top(), "the side bars go down");
    set(&mut h, PanelAlignment::Justify);
    let (start, end, bottom) = rects(&h);
    assert!(bottom.left() <= start.left() && bottom.right() >= end.right());
    assert!(start.bottom() <= bottom.top() + 1.0);
    set(&mut h, PanelAlignment::Start);
    let (start, end, bottom) = rects(&h);
    assert!(bottom.left() <= start.left() && bottom.right() <= end.left() + 1.0);
    assert!(end.bottom() > bottom.top());
    set(&mut h, PanelAlignment::End);
    let (start, end, bottom) = rects(&h);
    assert!(bottom.left() >= start.right() - 1.0 && bottom.right() >= end.right());
    assert!(start.bottom() > bottom.top());
    // A builder's default.
    let h = harness(builder(false).panel_alignment(PanelAlignment::Justify));
    assert_eq!(
        egui_pop_api::workbench::panel_alignment(h.state().handle()),
        PanelAlignment::Justify
    );
}

#[test]
fn an_application_sizes_the_shell_panels_and_starts_with_an_area_folded() {
    // The demo's tree is on the start side: there is something to fold.
    let builder = builder(false)
        .shell_panel_size(PANEL_INSPECTOR, 428.0)
        .folded(&[Area::StartSide]);
    let app = builder.build(&egui::Context::default()).unwrap();
    let mut h = Harness::builder()
        .with_size(egui::vec2(1280.0, 800.0))
        .build_ui_state(|ui, app: &mut App| app.show(ui), app);
    h.run();
    let handle = h.state().handle().clone();
    assert_eq!(handle.layout().folded, vec![Area::StartSide]);
    // The folded side bar is its strip, named for assistive technology.
    assert!(h.query_by_label("Unfold the start side bar").is_some());
    let inspector = handle
        .area_rect(Area::EndSide)
        .expect("the inspector is shown");
    assert!((inspector.width() - 428.0).abs() < 2.0, "{inspector:?}");
}

/// Vetoes the end side bar lying over the centre.
struct KeepEndBeside;

impl Plugin for KeepEndBeside {
    fn id(&self) -> PluginId {
        PluginId::from_static("keep-end-beside")
    }

    fn connect(&mut self, ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
        ctx.before(STEP_PLACE, |_, args| {
            if args["area"] == "end_side" && args["placement"] == "over" {
                Before::Veto("the inspector stays beside".into())
            } else {
                Before::Go
            }
        });
        Ok(())
    }
}

#[test]
fn a_side_area_lies_over_the_centre_through_a_step_a_plugin_may_veto() {
    let mut h = harness(builder(false).plugin(KeepEndBeside));
    let app = h.state().handle().clone();
    let centre = app.area_rect(Area::Center).unwrap();
    let tree = app.area_rect(Area::StartSide).unwrap();
    assert_eq!(app.area_placement(Area::StartSide), Placement::Beside);

    // Over: the strip keeps the room, the centre gets the side bar's.
    let out = app.place_area(Area::StartSide, Placement::Over).unwrap();
    assert!(out.ok(), "{out:?}");
    h.run();
    h.run();
    assert_eq!(app.area_placement(Area::StartSide), Placement::Over);
    assert_eq!(app.layout().over, [Area::StartSide]);
    let wide = app.area_rect(Area::Center).unwrap();
    assert!(wide.width() > centre.width() + 100.0, "{wide:?} {centre:?}");
    // The area lies over the centre, as wide as it was, still drawn.
    let over = app.area_rect(Area::StartSide).unwrap();
    assert!(over.intersects(wide), "{over:?} {wide:?}");
    assert!(
        (over.width() - tree.width()).abs() < 2.0,
        "{over:?} {tree:?}"
    );
    assert!(h.query_by_label("tree body").is_some());
    // Its strip folds it; the centre does not move.
    h.get_by_label("Fold the start side bar").click();
    h.run();
    h.run();
    assert_eq!(app.layout().folded, [Area::StartSide]);
    assert!(h.query_by_label("tree body").is_none());
    assert_eq!(app.area_rect(Area::Center).unwrap(), wide);
    h.get_by_label("Unfold the start side bar").click();
    h.run();
    h.run();
    assert!(app.layout().folded.is_empty());
    assert!(h.query_by_label("tree body").is_some());
    assert_eq!(app.area_rect(Area::Center).unwrap(), wide);

    // A plugin vetoes the end side bar lying over.
    let out = app.place_area(Area::EndSide, Placement::Over).unwrap();
    assert_eq!(out.vetoed(), Some("the inspector stays beside"));
    assert_eq!(app.area_placement(Area::EndSide), Placement::Beside);
    // Only side areas lie over.
    let out = app.place_area(Area::Bottom, Placement::Over).unwrap();
    assert!(
        matches!(out, Outcome::Done(Err(ref e)) if e.contains("only a side area")),
        "{out:?}"
    );
    // Back beside: the centre narrows again.
    assert!(
        app.place_area(Area::StartSide, Placement::Beside)
            .unwrap()
            .ok()
    );
    h.run();
    h.run();
    assert!(app.layout().over.is_empty());
    let back = app.area_rect(Area::Center).unwrap();
    assert!((back.width() - centre.width()).abs() < 2.0, "{back:?}");
}

#[test]
fn an_application_starts_an_area_over_and_the_placement_is_saved() {
    let dir = std::env::temp_dir().join(format!("egui-pop-over-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    {
        let h = harness(
            builder(false)
                .over(&[Area::StartSide, Area::Bottom])
                .persist(&dir),
        );
        let app = h.state().handle();
        // The bottom area always lies beside.
        assert_eq!(app.layout().over, [Area::StartSide]);
        app.place_area(Area::EndSide, Placement::Over);
        h.state().save().unwrap();
    }
    // A saved layout wins over the builder's.
    let h = harness(builder(false).persist(&dir));
    let app = h.state().handle();
    assert_eq!(app.area_placement(Area::StartSide), Placement::Over);
    assert_eq!(app.area_placement(Area::EndSide), Placement::Over);
    let _ = std::fs::remove_dir_all(&dir);
}
