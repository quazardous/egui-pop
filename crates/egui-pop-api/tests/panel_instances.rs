//! Panel instances: a panel declared with several opens as many times as
//! asked (`pop.panel.open` with `instance: "new"`), each `<kind>#<n>`
//! drawn by the kind's one function with a payload of its own, kept in the
//! layout and handed back on restore. Closing an instance drops it with
//! its payload; the journal says `panel.instance.opened` and `closed`.
//! The application names an instance's title (`pop.panel.retitle`), kept
//! with the layout; `null` gives the generated one back.

use egui_kittest::Harness;
use egui_kittest::kittest::Queryable as _;
use egui_pop_api::bus::{
    OpenPlace, PANEL_RETITLE, PANEL_RETITLE_POINTS, PanelOpenAsk, PanelRetitleAsk,
};
use egui_pop_api::workbench::instances::{EVENT_CLOSED, EVENT_OPENED, EVENT_RETITLED};
use egui_pop_api::workbench::{Change, DockControls};
use egui_pop_api::{App, AppHandle, Ctx, Plugin, PluginError};
use egui_pop_model::commands::Origin;
use egui_pop_model::ids::{CommandId, PanelId, PluginId};
use egui_pop_model::layout::SavedLayout;
use egui_pop_model::panels::{Area, PanelSpec};
use serde_json::{Value as Json, json};

/// Declared with several.
const GAL: PanelId = PanelId::from_static("demo.gallery");
/// Declared once.
const LOG: PanelId = PanelId::from_static("demo.log");

struct Demo;

impl Plugin for Demo {
    fn id(&self) -> PluginId {
        PluginId::from_static("demo")
    }

