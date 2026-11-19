---
egui-pop-core: minor
egui-pop-api: minor
egui-pop-views: major
egui-pop: minor
---

Thumbnail grids take a resize policy, `GridResize` (`egui_pop_core::thumbnail`, re-exported by `egui_pop_api::files::browser` and `egui_pop_views::library::grid`): `PerRow`, the default and the behaviour since 0.16, keeps the count per row on a resize, so a wider grid shows larger thumbnails; `FixedSize` keeps the thumbnails' size, so a wider grid shows more columns, as catalogue applications do. The file browser takes it with `FileBrowser::with_resize`, the in-app picker with `FilesPlugin::grid_resize`, the library grid with `ThumbGrid::resize`. Under `FixedSize` the zoom is a size: the browser's and picker's slider sets a size in points, `+` and `-` and `Ctrl` and the wheel step by a column, the zoom is kept as `ThumbZoom::Points` / `Zoom::Points` (`ThumbZoom::under` converts a zoom into a policy's form), the picker keeps it in `settings/files.thumbnail_points`, and the library grid signals `Signal::ZoomSize { points }` for the application to keep (a new variant of `Signal`: a match on it needs an arm). The gallery's library page switches between both.
