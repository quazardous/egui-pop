//! Zen mode on any panel: the panel that matters alone, full size, in the
//! window holding it (a side area's, a detached window's), named by the
//! tab menu's "Zen this panel" or found in order (the focused view, the
//! application's choice on `pop.zen.panel`, the centre's active tab);
//! leaving it draws the layout unchanged.

use std::sync::{Arc, Mutex};

use egui_kittest::Harness;
use egui_kittest::kittest::Queryable as _;
use egui_pop_api::bus::bus_contract::ListenOpts;
use egui_pop_api::bus::{self, ZEN_POINTS, ZenPanel};
use egui_pop_api::workbench::{CMD_LEAVE_ZEN, CMD_ZEN, Change, STEP_ZEN};
use egui_pop_api::{App, Ctx, Plugin, PluginError};
use egui_pop_model::ids::{PanelId, PluginId};
use egui_pop_model::panels::{Area, PanelKind, PanelSpec};
use serde_json::{Value as Json, json};

const VIEWER: PanelId = PanelId::from_static("demo.viewer");
const NOTES: PanelId = PanelId::from_static("demo.notes");
const COMPARE: PanelId = PanelId::from_static("demo.compare");
const PARAMS: PanelId = PanelId::from_static("demo.params");
const LOG: PanelId = PanelId::from_static("demo.log");

/// Every panel's body, by its panel.
const BODIES: [(PanelId, &str); 5] = [
    (VIEWER, "viewer body"),
    (NOTES, "notes body"),
    (COMPARE, "compare body"),
    (PARAMS, "params body"),
    (LOG, "log body"),
];

struct Demo;

impl Plugin for Demo {
    fn id(&self) -> PluginId {
        PluginId::from_static("demo")
    }

    fn connect(&mut self, ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
        let spec = |id: PanelId, title: &str, area: Area, kind: PanelKind| {
            PanelSpec::new(id, title, area)
                .kind(kind)
                .floatable(true)
                .detachable(true)
        };
        for (id, title, area, kind) in [
            (VIEWER, "Viewer", Area::Center, PanelKind::View),
            (NOTES, "Notes", Area::Center, PanelKind::Control),
            (COMPARE, "Compare", Area::EndSide, PanelKind::View),
            (PARAMS, "Params", Area::StartSide, PanelKind::Control),
            (LOG, "Log", Area::Bottom, PanelKind::Mixed),
        ] {
            let body = BODIES.iter().find(|(p, _)| *p == id).unwrap().1;
            ctx.panel(spec(id, title, area, kind), move |ui, _| {
                ui.label(body);
            })?;
        }
        Ok(())
    }
}

fn harness() -> Harness<'static, App> {
    let app = App::builder()
        .plugin(Demo)
        .build(&egui::Context::default())
        .unwrap();
    let mut h = Harness::builder()
        .with_size(egui::vec2(1000.0, 700.0))
        .build_ui_state(|ui, app: &mut App| app.show(ui), app);
    h.run();
    h
}

fn shown(h: &Harness<'static, App>, label: &str) -> bool {
    h.query_all_by_label(label).next().is_some()
}

