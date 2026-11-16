---
egui-pop-api: minor
egui-pop-test: minor
---

A panel tab dragged out of the main window shows where it can go in the detached panels' windows on every system. Where the system tells where windows are, the window under the pointer paints its drop zone over its body, as before. Where it hides window positions (Wayland), no window can tell the pointer is over it until the drag lets it go: each detached window that may take the tab now outlines its body (the kit's `dock-preview` border, disabled where the panel may not go) while the pointer is out of the main window, and the outline goes when the pointer comes back; letting go over one still joins it. `AppHandle::window_outlines` and the debug snapshot's `workbench.window_outlines` list the windows outlined. The drag as the other windows see it lives in one place (`workbench::drag_session`), the panel tabs' case of a drag across windows. In the test kit, `MultiWindow::shapes` gives what a window's last pass painted, `MultiWindow::render` (feature `wgpu`) a picture of it, and `MultiWindow::set_outer_position` places the window's inner rectangle too, as a window without the system's decorations.
