//! Palettes: floating panels over the workbench, opened floating, docked
//! and floated again, dragged (snapped, clamped), docked by a drop over
//! an area, hidden and shown by Tab, rolled up, pinned, giving the
//! keyboard back on Escape (and the focus a widget inside took), resized
//! by their edges, every change through its step.

use std::sync::{Arc, Mutex};

use egui::accesskit::Role;
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable as _;
use egui_pop_api::palettes::{
    self, CMD_RESET, STEP_CLOSE_GROUP, STEP_DOCK, STEP_GROUP, STEP_MOVE, STEP_RESIZE, STEP_UNGROUP,
};
use egui_pop_api::steps::{Before, Outcome};
use egui_pop_api::workbench::Change;
use egui_pop_api::{App, Ctx, Plugin, PluginError};
use egui_pop_model::ids::{PanelId, PluginId};
use egui_pop_model::layout::{AnchorEdge, Arrangement, FloatingGroup, Join};
use egui_pop_model::panels::{Area, PanelSpec};
use serde_json::{Value as Json, json};

const TOOLS: PanelId = PanelId::from_static("demo.tools");
const NAV: PanelId = PanelId::from_static("demo.nav");
const LOG: PanelId = PanelId::from_static("demo.log");
const COLOURS: PanelId = PanelId::from_static("demo.colours");

struct Demo {
    hooks: bool,
}

impl Plugin for Demo {
    fn id(&self) -> PluginId {
        PluginId::from_static("demo")
    }

    fn connect(&mut self, ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
        let mut size = String::from("12");
        ctx.panel(
            PanelSpec::new(TOOLS, "Tools", Area::StartSide)
                .palette()
                .min_size(160.0),
            move |ui, _| {
                ui.label("Brush");
                ui.text_edit_singleline(&mut size);
            },
        )?
        .panel(
            PanelSpec::new(NAV, "Navigator", Area::EndSide)
                .palette()
                .min_size(160.0)
                .closed(),
            |ui, _| {
                ui.label("Overview");
            },
        )?
        .panel(
            PanelSpec::new(COLOURS, "Colours", Area::EndSide)
                .palette()
                .min_size(160.0)
                .closed(),
            |ui, _| {
                ui.label("Swatches");
            },
        )?
        .panel(
            PanelSpec::new(LOG, "Log", Area::Bottom).floatable(true),
            |ui, _| {
                ui.label("log body");
            },
        )?;
        if self.hooks {
            ctx.before(STEP_DOCK, |_, _| Before::Veto("palettes float here".into()));
            ctx.before(STEP_GROUP, |_, _| {
                Before::Veto("palettes stay apart".into())
            });
            ctx.before(STEP_UNGROUP, |_, _| {
                Before::Veto("groups stay whole".into())
            });
            ctx.before(STEP_CLOSE_GROUP, |_, _| {
                Before::Veto("groups stay open".into())
            });
            ctx.before(STEP_RESIZE, |_, _| {
                Before::Veto("palettes keep their size".into())
            });
            ctx.before(STEP_MOVE, |_, args| {
                let mut a = args.clone();
                a["pos"] = json!([100.0, 100.0]);
                Before::Redirect(a)
            });
        }
        Ok(())
    }
}

fn harness(hooks: bool) -> Harness<'static, App> {
    let app = App::builder()
        .plugin(Demo { hooks })
        .build(&egui::Context::default())
        .unwrap();
    let mut h = Harness::builder()
        .with_size(egui::vec2(900.0, 600.0))
        .build_ui_state(|ui, app: &mut App| app.show(ui), app);
    h.run();
    h
}

fn floating(h: &Harness<'_, App>, p: &PanelId) -> Option<FloatingGroup> {
    h.state().handle().layout().floating_of(p).cloned()
}

fn events(h: &Harness<'_, App>, since: u64) -> Vec<String> {
    h.state()
        .handle()
        .events_since(since)
        .into_iter()
        .map(|e| e.name)
        .collect()
}

/// Drags from a point by an amount, in four moves.
fn drag_by(h: &mut Harness<'static, App>, from: egui::Pos2, by: egui::Vec2) {
    h.drag_at(from);
    h.run();
    for i in 1..=4 {
        h.hover_at(from + by * (i as f32 / 4.0));
        h.run();
    }
    h.drop_at(from + by);
    h.run();
    h.run();
}

#[test]
fn a_palette_opens_floating_docks_and_floats_again() {
    let mut h = harness(false);
    let tools = floating(&h, &TOOLS).expect("a palette floats");
    assert_eq!(tools.pos, FloatingGroup::cascade(0));
    assert!(h.query_by_label("Brush").is_some());
    assert_eq!(
        h.state().handle().layout().area_of(&LOG),
        Some(Area::Bottom)
    );
    // Docked: one model, two placements.
    let before = h.state().handle().last_event();
    h.state()
        .handle()
        .change_workbench(Change::Dock(TOOLS, Area::StartSide));
    h.run();
    assert!(floating(&h, &TOOLS).is_none());
    assert_eq!(
        h.state().handle().layout().area_of(&TOOLS),
        Some(Area::StartSide)
    );
    assert!(h.query_by_label("Brush").is_some(), "drawn in its area");
    let names = events(&h, before);
    assert!(names.contains(&"palette.dock".to_string()), "{names:?}");
    assert!(names.contains(&"palette.docked".to_string()), "{names:?}");
    // Floated again; a docked panel that is floatable floats too.
    h.state()
        .handle()
        .change_workbench(Change::Float(TOOLS, [300.0, 120.0]));
    h.state()
        .handle()
        .change_workbench(Change::Float(LOG, [40.0, 300.0]));
    h.run();
    assert_eq!(floating(&h, &TOOLS).unwrap().pos, [300.0, 120.0]);
    assert!(floating(&h, &LOG).is_some());
    assert!(h.query_by_label("log body").is_some());
    // A closed palette opens floating, through its own step.
    let before = h.state().handle().last_event();
    h.state().handle().set_panel_visible(&NAV, true);
    h.run();
    assert!(floating(&h, &NAV).is_some());
    assert!(events(&h, before).contains(&"palette.opened".to_string()));
}

