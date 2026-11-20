//! Opening a panel: `pop.panel.open` is the one way, whoever asks (its
//! toggle command, a tab bar's "+", a group's menu, an agent). Closed, a
//! panel opens where it was: among the same tabs at its index, floating
//! at its corner, in its window. That place gone, it joins the focused
//! panel's tab group when its areas allow it, else its declared area.

use std::path::Path;

use egui_kittest::Harness;
use egui_kittest::kittest::Queryable as _;
use egui_pop_api::bus::{self, OpenPlace, PANEL_OPEN, PANEL_OPEN_POINTS, PanelOpenAsk};
use egui_pop_api::workbench::{self, Change, DockControls};
use egui_pop_api::{App, Ctx, Plugin, PluginError};
use egui_pop_model::commands::Origin;
use egui_pop_model::ids::{CommandId, PanelId, PluginId};
use egui_pop_model::layout::DormantPlace;
use egui_pop_model::panels::{Area, PanelSpec};
use serde_json::{Value as Json, json};

const OUT: PanelId = PanelId::from_static("demo.out");
const LOG: PanelId = PanelId::from_static("demo.log");
const CON: PanelId = PanelId::from_static("demo.con");
const TREE: PanelId = PanelId::from_static("demo.tree");
const NOTES: PanelId = PanelId::from_static("demo.notes");
/// Kept to the bottom area.
const DOCK: PanelId = PanelId::from_static("demo.dock");

struct Demo;

impl Plugin for Demo {
    fn id(&self) -> PluginId {
        PluginId::from_static("demo")
    }

    fn connect(&mut self, ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
        for (id, title, area) in [
            (OUT, "Output", Area::Bottom),
            (LOG, "Log", Area::Bottom),
            (CON, "Console", Area::Bottom),
            (TREE, "Tree", Area::EndSide),
            (NOTES, "Notes", Area::EndSide),
        ] {
            ctx.panel(PanelSpec::new(id, title, area).floatable(true), |ui, _| {
                ui.label("body");
            })?;
        }
        ctx.panel(
            PanelSpec::new(DOCK, "Dock", Area::Bottom).only_in(&[Area::Bottom]),
            |ui, _| {
                ui.label("dock body");
            },
        )?;
        Ok(())
    }
}

fn harness(controls: DockControls) -> Harness<'static, App> {
    let app = App::builder()
        .plugin(Demo)
        .dock_controls(controls)
        .build(&egui::Context::default())
        .unwrap();
    let mut h = Harness::builder()
        .with_size(egui::vec2(1000.0, 700.0))
        .build_ui_state(|ui, app: &mut App| app.show(ui), app);
    h.run();
    h
}

fn change(h: &mut Harness<'static, App>, c: Change) {
    let out = h.state().handle().change_workbench(c.clone());
    assert!(out.is_some_and(|o| o.ok()), "{c:?}");
    h.run();
    h.run();
}

fn open(h: &mut Harness<'static, App>, ask: PanelOpenAsk) -> Result<Vec<String>, String> {
    let did = h.state().handle().open_panel(ask);
    h.run();
    h.run();
    did
}

fn last(panel: &PanelId) -> PanelOpenAsk {
    PanelOpenAsk::new(panel.as_str(), OpenPlace::Last, "test")
}

/// Runs a command as an agent would.
fn run(h: &mut Harness<'static, App>, id: &str, args: Json) -> Result<Json, String> {
    let out = h
        .state_mut()
        .run(
            &CommandId::new(id).unwrap(),
            args,
            Origin::Agent("helper".into()),
        )
        .now()
        .expect("answered now");
    h.run();
    h.run();
    out
}

#[test]
fn a_docked_panel_closed_mid_tabs_reopens_at_its_index() {
    let mut h = harness(DockControls::default());
    let app = h.state().handle().clone();
    let (before, _) = app.layout().group_of(&LOG).unwrap();
    assert_eq!(before, vec![OUT, LOG, CON, DOCK]);
    change(&mut h, Change::Hide(LOG));
    assert_eq!(
        app.layout().last_place(&LOG),
        Some(&DormantPlace::Docked {
            area: Area::Bottom,
            beside: vec![OUT, CON, DOCK],
            index: 1,
        })
    );
    let did = open(&mut h, last(&LOG)).unwrap();
    assert!(did.iter().any(|l| l.starts_with(PANEL_OPEN)), "{did:?}");
    let (after, shown) = app.layout().group_of(&LOG).unwrap();
    assert_eq!(after, before, "at its index among the same tabs");
    assert_eq!(after[shown], LOG, "shown");
    assert!(app.layout().closed.is_empty(), "its place used");
}

