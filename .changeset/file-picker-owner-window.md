---
egui-pop-api: patch
egui-pop: patch
---

The in-app file and folder picker opens over the window it was asked from: a choice made in an application window shows its modal there and blocks that window alone, instead of coming up over the main window; closing that window cancels the choice. `Files::owner` tells which window a choice belongs to.
