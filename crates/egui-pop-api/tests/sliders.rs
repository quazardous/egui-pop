//! The `slider.change` step, run from a slider's hook.

use egui_pop_api::sliders::{self, STEP_CHANGE};
use egui_pop_api::steps::Before;
use egui_pop_api::{App, Ctx, Plugin, PluginError};
use egui_pop_core::slider::{Cause, Change, Thumb, Verdict};
use egui_pop_model::ids::PluginId;
use serde_json::json;

/// Snaps `gain` to halves, refuses any change of `locked`.
struct Rules;

impl Plugin for Rules {
    fn id(&self) -> PluginId {
        PluginId::from_static("rules")
    }

    fn connect(&mut self, ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
        ctx.before(STEP_CHANGE, |_, args| {
            if args["slider"] == "locked" {
                return Before::Veto("locked".into());
            }
            let to = args["to"].as_f64().unwrap_or_default();
            let mut args = args.clone();
            args["to"] = json!((to * 2.0).round() / 2.0);
            Before::Redirect(args)
        });
        Ok(())
    }
}

#[test]
fn finished_changes_run_the_step_and_frames_go_through() {
    let app = App::builder()
        .plugin(Rules)
        .build(&egui::Context::default())
        .unwrap();
    let h = app.handle();
    assert!(h.diagnostics().is_empty(), "{:?}", h.diagnostics());
    let change = Change {
        thumb: Thumb::Value,
        from: 0.0,
        to: 1.3,
        cause: Cause::Drag,
        done: true,
    };
    let mut gain = sliders::hook(h, "gain");
    assert_eq!(gain(&change), Verdict::Redirect(1.5));
    let runs = h.step_traces().len();
    assert_eq!(
        gain(&Change {
            done: false,
            ..change
        }),
        Verdict::Go
    );
    assert_eq!(h.step_traces().len(), runs, "a drag's frames are not steps");
    let trace = h.step_traces().pop().unwrap();
    assert_eq!(trace.step, STEP_CHANGE);
    assert_eq!(trace.asked["cause"], "drag");
    assert_eq!(trace.asked["thumb"], "value");
    let mut locked = sliders::hook(h, "locked");
    assert_eq!(locked(&change), Verdict::Veto);
    // Already on a half: the change goes as it is.
    assert_eq!(gain(&Change { to: 2.5, ..change }), Verdict::Go);
}
