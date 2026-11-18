---
egui-pop-api: major
egui-pop-i18n: patch
egui-pop: major
---

The shortcuts editor says where each key listens when that is not every window: "Ctrl+B in the main window", a panel's keys "in the Log panel", another window's "in the Import window", a surface's by its name. The area toggles (Ctrl+B, Ctrl+J, Ctrl+Alt+B) now listen in the main window only (`window:main`), whose workbench they fold: pressed in another application window they no longer fold the main window's areas.
