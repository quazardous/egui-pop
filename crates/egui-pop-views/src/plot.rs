//! Charts: time series, scatters and bars, on `egui_plot`.
//!
//! It wraps egui_plot (MIT OR Apache-2.0), which draws the axes, the
//! grid, the legend and the series and takes the zoom, the pan and the
//! box zoom, and adds what an egui-pop view has:
//!
//! - the kit's look: the `plot` component's background, frame, axis
//!   text and grid, the `plot-cursor` component's time cursor (its line,
//!   its head, its colour under the pointer), and each series in its
//!   slot of the categorical palette (`palette.category.*`,
//!   [`DataPalette`]), or in the slot an entity's path hashes to
//!   ([`Series::entity`]);
//! - an accessible summary: the plot is one node, named by its label,
//!   described by its series (their names, kinds, point counts and
//!   ranges) and its time cursor;
//! - the point under the pointer, nearest within [`HOVER_RADIUS`], in
//!   the output and marked on the plot;
//! - time: an x axis in a timeline's [`TimeScale`] (seconds, timecode,
//!   bars and beats, [`Plot::time_scale`]), a time cursor the application
//!   gives ([`Plot::time_cursor`]) or a timeline's playhead, dragged both
//!   ways ([`Plot::timeline`]), its range highlighted, and every series'
//!   value at the pointer in a tooltip ([`Plot::values_tooltip`]);
//! - stepped lines ([`Kind::Steps`]) and error bands ([`Band`]);
//! - the `plots` collector ([`PlotPlugin`]): every plot drawn with its
//!   series, ranges and hovered point, for agents.
//!
//! ```
//! use egui_pop_views::plot::{Plot, Series};
//!
//! fn ui(ui: &mut egui::Ui, now: f64) {
//!     let series = [
//!         Series::line("Temperature", (0..100).map(|i| [i as f64, (i as f64 / 10.0).sin()])),
//!         Series::points("Samples", [[10.0, 0.5], [40.0, -0.3]]),
//!     ];
//!     let out = Plot::new("sensors", &series)
//!         .label("Sensors")
//!         .time_cursor(Some(now))
//!         .height(200.0)
//!         .show(ui);
//!     if let Some(h) = out.hovered {
//!         // `h.series`, `h.index`, `h.x`, `h.y`.
//!     }
//! }
//! ```
//!
//! Over a timeline's time:
//!
//! ```
//! use egui_pop_views::plot::{Band, Plot, Series};
//! use egui_pop_views::timeline::{TimeScale, TimelineState};
//!
//! fn ui(ui: &mut egui::Ui, timeline: &mut TimelineState) {
//!     let series = [
//!         Series::line("speed", (0..=40).map(|i| [i as f64 / 2.0, (i as f64 / 6.0).sin()]))
//!             .entity("/robot/speed"),
//!         Series::steps("gear", [[0.0, 1.0], [6.0, 2.0], [12.0, 3.0]]),
//!     ];
//!     let bands = [Band::new(0, (0..=40).map(|i| {
//!         let y = (i as f64 / 6.0).sin();
//!         [i as f64 / 2.0, y - 0.2, y + 0.2]
//!     }))];
//!     // The playhead is the cursor; dragging the cursor moves the playhead.
//!     Plot::new("robot", &series).bands(&bands).timeline(timeline).show(ui);
//! }
//! ```

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use egui::{
    Color32, Context, CursorIcon, Id, Order, Pos2, Rect, Response, RichText, Role, Sense, Shape,
    Stroke, TextStyle, Ui, pos2, vec2,
};
use egui_plot::{
    AxisHints, Bar, BarChart, Corner, FilledArea, GridInput, GridMark, Legend, Line, PlotPoint,
    PlotTransform, Points, Span,
};
use egui_pop_api::{Ctx, Plugin, PluginError};
use egui_pop_core::debug::FnCollector;
use egui_pop_core::kit::{Kit, KitCx, Part, State};
use egui_pop_core::lock;
use egui_pop_core::state::Slot;
use egui_pop_core::strings::{tr, tr_args};
use egui_pop_core::theme::{DataPalette, Tokens};
use egui_pop_core::tokens::Value;
use egui_pop_model::ids::PluginId;
use serde::Serialize;
use serde_json::{Value as Json, json};

use crate::strings as s;
use crate::timeline::{TimeRange, TimeScale, TimeView, TimelineState, TimelineTokens, ruler};

/// How far from a point, in points on screen, the pointer still hovers
/// it.
pub const HOVER_RADIUS: f32 = 16.0;

/// How a series is drawn.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
#[non_exhaustive]
pub enum Kind {
    /// A line through its points, in order.
    Line,
    /// Its points alone (a scatter).
    Points,
    /// A bar from 0 to each point's y, at its x.
    Bars,
    /// A stepped line: each value held until the next point's x (a state,
    /// a gear, a count that changes at instants).
    Steps,
}

impl Kind {
    fn word(self, ctx: &Context) -> String {
        tr(
            ctx,
            match self {
                Kind::Line => &s::PLOT_LINE,
                Kind::Points => &s::PLOT_POINTS,
                Kind::Bars => &s::PLOT_BARS,
                Kind::Steps => &s::PLOT_STEPS,
            },
        )
    }
}

/// A series of a plot: a name, a kind and its points.
#[derive(Clone, Debug, PartialEq)]
pub struct Series {
    /// Its name in the legend, the accessible summary and the collector.
    pub name: String,
    pub kind: Kind,
    /// Its points, `[x, y]`.
    pub points: Vec<[f64; 2]>,
    /// Its slot of the categorical palette (default: its place among the
    /// plot's series). Give an entity the same slot in every plot, so its
    /// colour follows it.
    pub slot: Option<usize>,
    /// A colour of its own, over the palette's.
    pub color: Option<Color32>,
}

impl Series {
    pub fn new(
        name: impl Into<String>,
        kind: Kind,
        points: impl IntoIterator<Item = [f64; 2]>,
    ) -> Self {
        Self {
            name: name.into(),
            kind,
            points: points.into_iter().collect(),
            slot: None,
            color: None,
        }
    }

