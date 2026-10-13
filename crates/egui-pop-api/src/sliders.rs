//! Sliders' steps (ADR 0011). egui-pop's sliders
//! (`egui_pop_core::slider`) are egui widgets: they see their changes
//! through a hook ([`Slider::on_change`](egui_pop_core::slider::Slider::on_change)),
//! and [`hook`] makes that hook run each finished change as the
//! `slider.change` step of the application:
//!
//! | Step | Arguments | Default |
//! |---|---|---|
//! | `slider.change` | `slider` (the name given to [`hook`]), `thumb` (`value`, `low`, `high`), `from`, `to`, `cause` (`drag`, `key`, `wheel`, `typed`, `reset`, `access`) | writes `to` |
//!
//! A change is finished at the end of its gesture: a drag's release
//! (from the value before the drag), a key, a wheel notch, a typed value,
//! a reset. A before-hook vetoes it (the slider keeps or goes back to
//! `from`) or redirects it (another `to`: snap, clamp); a run-hook gives
//! the value to write (its result); an after-hook follows it. Every run
//! publishes `slider.change` on the journal, so the value-change event
//! is observed there. The frames of a drag are not steps: the slider
//! follows the pointer, and the step decides at the release.
//!
//! ```
//! use egui_pop_api::sliders;
//! use egui_pop_core::slider::Slider;
//!
//! fn exposure(ui: &mut egui::Ui, app: &egui_pop_api::AppHandle, ev: &mut f64) {
//!     ui.add(Slider::new(ev, "Exposure", -3.0..=4.0).on_change(sliders::hook(app, "exposure")));
//! }
//! ```

use egui_pop_core::slider::{Change, Verdict};
use egui_pop_model::ids::StepId;
use serde_json::json;

use crate::app::AppHandle;
use crate::ctx::{Ctx, CtxError};
use crate::steps::{Outcome, StepSpec};

/// A slider's value changes, at the end of a gesture.
pub const STEP_CHANGE: StepId = StepId::from_static("slider.change");

pub(crate) fn register(ctx: &mut Ctx<'_>) -> Result<(), CtxError> {
    ctx.step(
        StepSpec::new(
            STEP_CHANGE,
            "A slider's value changes at the end of a gesture (`slider`, `thumb`, `from`, `to`, `cause`); a veto keeps `from`, a redirect or a run-hook gives another value.",
        )
        .args(json!({ "slider": "exposure", "thumb": "value", "from": 0.0, "to": 0.5, "cause": "drag" })),
    )?;
    Ok(())
}

/// A slider hook ([`Slider::on_change`](egui_pop_core::slider::Slider::on_change),
/// [`RangeSlider::on_change`](egui_pop_core::slider::RangeSlider::on_change))
/// running each finished change as the `slider.change` step, the slider
/// named `slider` in its arguments. A drag's frames go through.
pub fn hook<'a>(app: &'a AppHandle, slider: &'a str) -> impl FnMut(&Change) -> Verdict + 'a {
    move |c: &Change| {
        if !c.done {
            return Verdict::Go;
        }
        let args = json!({
            "slider": slider,
            "thumb": c.thumb.name(),
            "from": c.from,
            "to": c.to,
            "cause": c.cause.name(),
        });
        match app.step(&STEP_CHANGE, args, |_, args| Ok(args["to"].clone())) {
            Outcome::Done(Ok(v)) => match v.as_f64() {
                Some(to) if to == c.to => Verdict::Go,
                Some(to) => Verdict::Redirect(to),
                None => Verdict::Veto,
            },
            Outcome::Done(Err(_)) | Outcome::Vetoed { .. } => Verdict::Veto,
        }
    }
}
