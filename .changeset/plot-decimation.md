---
egui-pop-views: minor
egui-pop: minor
---

Plots draw series of millions of points: decimation per pixel column.

- **Decimated by default.** A line, a stepped line or an error band with more than `DECIMATION_THRESHOLD` points in view (1000) is drawn from at most four points a pixel column (its first, lowest, highest and last, in order), which cover the same pixels as the full series at the current zoom; a band keeps each column's lowest minimum and highest maximum, over the same columns. `Plot::decimation(None)` draws every point, `Plot::decimation(Some(n))` sets the threshold. The approach follows Rerun's time series view.
- **The points in view only.** A series sorted by x (a time series) is read in the x range in view, found by binary search, so a pass costs the points in view rather than the series' length; a series not sorted by x is drawn whole, as before.
- **The true sample under the pointer.** `PlotOutput::hovered` and the values at the pointer are read in the full series, never in the points drawn, by binary search on a sorted series; `values_at` no longer copies a series.
- **The collector** says each series' `drawn`, the points handed to the drawing this pass.
