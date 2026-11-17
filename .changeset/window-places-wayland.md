---
egui-pop-api: major
egui-pop-model: major
egui-pop-core: major
egui-pop-viewport: major
egui-pop-test: minor
---

Windows keep their place where the system hides positions (Wayland) and come back where they were elsewhere (X11, Windows). The main window keeps its size on Wayland (it kept none: egui gives no window rectangle there); a run on Wayland keeps the position an earlier X11 or Windows run saved instead of replacing it. Application windows keep their normal size while maximized, keep full screen (`windows::Geometry` gains `fullscreen` and `Geometry::follow`), keep a position kept earlier on Wayland, and a kept place off every screen opens on one. Detached panel windows keep their size on Wayland, keep maximized and full screen (`layout::Auxiliary` gains `maximized` and `fullscreen`), come back where they were when detached again after their window closed, and a place off every screen is brought back. On Wayland the View menu says under Full screen that the system places the windows (`window::POSITIONS_HIDDEN`, `window::LIST_PLACEMENT`). New: `window::screens`. The test kit's `MultiWindow` gains `builder`, `set_maximized`, `set_fullscreen` and `set_screens`.

**Breaking:** `windows::Geometry` has a new public field, `fullscreen`, and `layout::Auxiliary` two, `maximized` and `fullscreen`: a struct literal gives them (`false` keeps the old behaviour); a `Geometry` literal may end with `..Default::default()` instead.
