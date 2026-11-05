---
egui-pop-tokens: minor
egui-pop-core: major
egui-pop: major
egui-pop-api: major
egui-pop-editors: major
egui-pop-i18n: major
egui-pop-imaging: major
egui-pop-platform: major
egui-pop-test: major
egui-pop-viewport: major
egui-pop-views: major
---

Charts, and a categorical palette for them.

- **A categorical palette.** `palette.category.0` to `palette.category.9`
  are system tokens: one colour per series, in order, aliasing a
  reference group named by hue (`chart.blue`, `chart.orange`...) that
  `pop-dark`, `pop-light` and `pop-high-contrast` each give for their
  surfaces. Every slot reads at 3:1 on the canvas, the panel and the
  raised surface (new pairs in `contrast::PAIRS`), and the slots are told
  apart: `contrast::check_category` measures them in OKLab (ΔE OK,
  `contrast::delta_e`), neighbours at 15, any two at 10, neighbours at 8
  with protanopia or deuteranopia (`contrast::Vision::simulate`).
  `Color::to_oklab` and `Color::to_linear` are new. Code reads the
  palette through `Tokens::palette` (`DataPalette`, `series(i)`).
- **Two kit components**, `plot` (a chart's background and frame, its
  axis text and grid) and `plot-cursor` (its time cursor, a line across
  it). `Kit` gains the `plot` and `plot_cursor` fields, a breaking change
  for code building a `Kit` by hand (`Kit` is not `#[non_exhaustive]`
  yet); the crates depending on egui-pop-core take the major with it.
- **`egui_pop_views::plot`**: `Plot` wraps egui_plot (MIT OR
  Apache-2.0) for time series, scatters and bars in the kit's colours and
  the palette, with an accessible summary (its series' names, kinds,
  point counts and ranges), the point under the pointer
  (`PlotOutput::hovered`), an optional time cursor
  (`Plot::time_cursor(Option<f64>)`), linked axes and cursors, and the
  `plots` collector (`PlotPlugin`). The gallery's new `plot` page shows
  them.