#[test]
fn a_palette_is_dragged_by_its_band_snaps_and_docks_on_an_area() {
    let mut h = harness(false);
    let start = floating(&h, &TOOLS).unwrap().pos;
    let title = h.get_by_label("Tools").rect().center();
    let before = h.state().handle().last_event();
    drag_by(&mut h, title, egui::vec2(200.0, 120.0));
    let moved = floating(&h, &TOOLS).unwrap().pos;
    assert!(
        moved[0] > start[0] + 150.0 && moved[1] > start[1] + 80.0,
        "{start:?} {moved:?}"
    );
    let names = events(&h, before);
    assert!(names.contains(&"palette.moved".to_string()), "{names:?}");
    // Near the workbench's start edge: snapped onto it.
    let title = h.get_by_label("Tools").rect().center();
    drag_by(&mut h, title, egui::vec2(-moved[0] + 5.0, 0.0));
    assert_eq!(floating(&h, &TOOLS).unwrap().pos[0], 0.0, "snapped");
    // Far out: kept inside.
    let title = h.get_by_label("Tools").rect().center();
    drag_by(&mut h, title, egui::vec2(0.0, -400.0));
    let pos = floating(&h, &TOOLS).unwrap().pos;
    assert!(pos[0] >= 0.0 && pos[1] >= 0.0, "{pos:?}");
    // Dropped over the bottom panel: docked there.
    let title = h.get_by_label("Tools").rect().center();
    let target = h.get_by_label("log body").rect().center();
    drag_by(&mut h, title, target - title);
    assert!(floating(&h, &TOOLS).is_none());
    assert_eq!(
        h.state().handle().layout().area_of(&TOOLS),
        Some(Area::Bottom)
    );
}

#[test]
fn tab_hides_and_shows_the_palettes_but_not_a_pinned_one() {
    let mut h = harness(false);
    h.state().handle().set_panel_visible(&NAV, true);
    h.run();
    assert!(h.query_by_label("Overview").is_some());
    h.key_press(egui::Key::Tab);
    h.run();
    assert!(palettes::hidden(h.state().handle()));
    assert!(h.query_by_label("Brush").is_none());
    assert!(h.query_by_label("Overview").is_none());
    assert!(h.query_by_label("log body").is_some(), "docked panels stay");
    h.key_press(egui::Key::Tab);
    h.run();
    assert!(h.query_by_label("Brush").is_some());
    // Pinned: it stays.
    h.get_by_role_and_label(Role::Button, "Pin Tools").click();
    h.run();
    assert!(floating(&h, &TOOLS).unwrap().pinned);
    h.key_press(egui::Key::Tab);
    h.run();
    assert!(h.query_by_label("Brush").is_some(), "pinned");
    assert!(h.query_by_label("Overview").is_none());
}

#[test]
fn a_palette_takes_the_keyboard_only_when_clicked_and_escape_gives_it_back() {
    let mut h = harness(false);
    let field = || Role::TextInput;
    assert!(!h.get_by_role(field()).is_focused(), "not when it opens");
    h.get_by_role(field()).click();
    h.run();
    assert!(h.get_by_role(field()).is_focused());
    // Tab is focus navigation while a widget has the keyboard.
    h.key_press(egui::Key::Escape);
    h.run();
    assert!(!h.get_by_role(field()).is_focused());
    assert!(h.query_by_label("Brush").is_some(), "still open");
    // Back with the canvas: Tab is the palettes' toggle again.
    h.key_press(egui::Key::Tab);
    h.run();
    assert!(h.query_by_label("Brush").is_none());
}

#[test]
fn a_palette_rolls_up_to_its_band_and_closes_from_it() {
    let mut h = harness(false);
    h.get_by_role_and_label(Role::Button, "Roll up Tools")
        .click();
    h.run();
    assert!(floating(&h, &TOOLS).unwrap().rolled(&TOOLS));
    assert!(h.query_by_label("Brush").is_none());
    assert!(h.query_by_label("Tools").is_some(), "its band stays");
    h.get_by_role_and_label(Role::Button, "Roll down Tools")
        .click();
    h.run();
    assert!(h.query_by_label("Brush").is_some());
    h.get_by_role_and_label(Role::Button, "Close Tools").click();
    h.run();
    assert!(floating(&h, &TOOLS).is_none());
    assert!(!h.state().handle().panel_visible(&TOOLS));
    // Its toggle opens it floating again.
    h.state().handle().set_panel_visible(&TOOLS, true);
    h.run();
    assert!(floating(&h, &TOOLS).is_some());
}

#[test]
fn hooks_veto_a_dock_and_redirect_a_move_and_reset_puts_palettes_back() {
    let mut h = harness(true);
    let title = h.get_by_label("Tools").rect().center();
    drag_by(&mut h, title, egui::vec2(220.0, 140.0));
    assert_eq!(
        floating(&h, &TOOLS).unwrap().pos,
        [100.0, 100.0],
        "redirected"
    );
    let title = h.get_by_label("Tools").rect().center();
    let target = h.get_by_label("log body").rect().center();
    drag_by(&mut h, title, target - title);
    assert_eq!(
        floating(&h, &TOOLS).unwrap().pos,
        [100.0, 100.0],
        "the dock was vetoed: back where it started"
    );
    h.state().handle().run(CMD_RESET, Json::Null);
    h.run();
    // The move step still redirects: reset goes through it.
    assert_eq!(floating(&h, &TOOLS).unwrap().pos, [100.0, 100.0]);
    let mut h = harness(false);
    let title = h.get_by_label("Tools").rect().center();
    drag_by(&mut h, title, egui::vec2(220.0, 140.0));
    h.state().handle().run(CMD_RESET, Json::Null);
    h.run();
    assert_eq!(floating(&h, &TOOLS).unwrap().pos, FloatingGroup::cascade(0));
}

