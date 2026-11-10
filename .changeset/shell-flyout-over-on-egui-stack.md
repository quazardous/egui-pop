---
egui-pop-api: patch
---

The workbench's flyout and its side areas lying over the centre are drawn by the shell's egui_stack layout (0.25.6): the flyout through `LayoutArea::flyout`, its width still kept per panel, and lying over through `ZoneState::over`. The flyout stays an overlay of the stack, so Escape closes the top overlay first and a combo box or context menu opened in it counts as inside. Lying over is a lasting placement: a click away or Escape leaves it, and only its strip's grip folds it. Its edge on the centre's side resizes it, and it now keeps the same width as the area beside the centre.
