//! A panel's kind (`PanelKind`): a control or mixed panel carries the
//! "edits" mark in its tab's one mark slot and in its floating palette's
//! band, a view panel none; a busy or failed state wins the slot while it
//! shows; `pop.panel.describe` on the bus carries the kind. The chrome draws
//! the mark, never the panel.

use std::path::Path;

use egui_kittest::Harness;
use egui_pop_api::bus::{
    self, PANEL_DESCRIBE, PANEL_DESCRIBE_POINTS, PanelDescribe, PanelState, PanelStateAsk, mark_id,
};
use egui_pop_api::{App, Ctx, Plugin, PluginError};
use egui_pop_core::indicator;
use egui_pop_core::motion;
use egui_pop_core::theme::Tokens;
use egui_pop_model::clock::ManualClock;
use egui_pop_model::ids::{PanelId, PluginId};
use egui_pop_model::panels::{Area, PanelKind, PanelSpec};

const PARAMS: PanelId = PanelId::from_static("demo.params");
const SCOPES: PanelId = PanelId::from_static("demo.scopes");
const INFO: PanelId = PanelId::from_static("demo.info");
const TOOLS: PanelId = PanelId::from_static("demo.tools");
const NAV: PanelId = PanelId::from_static("demo.nav");

struct Demo;

impl Plugin for Demo {
    fn id(&self) -> PluginId {
        PluginId::from_static("demo")
    }

    fn connect(&mut self, ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
        // Three tabs in one area: a control, a mixed and a view panel;
        // two palettes, a control and a view.
        ctx.panel(
            PanelSpec::new(PARAMS, "Parameters", Area::EndSide).kind(PanelKind::Control),
            |ui, _| {
                ui.label("params body");
            },
        )?
        .panel(
            PanelSpec::new(SCOPES, "Scopes", Area::EndSide).kind(PanelKind::Mixed),
            |ui, _| {
                ui.label("scopes body");
            },
        )?
        .panel(PanelSpec::new(INFO, "Info", Area::EndSide), |ui, _| {
            ui.label("info body");
        })?
        .panel(
            PanelSpec::new(TOOLS, "Tools", Area::StartSide)
                .palette()
                .kind(PanelKind::Control)
                .closed(),
            |ui, _| {
                ui.label("tools body");
            },
        )?
        .panel(
            PanelSpec::new(NAV, "Nav", Area::StartSide)
                .palette()
                .closed(),
            |ui, _| {
                ui.label("nav body");
            },
        )?;
        Ok(())
    }
}

fn harness() -> (Harness<'static, App>, ManualClock) {
    let app = App::builder()
        .plugin(Demo)
        .build(&egui::Context::default())
        .unwrap();
    let mut h = Harness::builder()
        .with_size(egui::vec2(900.0, 500.0))
        .build_ui_state(|ui, app: &mut App| app.show(ui), app);
    let clock = motion::freeze(&h.ctx, 0.0);
    h.run();
    (h, clock)
}

/// A panel's one mark on its tab or palette band, when drawn and shown.
fn slot(h: &Harness<'_, App>, panel: &PanelId) -> Option<indicator::Record> {
    indicator::find(&h.ctx, &mark_id(panel.as_str(), true)).filter(|r| r.shown)
}

/// Opens a panel (a palette floats).
fn show(h: &mut Harness<'_, App>, panel: &PanelId) {
    h.state().handle().set_panel_visible(panel, true);
    h.run();
}

#[test]
fn a_control_or_mixed_tab_shows_the_edits_mark_a_view_tab_none() {
    let (mut h, _clock) = harness();
    for p in [PARAMS, SCOPES, INFO] {
        show(&mut h, &p);
    }
    let params = slot(&h, &PARAMS).expect("the control panel's tab mark");
    assert_eq!(params.state, "edit");
    assert_eq!(params.name.as_deref(), Some("Edits your work"));
    let scopes = slot(&h, &SCOPES).expect("the mixed panel's tab mark");
    assert_eq!(scopes.name.as_deref(), Some("Parts of it edit your work"));
    assert!(slot(&h, &INFO).is_none(), "a view panel's tab has none");
    // On the tab, never over the content.
    assert!(indicator::find(&h.ctx, &mark_id(PARAMS.as_str(), false)).is_none());
}