#[test]
fn a_floating_panel_closed_reopens_floating_where_it_was() {
    let mut h = harness(DockControls::default());
    let app = h.state().handle().clone();
    change(&mut h, Change::Float(TREE, [220.0, 140.0]));
    change(&mut h, Change::Hide(TREE));
    assert!(app.layout().floating_of(&TREE).is_none());
    open(&mut h, last(&TREE)).unwrap();
    let layout = app.layout();
    let g = layout.floating_of(&TREE).expect("floating again");
    assert_eq!(g.pos, [220.0, 140.0]);
}

#[test]
fn a_detached_panel_closed_reopens_in_a_window_at_its_place() {
    let mut h = harness(DockControls::default());
    let app = h.state().handle().clone();
    change(&mut h, Change::Detach(NOTES));
    let rect = app.layout().window_of(&NOTES).expect("detached").rect;
    change(&mut h, Change::Hide(NOTES));
    assert!(app.layout().window_of(&NOTES).is_none());
    open(&mut h, last(&NOTES)).unwrap();
    let layout = app.layout();
    let window = layout.window_of(&NOTES).expect("in a window again");
    assert_eq!(window.rect, rect);
}

#[test]
fn its_place_gone_a_panel_opens_in_the_focused_group() {
    let mut h = harness(DockControls::default());
    let app = h.state().handle().clone();
    change(&mut h, Change::Hide(LOG));
    // The tabs it sat with are gone.
    for p in [OUT, CON, DOCK] {
        change(&mut h, Change::Hide(p));
    }
    change(&mut h, Change::Focus(TREE));
    open(&mut h, last(&LOG)).unwrap();
    let (group, _) = app.layout().group_of(&LOG).unwrap();
    assert!(group.contains(&TREE), "{group:?}");
    assert_eq!(app.layout().area_of(&LOG), Some(Area::EndSide));
}

#[test]
fn its_place_gone_and_the_focused_group_refused_it_opens_in_its_area() {
    let mut h = harness(DockControls::default());
    let app = h.state().handle().clone();
    change(&mut h, Change::Hide(DOCK));
    for p in [OUT, LOG, CON] {
        change(&mut h, Change::Hide(p));
    }
    // The focused group is in the end side, where Dock may not go.
    change(&mut h, Change::Focus(TREE));
    open(&mut h, last(&DOCK)).unwrap();
    assert_eq!(app.layout().area_of(&DOCK), Some(Area::Bottom));
}

#[test]
fn the_toggle_command_opens_a_panel_where_it_was() {
    let mut h = harness(DockControls::default());
    let app = h.state().handle().clone();
    let (before, _) = app.layout().group_of(&LOG).unwrap();
    assert_eq!(
        run(&mut h, "demo.log.toggle", Json::Null).unwrap(),
        json!({ "shown": false })
    );
    assert!(!app.panel_visible(&LOG));
    assert_eq!(
        run(&mut h, "demo.log.toggle", Json::Null).unwrap(),
        json!({ "shown": true })
    );
    assert_eq!(app.layout().group_of(&LOG).unwrap().0, before);
}

#[test]
fn agents_open_a_panel_here_or_in_an_area_through_the_point() {
    let mut h = harness(DockControls::default());
    let app = h.state().handle().clone();
    change(&mut h, Change::Hide(LOG));
    // Here: in the tab group holding the tree.
    let out = run(
        &mut h,
        "workbench.panel.open",
        json!({ "panel": "demo.log", "place": "here", "group": "demo.tree" }),
    )
    .unwrap();
    assert_eq!(out["place"], "here");
    assert_eq!(
        app.layout().group_of(&LOG).unwrap().0,
        vec![TREE, NOTES, LOG]
    );
    // An area: moved there, open as it is.
    run(
        &mut h,
        "workbench.panel.open",
        json!({ "panel": "demo.log", "place": "start_side" }),
    )
    .unwrap();
    assert_eq!(app.layout().area_of(&LOG), Some(Area::StartSide));
    // Refused where its rules keep it out, and an unknown place.
    let refused = run(
        &mut h,
        "workbench.panel.open",
        json!({ "panel": "demo.dock", "place": "here", "group": "demo.tree" }),
    );
    assert!(refused.is_err(), "{refused:?}");
    assert_eq!(app.layout().area_of(&DOCK), Some(Area::Bottom));
    let unknown = run(
        &mut h,
        "workbench.panel.open",
        json!({ "panel": "demo.log", "place": "nowhere" }),
    );
    assert!(unknown.is_err(), "{unknown:?}");
}

