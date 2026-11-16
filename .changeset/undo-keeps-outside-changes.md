---
egui-pop-model: minor
egui-pop-api: minor
---

Undo, redo, jumps and cancelled groups no longer write over a value changed outside their history since the step: a settings file read back, a write that is not a step, a step in another history (the settings window's), a mixed step dropped with its plugin. Such a value keeps the outside change and is skipped. `Store::take_skipped` and `Store::take_skipped_in` list it, and the walk's answer and the `history.changed` event name it (`skipped`). Undoing one step out of order skips the same way and is refused when every value it changed was changed outside since. A group's or a gesture's step now goes before the steps made after its first write, so undoing everything comes back to where it began. A cancelled group puts back only the values still as it left them, and the first later step on a value changed meanwhile starts from the value before the group. Before, undoing all could stop on a value no state ever had.
