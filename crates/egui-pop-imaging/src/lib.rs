//! The image tools of `egui-pop-viewport` in an application: each
//! finished change of a tool runs as a step (ADR 0011), so plugins may
//! veto it, redirect it, replace it or follow it, and the journal
//! records it.
//!
//! The tools are egui widgets: they see their changes through a hook
//! (`on_change`). The functions here give a hook running each finished
//! change (`done`: a drag's release, a key, a click, a reset) as the
//! tool's step, named after the tool in its arguments; a drag's frames
//! go through untouched.
//!
//! | Step | Arguments | Default |
//! |---|---|---|
//! | `histogram.drag` | `histogram`, `from`, `to` (`{at, delta}`: where the drag began, 0..1 or `null` for keys, and how far it moved in widths), `cause` | writes `to` |
//! | `curve.change` | `curve`, `from`, `to` (`[[x, y], ...]`), `cause` (`drag`, `add`, `delete`, `reset`, `key`, `pick`, `access`) | writes `to` |
//! | `wheel.change` | `wheel`, `from`, `to` (`{hue, chroma, luminance}`), `cause` | writes `to` |
//! | `zoom.change` | `viewport`, `from`, `to` (`{zoom: "fit" or a number, center: [x, y]}`), `cause` (`command`, `drag`, `click`, `wheel`, `key`, `reset`) | writes `to` |
//! | `cursor.resolve` | `viewport`, `query` (`{tool, modifiers, dragging, panning, handle, over_image, brush}`), `default` (`{system}`, `{tool}`, `{image}` or `{brush}`) | gives `default` |
//! | `graph.connect`, `graph.disconnect`, `graph.move`, `graph.delete`, `graph.add` | `graph`, the edit's fields, `cause` | hands the edit to the application ([`graph`]) |
//!
//! A before-hook vetoes the change (the tool keeps, or goes back to,
//! `from`) or redirects it (another `to`); a run-hook gives the value to
//! write (its result); an after-hook follows it. [`ImagingPlugin`]
//! declares the steps.
//!
//! `cursor.resolve` is not a change: a viewport asks it which cursor to
//! show each time the situation under the pointer changes (the tool, the
//! keys held, a drag, a handle). A run-hook (or a redirect of `default`)
//! gives another cursor; a veto shows the default.
//!
//! ```
//! use egui_pop_api::AppHandle;
//! use egui_pop_viewport::curve::CurveEditor;
//!
//! fn tone(ui: &mut egui::Ui, app: &AppHandle, points: &mut Vec<[f32; 2]>) {
//!     CurveEditor::new(points, "Tone curve")
//!         .on_change(egui_pop_imaging::curve(app, "tone"))
//!         .show(ui);
//! }
//! ```

pub mod graph;

pub use graph::{GRAPHS, Graphs, show_graph};

use egui_pop_api::steps::{Outcome, StepSpec};
use egui_pop_api::{AppHandle, Ctx, Plugin, PluginError};
use egui_pop_model::ids::{PluginId, StepId};
use egui_pop_viewport::View;
use egui_pop_viewport::change::{Change, StepValue, Verdict};
use egui_pop_viewport::cursor::{Cursor, CursorQuery};
use egui_pop_viewport::histogram::Drag;
use egui_pop_viewport::wheel::WheelValue;
use serde_json::json;

/// A histogram's drag ends (or a key moves it).
pub const STEP_HISTOGRAM: StepId = StepId::from_static("histogram.drag");
/// A curve's points change.
pub const STEP_CURVE: StepId = StepId::from_static("curve.change");
/// A colour wheel's value changes.
pub const STEP_WHEEL: StepId = StepId::from_static("wheel.change");
/// A zoom control or a navigator changes a viewport's view.
pub const STEP_ZOOM: StepId = StepId::from_static("zoom.change");
/// A viewport asks which cursor to show.
pub const STEP_CURSOR: StepId = StepId::from_static("cursor.resolve");

/// The plugin's id.
pub const IMAGING_PLUGIN: PluginId = PluginId::from_static("egui_pop.imaging");

/// The steps, as [`ImagingPlugin`] declares them.
pub fn specs() -> Vec<StepSpec> {
    let mut specs = tool_specs();
    specs.extend(graph::specs());
    specs
}

/// The image tools' steps.
fn tool_specs() -> Vec<StepSpec> {
    vec![
        StepSpec::new(
            STEP_HISTOGRAM,
            "A histogram's drag ends, or a key moves it (`histogram`, `from`, `to`: {at, delta}, `cause`); a veto gives the gesture back, a redirect or a run-hook gives another distance.",
        )
        .args(json!({
            "histogram": "main",
            "from": { "at": 0.8, "delta": 0.0 },
            "to": { "at": 0.8, "delta": 0.12 },
            "cause": "drag",
        })),
        StepSpec::new(
            STEP_CURVE,
            "A curve's points change at the end of a gesture (`curve`, `from`, `to`: [[x, y], ...], `cause`); a veto keeps `from`, a redirect or a run-hook gives other points.",
        )
        .args(json!({
            "curve": "tone",
            "from": [[0.0, 0.0], [1.0, 1.0]],
            "to": [[0.0, 0.0], [0.5, 0.6], [1.0, 1.0]],
            "cause": "add",
        })),
        StepSpec::new(
            STEP_WHEEL,
            "A colour wheel's value changes at the end of a gesture (`wheel`, `from`, `to`: {hue, chroma, luminance}, `cause`); a veto keeps `from`, a redirect or a run-hook gives another value.",
        )
        .args(json!({
            "wheel": "lift",
            "from": { "hue": 0.0, "chroma": 0.0, "luminance": 0.0 },
            "to": { "hue": 210.0, "chroma": 0.2, "luminance": 0.0 },
            "cause": "drag",
        })),
        StepSpec::new(
            STEP_ZOOM,
            "A zoom control or a navigator changes a viewport's view (`viewport`, `from`, `to`: {zoom, center}, `cause`); a veto keeps `from`, a redirect or a run-hook gives another view.",
        )
        .args(json!({
            "viewport": "main",
            "from": { "zoom": "fit", "center": [0.0, 0.0] },
            "to": { "zoom": 1.0, "center": [3000.0, 2000.0] },
            "cause": "command",
        })),
        StepSpec::new(
            STEP_CURSOR,
            "A viewport asks which cursor to show when the situation under the pointer changes (`viewport`, `query`: {tool, modifiers, dragging, panning, handle, over_image, brush}, `default`: egui-pop's cursor); a run-hook or a redirect of `default` gives another ({system}, {tool}, {image} or {brush}), a veto shows the default.",
        )
        .args(json!({
            "viewport": "main",
            "query": CursorQuery::new(egui_pop_viewport::cursor::Tool::Zoom).to_json(),
            "default": { "tool": "zoom-in" },
        })),
    ]
}