    /// A line through `points`.
    pub fn line(name: impl Into<String>, points: impl IntoIterator<Item = [f64; 2]>) -> Self {
        Self::new(name, Kind::Line, points)
    }

    /// `points` alone, a scatter.
    pub fn points(name: impl Into<String>, points: impl IntoIterator<Item = [f64; 2]>) -> Self {
        Self::new(name, Kind::Points, points)
    }

    /// Bars of `[x, height]`.
    pub fn bars(name: impl Into<String>, points: impl IntoIterator<Item = [f64; 2]>) -> Self {
        Self::new(name, Kind::Bars, points)
    }

    /// A stepped line through `points`, sorted by x: each y held until
    /// the next x.
    pub fn steps(name: impl Into<String>, points: impl IntoIterator<Item = [f64; 2]>) -> Self {
        Self::new(name, Kind::Steps, points)
    }

    /// Its slot of the categorical palette.
    pub fn slot(mut self, slot: usize) -> Self {
        self.slot = Some(slot);
        self
    }

    /// The slot an entity's path hashes to ([`entity_slot`]): the entity
    /// keeps its colour in every plot and every run, whatever else is
    /// shown.
    pub fn entity(self, path: &str) -> Self {
        self.slot(entity_slot(path))
    }

    /// A colour of its own.
    pub fn color(mut self, color: Color32) -> Self {
        self.color = Some(color);
        self
    }

    /// Its x and y ranges, `None` without a finite point.
    pub fn ranges(&self) -> Option<([f64; 2], [f64; 2])> {
        let mut it = self
            .points
            .iter()
            .filter(|p| p[0].is_finite() && p[1].is_finite());
        let first = it.next()?;
        let mut x = [first[0]; 2];
        let mut y = [first[1]; 2];
        for p in it {
            x = [x[0].min(p[0]), x[1].max(p[0])];
            y = [y[0].min(p[1]), y[1].max(p[1])];
        }
        Some((x, y))
    }
}

/// The categorical palette's slot for an entity, from its path: a stable
/// hash (FNV-1a), the same on every platform and in every run. The idea
/// of colouring an entity by a hash of its path is Rerun's automatic
/// colour; here it lands on the palette's slots rather than on a hue, so
/// a kit's palette still holds. Two paths may share a slot: give one a
/// slot of its own ([`Series::slot`]) when they meet in a plot.
pub fn entity_slot(path: &str) -> usize {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in path.bytes() {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    // A multiple of every palette length up to 12 (their least common
    // multiple), small enough for a 32-bit `usize`: the slot modulo the
    // palette's length is the hash's.
    (h % 27_720) as usize
}

/// An error band: a fill between a minimum and a maximum along x, in the
/// colour of a series of the plot, under the lines.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct Band {
    /// The series whose colour it takes, by its index among the plot's
    /// series.
    pub series: usize,
    /// `[x, min, max]`, sorted by x.
    pub points: Vec<[f64; 3]>,
}

impl Band {
    /// A band in the colour of series `series`, through `[x, min, max]`.
    pub fn new(series: usize, points: impl IntoIterator<Item = [f64; 3]>) -> Self {
        Self {
            series,
            points: points.into_iter().collect(),
        }
    }
}

/// The point under the pointer.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Hovered {
    /// The series' index in the slice given to [`Plot::new`].
    pub series: usize,
    /// Its name.
    pub name: String,
    /// The point's index in the series.
    pub index: usize,
    pub x: f64,
    pub y: f64,
}

/// A series' value at an x.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[non_exhaustive]
pub struct ValueAt {
    /// The series' index in the slice given to [`Plot::new`].
    pub series: usize,
    pub name: String,
    pub y: f64,
}

/// Every line's and stepped line's value at an x: a line's between its
/// two points around it, a stepped line's the last point's at or before
/// it. Scatters and bars have none.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[non_exhaustive]
pub struct ValuesAt {
    pub x: f64,
    /// `x` as the time scale writes it, when the plot has one.
    pub text: Option<String>,
    pub values: Vec<ValueAt>,
}

/// What [`Plot::show`] returns.
#[derive(Debug)]
#[non_exhaustive]
pub struct PlotOutput {
    /// The plot area, named and described for screen readers.
    pub response: Response,
    /// The point under the pointer, if one is within [`HOVER_RADIUS`].
    pub hovered: Option<Hovered>,
    /// The pointer's position in the plot's values, while over it.
    pub pointer: Option<[f64; 2]>,
    /// What the plot shows, `([x_min, y_min], [x_max, y_max])`.
    pub bounds: ([f64; 2], [f64; 2]),
    /// Where the time cursor was dragged to this pass (on the scale's
    /// units: whole frames on frames); a bound timeline's playhead is
    /// already there.
    pub cursor_dragged: Option<f64>,
    /// The time cursor is under the pointer or dragged.
    pub cursor_hovered: bool,
    /// The values at the pointer while it is over the plot, else at the
    /// time cursor.
    pub values: Option<ValuesAt>,
}

/// The time cursor's head (`plot-cursor.head`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum CursorHead {
    /// A line alone.
    None,
    /// A triangle at the top, pointing down at the time.
    Triangle,
}

/// The time cursor's look beyond the kit's box: its head and its size.
#[derive(Clone, Copy, Debug, PartialEq)]
#[non_exhaustive]
pub struct CursorLook {
    pub head: CursorHead,
    /// The head's width, in points (`plot-cursor.head-size`).
    pub head_size: f32,
}

impl CursorLook {
    /// The look the theme installed in `ctx` gives.
    pub fn of(ctx: &Context) -> CursorLook {
        let t = Tokens::of(ctx);
        let head = match t.resolved().value("plot-cursor.head") {
            Some(Value::Other(Json::String(s))) if s == "triangle" => CursorHead::Triangle,
            _ => CursorHead::None,
        };
        CursorLook {
            head,
            head_size: t
                .resolved()
                .dimension("plot-cursor.head-size")
                .unwrap_or(10.0)
                .max(0.0),
        }
    }
}

