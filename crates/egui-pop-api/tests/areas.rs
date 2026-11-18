//! The interactive areas (`egui_pop_core::areas`) in a workbench: a
//! panel's areas name it, its edit areas are inert while its state is not
//! current (`panel.state`), and `ui.areas` on the bus lists them.

use std::path::Path;

use egui_kittest::Harness;
use egui_pop_api::bus::{self, AREA_POINTS, AREA_REGISTRY, PanelState, PanelStateAsk, UiAreas};
use egui_pop_api::{App, Ctx, Plugin, PluginError};
use egui_pop_core::areas::{self, AreaKind, AreaSpec, Grip};
use egui_pop_core::motion;
use egui_pop_model::ids::{PanelId, PluginId};
use egui_pop_model::panels::{Area, PanelKind, PanelSpec};

const SCOPES: PanelId = PanelId::from_static("demo.scopes");

struct Demo;

impl Plugin for Demo {
    fn id(&self) -> PluginId {
        PluginId::from_static("demo")
    }

    fn connect(&mut self, ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
        // A mixed panel: an edit area (an exposure grip) over a display
        // one (a scope's picture).
        ctx.panel(
            PanelSpec::new(SCOPES, "Scopes", Area::EndSide).kind(PanelKind::Mixed),
            |ui, _| {
                let sense = if areas::inert(ui) {
                    egui::Sense::hover()
                } else {
                    egui::Sense::drag()
                };
                let (rect, r) = ui.allocate_exact_size(egui::vec2(160.0, 40.0), sense);
                areas::register(
                    ui,
                    &r,
                    AreaSpec::edit("demo.exposure", "Exposure")
                        .value("+0.30 EV")
                        .path("exposure")
                        .grip(Grip::horizontal(rect.right_center())),
                );
                let (_, r) = ui.allocate_exact_size(egui::vec2(160.0, 40.0), egui::Sense::hover());
                areas::register(ui, &r, AreaSpec::display("demo.waveform"));
            },
        )?;
        Ok(())
    }
}

fn harness() -> Harness<'static, App> {
    let app = App::builder()
        .plugin(Demo)
        .build(&egui::Context::default())
        .unwrap();
    let mut h = Harness::builder()
        .with_size(egui::vec2(900.0, 500.0))
        .build_ui_state(|ui, app: &mut App| app.show(ui), app);
    motion::freeze(&h.ctx, 0.0);
    h.state().handle().set_panel_visible(&SCOPES, true);
    h.run();
    h
}

#[test]
fn ui_areas_lists_a_panels_areas_and_its_edit_area_is_inert_while_stale() {
    let mut h = harness();
    let app = h.state().handle().clone();
    let listed = app.ui_areas();
    let edit = listed.iter().find(|a| a.id == "demo.exposure").unwrap();
    assert_eq!(edit.kind, AreaKind::Edit);
    assert_eq!(edit.panel.as_deref(), Some(SCOPES.as_str()));
    assert_eq!(edit.path.as_deref(), Some("exposure"));
    assert!(!edit.inert);
    assert_eq!(edit.grips.len(), 1);
    let display = listed.iter().find(|a| a.id == "demo.waveform").unwrap();
    assert_eq!(display.kind, AreaKind::Display);
    assert!(display.grips.is_empty());
    let json = serde_json::to_value(edit).unwrap();
    assert_eq!(json["kind"], "edit");
    assert_eq!(json["control"], "Exposure");

    for state in [PanelState::Stale, PanelState::Updating] {
        app.request_panel_state(PanelStateAsk::new(SCOPES.as_str(), state, "demo"));
        h.run();
        let edit = app
            .ui_areas()
            .into_iter()
            .find(|a| a.id == "demo.exposure")
            .unwrap();
        assert!(edit.inert, "inert while not current");
        assert!(edit.grips.is_empty(), "no grip while inert");
    }
    let display = app
        .ui_areas()
        .into_iter()
        .find(|a| a.id == "demo.waveform")
        .unwrap();
    assert!(!display.inert, "a display area is never inert");
    app.request_panel_state(PanelStateAsk::new(
        SCOPES.as_str(),
        PanelState::Ready,
        "demo",
    ));
    h.run();
    let edit = app
        .ui_areas()
        .into_iter()
        .find(|a| a.id == "demo.exposure")
        .unwrap();
    assert!(!edit.inert && edit.grips.len() == 1, "live again");
}

#[test]
fn a_closed_panels_areas_leave_the_list() {
    let mut h = harness();
    let app = h.state().handle().clone();
    assert!(app.ui_areas().iter().any(|a| a.id == "demo.exposure"));
    app.set_panel_visible(&SCOPES, false);
    h.run();
    assert!(!app.ui_areas().iter().any(|a| a.id == "demo.exposure"));
}

#[test]
fn the_ui_areas_point_is_declared_with_its_listener() {
    let h = harness();
    let dump = h.state().handle().bus_order_of(&AREA_POINTS);
    assert!(dump.contains(AREA_REGISTRY), "{dump}");
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/bus/area-points.txt");
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
    for p in AREA_POINTS {
        assert!(
            bus::CONTRACT.iter().any(|d| d.id == p),
            "{p} in the contract"
        );
        assert!(bus::POINTS.contains(&p), "{p} in the points");
    }
    assert_eq!(<UiAreas as bus::bus_contract::Point>::ID, "ui.areas");
}
