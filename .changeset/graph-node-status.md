---
egui-pop-viewport: minor
egui-pop: minor
---

The node graph draws a node's status in its title bar:
`GraphEditor::status` gives each node an optional `NodeStatus`, a short
text drawn small at the title bar's right end (left of the `header`
slot), in a kit tone, an application's colour or a theme token's colour
(`StatusColor`), its detail shown as a tooltip. The text and the detail
are part of the node's accessible description. The gallery's node graph
page shows one.
