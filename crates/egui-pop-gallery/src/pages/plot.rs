//! The charts page: a time series with a time cursor, a scatter and bars,
//! on `egui_pop::views::plot` (egui_plot in the kit's colours and the
//! categorical palette), and the palette itself.

use egui::Ui;
use egui_pop::core::widgets;
use egui_pop::theme::Tokens;
use egui_pop::views::plot::{Plot, Series};

use super::{chip, section};

/// The page's state: the time cursor and what was last hovered.
#[derive(Debug)]
pub struct PlotState {
    /// The time the cursor stands at, in seconds.
    pub time: f64,
    /// Whether the time series shows its cursor.
    pub cursor: bool,
    /// The point last under the pointer, as said under the plots.
    pub hovered: Option<String>,
}

impl Default for PlotState {
    fn default() -> Self {
        Self {
            time: 6.0,
            cursor: true,
            hovered: None,
        }
    }
}

/// Three sensors over 20 seconds, a sample every 0.1 s.
pub fn time_series() -> Vec<Series> {
    let wave = |phase: f64, speed: f64, scale: f64| {
        (0..=200).map(move |i| {
            let t = f64::from(i) / 10.0;
            [
                t,
                scale * (t * speed + phase).sin() + 0.1 * (t * 3.1 + phase).cos(),
            ]
        })
    };
    vec![
        Series::line("Left wheel", wave(0.0, 0.6, 1.0)),
        Series::line("Right wheel", wave(1.2, 0.6, 0.9)),
        Series::line("Body pitch", wave(2.0, 0.25, 0.5)),
    ]
}

/// Two clusters of points, from a fixed sequence.
pub fn scatter() -> Vec<Series> {
    let cloud = |cx: f64, cy: f64, seed: u32| {
        (0..60u32).map(move |i| {
            let h = (i + seed).wrapping_mul(2_654_435_761);
            let a = f64::from(h % 1000) / 1000.0 * std::f64::consts::TAU;
            let r = f64::from((h >> 10) % 1000) / 1000.0;
            [cx + r * a.cos(), cy + 0.6 * r * a.sin()]
        })
    };
    vec![
        Series::points("Detections", cloud(1.0, 1.0, 0)),
        Series::points("Landmarks", cloud(2.4, 1.6, 7)),
    ]
}

/// Frames per second of two runs, by scene.
pub fn bars() -> Vec<Series> {
    vec![
        Series::bars(
            "Run A",
            [[0.0, 58.0], [1.0, 41.0], [2.0, 60.0], [3.0, 33.0]],
        ),
        Series::bars(
            "Run B",
            [[0.0, 60.0], [1.0, 52.0], [2.0, 57.0], [3.0, 45.0]],
        ),
    ]
}

pub fn show(ui: &mut Ui, s: &mut PlotState) {
    ui.label("Plots on egui_plot, in the kit's colours: each series takes its slot of the categorical palette. Drag to pan, scroll to zoom, drag with the right button to zoom on a box, double-click to reset.");
    section(ui, "Time series");
    let series = time_series();
    let out = Plot::new("gallery.plot.series", &series)
        .label("Wheel speeds")
        .x_label("seconds")
        .time_cursor(s.cursor.then_some(s.time))
        .link_cursor("gallery.plot")
        .height(130.0)
        .show(ui);
    let mut hovered = out.hovered;
    section(ui, "Scatter");
    let series = scatter();
    let out = Plot::new("gallery.plot.scatter", &series)
        .label("Detections and landmarks")
        .height(110.0)
        .show(ui);
    hovered = hovered.or(out.hovered);
    section(ui, "Bars");
    let series = bars();
    let out = Plot::new("gallery.plot.bars", &series)
        .label("Frames per second by scene")
        .y_label("fps")
        .height(160.0)
        .show(ui);
    hovered = hovered.or(out.hovered);
    if let Some(h) = hovered {
        s.hovered = Some(format!(
            "{} #{}: x {:.2}, y {:.2}",
            h.name, h.index, h.x, h.y
        ));
    }
    ui.label(match &s.hovered {
        Some(h) => format!("Last hovered: {h}"),
        None => "Hover a point or a bar.".into(),
    });
}

pub fn playground(ui: &mut Ui, s: &mut PlotState) {
    widgets::checkbox(ui, &mut s.cursor, "Time cursor");
    ui.add_enabled(
        s.cursor,
        egui::Slider::new(&mut s.time, 0.0..=20.0).text("time (s)"),
    );
    section(ui, "Categorical palette");
    let palette = Tokens::of(ui.ctx()).palette.clone();
    for (i, c) in palette.category.iter().enumerate() {
        ui.horizontal(|ui| {
            chip(ui, *c);
            ui.label(format!("palette.category.{i}"));
        });
    }
}
