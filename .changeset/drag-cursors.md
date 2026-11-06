---
egui-pop-model: major
egui-pop-core: major
egui-pop-api: major
egui-pop-viewport: major
egui-pop-views: major
egui-pop-editors: major
egui-pop-imaging: major
egui-pop-i18n: major
egui-pop-platform: major
egui-pop-test: major
egui-pop: major
---

Every draggable shows a cursor for its kind of drag, hovered and for the whole drag, even once the pointer has left it, and the pointer profile can change them.

- **Drag kinds.** `model::pointer::DragKind` names what a drag means, whatever the widget: `pan` and `scrub` (an open hand, closed while dragging), `move`, `carry` (a closed hand once it moves), `value` (a horizontal resize once it moves), `point` and `select` (a crosshair once they move), `text` (an I-beam), and the four resizes. `DragKind::cursors` gives each one's hover and drag cursors, as CSS cursor names.
- **The helper.** `core::pointer::drag_cursor(&response, kind)`, also `UiPop::drag_cursor`, shows the hover cursor while the response is hovered and nothing else is dragged, and the drag cursor every pass it is dragged, wherever the pointer is; `hold_drag_cursor` and `hover_drag_cursor` serve a widget that follows its drag itself, and `drag_cursor_icons` reads a kind's cursors.
- **The widgets.** The timeline's lanes had none: a fixed playhead's content (a DJ deck) now shows an open hand, closed while scrubbed or nudged; a scrub, a range and a middle-drag pan keep their cursor to the end; clips show move. Graph nodes and groups show move, a port's wire a closed hand, the canvas's pan, rubber band and wire ends keep theirs. Sliders, range sliders and the video's scrub bar, the curve editor, the colour wheels, the navigator, the graph's minimap, the thumbnail grid's rubber band, the compare view's panes, a tree's drag and drop, a palette's tabs and the code editor (an I-beam over its text) gained theirs; splitters, dock and palette edges, dialog grips, bands, item handles (now move), timeline markers and trim handles and the histogram go through the helper.
- **Profiles.** A `pointer-profile` document may give `drag_cursors`, by kind (`{"move": {"hover": "default"}}`); `PointerMap::drag_cursor` resolves them. Blender / node editors keeps the arrow over what can be panned or moved, as Blender does.

Migration: `Profile` and `PointerMap` gain a `drag_cursors` field; a struct literal of either needs it (`Default::default()`, an empty map).
