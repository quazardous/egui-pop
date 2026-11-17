---
egui-pop-api: patch
egui-pop: patch
egui-pop-test: minor
---

A detached panel's window no longer asks back the size the system just set. Its builder followed the window's live place, so the backend asked each size again one pass late: while an edge was dragged the window was pulled back, and where the compositor granted another size than the one asked (GNOME on Wayland) the width oscillated during the drag and kept oscillating after it. The window now opens with its place and keeps that builder while it lives; a place the system tells is followed into the layout (saved for the next run), and only a place the application changes on purpose (a layout applied) is asked, once, by explicit commands, each logged under the `egui_pop::window_size` target with its reason. `MultiWindow` patches each window's builder as eframe does: `MultiWindow::take_sent` gives every command the system received for a window (from its passes and from a builder that changed), and `MultiWindow::grant_sizes` makes the system obey a size asked, granting another one.
