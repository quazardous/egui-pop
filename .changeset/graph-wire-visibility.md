---
egui-pop-tokens: minor
egui-pop-viewport: minor
egui-pop: minor
---

The node graph's hovered and selected wires are easy to see, even short
ones between close nodes. A selected wire is drawn thicker in the accent
colour over a soft glow, with larger handles near its ends and its two
ports ringed; a hovered wire is drawn thicker and lighter and the pointer
becomes a hand. New tokens, given by every built-in theme and
contrast-checked against the canvas: `graph.wire-hover.color` and
`.width`, `graph.wire-selected.color`, `.width`, `.glow`, `.glow-width`
and `.glow-opacity`.
