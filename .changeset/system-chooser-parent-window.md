---
egui-pop-api: minor
egui-pop: minor
---

The system's file chooser (feature `native-dialogs`) is attached to the window it was asked from: the main window's chooser to the main window (the portal's parent window, the owner window on Windows, a sheet on macOS), where it was attached to none. A choice asked from another application window is attached to that window when its handles are recorded with `files::set_parent_window`, else to none, never to the main window. `AppBuilder::run_native` and `frame::record_native_window` record the main window's; `files::chooser_parent` tells which window a choice's chooser goes to.