#[test]
fn a_busy_or_failed_state_wins_the_slot_then_the_edits_mark_comes_back() {
    let (mut h, clock) = harness();
    show(&mut h, &PARAMS);
    let ask = |h: &Harness<'_, App>, state| {
        h.state()
            .handle()
            .request_panel_state(PanelStateAsk::new(PARAMS.as_str(), state, "demo"))
    };
    let t = Tokens::of(&h.ctx);
    let (delay, min) = (
        f64::from(t.motion.spinner_delay),
        f64::from(t.motion.spinner_min),
    );
    ask(&h, PanelState::Updating);
    h.run();
    assert_eq!(slot(&h, &PARAMS).unwrap().state, "edit", "within the delay");
    clock.set(delay + 0.01);
    h.run();
    assert_eq!(
        slot(&h, &PARAMS).unwrap().state,
        "busy",
        "the busy mark wins"
    );
    ask(
        &h,
        PanelState::Failed {
            reason: "no pixels".into(),
        },
    );
    h.run();
    assert_eq!(slot(&h, &PARAMS).unwrap().state, "failed");
    ask(&h, PanelState::Ready);
    clock.set(delay + min + 0.1);
    h.run();
    assert_eq!(slot(&h, &PARAMS).unwrap().state, "edit");
}

#[test]
fn a_floating_palette_shows_the_mark_in_its_band() {
    let (mut h, clock) = harness();
    for p in [TOOLS, NAV] {
        show(&mut h, &p);
    }
    let app = h.state().handle().clone();
    assert!(app.layout().floating_of(&TOOLS).is_some(), "it floats");
    let tools = slot(&h, &TOOLS).expect("the control palette's band mark");
    assert_eq!(tools.state, "edit");
    assert!(slot(&h, &NAV).is_none(), "a view palette's band has none");
    // Its band has the busy mark too, which wins.
    app.request_panel_state(PanelStateAsk::new(
        TOOLS.as_str(),
        PanelState::Stale,
        "demo",
    ));
    h.run();
    clock.set(f64::from(Tokens::of(&h.ctx).motion.spinner_delay) + 0.01);
    h.run();
    assert_eq!(slot(&h, &TOOLS).unwrap().state, "busy");
}

#[test]
fn panel_describe_carries_the_kind() {
    let (h, _clock) = harness();
    let app = h.state().handle();
    let kind = |p: &PanelId| app.describe_panel(p.as_str()).map(|d| d.kind);
    assert_eq!(kind(&PARAMS), Some(PanelKind::Control));
    assert_eq!(kind(&SCOPES), Some(PanelKind::Mixed));
    assert_eq!(kind(&INFO), Some(PanelKind::View));
    let tools = app.describe_panel(TOOLS.as_str()).unwrap();
    assert!(tools.palette);
    assert_eq!(tools.title, "Tools");
    let json = serde_json::to_value(&tools).unwrap();
    assert_eq!(json["kind"], "control");
    assert_eq!(app.describe_panel("demo.unknown"), None);
}

#[test]
fn the_panel_describe_point_is_declared_with_its_listener() {
    let (h, _) = harness();
    let dump = h.state().handle().bus_order_of(&PANEL_DESCRIBE_POINTS);
    assert!(dump.contains(PANEL_DESCRIBE), "{dump}");
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/bus/panel-describe-points.txt");
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
    for p in PANEL_DESCRIBE_POINTS {
        assert!(
            bus::CONTRACT.iter().any(|d| d.id == p),
            "{p} in the contract"
        );
        assert!(bus::POINTS.contains(&p), "{p} in the points");
    }
    assert_eq!(
        <PanelDescribe as bus::bus_contract::Point>::ID,
        "pop.panel.describe"
    );
}
