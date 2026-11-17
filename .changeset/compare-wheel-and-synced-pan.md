---
egui-pop-views: patch
egui-pop-core: patch
egui-pop-viewport: patch
egui-pop: patch
---

The library's comparison zooms and pans with the wheel as the image viewport does, through the same wheel path (`egui_pop_core::pointer::view`, where the viewport's `ViewGestures` and `ViewAction` now live, re-exported where they were): the pointer profile's `canvas.image` gestures, so the wheel alone zooms with darktable's profile and Ctrl and the wheel with the default one, where the comparison used to zoom with Ctrl and the wheel whatever the profile. Synced panes no longer drift apart while dragging: every pane moves by the same motion in points (a `PaneView`'s pan is now in pane sizes, not in each photo's fitted size), and a drag follows the pointer from the press, nothing lost to the drag threshold, so the image point under the pointer stays under it in every pane whatever the photos' shapes.