#[test]
fn a_palette_dropped_on_the_end_edge_follows_it_when_the_window_grows() {
    let mut h = harness(false);
    let close = |h: &Harness<'_, App>| {
        h.get_by_role_and_label(Role::Button, "Close Tools")
            .rect()
            .max
    };
    // Dragged right until it presses on the end edge: kept inside, on
    // the edge, anchored there. The pointer stays clear of the edge,
    // where a drop would dock the palette in the closed end side.
    let title = h.get_by_label("Tools").rect().center();
    drag_by(&mut h, title, egui::vec2(840.0 - title.x, 0.0));
    let f = floating(&h, &TOOLS).unwrap();
    assert_eq!(f.anchor.x, AnchorEdge::End, "{f:?}");
    assert_eq!(f.anchor.y, AnchorEdge::None, "{f:?}");
    let before = close(&h);
    let gap = 900.0 - before.x;
    // Wider and taller: it keeps its distance to the end edge, and its
    // place from the top.
    h.set_size(egui::vec2(1100.0, 700.0));
    h.run();
    h.run();
    let after = close(&h);
    assert!((1100.0 - after.x - gap).abs() < 0.5, "{before:?} {after:?}");
    assert!((after.y - before.y).abs() < 0.5, "{before:?} {after:?}");
    // The layout says where it now is.
    let moved = floating(&h, &TOOLS).unwrap();
    assert!((moved.pos[0] - f.pos[0] - 200.0).abs() < 0.5, "{moved:?}");
    // Dragged away from the edge: no longer anchored.
    let title = h.get_by_label("Tools").rect().center();
    drag_by(&mut h, title, egui::vec2(-300.0, 0.0));
    let f = floating(&h, &TOOLS).unwrap();
    assert!(f.anchor.is_none(), "{f:?}");
    h.set_size(egui::vec2(900.0, 600.0));
    h.run();
    let x = floating(&h, &TOOLS).unwrap().pos[0];
    h.set_size(egui::vec2(1100.0, 600.0));
    h.run();
    assert_eq!(floating(&h, &TOOLS).unwrap().pos[0], x, "stays put");
}

fn members(h: &Harness<'_, App>, p: &PanelId) -> Vec<PanelId> {
    floating(h, p).map_or_else(Vec::new, |g| g.panels().cloned().collect())
}

/// Opens the navigator floating at `pos`, apart from the tools.
fn open_nav(h: &mut Harness<'static, App>, pos: [f32; 2]) {
    h.state().handle().set_panel_visible(&NAV, true);
    h.state()
        .handle()
        .change_workbench(Change::MovePalette(NAV, pos));
    h.run();
    h.run();
}

/// The navigator stacked under the tools, through the step.
fn stacked(h: &mut Harness<'static, App>) {
    open_nav(h, [400.0, 100.0]);
    h.state()
        .handle()
        .change_workbench(Change::Group(vec![NAV], TOOLS, Join::Below));
    h.run();
    h.run();
    assert_eq!(members(h, &TOOLS), [TOOLS, NAV]);
}

