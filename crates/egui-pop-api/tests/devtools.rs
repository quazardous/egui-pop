//! Devtools: off by default, opened by F12 once the setting allows it,
//! its tabs reading the debug bridge, its actions going through a step a
//! plugin may veto.

use egui_kittest::Harness;
use egui_kittest::kittest::Queryable as _;
use egui_pop_api::app::STEP_DEBUG_CALL;
use egui_pop_api::devtools::{DevtoolsPlugin, PANEL_DEVTOOLS, SETTING_DEVTOOLS, STEP_CALL};
use egui_pop_api::steps::Before;
use egui_pop_api::{App, Ctx, DefaultPlugins, Plugin, PluginError};
use egui_pop_model::ids::PluginId;
use egui_pop_model::values::{Change, Set, Value};

struct Kiosk;

impl Plugin for Kiosk {
    fn id(&self) -> PluginId {
        PluginId::from_static("kiosk")
    }

    fn connect(&mut self, ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
        ctx.before(STEP_CALL, |_, args| {
            if args["collector"] == "plugins" {
                Before::Veto("no plugin switching here".into())
            } else {
                Before::Go
            }
        });
        Ok(())
    }
}

/// Vetoes every call of the `announcements` collector at the bridge's
/// own step, whoever calls.
struct BridgeKiosk;

impl Plugin for BridgeKiosk {
    fn id(&self) -> PluginId {
        PluginId::from_static("bridge_kiosk")
    }

    fn connect(&mut self, ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
        ctx.before(STEP_DEBUG_CALL, |_, args| {
            if args["collector"] == "announcements" {
                Before::Veto("no announcements here".into())
            } else {
                Before::Go
            }
        });
        Ok(())
    }
}

fn harness() -> Harness<'static, App> {
    let app = App::builder()
        .plugins(DefaultPlugins)
        .plugin(DevtoolsPlugin::default())
        .plugin(Kiosk)
        .plugin(BridgeKiosk)
        .build(&egui::Context::default())
        .unwrap();
    let mut h = Harness::builder()
        .with_size(egui::vec2(1000.0, 700.0))
        .build_ui_state(|ui, app: &mut App| app.show(ui), app);
    h.run();
    h
}

fn enable(h: &mut Harness<'_, App>) {
    let store = h.state().handle().store();
    egui_pop_core::lock(&store)
        .set(Set::new(
            &format!("settings/{SETTING_DEVTOOLS}"),
            Change::To(Value::Bool(true)),
            egui_pop_model::commands::Origin::User,
        ))
        .unwrap();
    h.run();
}

#[test]
fn devtools_open_only_once_the_setting_allows_them() {
    let mut h = harness();
    h.key_press(egui::Key::F12);
    h.run();
    assert!(!h.state().handle().panel_visible(&PANEL_DEVTOOLS));
    enable(&mut h);
    h.key_press(egui::Key::F12);
    h.run();
    h.run();
    assert!(h.state().handle().panel_visible(&PANEL_DEVTOOLS));
    assert!(h.query_by_label("Journal").is_some());
    // The keys tab shows the resolution of F12 itself.
    assert!(
        h.query_by_label_contains("F12: runs app.devtools")
            .is_some()
    );
    h.key_press(egui::Key::F12);
    h.run();
    assert!(!h.state().handle().panel_visible(&PANEL_DEVTOOLS));
}

#[test]
fn a_collector_action_from_the_panel_goes_through_a_step() {
    let mut h = harness();
    enable(&mut h);
    h.state().handle().focus_panel(&PANEL_DEVTOOLS);
    h.run();
    h.get_by_label("Collectors").click();
    h.run();
    h.get_by_label("plugins").click();
    h.run();
    h.get_by_label("set_enabled (mutates)").click();
    h.run();
    assert!(h.query_by_label_contains("vetoed by kiosk").is_some());
    let t = h.state().handle().step_traces().pop().unwrap();
    assert_eq!(t.step, STEP_CALL);
    // Turning the setting off closes the panel.
    let store = h.state().handle().store();
    egui_pop_core::lock(&store)
        .set(Set::new(
            &format!("settings/{SETTING_DEVTOOLS}"),
            Change::To(Value::Bool(false)),
            egui_pop_model::commands::Origin::User,
        ))
        .unwrap();
    h.run();
    h.run();
    assert!(!h.state().handle().panel_visible(&PANEL_DEVTOOLS));
}

#[test]
fn the_panel_is_the_user_and_a_veto_on_the_bridge_covers_it() {
    let mut h = harness();
    enable(&mut h);
    h.state().handle().focus_panel(&PANEL_DEVTOOLS);
    h.run();
    h.get_by_label("Collectors").click();
    h.run();
    // The bridge's step is vetoed for the panel as for agents.
    h.get_by_label("announcements").click();
    h.run();
    h.get_by_label("announce (mutates)").click();
    h.run();
    assert!(
        h.query_by_label_contains("vetoed by bridge_kiosk: no announcements here")
            .is_some()
    );
    // The panel calls as the user: an action agents may not call runs.
    h.get_by_label("focus").click();
    h.run();
    h.get_by_label("clear (mutates)").click();
    h.run();
    assert!(h.query_by_label_contains("ok: ").is_some());
    let t = h
        .state()
        .handle()
        .step_traces()
        .into_iter()
        .rfind(|t| t.step == STEP_DEBUG_CALL)
        .unwrap();
    assert_eq!(t.args["origin"], "user");
    assert_eq!(t.args["action"], "clear");
}

#[test]
fn the_screen_collector_tells_each_windows_scale() {
    let h = harness();
    let snap = h.state().handle().debug_snapshot();
    let w = &snap["screen"]["windows"][0];
    assert_eq!(w["viewport"], "root");
    assert!(w["pixels_per_point"].as_f64().unwrap() > 0.0);
}