/// The plot's grid (`plot.grid-color`, `plot.grid-fade`).
#[derive(Clone, Copy, Debug, PartialEq)]
#[non_exhaustive]
pub struct GridLook {
    /// The lines at their strongest.
    pub color: Color32,
    /// How much fainter denser lines are, 0 to 1.
    pub fade: f32,
}

impl GridLook {
    /// The grid the theme installed in `ctx` gives; the plot's axis text
    /// colour and egui_plot's fade without one.
    pub fn of(ctx: &Context) -> GridLook {
        let t = Tokens::of(ctx);
        GridLook {
            color: t
                .color("plot.grid-color")
                .unwrap_or_else(|| Kit::of(ctx).plot.style(State::REST).text),
            fade: t
                .resolved()
                .dimension("plot.grid-fade")
                .unwrap_or(0.5)
                .clamp(0.0, 1.0),
        }
    }
}

/// A plot widget.
#[derive(Debug)]
pub struct Plot<'a> {
    id: String,
    series: &'a [Series],
    label: Option<String>,
    height: Option<f32>,
    time_cursor: Option<f64>,
    x_label: Option<String>,
    y_label: Option<String>,
    legend: Option<bool>,
    link_x: Option<String>,
    link_cursor: Option<String>,
    interactive: bool,
    bar_width: f64,
    scale: Option<TimeScale>,
    timeline: Option<&'a mut TimelineState>,
    drag_cursor: bool,
    range: Option<TimeRange>,
    bands: &'a [Band],
    values_tooltip: Option<bool>,
    emphasis: Option<usize>,
}

impl<'a> Plot<'a> {
    /// A plot named `id` (in the collector and egui's memory) showing
    /// `series`.
    pub fn new(id: impl Into<String>, series: &'a [Series]) -> Self {
        Self {
            id: id.into(),
            series,
            label: None,
            height: None,
            time_cursor: None,
            x_label: None,
            y_label: None,
            legend: None,
            link_x: None,
            link_cursor: None,
            interactive: true,
            bar_width: 0.8,
            scale: None,
            timeline: None,
            drag_cursor: false,
            range: None,
            bands: &[],
            values_tooltip: None,
            emphasis: None,
        }
    }