/// Drags from a point to another, in four moves, checking the group hint
/// before the drop with `check`.
fn drag_to(
    h: &mut Harness<'static, App>,
    from: egui::Pos2,
    to: egui::Pos2,
    check: impl FnOnce(&Harness<'static, App>),
) {
    h.drag_at(from);
    h.run();
    for i in 1..=4 {
        h.hover_at(from + (to - from) * (i as f32 / 4.0));
        h.run();
    }
    check(h);
    h.drop_at(to);
    h.run();
    h.run();
}

#[test]
fn a_palette_dropped_on_another_ones_bottom_edge_stacks_and_the_group_moves_as_one() {
    let mut h = harness(false);
    open_nav(&mut h, [400.0, 100.0]);
    // Near the tools' bottom edge: the bottom of their text field.
    let field = h.get_by_role(Role::TextInput).rect();
    let to = egui::pos2(field.center().x, field.bottom() - 2.0);
    let from = h.get_by_label("Navigator").rect().center();
    let before = h.state().handle().last_event();
    drag_to(&mut h, from, to, |h| {
        let hint = palettes::group_hint(h.state().handle()).expect("a group hint");
        assert_eq!((hint.into.clone(), hint.at), (TOOLS, Join::Below));
        assert_eq!(hint.panels, [NAV]);
    });
    assert!(palettes::group_hint(h.state().handle()).is_none());
    assert_eq!(members(&h, &TOOLS), [TOOLS, NAV]);
    let names = events(&h, before);
    assert!(names.contains(&"palette.group".to_string()), "{names:?}");
    assert!(names.contains(&"palette.grouped".to_string()), "{names:?}");
    // Both drawn, one under the other, each with its own band.
    let tools = h.get_by_label("Tools").rect();
    let nav = h.get_by_label("Navigator").rect();
    assert!(nav.top() > tools.bottom(), "{tools:?} {nav:?}");
    assert!(h.query_by_label("Brush").is_some());
    assert!(h.query_by_label("Overview").is_some());
    // The group's band drags both.
    let start = floating(&h, &TOOLS).unwrap().pos;
    let band = h.get_by_label("Tools · Navigator").rect().center();
    drag_by(&mut h, band, egui::vec2(150.0, 100.0));
    let g = floating(&h, &NAV).unwrap();
    assert_eq!(members(&h, &TOOLS), [TOOLS, NAV], "still one group");
    assert!(
        g.pos[0] > start[0] + 100.0 && g.pos[1] > start[1] + 60.0,
        "{start:?} {g:?}"
    );
    let moved = h.get_by_label("Navigator").rect();
    assert!((moved.left() - nav.left() - (g.pos[0] - start[0])).abs() < 0.5);
}

#[test]
fn a_palette_dropped_on_another_ones_top_edge_stacks_above_it() {
    let mut h = harness(false);
    open_nav(&mut h, [400.0, 100.0]);
    let title = h.get_by_label("Tools").rect();
    let to = egui::pos2(title.center().x, title.top() + 2.0);
    let from = h.get_by_label("Navigator").rect().center();
    drag_to(&mut h, from, to, |h| {
        let hint = palettes::group_hint(h.state().handle()).expect("a group hint");
        assert_eq!(hint.at, Join::Above);
    });
    assert_eq!(members(&h, &TOOLS), [NAV, TOOLS]);
}

#[test]
fn a_member_dragged_out_of_its_group_floats_alone_and_each_rolls_up_on_its_own() {
    let mut h = harness(false);
    stacked(&mut h);
    // Rolled up, the navigator alone: the tools stay open.
    h.get_by_role_and_label(Role::Button, "Roll up Navigator")
        .click();
    h.run();
    h.run();
    let g = floating(&h, &TOOLS).unwrap();
    assert!(g.rolled(&NAV) && !g.rolled(&TOOLS));
    assert!(h.query_by_label("Overview").is_none());
    assert!(h.query_by_label("Brush").is_some());
    // Its own band drags it out, where it is dropped (clear of the
    // bottom edge, where a drop would dock it).
    let before = h.state().handle().last_event();
    let from = h.get_by_label("Navigator").rect().center();
    drag_by(&mut h, from, egui::vec2(350.0, 150.0));
    let names = events(&h, before);
    assert!(
        names.contains(&"palette.ungrouped".to_string()),
        "{names:?}"
    );
    assert_eq!(members(&h, &TOOLS), [TOOLS]);
    let nav = floating(&h, &NAV).unwrap();
    assert!(nav.is_lone() && nav.rolled(&NAV), "{nav:?}");
    assert!(nav.pos[0] > 300.0, "{nav:?}");
    // Dropped back over its own group, a member stays in it.
    stacked_again(&mut h);
    let from = h.get_by_label("Navigator").rect().center();
    drag_by(&mut h, from, egui::vec2(0.0, -6.0));
    assert_eq!(members(&h, &TOOLS), [TOOLS, NAV]);
}

fn stacked_again(h: &mut Harness<'static, App>) {
    h.state()
        .handle()
        .change_workbench(Change::Group(vec![NAV], TOOLS, Join::Below));
    h.run();
    h.run();
}

#[test]
fn hooks_veto_grouping_and_ungrouping() {
    let mut h = harness(true);
    // Placed through the layout: the hooks redirect every move.
    h.state().handle().set_panel_visible(&NAV, true);
    h.run();
    let mut layout = h.state().handle().layout();
    layout.floating_mut(&NAV).unwrap().pos = [400.0, 100.0];
    h.state().handle().set_layout(layout);
    h.run();
    h.run();
    assert_eq!(floating(&h, &NAV).unwrap().pos, [400.0, 100.0]);
    let out = h
        .state()
        .handle()
        .change_workbench(Change::Group(vec![NAV], TOOLS, Join::Below));
    assert!(matches!(out, Some(Outcome::Vetoed { .. })), "{out:?}");
    assert_eq!(members(&h, &TOOLS), [TOOLS], "vetoed: apart still");
    // Dragged on the tools' edge: vetoed, the navigator stays where it was.
    let field = h.get_by_role(Role::TextInput).rect();
    let from = h.get_by_label("Navigator").rect().center();
    let before = h.state().handle().last_event();
    drag_to(
        &mut h,
        from,
        egui::pos2(field.center().x, field.bottom() - 2.0),
        |_| {},
    );
    assert_eq!(members(&h, &TOOLS), [TOOLS]);
    assert_eq!(floating(&h, &NAV).unwrap().pos, [400.0, 100.0]);
    let names = events(&h, before);
    assert!(!names.contains(&"palette.grouped".to_string()), "{names:?}");
    // Grouped through the layout, the way out is vetoed too, by a step
    // or by a drag.
    let mut layout = h.state().handle().layout();
    layout.group(&[NAV], &TOOLS, Join::Below).unwrap();
    h.state().handle().set_layout(layout);
    h.run();
    h.run();
    assert_eq!(members(&h, &TOOLS), [TOOLS, NAV]);
    let out = h
        .state()
        .handle()
        .change_workbench(Change::Ungroup(NAV, [10.0, 10.0]));
    assert!(matches!(out, Some(Outcome::Vetoed { .. })), "{out:?}");
    let from = h.get_by_label("Navigator").rect().center();
    drag_by(&mut h, from, egui::vec2(350.0, 250.0));
    assert_eq!(members(&h, &TOOLS), [TOOLS, NAV], "still grouped");
    // Without hooks, the step takes it out.
    let mut h = harness(false);
    stacked(&mut h);
    let out = h
        .state()
        .handle()
        .change_workbench(Change::Ungroup(NAV, [10.0, 10.0]));
    assert!(matches!(out, Some(Outcome::Done(Ok(_)))), "{out:?}");
    assert!(floating(&h, &NAV).unwrap().is_lone());
}

#[test]
fn a_group_dropped_on_the_end_edge_keeps_its_anchor_when_the_window_grows() {
    let mut h = harness(false);
    stacked(&mut h);
    // Dragged right until it presses on the end edge, the pointer clear
    // of the edge, where a drop would dock the group.
    let band = h.get_by_label("Tools · Navigator").rect().center();
    drag_by(&mut h, band, egui::vec2(840.0 - band.x, 0.0));
    let g = floating(&h, &TOOLS).unwrap();
    assert_eq!(g.anchor.x, AnchorEdge::End, "{g:?}");
    assert_eq!(members(&h, &TOOLS), [TOOLS, NAV]);
    let close = |h: &Harness<'_, App>| {
        h.get_by_role_and_label(Role::Button, "Close Navigator")
            .rect()
            .max
    };
    let before = close(&h);
    let gap = 900.0 - before.x;
    h.set_size(egui::vec2(1100.0, 700.0));
    h.run();
    h.run();
    let after = close(&h);
    assert!((1100.0 - after.x - gap).abs() < 0.5, "{before:?} {after:?}");
    assert!((after.y - before.y).abs() < 0.5, "{before:?} {after:?}");
}

