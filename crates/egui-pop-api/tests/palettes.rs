//! Palettes: floating panels over the workbench, opened floating, docked
//! and floated again, dragged (snapped, clamped), docked by a drop over
//! an area, hidden and shown by Tab, rolled up, pinned, giving the
//! keyboard back on Escape, every change through its step.

use egui::accesskit::Role;
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable as _;
use egui_pop_api::palettes::{self, CMD_RESET, STEP_DOCK, STEP_GROUP, STEP_MOVE, STEP_UNGROUP};
use egui_pop_api::steps::{Before, Outcome};
use egui_pop_api::workbench::Change;
use egui_pop_api::{App, Ctx, Plugin, PluginError};
use egui_pop_model::ids::{PanelId, PluginId};
use egui_pop_model::layout::{AnchorEdge, FloatingGroup, Join};
use egui_pop_model::panels::{Area, PanelSpec};
use serde_json::{Value as Json, json};

const TOOLS: PanelId = PanelId::from_static("demo.tools");
const NAV: PanelId = PanelId::from_static("demo.nav");
const LOG: PanelId = PanelId::from_static("demo.log");

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
    // Its own band drags it out, where it is dropped.
    let before = h.state().handle().last_event();
    let from = h.get_by_label("Navigator").rect().center();
    drag_by(&mut h, from, egui::vec2(350.0, 250.0));
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
    open_nav(&mut h, [400.0, 100.0]);
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
    let band = h.get_by_label("Tools · Navigator").rect().center();
    drag_by(&mut h, band, egui::vec2(2000.0, 0.0));
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
