---
egui-pop-core: patch
egui-pop-api: patch
egui-pop: patch
---

Toolbars run on egui_stack's toolbar row (`egui_stack::toolbar::ToolRow`, egui_stack 0.22.3): it lays the tools out, cuts the row into More tools, takes the clicks and moves the focus (one Tab stop, the arrows, Home and End), and the kit paints the tools through its `DockPainter` (`tool_item`, `tool_more`, `tool_separator`), the painter of the docks and section stacks too. `toolbars::Toolbar` keeps its API, its tooltips, keys, greyed reasons, busy spinners and the `toolbars` collector. The rows of More tools take a menu's look but are now laid out by egui_stack: their labels start nearer the icon, the menu is narrower, and Align's submenu is egui's own. A tool with a title is rounded up to a whole point, so tools after it may move by less than a point.
