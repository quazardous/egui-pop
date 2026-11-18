---
egui-pop-api: patch
egui-pop: patch
---

The main window's place has one owner: `window::restore` (which `AppBuilder::run_native` and `run_native_with` call when the application persists) turns eframe's own copy off (`NativeOptions::persist_window`), so an application that also enables eframe's `persistence` feature no longer has eframe's saved place put over egui-pop's, repaired against the screens.
