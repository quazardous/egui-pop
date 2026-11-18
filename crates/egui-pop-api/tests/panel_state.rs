//! A panel's state on the bus (`pop.panel.state`): a panel whose content is
//! stale or updating is dimmed with a busy mark on its corner and its tab
//! once the state has lasted the delay, a fast update shows nothing, a
//! failure shows at once with its reason, and ready clears it. The state
//! may be asked from any thread; the panel draws none of it.

use std::path::Path;

use egui_kittest::Harness;
use egui_pop_api::bus::{
    self, PANEL_CHROME, PANEL_STATE_POINTS, PanelState, PanelStateAsk, PanelStateSet, mark_id,
};
use egui_pop_api::{App, Ctx, Plugin, PluginError};
use egui_pop_core::indicator;
use egui_pop_core::motion;
use egui_pop_core::theme::Tokens;
use egui_pop_model::clock::ManualClock;
use egui_pop_model::ids::{PanelId, PluginId};
use egui_pop_model::panels::{Area, PanelSpec};

const SCOPE: PanelId = PanelId::from_static("demo.scope");
const INFO: PanelId = PanelId::from_static("demo.info");

struct Demo;

impl Plugin for Demo {
    fn id(&self) -> PluginId {
        PluginId::from_static("demo")
    }

    fn connect(&mut self, ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
        // Two panels in one area: a tab each.
        ctx.panel(PanelSpec::new(SCOPE, "Scope", Area::EndSide), |ui, _| {
            ui.label("scope body");
        })?
        .panel(PanelSpec::new(INFO, "Info", Area::EndSide), |ui, _| {
            ui.label("info body");
        })?;
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

fn ask(h: &Harness<'_, App>, state: PanelState) -> Vec<String> {
    h.state()
        .handle()
        .request_panel_state(PanelStateAsk::new(SCOPE.as_str(), state, "demo"))
}

/// The content's mark, when drawn and shown.
fn mark(h: &Harness<'_, App>, on_tab: bool) -> Option<indicator::Record> {
    indicator::find(&h.ctx, &mark_id(SCOPE.as_str(), on_tab)).filter(|r| r.shown)
}

fn delay(h: &Harness<'_, App>) -> (f64, f64) {
    let t = Tokens::of(&h.ctx);
    (
        f64::from(t.motion.spinner_delay),
        f64::from(t.motion.spinner_min),
    )
}

#[test]
fn an_update_shows_after_the_delay_then_clears() {
    let (mut h, clock) = harness();
    let (delay, min) = delay(&h);
    let did = ask(&h, PanelState::Stale);
    assert!(did.iter().any(|d| d.starts_with(PANEL_CHROME)), "{did:?}");
    assert_eq!(
        h.state().handle().panel_state(SCOPE.as_str()),
        PanelState::Stale
    );
    h.run();
    assert!(mark(&h, false).is_none(), "nothing before the delay");
    // Stale then updating is one wait: the delay goes on from the first.
    clock.set(delay * 0.6);
    ask(&h, PanelState::Updating);
    h.run();
    assert!(mark(&h, false).is_none(), "still within the delay");
    clock.set(delay + 0.01);
    h.run();
    let shown = mark(&h, false).expect("the busy mark on the content");
    assert_eq!(shown.state, "busy");
    assert_eq!(shown.name.as_deref(), Some("Updating…"));
    let tab = mark(&h, true).expect("the busy mark on the tab");
    assert_eq!(tab.state, "busy");
    // The other panel shows nothing.
    assert!(indicator::find(&h.ctx, &mark_id(INFO.as_str(), false)).is_none());
    // Ready: kept for the minimum once shown, then cleared.
    ask(&h, PanelState::Ready);
    assert_eq!(
        h.state().handle().panel_state(SCOPE.as_str()),
        PanelState::Ready
    );
    h.run();
    assert!(mark(&h, false).is_some(), "kept for the minimum");
    clock.set(delay + 0.01 + min + 0.01);
    h.run();
    assert!(mark(&h, false).is_none(), "cleared with the new data");
    assert!(mark(&h, true).is_none(), "the tab's too");
}

#[test]
fn a_fast_update_never_flickers() {
    let (mut h, clock) = harness();
    let (delay, _) = delay(&h);
    for step in 0..4 {
        let at = f64::from(step) * delay;
        clock.set(at);
        ask(&h, PanelState::Updating);
        h.run();
        clock.set(at + delay * 0.5);
        h.run();
        ask(&h, PanelState::Ready);
        h.run();
        assert!(mark(&h, false).is_none(), "step {step}");
        assert!(mark(&h, true).is_none(), "step {step}");
    }
}

#[test]
fn a_failure_shows_at_once_with_its_reason() {
    let (mut h, _clock) = harness();
    ask(
        &h,
        PanelState::Failed {
            reason: "no pixels".into(),
        },
    );
    h.run();
    let shown = mark(&h, false).expect("the failed mark");
    assert_eq!(shown.state, "failed");
    assert_eq!(shown.name.as_deref(), Some("Update failed: no pixels"));
    assert_eq!(mark(&h, true).expect("on the tab").state, "failed");
    ask(&h, PanelState::Ready);
    h.run();
    assert!(mark(&h, false).is_none());
}

#[test]
fn the_state_may_be_asked_from_another_thread() {
    let (mut h, clock) = harness();
    let (delay, _) = delay(&h);
    let bus = h.state().handle().bus();
    let out = std::thread::spawn(move || {
        bus.alter::<PanelStateSet>(
            PanelStateAsk::new(SCOPE.as_str(), PanelState::Updating, "worker"),
            Vec::new(),
        )
    })
    .join()
    .unwrap();
    assert!(
        out.deferred.is_empty(),
        "run on the worker: {:?}",
        out.deferred
    );
    assert_eq!(
        h.state().handle().panel_state(SCOPE.as_str()),
        PanelState::Updating
    );
    clock.set(delay + 0.01);
    h.run();
    assert!(mark(&h, false).is_some());
}

#[test]
fn the_panel_state_point_is_declared_with_its_listener() {
    let (h, _) = harness();
    let dump = h.state().handle().bus_order_of(&PANEL_STATE_POINTS);
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/bus/panel-state-points.txt");
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
    for p in PANEL_STATE_POINTS {
        assert!(
            bus::CONTRACT.iter().any(|d| d.id == p),
            "{p} in the contract"
        );
        assert!(bus::POINTS.contains(&p), "{p} in the points");
    }
}
