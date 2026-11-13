---
egui-pop-agent: minor
---

The agent socket's `ui` method (and the MCP `ui` tool) gains two pointer actions: `hover` leaves the pointer over a node, so its tooltip shows after egui's delay, and `secondary_click` right-clicks it, opening its context menu. A test can now check where a tooltip or a context menu is drawn.