    /// Its accessible name (default: "Chart").
    pub fn label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }

    /// Its height (default: what is available, or egui_plot's aspect).
    pub fn height(mut self, height: f32) -> Self {
        self.height = Some(height);
        self
    }

    /// A vertical line at `time` (an x value), the time a timeline
    /// shows; `None`, none. The application passes it every pass.
    pub fn time_cursor(mut self, time: Option<f64>) -> Self {
        self.time_cursor = time;
        self
    }

    /// The time cursor follows the pointer when dragged; where it went is
    /// [`PlotOutput::cursor_dragged`] (default: off; on with a timeline).
    pub fn drag_cursor(mut self, on: bool) -> Self {
        self.drag_cursor = on;
        self
    }

    /// The x axis in a timeline's scale: x values are seconds, the ticks
    /// and labels the timeline ruler's (`1:05`, `00:00:02:10`, `17.2`).
    pub fn time_scale(mut self, scale: TimeScale) -> Self {
        self.scale = Some(scale);
        self
    }

    /// Follows a timeline: its scale on the x axis, its playhead as the
    /// time cursor, its range highlighted; dragging the cursor moves the
    /// playhead (on the scale's units, inside its bounds). Draw the
    /// timeline in the same pass, before or after: each shows what the
    /// other did.
    pub fn timeline(mut self, state: &'a mut TimelineState) -> Self {
        self.timeline = Some(state);
        self
    }

    /// A span of x highlighted under the series (a timeline's range); a
    /// timeline given by [`Plot::timeline`] gives its own.
    pub fn highlight_range(mut self, range: Option<TimeRange>) -> Self {
        self.range = range;
        self
    }

    /// Error bands under the lines ([`Band`]).
    pub fn bands(mut self, bands: &'a [Band]) -> Self {
        self.bands = bands;
        self
    }

    /// While the pointer is over the plot, a tooltip lists every line's
    /// value at its x, marked on the lines (default: on with a time scale
    /// or a timeline).
    pub fn values_tooltip(mut self, on: bool) -> Self {
        self.values_tooltip = Some(on);
        self
    }

    /// Draws one series thicker, the one an application has selected or
    /// hovered elsewhere.
    pub fn emphasize(mut self, series: Option<usize>) -> Self {
        self.emphasis = series;
        self
    }

    /// The x axis' title.
    pub fn x_label(mut self, label: impl Into<String>) -> Self {
        self.x_label = Some(label.into());
        self
    }

    /// The y axis' title.
    pub fn y_label(mut self, label: impl Into<String>) -> Self {
        self.y_label = Some(label.into());
        self
    }

    /// Shows the legend (default: with two series or more).
    pub fn legend(mut self, on: bool) -> Self {
        self.legend = Some(on);
        self
    }

    /// Zooms and pans the x axis together with the other plots of
    /// `group` (egui_plot's linked axes).
    pub fn link_x(mut self, group: impl Into<String>) -> Self {
        self.link_x = Some(group.into());
        self
    }

    /// Shows the pointer's x as a line in the other plots of `group`
    /// (egui_plot's linked cursors).
    pub fn link_cursor(mut self, group: impl Into<String>) -> Self {
        self.link_cursor = Some(group.into());
        self
    }

    /// Takes the zoom, the pan and the box zoom (default: on).
    pub fn interactive(mut self, on: bool) -> Self {
        self.interactive = on;
        self
    }

    /// The width a bar takes of the space between two x values, shared
    /// by the bar series side by side (default: 0.8).
    pub fn bar_width(mut self, width: f64) -> Self {
        self.bar_width = width;
        self
    }

    /// The colour series `index` is drawn in.
    pub fn color_of(palette: &DataPalette, series: &Series, index: usize) -> Color32 {
        series
            .color
            .unwrap_or_else(|| palette.series(series.slot.unwrap_or(index)))
    }

    pub fn show(self, ui: &mut Ui) -> PlotOutput {
        let Plot {
            id: plot_id,
            series,
            label,
            height,
            mut time_cursor,
            x_label,
            y_label,
            legend,
            link_x,
            link_cursor,
            interactive,
            bar_width,
            mut scale,
            mut timeline,
            mut drag_cursor,
            mut range,
            bands,
            values_tooltip,
            emphasis,
        } = self;
        let mut looping = false;
        if let Some(state) = timeline.as_deref() {
            scale = Some(state.scale);
            time_cursor = Some(state.playhead);
            range = state.range;
            looping = state.looping;
            drag_cursor = true;
        }
        let tooltip = values_tooltip.unwrap_or(scale.is_some());
        let id = Id::unique(("egui_pop_views::plot", &plot_id));
        let kit = Kit::of(ui.ctx());
        let tokens = Tokens::of(ui.ctx());
        // Last pass's hover: the area's text under the pointer.
        let hovered_before = ui.ctx().read_response(id).is_some_and(|r| r.hovered());
        let state = if hovered_before {
            State::HOVER
        } else {
            State::REST
        };
        let style = *kit.plot.style(state);
        let grid = GridLook::of(ui.ctx());
        let colors: Vec<Color32> = series
            .iter()
            .enumerate()
            .map(|(i, s)| Self::color_of(&tokens.palette, s, i))
            .collect();
        let slot = ui.painter().add(Shape::Noop);
        let legend = legend.unwrap_or(series.len() > 1);
        let mut plot = egui_plot::Plot::new(plot_id.as_str())
            .id(id)
            .show_background(false)
            .grid_color(grid.color)
            .grid_fade(grid.fade)
            .cursor_color(style.text)
            .allow_zoom(interactive)
            .allow_drag(interactive)
            .allow_scroll(interactive)
            .allow_boxed_zoom(interactive)
            .allow_double_click_reset(interactive);
        if let Some(h) = height {
            plot = plot.height(h);
        }
        if legend {
            plot = plot.legend(
                Legend::default()
                    .position(Corner::RightTop)
                    .follow_insertion_order(true),
            );
        }
        // The x axis' labels this pass, for the collector.
        let x_ticks: Arc<Mutex<Vec<String>>> = Arc::default();
        if let Some(sc) = scale {
            let font = TextStyle::Body.resolve(ui.style());
            let digit = ui
                .painter()
                .layout_no_wrap("0".into(), font, Color32::WHITE)
                .size()
                .x;
            let major = Arc::new(Mutex::new(f64::INFINITY));
            let (to_label, seen) = (Arc::clone(&major), Arc::clone(&x_ticks));
            plot = plot.x_grid_spacer(move |input: GridInput| {
                let (ticks, marks) = time_marks(&sc, input, digit);
                *lock(&major) = ticks.major;
                *lock(&seen) = marks.iter().filter_map(|m| m.label.clone()).collect();
                marks
                    .iter()
                    .map(|m| GridMark {
                        value: m.time,
                        step_size: if m.major { ticks.major } else { ticks.minor },
                    })
                    .collect()
            });
            let mut axis = AxisHints::new_x()
                .formatter(move |mark: GridMark, _| {
                    let major = *lock(&to_label);
                    if mark.step_size >= major * (1.0 - 1e-9) {
                        sc.label(mark.value, major)
                    } else {
                        String::new()
                    }
                })
                // The ruler already spaced the labels for their width.
                .label_spacing(1.0..=2.0);
            if let Some(l) = &x_label {
                axis = axis.label(l.clone());
            }
            plot = plot.custom_x_axes(vec![axis]);
        } else if let Some(l) = &x_label {
            plot = plot.x_axis_label(l.clone());
        }
        if let Some(l) = &y_label {
            plot = plot.y_axis_label(l.clone());
        }
        if tooltip {
            // The tooltip says the values: no coordinates beside the
            // pointer.
            plot = plot.label_formatter(|_| None);
        }
        if let Some(g) = &link_x {
            plot = plot.link_axis(Id::unique(("egui_pop_views::plot::axis", g)), [true, false]);
        }
        if let Some(g) = &link_cursor {
            plot = plot.link_cursor(
                Id::unique(("egui_pop_views::plot::cursor", g)),
                [true, false],
            );
        }
        let bar_series: Vec<usize> = (0..series.len())
            .filter(|&i| series[i].kind == Kind::Bars)
            .collect();
        let bar_w = bar_width / bar_series.len().max(1) as f64;
        let bar_offset = |i: usize| {
            let j = bar_series.iter().position(|&b| b == i).unwrap_or(0) as f64;
            (j - (bar_series.len() as f64 - 1.0) / 2.0) * bar_w
        };
        let ring = style.text;
        let tl = TimelineTokens::of(ui.ctx());
        let inner = ui
            .scope(|ui| {
                // egui_plot reads its axis text, its legend's and its
                // background from the visuals.
                let v = ui.visuals_mut();
                v.override_text_color = Some(style.text);
                v.extreme_bg_color = style.fill;
                plot.show(ui, |p| {
                    // The range first, under everything.
                    if let Some(r) = range.filter(|r| r.end > r.start) {
                        let edge = if looping {
                            tl.range_edge
                        } else {
                            tl.range_edge_idle
                        };
                        p.span(
                            Span::new("", r.start..=r.end)
                                .fill(tl.range_fill)
                                .border(Stroke::new(1.0, edge)),
                        );
                    }
                    for (b, band) in bands.iter().enumerate() {
                        let Some(color) = colors.get(band.series) else {
                            continue;
                        };
                        let pts: Vec<&[f64; 3]> = band
                            .points
                            .iter()
                            .filter(|q| q.iter().all(|v| v.is_finite()))
                            .collect();
                        let xs: Vec<f64> = pts.iter().map(|q| q[0]).collect();
                        let lo: Vec<f64> = pts.iter().map(|q| q[1].min(q[2])).collect();
                        let hi: Vec<f64> = pts.iter().map(|q| q[1].max(q[2])).collect();
                        p.add(
                            FilledArea::new("", &xs, &lo, &hi)
                                .fill_color(color.gamma_multiply(0.25))
                                .allow_hover(false)
                                .id(("band", b)),
                        );
                    }
                    for (i, s) in series.iter().enumerate() {
                        let color = colors[i];
                        let emphasized = emphasis == Some(i);
                        match s.kind {
                            Kind::Line => p.line(
                                Line::new(s.name.clone(), s.points.clone())
                                    .color(color)
                                    .width(2.0)
                                    .highlight(emphasized)
                                    .id(("series", i)),
                            ),
                            Kind::Steps => p.line(
                                // No band: egui_plot draws a dense line as
                                // one band of its columns' ranges, which
                                // joins a step's vertical edges into slopes.
                                Line::new(s.name.clone(), step_points(&s.points))
                                    .color(color)
                                    .width(2.0)
                                    .allow_band(false)
                                    .highlight(emphasized)
                                    .id(("series", i)),
                            ),
                            Kind::Points => p.points(
                                Points::new(s.name.clone(), s.points.clone())
                                    .color(color)
                                    .radius(3.0)
                                    .filled(true)
                                    .highlight(emphasized)
                                    .id(("series", i)),
                            ),
                            Kind::Bars => {
                                let off = bar_offset(i);
                                let bars = s
                                    .points
                                    .iter()
                                    .map(|q| {
                                        Bar::new(q[0] + off, q[1])
                                            .width(bar_w)
                                            .fill(color)
                                            .stroke(Stroke::NONE)
                                    })
                                    .collect();
                                p.bar_chart(
                                    BarChart::new(s.name.clone(), bars)
                                        .color(color)
                                        .highlight(emphasized)
                                        .id(("series", i)),
                                );
                            }
                        }
                    }
                    let pointer = p.pointer_coordinate().map(|q| [q.x, q.y]);
                    let hovered = p.response().hover_pos().and_then(|pos| {
                        nearest(series, p.transform(), pos, &|i| bar_offset(i), bar_w)
                    });
                    if let Some(h) = &hovered {
                        let s = &series[h.series];
                        let x = if s.kind == Kind::Bars {
                            h.x + bar_offset(h.series)
                        } else {
                            h.x
                        };
                        p.points(
                            Points::new("", vec![[x, h.y]])
                                .radius(5.0)
                                .filled(false)
                                .color(ring)
                                .allow_hover(false),
                        );
                    }
                    (hovered, pointer)
                })
            })
            .inner;
        let response = inner.response.clone();
        let transform = inner.transform;
        let (hovered, pointer) = inner.inner;
        let frame = *transform.frame();

        // The kit's box under the plot, now that its frame is known.
        kit.plot.paint_under(
            ui,
            slot,
            &Part {
                rect: frame,
                room: frame,
                state,
                label: None,
            },
            None,
        );

        // The time cursor: dragged, then drawn where it is now.
        let mut cursor_dragged = None;
        let mut cursor_hovered = false;
        if let Some(t) = time_cursor.filter(|t| t.is_finite()) {
            let look = CursorLook::of(ui.ctx());
            let mut cursor_state = State::REST;
            let mut at = t;
            if drag_cursor && frame.width() > 0.0 {
                let half = (look.head_size / 2.0).max(5.0);
                let x = transform
                    .position_from_point_x(t)
                    .clamp(frame.left(), frame.right());
                let r = ui.interact(
                    Rect::from_x_y_ranges(x - half..=x + half, frame.y_range()),
                    id.with("time_cursor"),
                    Sense::DRAG,
                );
                let text = match scale {
                    Some(sc) => sc.format(t),
                    None => number(t),
                };
                let name = tr(ui.ctx(), &s::PLOT_CURSOR);
                egui_pop_core::a11y::name(&r, Role::Slider, &name);
                ui.ctx()
                    .accesskit_node_builder(r.id, |node| node.set_value(text));
                if r.dragged()
                    && let Some(p) = r.interact_pointer_pos()
                {
                    let mut to = transform.value_from_position(p).x;
                    if let Some(sc) = scale {
                        to = sc.quantize(to);
                    }
                    if let Some(state) = &mut timeline {
                        state.set_playhead(to);
                        to = state.playhead;
                    }
                    if to != t {
                        cursor_dragged = Some(to);
                    }
                    at = to;
                }
                cursor_hovered = r.hovered() || r.dragged();
                if cursor_hovered {
                    ui.ctx().set_cursor_icon(CursorIcon::ResizeHorizontal);
                }
                cursor_state = State::REST
                    .with(State::HOVER, cursor_hovered)
                    .with(State::ACTIVE, r.dragged());
            }
            time_cursor = Some(at);
            let x = transform.position_from_point_x(at);
            if (frame.left()..=frame.right()).contains(&x) {
                // The kit's `plot-cursor`: a line across the part's
                // middle, and its head.
                let painter = ui.painter().with_clip_rect(frame);
                kit.plot_cursor.paint(
                    &KitCx {
                        painter: &painter,
                        ppp: ui.ctx().pixels_per_point(),
                    },
                    &Part {
                        rect: Rect::from_x_y_ranges(x..=x, frame.y_range()),
                        room: frame,
                        state: cursor_state,
                        label: None,
                    },
                );
                if look.head == CursorHead::Triangle && look.head_size > 0.0 {
                    let fill = kit.plot_cursor.style(cursor_state).fill;
                    let w = look.head_size / 2.0;
                    let top = frame.top();
                    painter.add(Shape::convex_polygon(
                        vec![
                            pos2(x - w, top),
                            pos2(x + w, top),
                            pos2(x, top + look.head_size * 0.7),
                        ],
                        fill,
                        Stroke::NONE,
                    ));
                }
            }
        }

        // The values at the pointer, else at the cursor.
        let over = ui
            .ctx()
            .pointer_hover_pos()
            .filter(|p| frame.contains(*p) && (response.hovered() || cursor_hovered));
        let values_x = over
            .map(|p| transform.value_from_position(p).x)
            .or(time_cursor.filter(|t| t.is_finite()));
        let values = values_x.map(|x| ValuesAt {
            x,
            text: scale.map(|sc| sc.format(x)),
            values: values_at(series, x),
        });
        if tooltip
            && let (Some(pos), Some(v)) = (over, &values)
            && !v.values.is_empty()
        {
            let painter = ui.painter().with_clip_rect(frame);
            let x = transform.position_from_point_x(v.x);
            for value in &v.values {
                let y = transform.position_from_point_y(value.y);
                painter.circle(
                    pos2(x, y),
                    3.5,
                    colors[value.series],
                    Stroke::new(1.0, style.fill),
                );
            }
            values_tooltip_ui(ui, id, pos, v, &colors);
        }

        let label = label.unwrap_or_else(|| tr(ui.ctx(), &s::PLOT));
        let cursor_text = time_cursor.filter(|t| t.is_finite()).map(|t| match scale {
            Some(sc) => sc.format(t),
            None => number(t),
        });
        let summary = summary_with(ui.ctx(), series, cursor_text);
        egui_pop_core::a11y::name(&response, Role::Figure, &label);
        let description = summary.clone();
        ui.ctx()
            .accesskit_node_builder(response.id, |node| node.set_description(description));

        let b = transform.bounds();
        let bounds = (b.min(), b.max());
        let x_ticks = std::mem::take(&mut *lock(&x_ticks));
        Plots::of(ui.ctx()).record(
            &plot_id,
            Record {
                label,
                summary,
                series: series
                    .iter()
                    .enumerate()
                    .map(|(i, s)| {
                        let ranges = s.ranges();
                        json!({
                            "name": s.name,
                            "kind": s.kind,
                            "points": s.points.len(),
                            "x": ranges.map(|r| r.0),
                            "y": ranges.map(|r| r.1),
                            "color": format!("#{:02x}{:02x}{:02x}", colors[i].r(), colors[i].g(), colors[i].b()),
                        })
                    })
                    .collect(),
                bounds,
                hovered: hovered.clone(),
                time_cursor,
                scale: scale.map(|sc| sc.kind()),
                x_ticks,
                range: range.map(|r| [r.start, r.end]),
                values: values.clone(),
                pass: ui.ctx().cumulative_pass_nr(),
            },
        );
        PlotOutput {
            response,
            hovered,
            pointer,
            bounds,
            cursor_dragged,
            cursor_hovered,
            values,
        }
    }
}