#[test]
fn a_group_docks_whole_as_tabs_and_floats_back_as_a_group() {
    let mut h = harness(false);
    stacked(&mut h);
    let before = h.state().handle().last_event();
    let band = h.get_by_label("Tools · Navigator").rect().center();
    let target = h.get_by_label("log body").rect().center();
    drag_by(&mut h, band, target - band);
    let layout = h.state().handle().layout();
    assert!(layout.floating.is_empty(), "{:?}", layout.floating);
    assert_eq!(layout.area_of(&TOOLS), Some(Area::Bottom));
    assert_eq!(layout.area_of(&NAV), Some(Area::Bottom));
    let names = events(&h, before);
    assert_eq!(
        names.iter().filter(|n| *n == "palette.docked").count(),
        1,
        "one step for the group: {names:?}"
    );
    // Floating one of them gives the group back.
    h.state()
        .handle()
        .change_workbench(Change::Float(NAV, [200.0, 150.0]));
    h.run();
    h.run();
    assert_eq!(members(&h, &NAV), [TOOLS, NAV]);
    assert_eq!(floating(&h, &NAV).unwrap().pos, [200.0, 150.0]);
    assert!(h.query_by_label("Tools · Navigator").is_some());
}

#[test]
fn a_palette_dropped_on_another_ones_middle_joins_it_as_a_tab() {
    let mut h = harness(false);
    open_nav(&mut h, [400.0, 100.0]);
    let to = h.get_by_label("Brush").rect().center();
    let from = h.get_by_label("Navigator").rect().center();
    drag_to(&mut h, from, to, |h| {
        let hint = palettes::group_hint(h.state().handle()).expect("a group hint");
        assert_eq!((hint.into.clone(), hint.at), (TOOLS, Join::Tab));
    });
    let g = floating(&h, &TOOLS).unwrap();
    assert_eq!(g.arrangement, Arrangement::Tabs);
    assert_eq!(members(&h, &TOOLS), [TOOLS, NAV]);
    // The dropped one shows; the other waits behind its tab.
    assert!(h.query_by_label("Overview").is_some());
    assert!(h.query_by_label("Brush").is_none());
    h.get_by_role_and_label(Role::Button, "Tools").click();
    h.run();
    h.run();
    assert!(h.query_by_label("Brush").is_some());
    assert!(h.query_by_label("Overview").is_none());
    assert_eq!(floating(&h, &TOOLS).unwrap().shown(), &TOOLS);
    // Saved and read back as tabs.
    let layout = h.state().handle().layout();
    let back = egui_pop_model::layout::Layout::from_json(&layout.to_json()).unwrap();
    assert_eq!(back.floating, layout.floating);
    // The shown member's band drags it out: the other is alone again.
    let band = h.get_by_role_and_label(Role::Button, "Close Tools").rect();
    let from = egui::pos2(band.left() - 64.0, band.center().y);
    drag_by(&mut h, from, egui::vec2(300.0, 250.0));
    assert!(floating(&h, &TOOLS).unwrap().is_lone());
    assert!(floating(&h, &NAV).unwrap().is_lone());
    assert!(h.query_by_label("Brush").is_some());
    assert!(h.query_by_label("Overview").is_some());
}

/// Where a palette's frame is drawn.
fn frame(h: &Harness<'_, App>, p: &PanelId) -> egui::Rect {
    h.ctx
        .memory(|m| m.area_rect(palettes::area_id(p)))
        .expect("the palette is drawn")
}

/// A point on a palette's right edge, halfway down.
fn right_edge(r: egui::Rect) -> egui::Pos2 {
    egui::pos2(r.right() - 1.0, r.center().y)
}

fn near(a: f32, b: f32) -> bool {
    (a - b).abs() < 1.0
}

#[test]
fn a_palette_is_resized_by_its_edges_and_keeps_its_size() {
    let mut h = harness(false);
    assert_eq!(floating(&h, &TOOLS).unwrap().size, None, "its content's");
    let start = frame(&h, &TOOLS);
    let before = h.state().handle().last_event();
    // The corner: both ways.
    drag_by(
        &mut h,
        start.right_bottom() - egui::vec2(1.0, 1.0),
        egui::vec2(60.0, 80.0),
    );
    let size = floating(&h, &TOOLS).unwrap().size.expect("resized");
    assert!(near(size[0], start.width() + 60.0), "{start:?} {size:?}");
    assert!(near(size[1], start.height() + 80.0), "{start:?} {size:?}");
    let drawn = frame(&h, &TOOLS);
    assert!(
        near(drawn.width(), size[0]) && near(drawn.height(), size[1]),
        "{drawn:?} {size:?}"
    );
    assert!(
        events(&h, before).contains(&"palette.resized".to_string()),
        "{:?}",
        events(&h, before)
    );
    // The right edge: the width only.
    drag_by(&mut h, right_edge(drawn), egui::vec2(-30.0, 0.0));
    let narrower = floating(&h, &TOOLS).unwrap().size.unwrap();
    assert!(near(narrower[0], size[0] - 30.0), "{narrower:?}");
    assert_eq!(narrower[1], size[1]);
    // Never narrower than its panel's least width (160).
    let at = right_edge(frame(&h, &TOOLS));
    drag_by(&mut h, at, egui::vec2(-1000.0, 0.0));
    let least = floating(&h, &TOOLS).unwrap().size.unwrap();
    assert!(least[0] >= 160.0 && least[0] < narrower[0], "{least:?}");
    assert!(frame(&h, &TOOLS).width() >= 160.0);
    // Kept in the layout.
    assert!(h.state().handle().layout().to_json().contains("\"size\""));
    // Its content's size again, through the step.
    let out = h
        .state()
        .handle()
        .change_workbench(Change::ResizePalette(TOOLS, None));
    assert!(matches!(out, Some(Outcome::Done(Ok(_)))), "{out:?}");
    h.run();
    h.run();
    assert_eq!(floating(&h, &TOOLS).unwrap().size, None);
    let back = frame(&h, &TOOLS);
    assert!(near(back.height(), start.height()), "{start:?} {back:?}");
}

