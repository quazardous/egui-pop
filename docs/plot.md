# Charts

`egui_pop::views::plot` draws time series, scatters and bars. It wraps
[egui_plot](https://github.com/emilk/egui_plot) (MIT OR Apache-2.0),
which draws the axes, the grid, the legend and the series, and takes the
pan, the zoom and the box zoom; egui-pop adds its look, an accessible
summary, the point under the pointer, a time cursor and a collector for
agents.

```sh
cargo add egui-pop --features views
```

```rust
use egui_pop::egui;
use egui_pop::views::plot::{Plot, Series};

fn ui(ui: &mut egui::Ui, now: f64) {
    let series = [
        Series::line("Left wheel", (0..200).map(|i| [i as f64 / 10.0, (i as f64 / 20.0).sin()])),
        Series::points("Samples", [[2.0, 0.4], [7.5, -0.6]]),
        Series::bars("Errors", [[5.0, 0.3], [12.0, 0.8]]),
    ];
    let out = Plot::new("wheels", &series)
        .label("Wheel speeds")
        .x_label("seconds")
        .time_cursor(Some(now))
        .height(200.0)
        .show(ui);
    if let Some(h) = out.hovered {
        // The point under the pointer: `h.series`, `h.index`, `h.x`, `h.y`.
        let _ = (h.name, h.x, h.y);
    }
}
```

![the charts page](../crates/egui-pop-gallery/tests/snapshots/plot-pop-dark.png)

## Series

A `Series` has a name (the legend's, the summary's and the collector's),
a kind (`Kind::Line`, `Kind::Points`, `Kind::Bars`) and its points,
`[x, y]`. Bars stand from 0 to each y at their x; several bar series
stand side by side, sharing `Plot::bar_width` (0.8 of the space between
two x values by default).

Each series takes its slot of the categorical palette
(`palette.category.*`, [tokens](tokens.md#the-categorical-palette)): by
default its place among the plot's series, or `Series::slot(n)`. Give
an entity the same slot in every plot, so its colour follows it;
`Series::color` sets a colour of its own. The legend shows with two
series or more (`Plot::legend` to choose).

## The look

The plot area is the kit's `plot` component ([kit.md](kit.md)): its
background and frame, its axis text, grid and crosshair in `text`
(`hover` while the pointer is over it). The time cursor is the
`plot-cursor` component, a line across the area (`width` thick, in
`fill`). The default kit draws the area on `surface.canvas` in
`border.default`, the axis text muted and the cursor in the accent.

## The time cursor

`Plot::time_cursor(Option<f64>)` draws a vertical line at an x value the
application gives at every pass, the time a timeline shows; `None` draws
none. It is not dragged here: the timeline owns the time, the plot shows
it. `Plot::link_cursor(group)` shows the pointer's x in the other plots
of a group, and `Plot::link_x(group)` zooms and pans their x axes
together (egui_plot's linked cursors and axes).

## The pointer

`PlotOutput::hovered` is the point nearest the pointer within
`HOVER_RADIUS` (16 points on screen), or the bar under it: its series,
its index and its values. It is marked with a ring on the plot.
`PlotOutput::pointer` is the pointer in the plot's values, and
`PlotOutput::bounds` what is in view. `Plot::interactive(false)` leaves
the pan and the zoom out (a plot in a scroll area that should scroll).

## Accessibility

A plot is one node, a figure named by `Plot::label` ("Chart" by
default), described by its series: each one's name, kind, point count
and x and y ranges, then the time cursor, as in "Left wheel (line):
200 points, x from 0 to 19.9, y from -1 to 1. Time cursor at 4". The
legend's entries are egui_plot's checkboxes, which hide a series.

## For agents

`PlotPlugin` declares the `plots` collector: every plot drawn in the
last two passes, by id, with its label and summary, its series (name,
kind, point count, ranges, colour), the bounds in view, the hovered
point and the time cursor (`Plots::of(ctx).describe()`).

## Why egui_plot

The survey ([widgets.md](widgets.md)) found egui_plot on egui 0.37
(0.38.0, 2026-10-19), maintained in egui's organisation, under MIT OR
Apache-2.0: lines, points, bars, box plots, a legend that hides series,
axes with formatters, linked axes and cursors, pan, zoom and box zoom.
It has no accessible description of a plot, no theme beyond egui's
visuals and no notion of a time given from outside: those are egui-pop's
part above, and nothing of egui_plot is redone.
