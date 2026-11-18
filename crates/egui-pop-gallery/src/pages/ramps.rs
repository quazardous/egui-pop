//! The colour ramps page: sliders and knobs with a colour for their
//! minimum and one for their maximum, the value drawn in the colour
//! between them (blended in Oklab), on a photo editor's controls: a
//! white balance's temperature (blue to amber) and tint (green to
//! magenta), a saturation (grey to colour), an exposure (dark to light)
//! and a clipping warning (neutral, then red). Each is shown in the
//! value mode (the fill, or the knob's arc and pointer, in the colour at
//! the value) and in the track mode (the whole track in the ramp, the
//! thumb or the pointer marking the value). Colours are explicit or the
//! theme's tokens (the saturation's grey and accent, the warning's
//! muted text and danger), so a theme switch follows; a high-contrast
//! theme draws the theme's own fill.

use egui::{Color32, Ui, vec2};
use egui_pop::core::knob::Knob;
use egui_pop::core::slider::{RampColor, Slider};

use super::section;

/// The page's values: the sliders', the knobs' and the faders'.
#[derive(Debug)]
pub struct State {
    value: [f64; 5],
    track: [f64; 5],
    knobs: [f64; 4],
    faders: [f64; 4],
}

impl Default for State {
    fn default() -> Self {
        Self {
            value: [6500.0, 30.0, 1.4, 1.0, 0.85],
            track: [4200.0, -40.0, 0.5, -1.5, 0.95],
            knobs: [5000.0, 1.6, 1.6, 2.0],
            faders: [8000.0, -60.0, 0.9, 0.97],
        }
    }
}

/// Blue to amber, a white balance's temperature.
const BLUE: Color32 = Color32::from_rgb(64, 120, 255);
const AMBER: Color32 = Color32::from_rgb(255, 170, 40);
/// Green to magenta, its tint.
const GREEN: Color32 = Color32::from_rgb(40, 190, 70);
const MAGENTA: Color32 = Color32::from_rgb(220, 60, 220);
/// Dark to light, an exposure.
const DARK: Color32 = Color32::from_gray(40);
const LIGHT: Color32 = Color32::from_gray(235);

/// The width of the page's sliders.
const WIDTH: f32 = 280.0;

fn temperature<'a>(v: &'a mut f64, label: &str) -> Slider<'a> {
    Slider::new(v, label, 2000.0..=12_000.0)
        .step(50.0)
        .integer()
        .unit("K")
        .default(6500.0)
        .ramp(BLUE, AMBER)
}

fn tint<'a>(v: &'a mut f64, label: &str) -> Slider<'a> {
    Slider::new(v, label, -100.0..=100.0)
        .step(1.0)
        .integer()
        .signed()
        .default(0.0)
        .ramp(GREEN, MAGENTA)
}

fn saturation<'a>(v: &'a mut f64, label: &str) -> Slider<'a> {
    // The theme's tokens: muted grey to its accent.
    Slider::new(v, label, 0.0..=2.0)
        .step(0.01)
        .display_scale(100.0)
        .precision(0)
        .unit("%")
        .default(1.0)
        .ramp("text.muted", "accent.default")
}

fn exposure<'a>(v: &'a mut f64, label: &str) -> Slider<'a> {
    Slider::new(v, label, -3.0..=4.0)
        .hard(-18.0..=18.0)
        .step(0.01)
        .unit("EV")
        .signed()
        .default(0.0)
        .ramp(DARK, LIGHT)
}

/// Neutral until 70 %, then to the theme's danger: a clipping warning.
fn warning_stops() -> [(f32, RampColor); 3] {
    [
        (0.0, RampColor::token("text.muted")),
        (0.7, RampColor::token("text.muted")),
        (1.0, RampColor::token("danger")),
    ]
}

fn highlights<'a>(v: &'a mut f64, label: &str) -> Slider<'a> {
    Slider::new(v, label, 0.0..=1.0)
        .step(0.01)
        .display_scale(100.0)
        .precision(0)
        .unit("%")
        .default(0.0)
        .ramp_stops(&warning_stops())
}

pub fn show(ui: &mut Ui, s: &mut State) {
    ui.label("Sliders and knobs with a colour for the minimum and one for the maximum: the value is drawn in the colour between them, blended in Oklab so the middle stays clear. The number and the thumb still say the value. The track mode shows the whole ramp, the thumb marking the value. A high-contrast theme keeps the theme's own fill.");
    let size = vec2(WIDTH, ui.spacing().interact_size.y);
    // The two modes side by side, the same controls in each.
    ui.horizontal_top(|ui| {
        ui.vertical(|ui| {
            ui.set_width(WIDTH);
            section(ui, "Value colour");
            let [t, n, sat, ev, hl] = &mut s.value;
            temperature(t, "Temperature").size(size).show(ui);
            tint(n, "Tint").size(size).show(ui);
            saturation(sat, "Saturation").size(size).show(ui);
            exposure(ev, "Exposure").size(size).show(ui);
            highlights(hl, "Highlights clipped").size(size).show(ui);
        });
        ui.vertical(|ui| {
            ui.set_width(WIDTH);
            section(ui, "Track gradient");
            let [t, n, sat, ev, hl] = &mut s.track;
            temperature(t, "Temperature on its track")
                .ramp_track()
                .size(size)
                .show(ui);
            tint(n, "Tint on its track")
                .ramp_track()
                .size(size)
                .show(ui);
            saturation(sat, "Saturation on its track")
                .ramp_track()
                .size(size)
                .show(ui);
            exposure(ev, "Exposure on its track")
                .ramp_track()
                .size(size)
                .show(ui);
            highlights(hl, "Clipping on its track")
                .ramp_track()
                .size(size)
                .show(ui);
        });
    });
    ui.horizontal_top(|ui| {
        ui.vertical(|ui| {
            ui.set_width(WIDTH);
            section(ui, "Knobs");
            ui.horizontal(|ui| {
                let size = vec2(56.0, 96.0);
                let [t, sat, ev, ev2] = &mut s.knobs;
                Knob::new(t, "Warmth", 2000.0..=12_000.0)
                    .step(50.0)
                    .integer()
                    .unit("K")
                    .ramp(BLUE, AMBER)
                    .size(size)
                    .show(ui);
                Knob::new(sat, "Sat", 0.0..=2.0)
                    .step(0.01)
                    .display_scale(100.0)
                    .precision(0)
                    .unit("%")
                    .ramp("text.muted", "accent.default")
                    .size(size)
                    .show(ui);
                Knob::new(ev, "EV", -3.0..=4.0)
                    .step(0.01)
                    .signed()
                    .unit("EV")
                    .ramp(DARK, LIGHT)
                    .size(size)
                    .show(ui);
                Knob::new(ev2, "EV track", -3.0..=4.0)
                    .step(0.01)
                    .signed()
                    .unit("EV")
                    .ramp(DARK, LIGHT)
                    .ramp_track()
                    .size(size)
                    .show(ui);
            });
        });
        ui.vertical(|ui| {
            ui.set_width(WIDTH);
            section(ui, "Faders");
            ui.horizontal(|ui| {
                let size = vec2(52.0, 120.0);
                let [t, n, hl, hl2] = &mut s.faders;
                temperature(t, "Temp").vertical().size(size).show(ui);
                tint(n, "Tint fader")
                    .ramp_track()
                    .vertical()
                    .size(size)
                    .show(ui);
                highlights(hl, "Clip").vertical().size(size).show(ui);
                highlights(hl2, "Clip track")
                    .ramp_track()
                    .vertical()
                    .size(size)
                    .show(ui);
            });
        });
    });
}