#[test]
fn a_hook_vetoes_a_resize_and_the_palette_goes_back() {
    let mut h = harness(true);
    let start = frame(&h, &TOOLS);
    drag_by(
        &mut h,
        start.right_bottom() - egui::vec2(1.0, 1.0),
        egui::vec2(60.0, 80.0),
    );
    assert_eq!(floating(&h, &TOOLS).unwrap().size, None);
    let back = frame(&h, &TOOLS);
    assert!(near(back.width(), start.width()), "{start:?} {back:?}");
    assert!(near(back.height(), start.height()), "{start:?} {back:?}");
    // Through the step from code: refused too.
    let out = h
        .state()
        .handle()
        .change_workbench(Change::ResizePalette(TOOLS, Some([300.0, 300.0])));
    assert_eq!(
        out.and_then(|o| o.vetoed().map(str::to_string)).as_deref(),
        Some("palettes keep their size")
    );
}

#[test]
fn a_group_is_resized_as_one_and_its_content_scrolls() {
    let mut h = harness(false);
    stacked(&mut h);
    let out = h
        .state()
        .handle()
        .change_workbench(Change::ResizePalette(TOOLS, Some([280.0, 150.0])));
    assert!(matches!(out, Some(Outcome::Done(Ok(_)))), "{out:?}");
    h.run();
    h.run();
    let g = floating(&h, &TOOLS).unwrap();
    assert_eq!(g.size, Some([280.0, 150.0]));
    let drawn = frame(&h, &TOOLS);
    assert!(
        near(drawn.width(), 280.0) && near(drawn.height(), 150.0),
        "{drawn:?}"
    );
    // Both members are still there, the column scrolling.
    assert!(h.query_by_label("Brush").is_some());
    assert!(h.query_by_label("Overview").is_some());
}

#[test]
fn a_palette_resized_off_the_end_edge_is_no_longer_anchored() {
    let mut h = harness(false);
    // Pressed on the end edge, as when dropped there: anchored.
    let title = h.get_by_label("Tools").rect().center();
    drag_by(&mut h, title, egui::vec2(840.0 - title.x, 0.0));
    let f = floating(&h, &TOOLS).unwrap();
    assert_eq!(f.anchor.x, AnchorEdge::End, "{f:?}");
    // Its right edge pulled in, away from the end edge.
    let r = frame(&h, &TOOLS);
    drag_by(&mut h, right_edge(r), egui::vec2(-40.0, 0.0));
    let f = floating(&h, &TOOLS).unwrap();
    assert!(f.size.is_some(), "{f:?}");
    assert_ne!(f.anchor.x, AnchorEdge::End, "{f:?}");
    // It stays where it was dropped: the left edge did not move.
    assert!(near(frame(&h, &TOOLS).left(), r.left()), "{r:?}");
}

/// Opens the colours floating at `pos`, apart from the others.
fn open_colours(h: &mut Harness<'static, App>, pos: [f32; 2]) {
    h.state().handle().set_panel_visible(&COLOURS, true);
    h.state()
        .handle()
        .change_workbench(Change::MovePalette(COLOURS, pos));
    h.run();
    h.run();
}

/// The navigator as a tab of the tools, shown, through the step.
fn tabbed(h: &mut Harness<'static, App>) {
    open_nav(h, [400.0, 100.0]);
    h.state()
        .handle()
        .change_workbench(Change::Group(vec![NAV], TOOLS, Join::Tab));
    h.run();
    h.run();
    let g = floating(h, &TOOLS).unwrap();
    assert_eq!(g.arrangement, Arrangement::Tabs);
    assert_eq!(g.shown(), &NAV);
}

#[test]
fn a_group_closes_from_its_band_menu_through_a_vetoable_step() {
    let mut h = harness(false);
    stacked(&mut h);
    // No Close on the group's band: an entry of its menu.
    assert!(
        h.query_by_label("Close Tools · Navigator").is_none(),
        "no close button on the group's band"
    );
    let before = h.state().handle().last_event();
    h.get_by_role_and_label(Role::Button, "Tools · Navigator menu")
        .click();
    h.run();
    h.run();
    h.get_by_label("Close group").click();
    h.run();
    h.run();
    let layout = h.state().handle().layout();
    assert!(layout.floating.is_empty(), "{:?}", layout.floating);
    assert!(layout.hidden.contains(&TOOLS) && layout.hidden.contains(&NAV));
    assert!(h.query_by_label("Brush").is_none());
    let names = events(&h, before);
    assert!(
        names.contains(&"palette.close_group".to_string()),
        "{names:?}"
    );
    assert!(
        names.contains(&"palette.group_closed".to_string()),
        "{names:?}"
    );
    // A hook vetoes it: the group stays open.
    let mut h = harness(true);
    let mut layout = h.state().handle().layout();
    layout.float(&NAV, [400.0, 100.0]);
    layout.group(&[NAV], &TOOLS, Join::Below).unwrap();
    h.state().handle().set_layout(layout);
    h.run();
    h.run();
    h.get_by_role_and_label(Role::Button, "Tools · Navigator menu")
        .click();
    h.run();
    h.run();
    h.get_by_label("Close group").click();
    h.run();
    h.run();
    assert_eq!(members(&h, &TOOLS), [TOOLS, NAV], "vetoed: still open");
    let out = h
        .state()
        .handle()
        .change_workbench(Change::CloseGroup(TOOLS));
    assert_eq!(
        out.and_then(|o| o.vetoed().map(str::to_string)).as_deref(),
        Some("groups stay open")
    );
}

