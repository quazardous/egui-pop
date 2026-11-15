---
egui-pop-api: minor
egui-pop: minor
---

`runner::window(viewport)` reaches the native window of each viewport egui-pop's runner knows, by its viewport id: a `runner::Window` implementing `HasWindowHandle` and `HasDisplayHandle`, what a file chooser's parent or a surface takes. The main window's is eframe's own, known on every system from its creation; another window's once its winit id was verified as its handle. `runner::windows` lists the viewports whose window is known, and `runner::window_handle(ViewportId::ROOT)` now answers eframe's handle before the ids are verified. Experimental, with the rest of the runner.
