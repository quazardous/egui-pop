//! Zen mode: the centre alone, through a step, Escape bringing the
//! whole workbench back, the saved layout left as it was.

use egui_kittest::Harness;
use egui_kittest::kittest::Queryable as _;
use egui_pop_api::frame::FramePlugin;
use egui_pop_api::workbench::{CMD_ZEN, STEP_ZEN};
use egui_pop_api::zones::{self, Block};
use egui_pop_api::{App, Ctx, DefaultPlugins, Plugin, PluginError};
use egui_pop_model::ids::{BlockId, PanelId, PluginId};
use egui_pop_model::panels::{Area, PanelSpec};

const LOG: PanelId = PanelId::from_static("demo.log");

struct Demo;

impl Plugin for Demo {
    fn id(&self) -> PluginId {
        PluginId::from_static("demo")
    }

    fn connect(&mut self, ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
        ctx.panel(PanelSpec::new(LOG, "Log", Area::Bottom), |ui, _| {
            ui.label("log body");
        })?;
        ctx.block(
            zones::MAIN,
            Block::add(BlockId::from_static("demo.main"), 0, |ui, _| {
                ui.label("main body");
            }),
        )
        .block(
            zones::SIDE_BAR,
            Block::add(BlockId::from_static("demo.side"), 0, |ui, _| {
                ui.label("side body");
            }),
        )
        .block(
            zones::STATUS_BAR,
            Block::add(BlockId::from_static("demo.status"), 0, |ui, _| {
                ui.label("status body");
            }),
        );
        Ok(())
    }
}

fn harness() -> Harness<'static, App> {
    let app = App::builder()
        .plugins(DefaultPlugins)
        .plugin(FramePlugin::own().name("Demo"))
        .plugin(Demo)
        .build(&egui::Context::default())
        .unwrap();
    let mut h = Harness::builder()
        .with_size(egui::vec2(900.0, 600.0))
        .build_ui_state(|ui, app: &mut App| app.show(ui), app);
    h.run();
    h
}

/// What zen mode hides, and the centre it keeps.
const AROUND: [&str; 5] = ["Demo", "Minimize", "side body", "log body", "status body"];

fn shown(h: &Harness<'static, App>, label: &str) -> bool {
    h.query_by_label(label).is_some()
}

#[test]
fn zen_mode_hides_the_bars_and_areas_and_escape_brings_them_back() {
    let mut h = harness();
    for label in AROUND {
        assert!(shown(&h, label), "{label} before zen mode");
    }
    assert!(shown(&h, "File"), "the menu bar");
    assert!(shown(&h, "main body"));
    let handle = h.state().handle().clone();
    // No default key: applications bind one.
    let entry = handle
        .palette()
        .into_iter()
        .find(|e| e.command == CMD_ZEN)
        .expect("zen mode in the palette");
    assert!(entry.keys.is_empty(), "{:?}", entry.keys);
    let layout = serde_json::to_value(handle.layout()).unwrap();
    let mark = handle.last_event();

    handle.run(CMD_ZEN, serde_json::json!(null));
    h.run();
    assert!(handle.zen());
    for label in AROUND {
        assert!(!shown(&h, label), "{label} in zen mode");
    }
    assert!(!shown(&h, "File"), "the menu bar in zen mode");
    assert!(shown(&h, "main body"), "the centre stays");
    // No caption published while the bar is hidden.
    let r = egui_pop_core::frame::regions(&h.ctx).expect("published");
    assert!(r.caption.is_none() && r.close.is_none(), "{r:?}");
    // A step, on the journal; the saved layout is not touched.
    let trace = handle.step_traces().pop().unwrap();
    assert_eq!(trace.step, STEP_ZEN);
    assert_eq!(trace.args["zen"], true);
    assert!(
        handle
            .events_since(mark)
            .iter()
            .any(|e| e.name == "workbench.zen" && e.data["args"]["zen"] == true),
        "the journal says so"
    );
    assert_eq!(serde_json::to_value(handle.layout()).unwrap(), layout);

    // Escape leaves it.
    h.key_press(egui::Key::Escape);
    h.run();
    assert!(!handle.zen());
    for label in AROUND {
        assert!(shown(&h, label), "{label} after zen mode");
    }
    assert!(shown(&h, "File"));
    let trace = handle.step_traces().pop().unwrap();
    assert_eq!(trace.step, STEP_ZEN);
    assert_eq!(trace.args["zen"], false);
    assert!(
        egui_pop_core::frame::regions(&h.ctx)
            .and_then(|r| r.caption)
            .is_some()
    );
    // Escape does nothing more out of zen mode.
    let traces = handle.step_traces().len();
    h.key_press(egui::Key::Escape);
    h.run();
    assert_eq!(handle.step_traces().len(), traces);
}

#[test]
fn a_plugin_may_veto_zen_mode() {
    struct Keep;
    impl Plugin for Keep {
        fn id(&self) -> PluginId {
            PluginId::from_static("keep")
        }
        fn connect(&mut self, ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
            ctx.before(STEP_ZEN, |_, _| {
                egui_pop_api::steps::Before::Veto("the inspector must stay".into())
            });
            Ok(())
        }
    }
    let app = App::builder()
        .plugins(DefaultPlugins)
        .plugin(Demo)
        .plugin(Keep)
        .build(&egui::Context::default())
        .unwrap();
    let out = app.handle().set_zen(true).expect("ran at once");
    assert!(out.vetoed().is_some());
    assert!(!app.handle().zen());
}