#[test]
fn a_tab_dragged_out_of_its_tabs_floats_alone_or_joins_where_it_is_dropped() {
    let mut h = harness(false);
    tabbed(&mut h);
    // The tab itself, not the member's band: dragged out, it floats alone.
    let before = h.state().handle().last_event();
    let from = h
        .get_by_role_and_label(Role::Button, "Tools")
        .rect()
        .center();
    drag_by(&mut h, from, egui::vec2(350.0, 250.0));
    let names = events(&h, before);
    assert!(names.contains(&"palette.ungroup".to_string()), "{names:?}");
    assert!(
        names.contains(&"palette.ungrouped".to_string()),
        "{names:?}"
    );
    assert!(floating(&h, &TOOLS).unwrap().is_lone());
    assert!(floating(&h, &NAV).unwrap().is_lone());
    assert!(h.query_by_label("Brush").is_some());
    assert!(h.query_by_label("Overview").is_some());
    // Dropped on another palette's middle, a tab joins it as a tab.
    let mut h = harness(false);
    tabbed(&mut h);
    open_colours(&mut h, [500.0, 300.0]);
    let from = h
        .get_by_role_and_label(Role::Button, "Tools")
        .rect()
        .center();
    let to = frame(&h, &COLOURS).center();
    drag_to(&mut h, from, to, |h| {
        let hint = palettes::group_hint(h.state().handle()).expect("a group hint");
        assert_eq!((hint.into.clone(), hint.at), (COLOURS, Join::Tab));
        assert_eq!(hint.panels, [TOOLS]);
    });
    assert_eq!(members(&h, &COLOURS), [COLOURS, TOOLS]);
    assert_eq!(floating(&h, &COLOURS).unwrap().shown(), &TOOLS);
    assert!(floating(&h, &NAV).unwrap().is_lone());
    // Dropped back on its own tabs, it stays.
    let mut h = harness(false);
    tabbed(&mut h);
    let from = h
        .get_by_role_and_label(Role::Button, "Tools")
        .rect()
        .center();
    drag_by(&mut h, from, egui::vec2(0.0, 40.0));
    assert_eq!(members(&h, &TOOLS), [TOOLS, NAV]);
    // A click still shows it.
    h.get_by_role_and_label(Role::Button, "Tools").click();
    h.run();
    h.run();
    assert_eq!(floating(&h, &TOOLS).unwrap().shown(), &TOOLS);
}

#[test]
fn closing_the_shown_tab_shows_its_right_neighbour_or_the_left_one_at_the_end() {
    let mut h = harness(false);
    open_nav(&mut h, [400.0, 100.0]);
    open_colours(&mut h, [500.0, 300.0]);
    h.state()
        .handle()
        .change_workbench(Change::Group(vec![NAV, COLOURS], TOOLS, Join::Tab));
    h.run();
    h.run();
    assert_eq!(members(&h, &TOOLS), [TOOLS, NAV, COLOURS]);
    assert_eq!(floating(&h, &TOOLS).unwrap().shown(), &NAV);
    // The middle tab closed: the one on its right shows.
    h.get_by_role_and_label(Role::Button, "Close Navigator")
        .click();
    h.run();
    h.run();
    assert_eq!(members(&h, &TOOLS), [TOOLS, COLOURS]);
    assert_eq!(floating(&h, &TOOLS).unwrap().shown(), &COLOURS);
    assert!(h.query_by_label("Swatches").is_some());
    assert!(h.query_by_label("Brush").is_none());
    // The last tab closed: the one on its left shows.
    h.get_by_role_and_label(Role::Button, "Close Colours")
        .click();
    h.run();
    h.run();
    assert!(floating(&h, &TOOLS).unwrap().is_lone());
    assert!(h.query_by_label("Brush").is_some());
}

#[test]
fn a_palette_dropped_on_the_edge_of_tabs_stacks_beside_them_and_the_tabs_stay_tabs() {
    let mut h = harness(false);
    tabbed(&mut h);
    open_colours(&mut h, [500.0, 300.0]);
    // On the bottom edge of the group of tabs.
    let tabs = frame(&h, &TOOLS);
    let to = egui::pos2(tabs.center().x, tabs.bottom() - 4.0);
    let from = h.get_by_label("Colours").rect().center();
    let before = h.state().handle().last_event();
    drag_to(&mut h, from, to, |h| {
        let hint = palettes::group_hint(h.state().handle()).expect("a group hint");
        assert_eq!((hint.into.clone(), hint.at), (NAV, Join::Below));
    });
    assert!(
        events(&h, before).contains(&"palette.grouped".to_string()),
        "{:?}",
        events(&h, before)
    );
    let g = floating(&h, &TOOLS).unwrap();
    assert_eq!(g.arrangement, Arrangement::Stack, "{g:?}");
    assert_eq!(g.members.len(), 2, "{g:?}");
    let nested = g.member(&NAV).unwrap();
    assert!(nested.is_tabs());
    assert_eq!(nested.panels().cloned().collect::<Vec<_>>(), [TOOLS, NAV]);
    assert_eq!(nested.shown(), &NAV);
    assert_eq!(g.member(&COLOURS).unwrap().panels().count(), 1);
    // Drawn as tabs over the colours: both tabs, the shown one's content.
    assert!(h.query_by_role_and_label(Role::Button, "Tools").is_some());
    assert!(h.query_by_label("Overview").is_some());
    assert!(h.query_by_label("Brush").is_none());
    assert!(h.query_by_label("Swatches").is_some());
    let overview = h.get_by_label("Overview").rect();
    let swatches = h.get_by_label("Swatches").rect();
    assert!(
        swatches.top() > overview.bottom(),
        "{overview:?} {swatches:?}"
    );
    // Another tab shows from the nested tabs.
    h.get_by_role_and_label(Role::Button, "Tools").click();
    h.run();
    h.run();
    assert!(h.query_by_label("Brush").is_some());
    assert!(h.query_by_label("Overview").is_none());
    assert_eq!(
        floating(&h, &TOOLS).unwrap().member(&NAV).unwrap().shown(),
        &TOOLS
    );
    // Docked whole and floated back: nested as it was.
    let was = floating(&h, &TOOLS).unwrap();
    let out = h
        .state()
        .handle()
        .change_workbench(Change::DockGroup(COLOURS, Area::EndSide));
    assert!(matches!(out, Some(Outcome::Done(Ok(_)))), "{out:?}");
    h.run();
    h.run();
    let layout = h.state().handle().layout();
    assert!(layout.floating.is_empty(), "{:?}", layout.floating);
    for p in [&TOOLS, &NAV, &COLOURS] {
        assert_eq!(layout.area_of(p), Some(Area::EndSide), "{p}");
    }
    h.state()
        .handle()
        .change_workbench(Change::Float(TOOLS, was.pos));
    h.run();
    h.run();
    let back = floating(&h, &TOOLS).unwrap();
    assert_eq!(back.members, was.members);
    assert_eq!(back.arrangement, Arrangement::Stack);
}

