---
egui-pop-api: minor
---

A tab dragged out of a detached panel's window and let go over another window of the application docks there: over the main window, in the area under the pointer (the centre, a side or the bottom area, a closed one along the areas' edge), which shows the same drop zone as a drag inside the main window, with the same refusals and their reasons (`only_in`); over another detached window, among its tabs. Let go outside every window, the tab stays. Where the system tells where windows are (X11, Windows) the zone shows during the drag; where it hides them (Wayland) the drop goes to the window the pointer is over once let go, with no zone shown before. A detached window now always shows its tab bar, even with one panel, with a Dock back button at its end (the shown panel docks back where it came from, else in its own area); the `workbench.panel.dock_back` command is bound to `Ctrl+Shift+Home`.
