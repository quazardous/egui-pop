---
egui-pop-core: minor
egui-pop-api: minor
egui-pop: minor
---

The file browser's thumbnails select with a rubber band. Drag from the room around the cells and the selection follows the band as it is drawn: the band's cells alone, or added to the selection with Ctrl or Shift held. The focus goes to the band's last cell when it is let go. The grid under it, `ThumbnailGrid`, offers the band to any application: turn it on with `rubber_band(true)`, then `show_with_band` returns a `GridOutput` whose `Band` lists the cells it touches, scrolled away or not (`cells_in`).