/// The panels' bodies drawn, in `BODIES`' order.
fn bodies(h: &Harness<'static, App>) -> Vec<&'static str> {
    BODIES
        .iter()
        .map(|(_, b)| *b)
        .filter(|b| shown(h, b))
        .collect()
}

fn layout_json(h: &Harness<'static, App>) -> Json {
    serde_json::to_value(h.state().handle().layout()).unwrap()
}

fn zen(h: &mut Harness<'static, App>, args: Json) {
    h.state().handle().run(CMD_ZEN, args);
    h.run();
    h.run();
}

#[test]
fn a_side_panel_zenned_is_drawn_alone_and_leaving_restores_the_layout() {
    let mut h = harness();
    let app = h.state().handle().clone();
    assert_eq!(
        bodies(&h),
        ["viewer body", "compare body", "params body", "log body"]
    );
    let before = layout_json(&h);

    zen(&mut h, json!({ "panel": COMPARE }));
    assert!(app.zen());
    assert_eq!(app.zen_panel(), Some(COMPARE));
    assert_eq!(bodies(&h), ["compare body"], "the side panel alone");
    assert!(!shown(&h, "File"), "no menu bar");
    let trace = app.step_traces().pop().unwrap();
    assert_eq!(trace.step, STEP_ZEN);
    assert_eq!(trace.args["panel"], "demo.compare");
    assert_eq!(layout_json(&h), before, "the layout is not touched");

    // Escape leaves it: the layout drawn as it was.
    h.key_press(egui::Key::Escape);
    h.run();
    assert!(!app.zen());
    assert_eq!(app.zen_panel(), None);
    assert_eq!(
        bodies(&h),
        ["viewer body", "compare body", "params body", "log body"]
    );
    assert!(shown(&h, "File"));
    assert_eq!(layout_json(&h), before);
}

#[test]
fn a_detached_windows_panel_is_zenned_in_its_window() {
    let mut h = harness();
    let app = h.state().handle().clone();
    assert!(app.change_workbench(Change::Detach(COMPARE)).unwrap().ok());
    h.run();
    h.run();
    assert!(app.layout().window_of(&COMPARE).is_some());
    let before = layout_json(&h);

    zen(&mut h, json!({ "panel": COMPARE }));
    assert_eq!(app.zen_panel(), Some(COMPARE));
    // The main window keeps its workbench; the window shows its panel
    // alone, without its tab.
    assert!(shown(&h, "File"), "the main window is as it was");
    assert!(shown(&h, "viewer body") && shown(&h, "compare body"));
    assert!(
        h.query_by_role_and_label(egui::accesskit::Role::Tab, "Compare")
            .is_none(),
        "no tab in the zenned window"
    );
    assert_eq!(layout_json(&h), before);

    app.run(CMD_LEAVE_ZEN, Json::Null);
    h.run();
    h.run();
    assert!(!app.zen());
    assert!(
        h.query_by_role_and_label(egui::accesskit::Role::Tab, "Compare")
            .is_some()
    );
    assert_eq!(layout_json(&h), before);
}

#[test]
fn the_tab_menu_zens_its_panel() {
    let mut h = harness();
    let app = h.state().handle().clone();
    // Notes, the centre's tab behind the Viewer (its tab says it edits).
    h.get_by_role_and_label(egui::accesskit::Role::Tab, "Notes, Edits your work")
        .click_secondary();
    h.run();
    // With its key beside it.
    h.get_by_label_contains("Zen this panel").click();
    h.run();
    h.run();
    // Named, a control panel is zenned too.
    assert_eq!(app.zen_panel(), Some(NOTES));
    assert_eq!(bodies(&h), ["notes body"]);
}

#[test]
fn without_a_panel_named_the_focused_view_then_the_apps_choice_then_the_centre() {
    let mut h = harness();
    let app = h.state().handle().clone();
    // Nothing focused, no choice: the centre's active tab.
    assert_eq!(app.zen_target(), Some(VIEWER));

    // The application's choice, over the centre.
    let choice = Arc::new(Mutex::new(Some(COMPARE.to_string())));
    let answer = choice.clone();
    app.bus()
        .listen::<ZenPanel>("demo.zen", ListenOpts::new(), move |_, value| {
            *value = answer.lock().unwrap().clone();
            Ok(())
        })
        .unwrap();
    assert_eq!(app.zen_target(), Some(COMPARE));
    // A choice that is not open is passed over.
    *choice.lock().unwrap() = Some("demo.nowhere".into());
    assert_eq!(app.zen_target(), Some(VIEWER));
    *choice.lock().unwrap() = Some(COMPARE.to_string());

    // A focused control panel is passed over; a focused view wins.
    assert!(app.focus_panel(&PARAMS).unwrap().ok());
    h.run();
    assert_eq!(app.zen_target(), Some(COMPARE));
    assert!(app.focus_panel(&LOG).unwrap().ok());
    h.run();
    assert_eq!(app.zen_target(), Some(LOG), "a mixed panel is a view");

    // The key (Shift+F11) zens it; again, it leaves.
    h.key_press_modifiers(egui::Modifiers::SHIFT, egui::Key::F11);
    h.run();
    h.run();
    assert_eq!(app.zen_panel(), Some(LOG));
    assert_eq!(bodies(&h), ["log body"]);
    zen(&mut h, Json::Null);
    assert!(!app.zen());
}

#[test]
fn a_panel_not_open_is_refused() {
    let mut h = harness();
    let app = h.state().handle().clone();
    assert!(app.change_workbench(Change::Hide(COMPARE)).unwrap().ok());
    h.run();
    let out = app.set_zen_panel(&COMPARE).unwrap();
    assert!(!out.ok(), "{out:?}");
    assert!(!app.zen());
}

#[test]
fn the_zen_point_is_declared() {
    let h = harness();
    for p in ZEN_POINTS {
        assert!(
            bus::CONTRACT.iter().any(|d| d.id == p),
            "{p} in the contract"
        );
        assert!(bus::POINTS.contains(&p), "{p} in the points");
    }
    assert_eq!(<ZenPanel as bus::bus_contract::Point>::ID, "pop.zen.panel");
    // No listener of egui-pop's own: the application answers.
    assert_eq!(h.state().handle().zen_choice(), None);
}
