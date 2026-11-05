//! Charts: time series, scatters and bars, on `egui_plot`.
//!
//! It wraps egui_plot (MIT OR Apache-2.0), which draws the axes, the
//! grid, the legend and the series and takes the zoom, the pan and the
//! box zoom, and adds what an egui-pop view has:
//!
//! - the kit's look: the `plot` component's background, frame and axis
//!   text, the `plot-cursor` component's time cursor, and each series in
//!   its slot of the categorical palette (`palette.category.*`,
//!   [`DataPalette`]);
//! - an accessible summary: the plot is one node, named by its label,
//!   described by its series (their names, kinds, point counts and
//!   ranges) and its time cursor;
//! - the point under the pointer, nearest within [`HOVER_RADIUS`], in
//!   the output and marked on the plot;
//! - an optional time cursor, a vertical line at a time the application
//!   gives ([`Plot::time_cursor`]), to follow a timeline;
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

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use egui::{Color32, Context, Id, Pos2, Rect, Response, Role, Shape, Stroke, Ui};
use egui_plot::{Bar, BarChart, Corner, Legend, Line, PlotPoint, PlotTransform, Points};
use egui_pop_api::{Ctx, Plugin, PluginError};
use egui_pop_core::debug::FnCollector;
use egui_pop_core::kit::{Kit, KitCx, Part, State};
use egui_pop_core::lock;
use egui_pop_core::state::Slot;
use egui_pop_core::strings::{tr, tr_args};
use egui_pop_core::theme::{DataPalette, Tokens};
use egui_pop_model::ids::PluginId;
use serde::Serialize;
use serde_json::{Value as Json, json};

use crate::strings as s;

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
}

impl Kind {
    fn word(self, ctx: &Context) -> String {
        tr(
            ctx,
            match self {
                Kind::Line => &s::PLOT_LINE,
                Kind::Points => &s::PLOT_POINTS,
                Kind::Bars => &s::PLOT_BARS,
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

    /// Its slot of the categorical palette.
    pub fn slot(mut self, slot: usize) -> Self {
        self.slot = Some(slot);
        self
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
        let id = Id::unique(("egui_pop_views::plot", &self.id));
        let kit = Kit::of(ui.ctx());
        let tokens = Tokens::of(ui.ctx());
        // Last pass's hover: the area's text and grid under the pointer.
        let hovered_before = ui.ctx().read_response(id).is_some_and(|r| r.hovered());
        let state = if hovered_before {
            State::HOVER
        } else {
            State::REST
        };
        let style = *kit.plot.style(state);
        let colors: Vec<Color32> = self
            .series
            .iter()
            .enumerate()
            .map(|(i, s)| Self::color_of(&tokens.palette, s, i))
            .collect();
        let slot = ui.painter().add(Shape::Noop);
        let legend = self.legend.unwrap_or(self.series.len() > 1);
        let mut plot = egui_plot::Plot::new(self.id.as_str())
            .id(id)
            .show_background(false)
            .grid_color(style.text)
            .cursor_color(style.text)
            .allow_zoom(self.interactive)
            .allow_drag(self.interactive)
            .allow_scroll(self.interactive)
            .allow_boxed_zoom(self.interactive)
            .allow_double_click_reset(self.interactive);
        if let Some(h) = self.height {
            plot = plot.height(h);
        }
        if legend {
            plot = plot.legend(
                Legend::default()
                    .position(Corner::RightTop)
                    .follow_insertion_order(true),
            );
        }
        if let Some(l) = &self.x_label {
            plot = plot.x_axis_label(l.clone());
        }
        if let Some(l) = &self.y_label {
            plot = plot.y_axis_label(l.clone());
        }
        if let Some(g) = &self.link_x {
            plot = plot.link_axis(Id::unique(("egui_pop_views::plot::axis", g)), [true, false]);
        }
        if let Some(g) = &self.link_cursor {
            plot = plot.link_cursor(
                Id::unique(("egui_pop_views::plot::cursor", g)),
                [true, false],
            );
        }
        let bar_series: Vec<usize> = (0..self.series.len())
            .filter(|&i| self.series[i].kind == Kind::Bars)
            .collect();
        let bar_w = self.bar_width / bar_series.len().max(1) as f64;
        let bar_offset = |i: usize| {
            let j = bar_series.iter().position(|&b| b == i).unwrap_or(0) as f64;
            (j - (bar_series.len() as f64 - 1.0) / 2.0) * bar_w
        };
        let ring = style.text;
        let series = self.series;
        let inner = ui
            .scope(|ui| {
                // egui_plot reads its axis text, its legend's and its
                // background from the visuals.
                let v = ui.visuals_mut();
                v.override_text_color = Some(style.text);
                v.extreme_bg_color = style.fill;
                plot.show(ui, |p| {
                    for (i, s) in series.iter().enumerate() {
                        let color = colors[i];
                        match s.kind {
                            Kind::Line => p.line(
                                Line::new(s.name.clone(), s.points.clone())
                                    .color(color)
                                    .width(2.0)
                                    .id(("series", i)),
                            ),
                            Kind::Points => p.points(
                                Points::new(s.name.clone(), s.points.clone())
                                    .color(color)
                                    .radius(3.0)
                                    .filled(true)
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
        if let Some(t) = self.time_cursor.filter(|t| t.is_finite()) {
            let x = transform.position_from_point_x(t);
            if (frame.left()..=frame.right()).contains(&x) {
                // The kit's `plot-cursor`: a line across the part's
                // middle.
                let painter = ui.painter().with_clip_rect(frame);
                kit.plot_cursor.paint(
                    &KitCx {
                        painter: &painter,
                        ppp: ui.ctx().pixels_per_point(),
                    },
                    &Part {
                        rect: Rect::from_x_y_ranges(x..=x, frame.y_range()),
                        room: frame,
                        state: State::REST,
                        label: None,
                    },
                );
            }
        }

        let label = self.label.clone().unwrap_or_else(|| tr(ui.ctx(), &s::PLOT));
        let summary = summary(ui.ctx(), series, self.time_cursor);
        egui_pop_core::a11y::name(&response, Role::Figure, &label);
        let description = summary.clone();
        ui.ctx()
            .accesskit_node_builder(response.id, |node| node.set_description(description));

        let b = transform.bounds();
        let bounds = (b.min(), b.max());
        Plots::of(ui.ctx()).record(
            &self.id,
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
                time_cursor: self.time_cursor,
                pass: ui.ctx().cumulative_pass_nr(),
            },
        );
        PlotOutput {
            response,
            hovered,
            pointer,
            bounds,
        }
    }
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
    if let Some(t) = time_cursor.filter(|t| t.is_finite()) {
        parts.push(tr_args(ctx, &s::PLOT_TIME_CURSOR, &[("time", &number(t))]));
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
    /// bounds in view, the hovered point and the time cursor.
    pub fn state(&self, id: &str) -> Option<Json> {
        lock(&self.records).get(id).map(|r| {
            json!({
                "label": r.label,
                "summary": r.summary,
                "series": r.series,
                "bounds": { "min": r.bounds.0, "max": r.bounds.1 },
                "hovered": r.hovered,
                "time_cursor": r.time_cursor,
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
                "The plots drawn, by id: their label and accessible summary, their series (name, kind, point count, x and y ranges, colour), the bounds in view, the point under the pointer (series, index, x, y) and the time cursor.",
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
