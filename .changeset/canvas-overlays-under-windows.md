---
egui-pop-viewport: patch
---

A canvas's navigation hint, its hand mode indicator and the node graph's overview are drawn just above their canvas (a sublayer of its layer) instead of in the foreground order: a window, a dialog or a popup over the canvas now lies over them, where they were drawn on top of it.
