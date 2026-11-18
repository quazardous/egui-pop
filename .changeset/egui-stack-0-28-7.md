---
egui-pop-api: patch
egui-pop: patch
---

egui_stack 0.28.7: every tab bar button paints through one helper, the line under the bar running under each of them. A detached window keeps its look and its moves: egui-pop's windows are native floats (embedded in the main window where the backend has no windows, as in kittest and on the web, with egui's own title bar), which the new in-window float title bar default does not touch.
