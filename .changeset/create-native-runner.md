---
egui-pop-winit: minor
egui-pop-api: minor
egui-pop: minor
---

Native applications now run eframe on an event loop egui-pop owns (`eframe::create_native`), behind the new `create-native` feature, on by default; eframe still creates, paints and runs every window, and the web is unchanged. `AppBuilder::run_native` and `run_native_with` read the screens before the main window is created, so the place restored from the last run is repaired against them (a window left on a screen that is gone comes back), and each window's raw handles are read from its winit id once painted and recorded per viewport (`files::set_parent_window`, `frame::set_native_window`), forgotten when the window closes. `runner::screens`, `runner::window_handle` and `runner::ids_verified` say what the runner saw; `frame::forget_native_window` is new. A new crate, `egui-pop-winit`, turns a winit window id into the window's raw handles (Win32, X11, Wayland), its `unsafe` kept there.
