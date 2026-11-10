//! The faders and knobs page: the slider's other forms. A strip of
//! vertical sliders (a mixer's faders: the value at the top, the label at
//! the bottom, up is more), knobs in the three layouts their room gives
//! them, horizontal sliders shrinking to narrow slots (the label goes to
//! the tooltip, then the unit), and a standalone section band, the fold
//! a card's "More (n)" row is drawn with.

use egui::{Ui, vec2};
use egui_pop::core::knob::Knob;
use egui_pop::core::sections;
use egui_pop::core::slider::{Gradient, Slider};

use super::section;

/// The page's values.
#[derive(Debug)]
pub struct State {
    faders: [f64; 4],
    hue: f64,
    knobs: [f64; 3],
    narrow: [f64; 4],
    more_open: bool,
}

impl Default for State {
    fn default() -> Self {
        Self {
            faders: [0.67, -1.0, 0.25, 0.8],
            hue: 210.0,
            knobs: [0.67, 48.0, 30.0],
            narrow: [0.67, 0.67, 0.67, 0.67],
            more_open: false,
        }
    }
}

pub fn show(ui: &mut Ui, s: &mut State) {
    ui.label("The slider's gestures and history along the other axis, on a dial, and in narrow slots. Drag up for more on a fader and a knob (from a knob's rim, drag around); Shift makes a knob's drag finer; double click resets.");
    section(ui, "Faders");
    ui.horizontal(|ui| {
        let size = vec2(52.0, 180.0);
        Slider::new(&mut s.faders[0], "Exposure", -3.0..=4.0)
            .hard(-18.0..=18.0)
            .step(0.01)
            .unit("EV")
            .signed()
            .default(0.0)
            .vertical()
            .size(size)
            .show(ui);
        Slider::new(&mut s.faders[1], "Black", -3.0..=3.0)
            .step(0.01)
            .signed()
            .default(0.0)
            .vertical()
            .size(size)
            .show(ui);
        Slider::new(&mut s.faders[2], "Highlights", 0.0..=1.0)
            .step(0.01)
            .display_scale(100.0)
            .precision(0)
            .unit("%")
            .default(0.0)
            .vertical()
            .size(size)
            .show(ui);
        Slider::new(&mut s.faders[3], "Saturation", 0.0..=1.0)
            .step(0.01)
            .default(0.5)
            .vertical()
            .size(vec2(32.0, 180.0))
            .show(ui);
        Slider::new(&mut s.hue, "Hue", 0.0..=360.0)
            .step(1.0)
            .unit("°")
            .gradient(Gradient::hue())
            .default(0.0)
            .vertical()
            .size(size)
            .show(ui);
    });
    section(ui, "Knobs");
    ui.horizontal(|ui| {
        Knob::new(&mut s.knobs[0], "Exposure", -3.0..=4.0)
            .hard(-18.0..=18.0)
            .step(0.01)
            .unit("EV")
            .signed()
            .default(0.0)
            .size(vec2(150.0, 48.0))
            .show(ui);
        Knob::new(&mut s.knobs[1], "Feather", 0.0..=200.0)
            .step(1.0)
            .integer()
            .unit("px")
            .default(0.0)
            .size(vec2(64.0, 110.0))
            .show(ui);
        Knob::new(&mut s.knobs[2], "Angle", -180.0..=180.0)
            .step(1.0)
            .unit("°")
            .default(0.0)
            .size(vec2(56.0, 56.0))
            .show(ui);
    });
    section(ui, "Narrow slots");
    for (i, width) in [240.0_f32, 110.0, 64.0, 40.0].into_iter().enumerate() {
        ui.horizontal(|ui| {
            ui.add_sized(vec2(60.0, 20.0), egui::Label::new(format!("{width} pt")));
            Slider::new(
                &mut s.narrow[i],
                &format!("Exposure at {width} pt"),
                -3.0..=4.0,
            )
            .step(0.01)
            .unit("EV")
            .signed()
            .default(0.0)
            .size(vec2(width, ui.spacing().interact_size.y))
            .show(ui);
        });
    }
    section(ui, "A fold");
    ui.scope(|ui| {
        ui.set_max_width(320.0);
        if sections::band(ui, "gallery.faders.more", "More", Some(3), s.more_open).clicked() {
            s.more_open = !s.more_open;
        }
        if s.more_open {
            ui.label("Contrast, Vibrance, Clarity");
        }
    });
}
