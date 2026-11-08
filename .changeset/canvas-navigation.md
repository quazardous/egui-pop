---
egui-pop-core: minor
egui-pop-api: minor
egui-pop-viewport: minor
egui-pop-i18n: patch
egui-pop: minor
---

One navigation model for canvases larger than their view
(`egui_pop::viewport::navigation`), on the node graph first:

- **Hand mode**: H toggles it (Space held does the same for a moment);
  a primary drag then pans from anywhere on the graph, for a pen, a
  touch screen or a mouse without a middle button. The grab cursor and
  an indicator in the corner show it.
- **Navigation commands** in the palette, with keys, agent-callable:
  `view.fit` (Shift+1; Home on the canvas), `view.frame_selection` (F
  everywhere; Blender `.`, Document Shift+2), `view.zoom_in`,
  `view.zoom_out`, `view.zoom_100` (Ctrl and `+`, `-`, `0`),
  `view.pan_left`, `_right`, `_up`, `_down` (Ctrl and the arrows) and
  `view.hand` (H). They go to the canvas under the pointer or with the
  keyboard (`egui_pop::core::canvas`; the `canvasActive` context key);
  on a widget alone, the canvas reads the same keys itself.
- **The overview**: the graph's minimap shows in the bottom-right corner
  when the graph is much larger than the view (`GraphEditor::overview`).
- **The view kept** across sessions: `GraphEditor::keep_view(id)`.
- **A one-time hint** on a canvas's first use ("Middle-drag, Space+drag
  or H to pan · Ctrl+wheel to zoom · F to frame"), dismissed by the
  first pan or a click, remembered (`GraphEditor::hint(false)` turns it
  off).
