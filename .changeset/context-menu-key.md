---
egui-pop-core: minor
egui-pop-api: minor
egui-pop-views: patch
---

Context menus open by the keyboard: Shift+F10 or the `Menu` key opens
the context menu of the focused widget, under it rather than at the
pointer, for `pointer::claim::context_menu` and so for
`menus::context_menu_with`; a tree opens its cursor row's. New:
`pointer::claim::menu_key` (whether the key was pressed),
`pointer::claim::context_menu_opened` and `menus::context_menu_opened`,
for a widget whose rows open menus while it keeps the focus itself.
