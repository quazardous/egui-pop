---
egui-pop-viewport: minor
egui-pop-core: minor
egui-pop-model: minor
egui-pop-api: patch
egui-pop-views: patch
egui-pop-i18n: patch
---

The image viewport and plots move like the node graph: the image
viewport takes the canvas navigation commands (`view.fit`,
`view.zoom_in`, `view.zoom_100`, `view.pan_*`, `view.hand`...) and
their keys, is the canvas in use (`canvasActive`) under the pointer or
with the keyboard, and pans in hand mode from anywhere, a handle
included; plots take the same commands as the canvas in use. Two
fingers on a touch screen pan every canvas, and a pinch zooms it.

A zoom control for any canvas, `navigation::CanvasZoom`: zoom out, the
zoom in percent with a menu (fit, frame the selection, 100 %, steps),
zoom in and hand mode, sending the canvas commands (a step is
`view.zoom_to:<percent>`); `GraphOutput::canvas` and
`ViewportOutput::canvas` name the canvas it drives, and each canvas
publishes its zoom (`egui_pop_core::canvas::zoom`). Hand mode now lives
in `egui_pop_core::canvas`.

The navigation keys are the pointer profile's: a profile document's
`view_keys` (Photoshop's Ctrl+1 and Ctrl+0, darktable's Alt+1 and
Alt+3, Document's Shift+1 and Shift+2, Blender's `.`), loaded into the
keymap with its gestures, so the shortcuts editor shows them and the
user rebinds them. `PointerMap::view_keys` gives those in force;
`NavKeys` is now a list of bindings read from it, and a canvas leaves
the keys to an application's keymap (`PointerMap::keys_dispatched`).
