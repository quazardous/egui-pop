---
egui-pop-api: minor
egui-pop-test: patch
---

A host that runs the event loop itself (eframe on its own winit loop, without the `create-native` feature) tells egui-pop the screens: `window::set_screens` takes them as `window::Screen` (the monitor's name as winit gives it, its rectangle in points, its scale factor; `Screen::from_physical` builds one from winit's monitor), and `window::repair_builder` repairs the main window's kept place against them from eframe's `window_builder` hook. A window kept on a screen now gone (an unplugged monitor, on X11 or Windows) then reopens on one, whoever runs the loop; where positions are hidden (Wayland) nothing moves. `window::screen_list` gives the screens told with their names and scales. egui-pop's native runner tells them the same way, and again when a window's scale factor changes or a window moves. `egui-pop-test`: `MultiWindow::set_screens` tells them through the same path.
