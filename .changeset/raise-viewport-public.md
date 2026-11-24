---
egui-pop-api: minor
egui-pop: minor
---

`AppHandle::raise_viewport(viewport, owner)` asks the window a viewport
draws (the main window, a detached panel's, an application window's) to
the front through the bus: `pop.window.activate` with the reason
`command`, so the registry's active window follows. It says whether a
window of the registry draws that viewport.
