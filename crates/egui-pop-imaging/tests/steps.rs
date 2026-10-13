//! Each tool's finished changes run its step; a drag's frames do not.

use egui_pop_api::steps::Before;
use egui_pop_api::{App, Ctx, Plugin, PluginError};
use egui_pop_imaging::{ImagingPlugin, STEP_CURVE, STEP_HISTOGRAM, STEP_WHEEL, STEP_ZOOM};
use egui_pop_model::ids::PluginId;
use egui_pop_viewport::change::{Cause, Change, Verdict};
use egui_pop_viewport::histogram::Drag;
use egui_pop_viewport::wheel::WheelValue;
use egui_pop_viewport::{View, Zoom};
use serde_json::json;

/// Snaps a histogram's drag to tenths, refuses any change of the
/// `locked` curve, keeps wheels' chroma under a half, and keeps zooms
/// at 100 % at most.
struct Rules;

impl Plugin for Rules {
    fn id(&self) -> PluginId {
        PluginId::from_static("rules")
    }

    fn requires(&self) -> Vec<PluginId> {
        vec![egui_pop_imaging::IMAGING_PLUGIN]
    }

    fn connect(&mut self, ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
        ctx.before(STEP_HISTOGRAM, |_, args| {
            let d = args["to"]["delta"].as_f64().unwrap_or_default();
            let mut args = args.clone();
            args["to"]["delta"] = json!((d * 10.0).round() / 10.0);
            Before::Redirect(args)
        });
        ctx.before(STEP_CURVE, |_, args| {
            if args["curve"] == "locked" {
                Before::Veto("locked".into())
            } else {
                Before::Go
            }
        });
        ctx.before(STEP_WHEEL, |_, args| {
            let c = args["to"]["chroma"].as_f64().unwrap_or_default();
            let mut args = args.clone();
            args["to"]["chroma"] = json!(c.min(0.5));
            Before::Redirect(args)
        });
        ctx.before(STEP_ZOOM, |_, args| match args["to"]["zoom"].as_f64() {
            Some(z) if z > 1.0 => {
                let mut args = args.clone();
                args["to"]["zoom"] = json!(1.0);
                Before::Redirect(args)
            }
            _ => Before::Go,
        });
        Ok(())
    }
}

fn change<T: Clone>(from: T, to: T, cause: Cause, done: bool) -> Change<T> {
    Change {
        from,
        to,
        cause,
        done,
    }
}

#[test]
fn finished_changes_run_their_step_and_frames_go_through() {
    let app = App::builder()
        .plugin(ImagingPlugin)
        .plugin(Rules)
        .build(&egui::Context::default())
        .unwrap();
    let h = app.handle();
    assert!(h.diagnostics().is_empty(), "{:?}", h.diagnostics());

    let mut hist = egui_pop_imaging::histogram(h, "main");
    let d = |delta| Drag {
        at: Some(0.75),
        delta,
    };
    let runs = h.step_traces().len();
    assert_eq!(
        hist(&change(d(0.0), d(0.13), Cause::Drag, false)),
        Verdict::Allow
    );
    assert_eq!(h.step_traces().len(), runs, "a drag's frames are not steps");
    match hist(&change(d(0.0), d(0.13), Cause::Drag, true)) {
        Verdict::Redirect(to) => assert!((to.delta - 0.1).abs() < 1e-6, "{to:?}"),
        v => panic!("{v:?}"),
    }
    let trace = h.step_traces().pop().unwrap();
    assert_eq!(trace.step, STEP_HISTOGRAM);
    assert_eq!(trace.asked["histogram"], "main");
    assert_eq!(trace.asked["cause"], "drag");

    let identity = vec![[0.0f32, 0.0], [1.0, 1.0]];
    let added = vec![[0.0f32, 0.0], [0.5, 0.6], [1.0, 1.0]];
    let add = change(identity.clone(), added.clone(), Cause::Add, true);
    assert_eq!(egui_pop_imaging::curve(h, "tone")(&add), Verdict::Allow);
    assert_eq!(egui_pop_imaging::curve(h, "locked")(&add), Verdict::Veto);

    let w = |chroma| WheelValue {
        hue: 210.0,
        chroma,
        luminance: 0.0,
    };
    match egui_pop_imaging::wheel(h, "lift")(&change(w(0.0), w(0.8), Cause::Drag, true)) {
        Verdict::Redirect(to) => assert_eq!(to.chroma, 0.5),
        v => panic!("{v:?}"),
    }

    let to = View::at(4.0, [10.0, 20.0]);
    match egui_pop_imaging::zoom(h, "main")(&change(View::fit(), to, Cause::Command, true)) {
        Verdict::Redirect(v) => assert_eq!(v.zoom, Zoom::Px(1.0)),
        v => panic!("{v:?}"),
    }
}
