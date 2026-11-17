---
egui-pop-viewport: minor
egui-pop-core: minor
egui-pop-api: minor
egui-pop: minor
---

The image surface is per window: every window drawing the image gets a surface of its own, under its own window surface and negotiated for what that window wants (`image_surface::install_window`, or `set_window_source` for windows opened later), freed when the window closes (`close_window`). A detached viewer on a wide-gamut screen can draw wide while the main window stays sRGB. Each window's image surface (route, format, colour space, or why it has none) shows in `windows.list` (`WindowRecord::surface`, from `egui_pop_core::window_surface`) and in the viewport collector's `windows`.
