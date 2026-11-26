---
egui-pop-api: minor
egui-pop: minor
---

An application gives a command a live title and tooltip on the bus
(`pop.command.label`, `bus::CommandLabel`, its value a `bus::CommandText`):
menu items, toolbar buttons, the menu bar's end and the layout controls
show the title, and the tooltips the tooltip (with the key binding where
they give it), asked each time they are drawn. `AppHandle::command_text`
reads the answer and `AppHandle::command_tooltip` the tooltip shown.
Without an answer, the declared title and description are shown, as before.
