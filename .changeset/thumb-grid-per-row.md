---
egui-pop-views: major
egui-pop: major
---

The thumbnail grid's zoom is a count of thumbnails per row, `Zoom::PerRow(n)`: the cells share the panel's width, so the zoom survives a resize, and one per row fills the panel. + and -, Ctrl and the wheel step through 1, 2, 3, 4, 5, 6, 8, 10… per row; a panel too narrow for `n` cells of `MIN_SIZE` (48 points) shows as many as fit and keeps `n`. Breaking: `LibraryState::set_size` becomes `set_zoom` (`zoom` reads it, `size` is the cell width of the last pass), `Signal::Zoom` carries `per_row`, and `MAX_SIZE`, `DEFAULT_SIZE` and `ZOOM_STEP` are gone. A size stored in points converts once: `Zoom::from_points`, or `set_zoom(Zoom::Points(p))`, which the next pass turns into its count and signals.