    fn connect(&mut self, ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
        ctx.instance_panel(
            PanelSpec::new(GAL, "Gallery", Area::Center),
            |ui, app, inst| {
                let payload = app.instance_payload(&inst.id);
                let filter = payload.get("filter").and_then(Json::as_str).unwrap_or("-");
                ui.label(format!("{} #{} shows {filter}", inst.title, inst.number));
            },
        )?;
        ctx.panel(PanelSpec::new(LOG, "Log", Area::Bottom), |ui, _| {
            ui.label("log body");
        })?;
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

/// Opens a new instance of the gallery in an area; its id.
fn new_in(h: &mut Harness<'static, App>, area: Area) -> PanelId {
    let app = h.state().handle().clone();
    let did = app
        .open_panel(PanelOpenAsk::new(GAL.as_str(), OpenPlace::Area(area), "test").instance("new"))
        .unwrap();
    h.run();
    h.run();
    let id = app.panel_instances(&GAL).last().cloned().unwrap();
    assert!(did.iter().any(|l| l.contains(id.as_str())), "{did:?}");
    id
}

fn events(app: &AppHandle, name: &str) -> Vec<Json> {
    app.events_since(Default::default())
        .into_iter()
        .filter(|e| e.name == name)
        .map(|e| e.data)
        .collect()
}

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
fn two_instances_keep_separate_payloads() {
    let mut h = harness(DockControls::default());
    let app = h.state().handle().clone();
    let second = new_in(&mut h, Area::Bottom);
    let third = new_in(&mut h, Area::EndSide);
    assert_eq!(second.as_str(), "demo.gallery#2");
    assert_eq!(third.as_str(), "demo.gallery#3");
    assert_eq!(
        app.panel_instances(&GAL),
        vec![GAL, second.clone(), third.clone()]
    );
    app.set_instance_payload(&second, json!({ "filter": "red" }))
        .unwrap();
    app.set_instance_payload(&third, json!({ "filter": "blue" }))
        .unwrap();
    h.run();
    // One draw function, told which instance it draws.
    assert!(h.query_by_label("Gallery #1 shows -").is_some());
    assert!(h.query_by_label("Gallery 2 #2 shows red").is_some());
    assert!(h.query_by_label("Gallery 3 #3 shows blue").is_some());
    assert_eq!(app.instance_payload(&second)["filter"], "red");
    // Said on the journal.
    let opened = events(&app, EVENT_OPENED);
    assert_eq!(opened.len(), 2, "{opened:?}");
    assert_eq!(opened[0]["panel"], "demo.gallery#2");
    assert_eq!(opened[0]["kind"], "demo.gallery");
    // A panel declared once has no new instance.
    let refused =
        app.open_panel(PanelOpenAsk::new(LOG.as_str(), OpenPlace::Last, "test").instance("new"));
    assert!(refused.unwrap_err().contains("opens once"));
    assert!(app.set_instance_payload(&LOG, json!(1)).is_err());
}

#[test]
fn instances_survive_a_layout_save_and_restore() {
    let mut h = harness(DockControls::default());
    let app = h.state().handle().clone();
    let second = new_in(&mut h, Area::Bottom);
    let third = new_in(&mut h, Area::EndSide);
    app.set_instance_payload(&second, json!({ "filter": "red" }))
        .unwrap();
    app.set_instance_payload(&third, json!({ "filter": "blue" }))
        .unwrap();
    let saved = serde_json::to_string(&app.saved_layout()).unwrap();
    // A layout saved before instances existed still loads.
    let mut old: Json = serde_json::from_str(&saved).unwrap();
    old.as_object_mut().unwrap().remove("instances");
    let old: SavedLayout = serde_json::from_value(old).unwrap();
    assert!(old.layout.instances.is_empty());

    let mut again = harness(DockControls::default());
    let restored = again.state().handle().clone();
    assert!(restored.panel_instances(&GAL) == vec![GAL]);
    let loaded: SavedLayout = serde_json::from_str(&saved).unwrap();
    restored.set_layout(loaded.layout);
    again.run();
    again.run();
    assert_eq!(
        restored.panel_instances(&GAL),
        vec![GAL, second.clone(), third.clone()]
    );
    assert_eq!(restored.layout().area_of(&second), Some(Area::Bottom));
    assert_eq!(restored.layout().area_of(&third), Some(Area::EndSide));
    assert_eq!(restored.instance_payload(&second)["filter"], "red");
    assert_eq!(restored.instance_payload(&third)["filter"], "blue");
    assert!(again.query_by_label("Gallery 2 #2 shows red").is_some());
    assert!(again.query_by_label("Gallery 3 #3 shows blue").is_some());
    assert_eq!(events(&restored, EVENT_OPENED).len(), 2);
    // The next one counts on.
    let fourth = new_in(&mut again, Area::Bottom);
    assert_eq!(fourth.as_str(), "demo.gallery#4");
}

#[test]
fn closing_an_instance_drops_it_and_leaves_the_others() {
    let mut h = harness(DockControls::default());
    let app = h.state().handle().clone();
    let second = new_in(&mut h, Area::Bottom);
    let third = new_in(&mut h, Area::EndSide);
    app.set_instance_payload(&second, json!({ "filter": "red" }))
        .unwrap();
    app.set_instance_payload(&third, json!({ "filter": "blue" }))
        .unwrap();
    let out = app.change_workbench(Change::Hide(second.clone()));
    assert!(out.is_some_and(|o| o.ok()));
    h.run();
    h.run();
    // Gone, its payload with it, not kept closed.
    let layout = app.layout();
    assert!(!layout.shows(&second));
    assert!(!layout.hidden.contains(&second));
    assert!(layout.instance(&second).is_none());
    assert_eq!(app.instance_payload(&second), Json::Null);
    assert!(h.query_by_label("Gallery 2 #2 shows red").is_none());
    // The other untouched.
    assert_eq!(layout.area_of(&third), Some(Area::EndSide));
    assert_eq!(app.instance_payload(&third)["filter"], "blue");
    assert!(h.query_by_label("Gallery 3 #3 shows blue").is_some());
    let closed = events(&app, EVENT_CLOSED);
    assert_eq!(closed.len(), 1, "{closed:?}");
    assert_eq!(closed[0]["panel"], "demo.gallery#2");
    // Opening it again by id: it is no more.
    let again = app.open_panel(
        PanelOpenAsk::new(GAL.as_str(), OpenPlace::Last, "test").instance(second.as_str()),
    );
    assert!(again.is_err(), "{again:?}");
    // The panel declared closes as any panel: its place kept.
    let out = app.change_workbench(Change::Hide(GAL));
    assert!(out.is_some_and(|o| o.ok()));
    h.run();
    assert!(app.layout().hidden.contains(&GAL));
    assert!(app.layout().last_place(&GAL).is_some());
    assert_eq!(events(&app, EVENT_CLOSED).len(), 1);
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
fn a_tab_bars_plus_opens_a_new_instance_there() {
    let mut h = harness(DockControls::default().add_buttons(true));
    let app = h.state().handle().clone();
    click_bottom_plus(&mut h);
    h.get_by_label("New Gallery").click();
    h.run();
    h.run();
    let second = PanelId::new("demo.gallery#2").unwrap();
    assert_eq!(app.panel_instances(&GAL), vec![GAL, second.clone()]);
    let (group, _) = app.layout().group_of(&second).unwrap();
    assert!(group.contains(&LOG), "{group:?}");
    // Again: a third, in the same group.
    click_bottom_plus(&mut h);
    h.get_by_label("New Gallery").click();
    h.run();
    h.run();
    let third = PanelId::new("demo.gallery#3").unwrap();
    assert_eq!(app.layout().area_of(&third), Some(Area::Bottom));
}

#[test]
fn an_agent_opens_a_new_instance_through_the_point() {
    let mut h = harness(DockControls::default());
    let app = h.state().handle().clone();
    let out = run(
        &mut h,
        "workbench.panel.open",
        json!({ "panel": "demo.gallery", "instance": "new", "place": "bottom" }),
    )
    .unwrap();
    assert_eq!(out["instance"], "demo.gallery#2");
    let second = PanelId::new("demo.gallery#2").unwrap();
    assert_eq!(app.layout().area_of(&second), Some(Area::Bottom));
    // It opens an instance by its id, here in another area.
    run(
        &mut h,
        "workbench.panel.open",
        json!({ "panel": "demo.gallery", "instance": "demo.gallery#2", "place": "end_side" }),
    )
    .unwrap();
    assert_eq!(app.layout().area_of(&second), Some(Area::EndSide));
    // An instance of another panel is refused.
    let wrong = run(
        &mut h,
        "workbench.panel.open",
        json!({ "panel": "demo.log", "instance": "demo.gallery#2" }),
    );
    assert!(wrong.is_err(), "{wrong:?}");
    // Closed by the agent: dropped.
    run(
        &mut h,
        "workbench.panel.close",
        json!({ "panel": "demo.gallery#2" }),
    )
    .unwrap();
    assert!(app.panel_instances(&GAL) == vec![GAL]);
}

#[test]
fn the_app_names_an_instances_title_and_the_layout_keeps_it() {
    let mut h = harness(DockControls::default());
    let app = h.state().handle().clone();
    let second = new_in(&mut h, Area::Bottom);
    let third = new_in(&mut h, Area::EndSide);
    // Through the point, as an agent or another part would ask it.
    let did = app
        .retitle_panel(PanelRetitleAsk::new(second.as_str(), Some("Picks".into())))
        .unwrap();
    assert!(did.iter().any(|l| l.contains("retitled")), "{did:?}");
    h.run();
    h.run();
    // The tab, read through accessibility, and the instance drawn.
    assert!(h.query_by_label("Picks").is_some());
    assert!(h.query_by_label("Picks #2 shows -").is_some());
    assert!(h.query_by_label("Gallery 2 #2 shows -").is_none());
    // The other keeps its generated title.
    assert!(h.query_by_label("Gallery 3").is_some());
    assert!(h.query_by_label("Gallery 3 #3 shows -").is_some());
    assert_eq!(app.describe_panel(second.as_str()).unwrap().title, "Picks");
    let said = events(&app, EVENT_RETITLED);
    assert_eq!(said.len(), 1, "{said:?}");
    assert_eq!(
        said[0],
        json!({ "panel": "demo.gallery#2", "kind": "demo.gallery", "title": "Picks" })
    );
    // The same title again changes nothing, and says nothing.
    app.set_instance_title(&second, Some("Picks".into()))
        .unwrap();
    assert_eq!(events(&app, EVENT_RETITLED).len(), 1);

    // Saved and restored with the layout.
    let saved = serde_json::to_string(&app.saved_layout()).unwrap();
    let mut again = harness(DockControls::default());
    let restored = again.state().handle().clone();
    let loaded: SavedLayout = serde_json::from_str(&saved).unwrap();
    restored.set_layout(loaded.layout);
    again.run();
    again.run();
    assert_eq!(
        restored
            .layout()
            .instance(&second)
            .unwrap()
            .title
            .as_deref(),
        Some("Picks")
    );
    assert!(again.query_by_label("Picks").is_some());
    assert!(again.query_by_label("Picks #2 shows -").is_some());
    assert!(again.query_by_label("Gallery 3 #3 shows -").is_some());

    // `null` gives the generated title back.
    restored.set_instance_title(&second, None).unwrap();
    again.run();
    again.run();
    assert!(again.query_by_label("Picks").is_none());
    assert!(again.query_by_label("Gallery 2 #2 shows -").is_some());
    assert_eq!(restored.layout().instance(&second).unwrap().title, None);
    let said = events(&restored, EVENT_RETITLED);
    assert_eq!(said.last().unwrap()["title"], Json::Null);
    // An instance without a title saves none: old layouts read the same.
    let saved: Json = serde_json::to_value(restored.saved_layout()).unwrap();
    let instances = saved["instances"].as_array().cloned().unwrap_or_default();
    assert!(
        instances.iter().all(|i| i.get("title").is_none()),
        "{saved}"
    );
    assert!(again.query_by_label("Gallery 3").is_some());
    assert_eq!(restored.layout().area_of(&third), Some(Area::EndSide));
}

#[test]
fn retitling_anything_but_an_open_instance_is_refused() {
    let mut h = harness(DockControls::default());
    let app = h.state().handle().clone();
    let second = new_in(&mut h, Area::Bottom);
    let refused = |id: &str, title: Option<&str>| {
        app.retitle_panel(PanelRetitleAsk::new(id, title.map(str::to_string)))
            .unwrap_err()
    };
    // A panel declared once.
    assert!(refused("demo.log", Some("x")).contains("no instance"));
    // The panel declared, instance 1.
    assert!(refused("demo.gallery", Some("x")).contains("panel declared"));
    // An instance not open.
    assert!(refused("demo.gallery#7", Some("x")).contains("no open instance"));
    // Not a panel at all.
    assert!(!refused("nobody.here", Some("x")).is_empty());
    // A blank title.
    assert!(refused(second.as_str(), Some("  ")).contains("blank"));
    // Refused from code too, and nothing said.
    assert!(app.set_instance_title(&LOG, Some("x".into())).is_err());
    assert!(events(&app, EVENT_RETITLED).is_empty());
    // The point is declared, with its listener.
    let dump = app.bus_order_of(&PANEL_RETITLE_POINTS);
    assert!(dump.contains(PANEL_RETITLE), "{dump}");
    for p in PANEL_RETITLE_POINTS {
        assert!(egui_pop_api::bus::POINTS.contains(&p));
        assert!(egui_pop_api::bus::CONTRACT.iter().any(|d| d.id == p));
    }
}
