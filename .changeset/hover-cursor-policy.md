---
egui-pop-tokens: minor
egui-pop-model: minor
egui-pop-core: minor
egui-pop-api: minor
egui-pop-viewport: minor
egui-pop-views: minor
egui-pop: minor
---

A draggable shows its drag cursor hovered only when dragging is its only action; where a click does something, the arrow stays until the drag starts. The widget, the pointer profile and the kit can each change the cursors and that rule.

- **The rule.** `model::pointer::HoverPolicy` says when a kind's hover cursor shows: `only-action` (every kind's default but `text`'s: only when nothing happens on a click or a double click), `always` (text's I-beam) or `never`. Whether an element clicks is read from its response's sense; `DragCursorFor::clicks` says it otherwise. Splitters, side area, flyout and palette edges, dialog grips, item handles, the histogram and a view frame keep their hover cursor; timeline lanes and DJ decks (a click seeks), clips, markers, trim handles, graph nodes and bands (a click selects), and the compare view's panes (a double click fits) show the arrow until dragged.
- **Split kinds.** `DragKind::SplitColumns` and `SplitRows` (a column and a row resize) are the splitters' and the side areas' and flyouts' edges. The navigator's and the graph minimap's view frame show move, the arrow elsewhere on them (a click jumps there).
- **Three levels**, the most specific winning: `core::pointer::drag_cursor` returns a `DragCursorFor`, applied when dropped, with `.hover(cursor)`, `.dragging(cursor)` and `.hover_policy(policy)`; a pointer profile's `drag_cursors` take a `policy` beside `hover` and `drag` (`PointerMap::drag_cursor_over`); a kit gives `drag-cursor.<kind>.hover`, `.drag` and `.policy` (`kit::DragCursorKit`, the default kit giving every kind's own). `core::pointer::resolve_drag_cursor` gives the result; `hover_drag_cursor` takes whether the element clicks.