#[test]
fn a_nested_tab_groups_band_takes_it_out_of_its_stack_whole() {
    let mut h = harness(false);
    tabbed(&mut h);
    open_colours(&mut h, [500.0, 300.0]);
    h.state()
        .handle()
        .change_workbench(Change::Group(vec![COLOURS], NAV, Join::Below));
    h.run();
    h.run();
    assert!(
        floating(&h, &TOOLS)
            .unwrap()
            .member(&NAV)
            .unwrap()
            .is_tabs()
    );
    // Its band between its tabs and its buttons.
    let tab = h.get_by_role_and_label(Role::Button, "Navigator").rect();
    let roll = h
        .get_by_role_and_label(Role::Button, "Roll up Navigator")
        .rect();
    assert!(roll.left() > tab.right() + 8.0, "{tab:?} {roll:?}");
    let from = egui::pos2((tab.right() + roll.left()) / 2.0, tab.center().y);
    let before = h.state().handle().last_event();
    drag_by(&mut h, from, egui::vec2(300.0, 250.0));
    let names = events(&h, before);
    assert!(
        names.contains(&"palette.ungrouped".to_string()),
        "{names:?}"
    );
    let g = floating(&h, &TOOLS).unwrap();
    assert_eq!(g.arrangement, Arrangement::Tabs, "{g:?}");
    assert_eq!(members(&h, &TOOLS), [TOOLS, NAV]);
    assert!(floating(&h, &COLOURS).unwrap().is_lone());
    // Through the step: `palette.ungroup` with `tabs: true`.
    let mut h = harness(false);
    tabbed(&mut h);
    open_colours(&mut h, [500.0, 300.0]);
    h.state()
        .handle()
        .change_workbench(Change::Group(vec![COLOURS], NAV, Join::Above));
    h.run();
    let out = h
        .state()
        .handle()
        .change_workbench(Change::UngroupTabs(TOOLS, [300.0, 300.0]));
    assert!(matches!(out, Some(Outcome::Done(Ok(_)))), "{out:?}");
    assert_eq!(members(&h, &NAV), [TOOLS, NAV]);
    assert_eq!(floating(&h, &NAV).unwrap().pos, [300.0, 300.0]);
}

const MASKS: PanelId = PanelId::from_static("tree.masks");

/// A palette holding a tree: a focusable widget that takes the focus
/// when one of its rows is clicked, and keeps the arrows and Escape.
struct Masks {
    keys: Arc<Mutex<Vec<egui::Key>>>,
}

impl Plugin for Masks {
    fn id(&self) -> PluginId {
        PluginId::from_static("tree")
    }

    fn connect(&mut self, ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
        let keys = self.keys.clone();
        ctx.panel(
            PanelSpec::new(MASKS, "Masks", Area::StartSide)
                .palette()
                .min_size(160.0),
            move |ui, _| tree(ui, &keys),
        )?;
        Ok(())
    }
}

fn tree(ui: &mut egui::Ui, keys: &Mutex<Vec<egui::Key>>) {
    let id = ui.make_persistent_id("masks.tree");
    // The tree first, as a tree view registers itself before its rows.
    let rect = egui::Rect::from_min_size(ui.cursor().min, egui::vec2(ui.available_width(), 48.0));
    let tree = ui.interact(rect, id, egui::Sense::focusable_noninteractive());
    tree.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Other, true, "Mask tree"));
    for row in ["Mask 1", "Mask 2"] {
        if ui.button(row).clicked() {
            ui.memory_mut(|m| m.request_focus(id));
        }
    }
    if ui.memory(|m| m.has_focus(id)) {
        let filter = egui::EventFilter {
            horizontal_arrows: true,
            vertical_arrows: true,
            escape: true,
            tab: false,
        };
        ui.memory_mut(|m| m.set_focus_lock_filter(id, filter));
        if ui.input(|i| i.key_pressed(egui::Key::Delete)) {
            lock(keys).push(egui::Key::Delete);
        }
    }
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

#[test]
fn a_tree_in_a_palette_keeps_the_focus_its_clicked_row_gave_it() {
    let keys = Arc::new(Mutex::new(Vec::new()));
    let app = App::builder()
        .plugin(Masks { keys: keys.clone() })
        .build(&egui::Context::default())
        .unwrap();
    let mut h = Harness::builder()
        .with_size(egui::vec2(900.0, 600.0))
        .build_ui_state(|ui, app: &mut App| app.show(ui), app);
    h.run();
    assert!(floating(&h, &MASKS).is_some(), "a palette");
    let tree = h.ctx.memory(|m| m.focused());
    assert!(tree.is_none(), "nothing focused when it opens");
    h.get_by_label("Mask 1").click();
    h.run();
    let focused = h.ctx.memory(|m| m.focused());
    assert!(
        h.get_by_label("Mask tree").is_focused(),
        "the tree keeps the focus its row gave it, not {focused:?}"
    );
    h.key_press(egui::Key::Delete);
    h.run();
    assert_eq!(*lock(&keys), [egui::Key::Delete], "the tree gets its keys");
    // Escape in the palette still gives the keyboard back.
    h.key_press(egui::Key::Escape);
    h.run();
    assert!(!h.get_by_label("Mask tree").is_focused());
    assert!(h.ctx.memory(|m| m.focused()).is_none());
    assert!(h.query_by_label("Mask 1").is_some(), "still open");
}