/// Declares the image tools' steps, and the node editors' steps,
/// registry ([`GRAPHS`]) and agent commands.
#[derive(Debug, Default)]
pub struct ImagingPlugin;

impl Plugin for ImagingPlugin {
    fn id(&self) -> PluginId {
        IMAGING_PLUGIN
    }

    fn title(&self) -> String {
        "Image tools".into()
    }

    fn connect(&mut self, ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
        for spec in specs() {
            ctx.step(spec)?;
        }
        graph::connect(ctx)?;
        Ok(())
    }
}

/// A hook running each finished change as `step`, the tool named
/// `name` under `key` in its arguments.
fn hook<'a, T: StepValue>(
    app: &'a AppHandle,
    step: StepId,
    key: &'static str,
    name: &'a str,
) -> impl FnMut(&Change<T>) -> Verdict<T> + 'a {
    move |c: &Change<T>| {
        if !c.done {
            return Verdict::Allow;
        }
        let mut args = json!({
            "from": c.from.to_json(),
            "to": c.to.to_json(),
            "cause": c.cause.name(),
        });
        args[key] = json!(name);
        match app.step(&step, args, |_, args| Ok(args["to"].clone())) {
            Outcome::Done(Ok(v)) => match T::from_json(&v) {
                Some(to) if to == c.to => Verdict::Allow,
                Some(to) => Verdict::Redirect(to),
                None => Verdict::Veto,
            },
            Outcome::Done(Err(_)) | Outcome::Vetoed { .. } => Verdict::Veto,
        }
    }
}

/// A histogram hook ([`Histogram::on_change`](egui_pop_viewport::histogram::Histogram::on_change))
/// running each finished change as the `histogram.drag` step.
pub fn histogram<'a>(
    app: &'a AppHandle,
    name: &'a str,
) -> impl FnMut(&Change<Drag>) -> Verdict<Drag> + 'a {
    hook(app, STEP_HISTOGRAM, "histogram", name)
}

/// A curve's points, `[x, y]` in 0..1.
pub type Points = Vec<[f32; 2]>;

/// A curve editor hook ([`CurveEditor::on_change`](egui_pop_viewport::curve::CurveEditor::on_change))
/// running each finished change as the `curve.change` step.
pub fn curve<'a>(
    app: &'a AppHandle,
    name: &'a str,
) -> impl FnMut(&Change<Points>) -> Verdict<Points> + 'a {
    hook(app, STEP_CURVE, "curve", name)
}

/// A colour wheel hook ([`ColorWheel::on_change`](egui_pop_viewport::wheel::ColorWheel::on_change))
/// running each finished change as the `wheel.change` step.
pub fn wheel<'a>(
    app: &'a AppHandle,
    name: &'a str,
) -> impl FnMut(&Change<WheelValue>) -> Verdict<WheelValue> + 'a {
    hook(app, STEP_WHEEL, "wheel", name)
}

/// A zoom control or navigator hook
/// ([`Navigator::on_change`](egui_pop_viewport::navigator::Navigator::on_change),
/// [`ZoomControl::on_change`](egui_pop_viewport::navigator::ZoomControl::on_change))
/// running each finished change as the `zoom.change` step, the viewport
/// named `name`.
pub fn zoom<'a>(
    app: &'a AppHandle,
    name: &'a str,
) -> impl FnMut(&Change<View>) -> Verdict<View> + 'a {
    hook(app, STEP_ZOOM, "viewport", name)
}

/// A viewport's cursor hook ([`Viewport::on_cursor`](egui_pop_viewport::Viewport::on_cursor))
/// running each new situation as the `cursor.resolve` step, the viewport
/// named `name`: plugins give another cursor for a tool, a key held, a
/// handle. An answer that is not a cursor keeps the default.
pub fn cursor<'a>(
    app: &'a AppHandle,
    name: &'a str,
) -> impl FnMut(&CursorQuery, Cursor) -> Cursor + 'a {
    move |q: &CursorQuery, default: Cursor| {
        let args = json!({
            "viewport": name,
            "query": q.to_json(),
            "default": default.to_json(),
        });
        match app.step(&STEP_CURSOR, args, |_, args| Ok(args["default"].clone())) {
            Outcome::Done(Ok(v)) => Cursor::from_json(&v).unwrap_or(default),
            Outcome::Done(Err(_)) | Outcome::Vetoed { .. } => default,
        }
    }
}