/// The tooltip listing the values, beside the pointer.
fn values_tooltip_ui(ui: &Ui, id: Id, pos: Pos2, v: &ValuesAt, colors: &[Color32]) {
    egui::Area::new(id.with("values"))
        .order(Order::Tooltip)
        .fixed_pos(pos + vec2(16.0, 16.0))
        .interactable(false)
        .constrain(true)
        .show(ui.ctx(), |ui| {
            egui::Frame::popup(ui.style()).show(ui, |ui| {
                let x = v.text.clone().unwrap_or_else(|| number(v.x));
                ui.label(RichText::new(x).monospace().strong());
                for value in &v.values {
                    ui.horizontal(|ui| {
                        let (r, _) = ui.allocate_exact_size(vec2(10.0, 10.0), Sense::hover());
                        ui.painter().rect_filled(r, 2.0, colors[value.series]);
                        ui.label(format!("{}: {}", value.name, number(value.y)));
                    });
                }
            });
        });
}

/// A stepped line's points: each y held until the next x.
fn step_points(points: &[[f64; 2]]) -> Vec<[f64; 2]> {
    let mut out = Vec::with_capacity(points.len() * 2);
    for (i, p) in points.iter().enumerate() {
        if i > 0 {
            out.push([p[0], points[i - 1][1]]);
        }
        out.push(*p);
    }
    out
}

