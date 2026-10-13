//! The sliders page: the slider family in the manner of darktable's
//! bauhaus sliders. It shows the compact row (soft and hard range, typed
//! expressions, signed values, a quad button as a toggle or a picker),
//! a logarithmic bar, gradient tracks (hue, temperature) and range
//! sliders. It also shows the `slider.change` step, which the page's
//! plugin hooks to snap one slider to halves.

use std::cell::RefCell;
use std::rc::Rc;

use egui::Ui;
use egui_pop::api::sliders::{self, STEP_CHANGE};
use egui_pop::api::steps::Before;
use egui_pop::api::stories::Story;
use egui_pop::api::{AppHandle, Ctx, Plugin, PluginError};
use egui_pop::core::icon::Icon;
use egui_pop::core::slider::{Gradient, RangeSlider, Slider};
use egui_pop::model::ids::{PluginId, StoryId};
use serde_json::json;

use super::section;
use crate::shell::SHELL;

pub const SLIDERS: PluginId = PluginId::from_static("gallery.sliders");

/// The slider the page's before-hook snaps to halves.
pub const SNAPPED: &str = "gallery.snapped";

#[derive(Debug)]
struct State {
    exposure: f64,
    contrast: f64,
    contrast_on: bool,
    black: f64,
    picks: u32,
    radius: f64,
    hue: f64,
    temperature: f64,
    iso: (f64, f64),
    focal: (f64, f64),
    stepped: f64,
    snapped: f64,
    last: String,
}

impl Default for State {
    fn default() -> Self {
        Self {
            exposure: 0.5,
            contrast: 0.12,
            contrast_on: true,
            black: -0.002,
            picks: 0,
            radius: 8.0,
            hue: 210.0,
            temperature: 5200.0,
            iso: (100.0, 3200.0),
            focal: (24.0, 70.0),
            stepped: 5.0,
            snapped: 5.0,
            last: "none yet".into(),
        }
    }
}

/// The sliders page.
#[derive(Debug, Default)]
pub struct SlidersPage;

impl Plugin for SlidersPage {
    fn id(&self) -> PluginId {
        SLIDERS
    }

    fn title(&self) -> String {
        "Sliders".into()
    }

    fn requires(&self) -> Vec<PluginId> {
        vec![SHELL]
    }

    fn connect(&mut self, ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
        // A before-hook on the step: one slider's values snap to halves.
        ctx.before(STEP_CHANGE, |_, args| {
            if args["slider"] != SNAPPED {
                return Before::Go;
            }
            let to = args["to"].as_f64().unwrap_or_default();
            let mut args = args.clone();
            args["to"] = json!((to * 2.0).round() / 2.0);
            Before::Redirect(args)
        });
        let state = Rc::new(RefCell::new(State::default()));
        ctx.story(
            Story::new(StoryId::from_static("sliders"), "Sliders", move |ui, app| {
                show(ui, app, &mut state.borrow_mut())
            })
            .description("darktable-style sliders: the compact row (soft and hard range, typed expressions, fine and coarse, reset, a quad button), logarithmic, gradient tracks, range sliders, and the slider.change step.")
            .group("Components", 46),
        )?;
        Ok(())
    }
}

fn show(ui: &mut Ui, app: &AppHandle, s: &mut State) {
    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.set_max_width(560.0);
            ui.label("Drag (Ctrl: finer, Shift: coarser; Escape puts the value back), arrows and Page Up or Down when focused, the wheel when focused or with Alt, Enter to type a value or an expression (x is the current value), double click to reset.");
            section(ui, "The compact row");
            ui.add(
                Slider::new(&mut s.exposure, "Exposure", -3.0..=4.0)
                    .hard(-18.0..=18.0)
                    .step(0.01)
                    .unit("EV")
                    .signed()
                    .default(0.0),
            );
            ui.add(
                Slider::new(&mut s.contrast, "Contrast", -1.0..=1.0)
                    .step(0.01)
                    .display_scale(100.0)
                    .precision(0)
                    .unit("%")
                    .signed()
                    .default(0.0)
                    .quad_toggle(Icon::Visible, "Contrast on", &mut s.contrast_on),
            );
            let black = Slider::new(&mut s.black, "Black level", -0.1..=0.1)
                .step(0.001)
                .default(0.0)
                .quad(Icon::Picker, "Pick the black level")
                .show(ui);
            if black.quad_clicked() {
                s.picks += 1;
            }
            ui.label(format!("Picker clicked {} times", s.picks));
            section(ui, "Steps: slider.change");
            ui.label("Each finished change runs the slider.change step; the page's plugin snaps the second slider to halves with a before-hook.");
            let a = Slider::new(&mut s.stepped, "Stepped", 0.0..=10.0)
                .step(0.1)
                .default(5.0)
                .on_change(sliders::hook(app, "gallery.stepped"))
                .show(ui);
            let b = Slider::new(&mut s.snapped, "Snapped to halves", 0.0..=10.0)
                .step(0.1)
                .default(5.0)
                .on_change(sliders::hook(app, SNAPPED))
                .show(ui);
            for c in a.changes.iter().chain(&b.changes).filter(|c| c.done) {
                s.last = format!(
                    "{} from {:.1} to {:.1} by {}",
                    c.thumb.name(),
                    c.from,
                    c.to,
                    c.cause.name()
                );
            }
            ui.label(format!("Last change: {}", s.last));
            section(ui, "Logarithmic");
            ui.add(
                Slider::new(&mut s.radius, "Radius", 0.5..=500.0)
                    .logarithmic()
                    .hard(0.1..=2000.0)
                    .step(0.1)
                    .unit("px")
                    .default(8.0),
            );
            section(ui, "Gradient tracks");
            ui.add(
                Slider::new(&mut s.hue, "Hue", 0.0..=360.0)
                    .step(1.0)
                    .unit("°")
                    .gradient(Gradient::hue())
                    .default(0.0),
            );
            ui.add(
                Slider::new(&mut s.temperature, "Temperature", 2000.0..=12000.0)
                    .hard(1000.0..=40000.0)
                    .step(10.0)
                    .integer()
                    .unit("K")
                    .gradient(Gradient::temperature(2000.0, 12000.0))
                    .default(6500.0),
            );
            section(ui, "Range");
            ui.add(
                RangeSlider::new(&mut s.iso.0, &mut s.iso.1, "ISO", 50.0..=25600.0)
                    .logarithmic()
                    .integer()
                    .default(100.0, 6400.0),
            );
            ui.add(
                RangeSlider::new(&mut s.focal.0, &mut s.focal.1, "Focal length", 10.0..=600.0)
                    .step(1.0)
                    .unit("mm")
                    .default(24.0, 70.0),
            );
        });
}
