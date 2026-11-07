---
egui-pop-core: patch
egui-pop: patch
---

A window drawn with its own title bar keeps answering clicks after the
compositor moved or resized it on Wayland (labwc, GNOME), which never
hands back the release of the press that started the move: egui held the
button down, nothing hovered, and the window buttons or a section's
title answered only after another click. The window's next pass now starts
with the button released (`frame::compositor_grab`, called after
`StartDrag` and `BeginResize`).
