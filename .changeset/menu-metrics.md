---
egui-pop-tokens: minor
egui-pop-model: minor
egui-pop-api: minor
egui-pop: minor
---

A theme sets a menu's metrics: `menu-item.height` (a row's least
height), `menu-item.padding-x` and `padding-y` (the room around a
label), `menu-item.width` (the menu's least width, instead of the
popup's 200 points) and `menu-item.separator-inset` (0: separators
across the whole menu). A `menu-item-gutter.width` of 0 now keeps no
gutter (it fell back to the default width). `CommandSpec::menu_note`
ends a command's menu entries with a muted note in the keys' column,
and `CommandSpec::opens_link` with a ↗. Nothing set, menus look as
before.
