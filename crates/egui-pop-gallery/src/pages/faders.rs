//! The faders and knobs page: the slider's other forms. A strip of
//! vertical sliders (a mixer's faders: the value at the top, the label at
//! the bottom, up is more), knobs in the three layouts their room gives
//! them, an endless knob whose value wraps and whose changes come as
//! nudges, horizontal sliders shrinking to narrow slots (the label goes to
//! the tooltip, then the unit), and a standalone section band, the fold
//! a card's "More (n)" row is drawn with.

use egui::{Ui, vec2};
use egui_pop::core::knob::{Endless, Knob, KnobDrag};
use egui_pop::core::sections;
use egui_pop::core::slider::{Cause, Gradient, Slider};

use super::section;

/// The page's values.
#[derive(Debug)]
pub struct State {
    faders: [f64; 4],
    hue: f64,
    knobs: [f64; 3],
    /// The endless knob's value, and the sum of its changes' deltas.
    rotation: f64,
    nudged: f64,
    narrow: [f64; 4],
    more_open: bool,
}

impl Default for State {
    fn default() -> Self {
        Self {
            faders: [0.67, -1.0, 0.25, 0.8],
            hue: 210.0,
            knobs: [0.67, 48.0, 30.0],
            rotation: 350.0,
            nudged: 0.0,
            narrow: [0.67, 0.67, 0.67, 0.67],
            more_open: false,
        }
    }
}

pub fn show(ui: &mut Ui, s: &mut State) {
    ui.label("The slider's gestures and history along the other axis, on a dial, and in narrow slots. Drag up for more on a fader and a knob (the Angle knob turns around); Ctrl makes a drag finer and Shift coarser; double click resets; a right click opens “Type a value” and “Reset”. The endless knob wraps past 360° and reports each change as a nudge.");
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
        // An angle turns around: the circular drag, for this knob only.
        Knob::new(&mut s.knobs[2], "Angle", -180.0..=180.0)
            .drag(KnobDrag::Circular)
            .step(1.0)
            .unit("°")
            .default(0.0)
            .size(vec2(56.0, 56.0))
            .show(ui);
        // An endless knob: no ends, ticks turning with the value; a
        // host reads the changes' deltas as nudges.
        let out = Knob::new(&mut s.rotation, "Rotation", 0.0..=360.0)
            .endless(Endless::Wrap)
            .step(1.0)
            .unit("°")
            .default(0.0)
            .size(vec2(150.0, 48.0))
            .show(ui);
        s.nudged += out
            .changes
            .iter()
            // A drag's end repeats its frames' motion.
            .filter(|c| !(c.done && c.cause == Cause::Drag))
            .map(|c| c.delta)
            .sum::<f64>();
        ui.label(format!("Nudged by {:+.0}°", s.nudged));
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