/// Each line's and stepped line's value at `x` (their points sorted by
/// x); none outside a series' x range.
pub fn values_at(series: &[Series], x: f64) -> Vec<ValueAt> {
    if !x.is_finite() {
        return Vec::new();
    }
    series
        .iter()
        .enumerate()
        .filter_map(|(i, s)| {
            let pts: Vec<&[f64; 2]> = s
                .points
                .iter()
                .filter(|p| p[0].is_finite() && p[1].is_finite())
                .collect();
            let first = pts.first()?;
            let last = pts.last()?;
            if x < first[0] || x > last[0] {
                return None;
            }
            // The first point past x.
            let k = pts.partition_point(|p| p[0] <= x);
            let y = match s.kind {
                Kind::Line => {
                    if k >= pts.len() {
                        last[1]
                    } else {
                        let (a, b) = (pts[k - 1], pts[k]);
                        let f = (x - a[0]) / (b[0] - a[0]);
                        a[1] + (b[1] - a[1]) * f
                    }
                }
                Kind::Steps => pts[k - 1][1],
                _ => return None,
            };
            Some(ValueAt {
                series: i,
                name: s.name.clone(),
                y,
            })
        })
        .collect()
}

/// The time axis' ticks for egui_plot's grid: the timeline ruler's for
/// the span in view and the plot's width (from the grid's input: its
/// step is egui_plot's least grid spacing, 8 points), labels measured as
/// `digit` points a character.
fn time_marks(scale: &TimeScale, input: GridInput, digit: f32) -> (ruler::Ticks, Vec<ruler::Mark>) {
    // egui_plot's `grid_spacing` minimum, which `base_step_size` is.
    const LEAST_SPACING: f64 = 8.0;
    let (a, b) = input.bounds;
    let span = b - a;
    let per_point = input.base_step_size / LEAST_SPACING;
    if !(span.is_finite() && span > 0.0 && per_point.is_finite() && per_point > 0.0) {
        let none = ruler::Ticks {
            major: f64::INFINITY,
            minor: f64::INFINITY,
        };
        return (none, Vec::new());
    }
    let width = (span / per_point) as f32;
    let view = TimeView::new(a, span);
    let measure = |text: &str| text.chars().count() as f32 * digit;
    let ticks = ruler::ticks(scale, &view, width, &measure);
    let marks = ruler::marks(scale, &view, width, ticks);
    (ticks, marks)
}

