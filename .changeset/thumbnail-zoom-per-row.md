---
egui-pop-core: minor
egui-pop-api: major
egui-pop-views: minor
egui-pop: major
---

The file browser's and the in-app picker's thumbnail zoom is a count of thumbnails per row (`files::browser::ThumbZoom`, the library grid's `Zoom` on the API's side, converted with `From`), not a size: the cells share the grid's width, so the zoom survives a resize, and the largest step shows one thumbnail across. `FileBrowser::zoom`, `set_zoom` and `set_thumbnails_per_row` set it, `FilePicker::zoom` and `set_zoom` the picker's; `ThumbnailGrid::room` gives the width and gap a grid lays its cells out in. The picker keeps the count in `settings/files.thumbnails_per_row` and still reads a size in points kept in `settings/files.thumbnail_size` by an older version, converted at the grid's width. Breaking: `files::thumbnail_size` and `files::want_thumbnail_size` are replaced by `files::thumbnail_zoom` and `files::want_thumbnails_per_row`, `settings/files.thumbnail_size` is no longer declared, `FileBrowser::set_thumbnail_size` asks for a size converted to a count at the next draw, and `ThumbnailsMode` draws up to one thumbnail across rather than 256 points.
