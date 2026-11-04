---
egui-pop-api: minor
egui-pop-core: minor
---

An inspect picker in the menu bar, and a crosshair while inspecting. The
menu bar's end shows the command items of a new menu,
`app::MENU_BAR_END`, as icon buttons (`menus::bar_end`): each the
command's icon, pressed while its `checked` clause holds, its title and
key binding in the tooltip, left out where the command does not apply.
`devtools.inspect` goes there with the `phosphor:cursor-click` icon, so
it shows while the devtools setting is on, pressed while the mode is (a
new `inspecting` context key, `inspect::CONTEXT_INSPECTING`); pressed
again, it leaves the mode rather than being picked
(`egui_pop_core::inspect::pass_through`). While inspecting, the pointer
is a crosshair (`inspect::CURSOR`) everywhere but over the inspect
dialog.