/// The point nearest `pos` on screen, within [`HOVER_RADIUS`]; a bar
/// is hovered anywhere over it.
fn nearest(
    series: &[Series],
    transform: &PlotTransform,
    pos: Pos2,
    bar_offset: &dyn Fn(usize) -> f64,
    bar_w: f64,
) -> Option<Hovered> {
    let mut best: Option<(f32, usize, usize)> = None;
    for (i, s) in series.iter().enumerate() {
        for (j, q) in s.points.iter().enumerate() {
            if !(q[0].is_finite() && q[1].is_finite()) {
                continue;
            }
            let d = if s.kind == Kind::Bars {
                let x = q[0] + bar_offset(i);
                let a = transform.position_from_point(&PlotPoint::new(x - bar_w / 2.0, 0.0));
                let b = transform.position_from_point(&PlotPoint::new(x + bar_w / 2.0, q[1]));
                if Rect::from_two_pos(a, b).contains(pos) {
                    0.0
                } else {
                    continue;
                }
            } else {
                let p = transform.position_from_point(&PlotPoint::new(q[0], q[1]));
                let d = p.distance(pos);
                if d > HOVER_RADIUS {
                    continue;
                }
                d
            };
            if best.is_none_or(|b| d < b.0) {
                best = Some((d, i, j));
            }
        }
    }
    best.map(|(_, i, j)| Hovered {
        series: i,
        name: series[i].name.clone(),
        index: j,
        x: series[i].points[j][0],
        y: series[i].points[j][1],
    })
}

/// A number as the summary says it: up to four significant decimals.
fn number(v: f64) -> String {
    if v == 0.0 || !v.is_finite() {
        return format!("{v}");
    }
    let decimals = (3 - v.abs().log10().floor() as i32).clamp(0, 6) as usize;
    let s = format!("{v:.decimals$}");
    if s.contains('.') {
        s.trim_end_matches('0').trim_end_matches('.').to_string()
    } else {
        s
    }
}

/// The accessible description: each series, then the time cursor.
pub fn summary(ctx: &Context, series: &[Series], time_cursor: Option<f64>) -> String {
    summary_with(
        ctx,
        series,
        time_cursor.filter(|t| t.is_finite()).map(number),
    )
}

/// The description with the time cursor already written (in a time
/// scale's text, or as a number).
fn summary_with(ctx: &Context, series: &[Series], time_cursor: Option<String>) -> String {
    let mut parts: Vec<String> = series
        .iter()
        .map(|s| {
            let kind = s.kind.word(ctx);
            match s.ranges() {
                Some((x, y)) => tr_args(
                    ctx,
                    &s::PLOT_SERIES,
                    &[
                        ("name", &s.name),
                        ("kind", &kind),
                        ("count", &s.points.len().to_string()),
                        ("x_min", &number(x[0])),
                        ("x_max", &number(x[1])),
                        ("y_min", &number(y[0])),
                        ("y_max", &number(y[1])),
                    ],
                ),
                None => tr_args(
                    ctx,
                    &s::PLOT_SERIES_EMPTY,
                    &[("name", &s.name), ("kind", &kind)],
                ),
            }
        })
        .collect();
    if let Some(t) = time_cursor {
        parts.push(tr_args(ctx, &s::PLOT_TIME_CURSOR, &[("time", &t)]));
    }
    parts.join(". ")
}

/// A plot as last drawn.
#[derive(Clone, Debug)]
struct Record {
    label: String,
    summary: String,
    series: Vec<Json>,
    bounds: ([f64; 2], [f64; 2]),
    hovered: Option<Hovered>,
    time_cursor: Option<f64>,
    scale: Option<&'static str>,
    x_ticks: Vec<String>,
    range: Option<[f64; 2]>,
    values: Option<ValuesAt>,
    pass: u64,
}

#[derive(Clone, Debug, Default)]
struct Shared(Arc<Mutex<BTreeMap<String, Record>>>);

const SLOT: Slot<Shared> = Slot::new("views.plot");

/// The plots drawn in a context, by id: what the `plots` collector
/// shows.
#[derive(Clone, Debug)]
pub struct Plots {
    ctx: Context,
    records: Arc<Mutex<BTreeMap<String, Record>>>,
}

impl Plots {
    /// The context's plots.
    pub fn of(ctx: &Context) -> Self {
        Self {
            ctx: ctx.clone(),
            records: SLOT.get_or_insert_with(ctx, Shared::default).0,
        }
    }

    fn record(&self, id: &str, record: Record) {
        lock(&self.records).insert(id.to_string(), record);
    }

    /// The ids of the plots drawn in the last two passes.
    pub fn ids(&self) -> Vec<String> {
        let now = self.ctx.cumulative_pass_nr();
        lock(&self.records)
            .iter()
            .filter(|(_, r)| r.pass + 1 >= now)
            .map(|(id, _)| id.clone())
            .collect()
    }

    /// What a plot showed when last drawn: its label, its summary, its
    /// series (name, kind, point count, x and y ranges, colour), the
    /// bounds in view, the hovered point, the time cursor, the time
    /// scale and the x axis' labels, the highlighted range and the values
    /// at the pointer or the cursor.
    pub fn state(&self, id: &str) -> Option<Json> {
        lock(&self.records).get(id).map(|r| {
            json!({
                "label": r.label,
                "summary": r.summary,
                "series": r.series,
                "bounds": { "min": r.bounds.0, "max": r.bounds.1 },
                "hovered": r.hovered,
                "time_cursor": r.time_cursor,
                "scale": r.scale,
                "x_ticks": r.x_ticks,
                "range": r.range,
                "values": r.values,
            })
        })
    }

    /// What the `plots` collector shows: every plot drawn in the last two
    /// passes, by id.
    pub fn describe(&self) -> Json {
        let ids = self.ids();
        let plots: serde_json::Map<String, Json> = ids
            .into_iter()
            .filter_map(|id| self.state(&id).map(|s| (id, s)))
            .collect();
        json!({ "plots": plots })
    }
}

/// The plugin's id.
pub const PLUGIN: PluginId = PluginId::from_static("egui_pop.plot");

/// Declares the `plots` collector. Connect it when an application's
/// plots should be read by agents; the widgets draw without it.
#[derive(Debug, Default)]
pub struct PlotPlugin;

impl Plugin for PlotPlugin {
    fn id(&self) -> PluginId {
        PLUGIN
    }

    fn title(&self) -> String {
        "Plots".into()
    }

