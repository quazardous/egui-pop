---
egui-pop-api: patch
egui-pop: patch
---

A menu entry's ↗ link mark (`CommandSpec::opens_link`) follows
`menu-item.key-font-size` when a theme sets it: three quarters of the
keys' font size across, in the keys' colour, as a glyph among them,
instead of keeping an inline icon's size.
