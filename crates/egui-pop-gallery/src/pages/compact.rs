//! The compact controls page: what a dense row (a HUD's card, a
//! property sheet) is built from, each control in a slot of a given
//! size with no label beside it. Scrub fields (the slider's scrub look),
//! a text, a number and a choice in compact cells, a value's marks
//! (changed, held, moved), value bars across and upright, and the menu
//! rows of a "More" fold.

use egui::{Ui, vec2};
use egui_pop::core::compact::{ChoiceCell, MenuRow};
use egui_pop::core::indicator::{Indicator, ValueMark};
use egui_pop::core::input::Input;
use egui_pop::core::progress::Meter;
use egui_pop::core::slider::Slider;
use egui_pop::core::ui::Tone;

use super::section;

/// The blend modes of the choice cell.
const MODES: [&str; 4] = ["Normal", "Multiply", "Screen", "Overlay"];

/// The page's values.
#[derive(Debug)]
pub struct State {
    exposure: f64,
    contrast: f64,
    radius: f64,
    narrow: f64,
    name: String,
    opacity: f64,
    mode: usize,
    levels: [f64; 3],
    revealed: Vec<&'static str>,
}

impl Default for State {
    fn default() -> Self {
        Self {
            exposure: 0.7,
            contrast: 0.0,
            radius: 12.0,
            narrow: 0.7,
            name: "Sky".into(),
            opacity: 80.0,
            mode: 1,
            levels: [0.35, 0.72, 0.94],
            revealed: Vec::new(),
        }
    }
}

/// The height of a dense row: tighter than a form's.
const ROW: f32 = 22.0;

pub fn show(ui: &mut Ui, s: &mut State) {
    ui.label("Controls for slots of a given size, with no label beside them. A scrub field drags along its whole width (Ctrl finer, Shift coarser), steps with the arrows once focused, resets on a double click and takes a typed value on Enter. A choice cell's list opens with Space; while it is closed, Up and Down choose. The marks say a value changed from its default (click it to reset), is held, or was just moved.");
    section(ui, "Scrub fields");
    let size = vec2(220.0, ROW);
    Slider::new(&mut s.exposure, "Exposure", -3.0..=4.0)
        .step(0.01)
        .unit("EV")
        .signed()
        .default(0.0)
        .scrub()
        .size(size)
        .show(ui);
    Slider::new(&mut s.contrast, "Contrast", -1.0..=1.0)
        .step(0.01)
        .signed()
        .default(0.0)
        .scrub()
        .size(size)
        .show(ui);
    Slider::new(&mut s.radius, "Radius", 0.0..=100.0)
        .step(1.0)
        .integer()
        .unit("px")
        .default(12.0)
        .scrub()
        .size(size)
        .show(ui);
    // Too narrow for its label: the value alone, the label in the
    // tooltip.
    Slider::new(&mut s.narrow, "Exposure, narrow", -3.0..=4.0)
        .step(0.01)
        .unit("EV")
        .signed()
        .default(0.0)
        .scrub()
        .size(vec2(72.0, ROW))
        .show(ui);

    section(ui, "Compact cells");
    ui.horizontal(|ui| {
        Input::text(&mut s.name, "Layer name")
            .placeholder("Name")
            .cell(vec2(110.0, ROW))
            .show(ui);
        Input::number(&mut s.opacity, "Opacity")
            .unit("%")
            .precision(0)
            .cell(vec2(80.0, ROW))
            .show(ui);
        ChoiceCell::new("Blend mode", &mut s.mode, &MODES)
            .size(vec2(110.0, ROW))
            .show(ui);
    });

    section(ui, "Value marks");
    ui.horizontal(|ui| {
        let changed = s.exposure != 0.0;
        if changed
            && ui
                .add(
                    Indicator::mark(ValueMark::Changed)
                        .id("exposure.changed")
                        .interactive("Reset Exposure"),
                )
                .clicked()
        {
            s.exposure = 0.0;
        }
        ui.label("the marks of Exposure");
        ui.add(Indicator::mark(ValueMark::Held("Auto exposure".into())).id("exposure.held"));
        ui.add(Indicator::mark(ValueMark::Moved("Mask 1".into())).id("exposure.moved"));
    });

    section(ui, "Value bars");
    ui.horizontal(|ui| {
        ui.vertical(|ui| {
            for (i, name) in ["Shadows", "Midtones", "Highlights"].iter().enumerate() {
                ui.add(
                    Meter::new(s.levels[i], 0.0, 1.0)
                        .label(*name)
                        .threshold(0.9, Tone::Warning)
                        .size(vec2(180.0, 6.0)),
                );
            }
        });
        for (i, name) in ["Left", "Right"].iter().enumerate() {
            ui.add(
                Meter::new(s.levels[i + 1], 0.0, 1.0)
                    .label(format!("{name} level"))
                    .threshold(0.9, Tone::Danger)
                    .vertical()
                    .size(vec2(8.0, 72.0)),
            );
        }
    });

    section(ui, "Menu rows");
    ui.scope(|ui| {
        ui.set_max_width(240.0);
        for name in ["Contrast", "Vibrance"] {
            if ui
                .add(MenuRow::new(format!("Show {name}")).note("hidden"))
                .clicked()
            {
                s.revealed.push(name);
            }
        }
        if ui.add(MenuRow::new("Show all")).clicked() {
            s.revealed.extend(["Contrast", "Vibrance"]);
        }
    });
    if !s.revealed.is_empty() {
        ui.label(format!("Shown: {}", s.revealed.join(", ")));
    }
}