    fn connect(&mut self, ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
        let egui = ctx.app().egui();
        ctx.collector(
            FnCollector::new(
                "plots",
                "The plots drawn, by id: their label and accessible summary, their series (name, kind, point count, x and y ranges, colour), the bounds in view, the point under the pointer (series, index, x, y), the time cursor, the time scale and the x axis' labels, the highlighted range, and every line's value at the pointer or the cursor.",
            )
            .snapshot(move |_| Plots::of(&egui).describe()),
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_series_knows_its_ranges() {
        let s = Series::line("a", [[1.0, 5.0], [3.0, -2.0], [f64::NAN, 9.0], [2.0, 0.0]]);
        assert_eq!(s.ranges(), Some(([1.0, 3.0], [-2.0, 5.0])));
        assert_eq!(Series::points("b", []).ranges(), None);
    }

    #[test]
    fn numbers_keep_four_significant_digits() {
        assert_eq!(number(0.0), "0");
        assert_eq!(number(12.5), "12.5");
        assert_eq!(number(1234.5678), "1235");
        assert_eq!(number(0.012_345), "0.01235");
        assert_eq!(number(-3.0), "-3");
    }

    #[test]
    fn the_summary_says_each_series_and_the_cursor() {
        let ctx = Context::default();
        let series = [
            Series::line("Speed", [[0.0, 1.0], [10.0, 3.5]]),
            Series::bars("Count", []),
        ];
        let s = summary(&ctx, &series, Some(4.0));
        assert_eq!(
            s,
            "Speed (line): 2 points, x from 0 to 10, y from 1 to 3.5. Count (bars): no points. Time cursor at 4"
        );
    }

    #[test]
    fn a_stepped_line_holds_each_value_until_the_next_point() {
        assert_eq!(
            step_points(&[[0.0, 1.0], [2.0, 3.0], [5.0, 2.0]]),
            vec![[0.0, 1.0], [2.0, 1.0], [2.0, 3.0], [5.0, 3.0], [5.0, 2.0]]
        );
        assert!(step_points(&[]).is_empty());
    }

    #[test]
    fn values_at_an_x_interpolate_lines_and_hold_steps() {
        let series = [
            Series::line("speed", [[0.0, 0.0], [10.0, 5.0]]),
            Series::steps("gear", [[0.0, 1.0], [4.0, 2.0], [8.0, 3.0]]),
            Series::points("samples", [[4.0, 9.0]]),
            Series::line("late", [[6.0, 1.0], [9.0, 1.0]]),
        ];
        let v = values_at(&series, 4.0);
        let pairs: Vec<(&str, f64)> = v.iter().map(|v| (v.name.as_str(), v.y)).collect();
        assert_eq!(pairs, vec![("speed", 2.0), ("gear", 2.0)]);
        let v = values_at(&series, 7.0);
        let pairs: Vec<(&str, f64)> = v.iter().map(|v| (v.name.as_str(), v.y)).collect();
        assert_eq!(pairs, vec![("speed", 3.5), ("gear", 2.0), ("late", 1.0)]);
        // At the last point, its value; past it, none.
        assert_eq!(values_at(&series, 10.0)[0].y, 5.0);
        assert!(values_at(&series, 11.0).is_empty());
    }

    #[test]
    fn an_entity_keeps_its_slot() {
        let a = entity_slot("/world/robot/left_wheel");
        assert_eq!(a, entity_slot("/world/robot/left_wheel"));
        // FNV-1a's offset basis for the empty path.
        assert_eq!(
            entity_slot(""),
            (0xcbf2_9ce4_8422_2325_u64 % 27_720) as usize
        );
        let palette = Tokens::default().palette;
        let s = Series::line("left", []).entity("/world/robot/left_wheel");
        assert_eq!(Plot::color_of(&palette, &s, 0), palette.series(a));
        // Spread over the slots: twenty paths take at least five colours.
        let slots: std::collections::BTreeSet<usize> = (0..20)
            .map(|i| entity_slot(&format!("/sensor/{i}")) % palette.category.len())
            .collect();
        assert!(slots.len() >= 5, "{slots:?}");
    }

    /// The labels the axis shows for `span` seconds across `width` points.
    fn labels(scale: TimeScale, from: f64, span: f64, width: f64) -> Vec<String> {
        let input = GridInput {
            bounds: (from, from + span),
            base_step_size: span / width * 8.0,
        };
        let (_, marks) = time_marks(&scale, input, 7.0);
        marks.into_iter().filter_map(|m| m.label).collect()
    }

    #[test]
    fn the_time_axis_ticks_follow_the_scale() {
        // Seconds: a minute across 600 points, a label every ten seconds.
        let l = labels(TimeScale::Seconds, 0.0, 60.0, 600.0);
        assert_eq!(l.first().map(String::as_str), Some("0:00"));
        assert!(l.contains(&"0:10".to_string()), "{l:?}");
        // Frames: timecode labels, two seconds at 25 fps.
        let l = labels(TimeScale::frames(25), 0.0, 2.0, 800.0);
        assert!(
            l.iter().all(|s| s.len() == 11 && s.starts_with("00:00:0")),
            "{l:?}"
        );
        assert!(
            l.contains(&"00:00:00:10".to_string()) && l.contains(&"00:00:02:00".to_string()),
            "{l:?}"
        );
        // Bars and beats at 120 BPM: a bar is two seconds, labelled by bar.
        let l = labels(TimeScale::beats(120.0), 0.0, 16.0, 400.0);
        assert!(
            l.contains(&"1".to_string()) && l.contains(&"2".to_string()),
            "{l:?}"
        );
        // Nothing to show: no ticks.
        assert!(labels(TimeScale::Seconds, 0.0, 0.0, 400.0).is_empty());
    }

    #[test]
    fn a_series_takes_its_slot_or_its_place() {
        let palette = Tokens::default().palette;
        let a = Series::line("a", []);
        assert_eq!(Plot::color_of(&palette, &a, 2), palette.category[2]);
        let b = Series::line("b", []).slot(7);
        assert_eq!(Plot::color_of(&palette, &b, 0), palette.category[7]);
        // Past the last slot, round again.
        assert_eq!(palette.series(12), palette.category[2]);
        let c = Series::line("c", []).color(Color32::RED);
        assert_eq!(Plot::color_of(&palette, &c, 0), Color32::RED);
    }
}
