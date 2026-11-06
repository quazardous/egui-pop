//! The plot over time page: a recording of twenty seconds drawn by a
//! timeline and a plot sharing one `TimelineState` (the playhead is the
//! plot's time cursor, dragging either moves both, the timeline's looping
//! range is highlighted on the plot), with a line in an entity's colour
//! and its error band, a target and a stepped gear; then a plot whose
//! axis counts bars and beats.

use egui::Ui;
use egui_pop::core::widgets;
use egui_pop::views::plot::{Band, Plot, Series};
use egui_pop::views::timeline::{TimeRange, TimeScale, Timeline, TimelineState};

use super::section;

/// The recording's length, in seconds.
pub const LENGTH: f64 = 20.0;

/// The page's state: the timeline both views follow.
#[derive(Debug)]
pub struct PlotTimeState {
    pub timeline: TimelineState,
    /// The values the plot last gave, as said under it.
    pub values: Option<String>,
}

impl Default for PlotTimeState {
    fn default() -> Self {
        let mut timeline = TimelineState::new(TimeScale::Seconds, LENGTH);
        timeline.playhead = 6.0;
        timeline.range = Some(TimeRange::new(9.0, 13.0));
        timeline.looping = true;
        Self {
            timeline,
            values: None,
        }
    }
}

/// The robot's speed, every tenth of a second.
fn speed(t: f64) -> f64 {
    1.2 + (t * 0.45).sin() + 0.25 * (t * 1.7).cos()
}

/// The speed, the speed asked for, and the gear, and the speed's error
/// band: the series the plot shows.
pub fn recording() -> (Vec<Series>, Vec<Band>) {
    let samples = || (0..=200).map(|i| f64::from(i) / 10.0);
    let series = vec![
        Series::line("Speed", samples().map(|t| [t, speed(t)])).entity("/robot/speed"),
        Series::steps(
            "Target",
            [
                [0.0, 1.0],
                [4.0, 2.0],
                [9.0, 1.5],
                [13.0, 0.5],
                [17.0, 1.0],
                [LENGTH, 1.0],
            ],
        )
        .entity("/robot/target"),
        Series::steps(
            "Gear",
            [
                [0.0, 0.0],
                [2.5, 1.0],
                [6.5, 2.0],
                [11.0, 1.0],
                [15.5, 2.0],
                [LENGTH, 2.0],
            ],
        )
        .entity("/robot/gearbox"),
    ];
    // The error grows with the speed.
    let band = Band::new(
        0,
        samples().map(|t| {
            let v = speed(t);
            let e = 0.08 + 0.06 * v.abs();
            [t, v - e, v + e]
        }),
    );
    (series, vec![band])
}

/// Kicks at 120 BPM over eight bars: one a beat, louder on the downbeat.
pub fn kicks() -> Vec<Series> {
    let beat = 0.5;
    vec![
        Series::steps(
            "Kick",
            (0..32).flat_map(|b| {
                let t = f64::from(b) * beat;
                let level = if b % 4 == 0 { 1.0 } else { 0.6 };
                [[t, level], [t + beat * 0.25, 0.0]]
            }),
        )
        .entity("/drums/kick"),
    ]
}

pub fn show(ui: &mut Ui, s: &mut PlotTimeState) {
    ui.label("A timeline and a plot over the same time: the playhead is the plot's time cursor. Drag the cursor on the plot, or scrub the timeline, and both follow; the timeline's looping range is shaded on the plot. Hover the plot for every line's value at the pointer.");
    section(
        ui,
        "A recording: speed, its error band, a target and the gear",
    );
    Timeline::new(&mut s.timeline)
        .id("gallery.plot-time")
        .label("Recording")
        .show(ui);
    let (series, bands) = recording();
    let out = Plot::new("gallery.plot.time", &series)
        .label("Robot over time")
        .bands(&bands)
        .timeline(&mut s.timeline)
        .height(170.0)
        .show(ui);
    if let Some(v) = out.values.filter(|v| !v.values.is_empty()) {
        let at = v.text.unwrap_or_default();
        let list: Vec<String> = v
            .values
            .iter()
            .map(|v| format!("{} {:.2}", v.name, v.y))
            .collect();
        s.values = Some(format!("{at}: {}", list.join(", ")));
    }
    ui.label(format!(
        "Playhead {}",
        s.timeline.scale.format(s.timeline.playhead)
    ));
    if let Some(v) = &s.values {
        ui.label(format!("Values at {v}"));
    }
    section(ui, "Bars and beats at 120 BPM");
    let series = kicks();
    Plot::new("gallery.plot.beats", &series)
        .label("Kicks")
        .time_scale(TimeScale::beats(120.0))
        .legend(false)
        .height(90.0)
        .show(ui);
}

pub fn playground(ui: &mut Ui, s: &mut PlotTimeState) {
    ui.label("The recording's time scale:");
    for (name, scale) in [
        ("Seconds", TimeScale::Seconds),
        ("Frames at 25 fps", TimeScale::frames(25)),
        ("Bars and beats at 120 BPM", TimeScale::beats(120.0)),
    ] {
        let mut on = s.timeline.scale == scale;
        if widgets::checkbox(ui, &mut on, name).changed() && on {
            s.timeline.scale = scale;
            s.timeline.playhead = s.timeline.clamp(s.timeline.playhead);
        }
    }
    let mut looping = s.timeline.looping;
    if widgets::checkbox(ui, &mut looping, "Loop the range").changed() {
        s.timeline.looping = looping;
    }
}