/// Right-clicks the empty part of the bottom area's tab bar, after its
/// last tab.
fn right_click_bottom_bar(h: &mut Harness<'static, App>, last_tab: &str) {
    let tab = h.get_by_label(last_tab).rect();
    let at = egui::pos2(tab.right() + 120.0, tab.center().y);
    let button = |pressed| egui::Event::PointerButton {
        pos: at,
        button: egui::PointerButton::Secondary,
        pressed,
        modifiers: egui::Modifiers::NONE,
    };
    h.input_mut().events.push(egui::Event::PointerMoved(at));
    h.run();
    h.input_mut().events.push(button(true));
    h.input_mut().events.push(button(false));
    h.run();
}

#[test]
fn the_group_menu_opens_a_panel_in_that_group() {
    let mut h = harness(DockControls::default());
    let app = h.state().handle().clone();
    change(&mut h, Change::Move(TREE, Area::StartSide));
    right_click_bottom_bar(&mut h, "Dock");
    let submenu = h.get_by_label("Open panel").rect();
    h.get_by_label("Open panel").hover();
    h.run();
    // Tree, beside the submenu: every panel the bottom allows, not its own.
    let entry = h
        .get_all_by_label("Tree")
        .map(|n| n.rect())
        .find(|r| {
            (r.center().y - submenu.center().y).abs() < submenu.height() * 3.0
                && (r.left() >= submenu.right() - 8.0 || r.right() <= submenu.left() + 8.0)
        })
        .expect("the submenu lists Tree");
    h.input_mut()
        .events
        .push(egui::Event::PointerMoved(entry.center()));
    h.run();
    h.get_all_by_label("Tree")
        .find(|n| n.rect() == entry)
        .unwrap()
        .click();
    h.run();
    h.run();
    let (group, _) = app.layout().group_of(&TREE).unwrap();
    assert!(group.contains(&OUT), "{group:?}");
    assert_eq!(app.layout().area_of(&TREE), Some(Area::Bottom));
}

/// Clicks the "+" ending the bottom area's tab bar (the lowest one).
fn click_bottom_plus(h: &mut Harness<'static, App>) {
    let plus = h
        .query_all_by_label("Add tab")
        .max_by(|a, b| a.rect().center().y.total_cmp(&b.rect().center().y))
        .expect("a \"+\" on the bottom tab bar");
    plus.click();
    h.run();
    h.run();
}

#[test]
fn the_tab_bars_plus_lists_the_closed_panels_and_opens_one_there() {
    let mut h = harness(DockControls::default().add_buttons(true));
    let app = h.state().handle().clone();
    change(&mut h, Change::Hide(NOTES));
    click_bottom_plus(&mut h);
    // Only the closed panels: Notes, not the open Tree.
    assert!(h.query_by_label("Notes").is_some());
    h.get_by_label("Notes").click();
    h.run();
    h.run();
    assert_eq!(app.layout().area_of(&NOTES), Some(Area::Bottom));
    // Nothing closed left: said, greyed.
    click_bottom_plus(&mut h);
    assert!(
        h.query_by_label("No closed panel can open here").is_some(),
        "an empty menu says why"
    );
    assert!(workbench::dock_controls(&app).add_buttons);
}

#[test]
fn the_panel_open_point_is_declared_with_its_listener() {
    let h = harness(DockControls::default());
    let dump = h.state().handle().bus_order_of(&PANEL_OPEN_POINTS);
    assert!(dump.contains(PANEL_OPEN), "{dump}");
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/bus/panel-open-points.txt");
    if std::env::var_os("UPDATE_BUS_ORDER").is_some() {
        std::fs::write(&path, &dump).unwrap();
    }
    let reviewed = std::fs::read_to_string(&path).unwrap_or_default();
    assert_eq!(
        dump,
        reviewed,
        "review it, then UPDATE_BUS_ORDER=1 writes {}",
        path.display()
    );
    for p in PANEL_OPEN_POINTS {
        assert!(bus::POINTS.contains(&p), "{p} in the points");
        assert!(
            bus::CONTRACT.iter().any(|d| d.id == p),
            "{p} in the contract"
        );
    }
}
