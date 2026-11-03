//! The curves and wheels page: a tone curve over the luminance
//! histogram, with the targeted adjustment on a grey ramp standing in
//! for the image, and the lift, gamma and gain wheels with a hue and
//! saturation picker. Each change runs its step (`curve.change`,
//! `wheel.change`); the page's plugin caps the gain wheel's chroma with
//! a before-hook.

use std::cell::RefCell;
use std::rc::Rc;

use egui::{Color32, Sense, Ui, vec2};
use egui_pop::api::steps::Before;
use egui_pop::api::stories::Story;
use egui_pop::api::{AppHandle, Ctx, Plugin, PluginError};
use egui_pop::imaging::{self, STEP_CURVE, STEP_WHEEL};
use egui_pop::model::ids::{PluginId, StoryId};
use egui_pop::viewport::curve::{CurveEditor, Pick, PickAction, identity};
use egui_pop::viewport::wheel::{ColorWheel, WheelValue};
use serde_json::json;

use super::section;
use crate::shell::SHELL;

pub const GRADING: PluginId = PluginId::from_static("gallery.grading");

/// The gain wheel's chroma, at most (the page's before-hook).
pub const GAIN_CAP: f64 = 0.6;

/// The page's state.
#[derive(Debug)]
pub struct State {
    pub points: Vec<[f32; 2]>,
    /// Luminance bins behind the curve.
    pub bins: Vec<f32>,
    pub wheels: [WheelValue; 3],
    pub colour: WheelValue,
    /// Whether a pick gesture is under way on the ramp.
    picking: bool,
}

impl Default for State {
    fn default() -> Self {
        let bins = (0..64)
            .map(|i| {
                let x = i as f32 / 63.0;
                // Two humps: shadows and highlights.
                let a = (-((x - 0.3) / 0.12).powi(2)).exp();
                let b = 0.6 * (-((x - 0.72) / 0.1).powi(2)).exp();
                1000.0 * (a + b) + 20.0
            })
            .collect();
        State {
            points: vec![[0.0, 0.0], [0.25, 0.2], [0.75, 0.82], [1.0, 1.0]],
            bins,
            wheels: [
                WheelValue {
                    hue: 220.0,
                    chroma: 0.25,
                    luminance: -0.05,
                },
                WheelValue::default(),
                WheelValue {
                    hue: 40.0,
                    chroma: 0.2,
                    luminance: 0.1,
                },
            ],
            colour: WheelValue {
                hue: 200.0,
                chroma: 0.6,
                luminance: 0.5,
            },
            picking: false,
        }
    }
}

/// The page.
#[derive(Debug, Default)]
pub struct GradingPage;

impl Plugin for GradingPage {
    fn id(&self) -> PluginId {
        GRADING
    }

    fn title(&self) -> String {
        "Curves and wheels".into()
    }

    fn requires(&self) -> Vec<PluginId> {
        vec![SHELL, imaging::IMAGING_PLUGIN]
    }

    fn connect(&mut self, ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
        // A before-hook on the wheels' step: the gain's chroma is capped.
        ctx.before(STEP_WHEEL, |_, args| {
            let chroma = args["to"]["chroma"].as_f64().unwrap_or_default();
            if args["wheel"] != "gain" || chroma <= GAIN_CAP {
                return Before::Go;
            }
            let mut args = args.clone();
            args["to"]["chroma"] = json!(GAIN_CAP);
            Before::Redirect(args)
        });
        let state = Rc::new(RefCell::new(State::default()));
        ctx.story(
            Story::new(StoryId::from_static("grading"), "Curves and wheels", move |ui, app| {
                show(ui, app, &mut state.borrow_mut())
            })
            .description("A tone curve over a histogram, with the value picked in the image (targeted adjustment), the lift, gamma and gain wheels and a hue and saturation picker; each change a step.")
            .group("Components", 76),
        )?;
        Ok(())
    }
}

/// The page.
pub fn show(ui: &mut Ui, app: &AppHandle, s: &mut State) {
    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.label("On the curve: a press adds a point, a drag moves it, a double or right click deletes it, a double click elsewhere resets. Focused: Page Up and Page Down select, the arrows move, Insert adds, Delete removes, R resets.");
            section(ui, "A tone curve over the luminance histogram");
            let pick = ramp(ui, s);
            ui.add_space(4.0);
            CurveEditor::new(&mut s.points, "Tone curve")
                .histogram(&s.bins)
                .default_points(identity())
                .pick(pick)
                .size(vec2(260.0, 260.0))
                .on_change(imaging::curve(app, "tone"))
                .show(ui);
            section(ui, "Colour wheels: lift, gamma and gain");
            ui.horizontal_top(|ui| {
                for (value, (label, name)) in s
                    .wheels
                    .iter_mut()
                    .zip([("Lift", "lift"), ("Gamma", "gamma"), ("Gain", "gain")])
                {
                    ColorWheel::offset(value, label)
                        .size(170.0)
                        .on_change(imaging::wheel(app, name))
                        .show(ui);
                }
            });
            ui.label(format!("The page's plugin keeps the gain's chroma under {GAIN_CAP}."));
            section(ui, "Hue and saturation picker");
            ColorWheel::picker(&mut s.colour, "Colour")
                .size(170.0)
                .on_change(imaging::wheel(app, "colour"))
                .show(ui);
            let last = app
                .step_traces()
                .into_iter()
                .rev()
                .find(|t| t.step == STEP_CURVE || t.step == STEP_WHEEL);
            ui.weak(match last {
                Some(t) => format!(
                    "Last step: {} ({}, {})",
                    t.step,
                    t.args["curve"].as_str().or(t.args["wheel"].as_str()).unwrap_or("?"),
                    t.args["cause"].as_str().unwrap_or("?")
                ),
                None => "Last step: none yet".to_string(),
            });
        });
}

/// A grey ramp standing in for the image: the value under the pointer
/// is its input on the curve; a press selects or adds the point there,
/// a vertical drag moves it.
fn ramp(ui: &mut Ui, s: &mut State) -> Option<Pick> {
    ui.label("Targeted adjustment: point at the ramp, press and drag up or down.");
    let (rect, response) = ui.allocate_exact_size(vec2(260.0, 22.0), Sense::drag());
    if ui.is_rect_visible(rect) {
        let steps = 32;
        for i in 0..steps {
            let x0 = rect.left() + rect.width() * i as f32 / steps as f32;
            let x1 = rect.left() + rect.width() * (i + 1) as f32 / steps as f32;
            let g = egui::ecolor::gamma_u8_from_linear_f32(i as f32 / (steps - 1) as f32);
            ui.painter().rect_filled(
                egui::Rect::from_min_max(egui::pos2(x0, rect.top()), egui::pos2(x1, rect.bottom())),
                0.0,
                Color32::from_gray(g),
            );
        }
    }
    egui_pop::core::a11y::name(&response, egui::Role::Unknown, "Picking ramp");
    let x = response
        .interact_pointer_pos()
        .or(response.hover_pos())
        .map(|p| ((p.x - rect.left()) / rect.width()).clamp(0.0, 1.0))?;
    let action = if response.drag_started() {
        s.picking = true;
        PickAction::Press
    } else if response.drag_stopped() {
        s.picking = false;
        PickAction::Release
    } else if s.picking {
        PickAction::Move(-response.drag_delta().y / 200.0)
    } else {
        PickAction::Hover
    };
    Some(Pick { x, action })
}
