---
egui-pop-tokens: minor
egui-pop-views: minor
egui-pop-i18n: minor
egui-pop: minor
---

Plots over a timeline's time, and two new parts for the plot's look.

- **Time on the x axis.** `Plot::time_scale(TimeScale)` puts the x axis in a timeline's scale (seconds, timecode, bars and beats): x values are seconds, the ticks and labels the timeline ruler's for the span in view and the plot's width.
- **A time cursor dragged both ways.** `Plot::timeline(&mut TimelineState)` takes the timeline's scale, shows its playhead as the time cursor, moves the playhead when the cursor is dragged, and shades its range (in the `timeline.range.*` colours, the edges by whether it loops). Without a timeline, `Plot::drag_cursor(true)` makes the cursor draggable and `PlotOutput::cursor_dragged` says where it went; `PlotOutput::cursor_hovered` says the pointer is on it; `Plot::highlight_range` shades a range. A draggable cursor is a slider named "Time cursor".
- **Stepped lines and error bands.** `Kind::Steps` (`Series::steps`) holds each value until the next point; `Band::new(series, [[x, min, max], ...])` with `Plot::bands` fills between a minimum and a maximum in its series' colour.
- **Entity colours.** `Series::entity(path)` takes the categorical palette's slot a stable hash of the path gives (`entity_slot`, FNV-1a), the same in every plot, run and platform.
- **Values at the pointer.** `PlotOutput::values` (`ValuesAt`, `values_at`) is every line's value at the pointer's x, else at the cursor; `Plot::values_tooltip` lists them beside the pointer (on by default with a time scale or a timeline). `Plot::emphasize` draws one series thicker. The `plots` collector adds the time scale, the x axis' labels, the range and the values.
- **The plot's grid** is a part of the `plot` component: `plot.grid-color` and `plot.grid-fade` (`GridLook`), by default the muted text at egui_plot's fade (0.5), as before; a decoration, not contrast-checked.
- **The time cursor's head and hover colour**: `plot-cursor.head` (`none` or `triangle`), `plot-cursor.head-size` and `plot-cursor.hover.fill` (`CursorLook`); by default no head and the same accent, as before. The hover colour is checked as a user-interface part over the plot. The Rerun-like kit gives the grid Rerun's colour and fade and the cursor its triangle, strong under the pointer.
