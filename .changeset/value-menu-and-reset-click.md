---
egui-pop-model: minor
egui-pop-i18n: minor
egui-pop-core: major
---

A slider's and a knob's right click (or the context menu key) opens a
menu: "Type a value" opens the typed entry from the pointer, and
"Reset" puts back the default when there is one. `Slider::menu(false)`
and `Knob::menu(false)` turn it off for a host that gives the widget a
menu of its own; that menu's item calls
`slider::request_typed_entry(ctx, response.id)`. A click with the
modifiers bound to the new `gesture.value.reset` gesture
(`Ctrl+PrimaryClick`, as in Reason or Kontakt) resets a slider or a
knob too; no shipped profile binds it, so a double click stays the
reset everywhere. To move: a struct literal of `slider::Gestures` gives
`reset_click` too (or uses `..Default::default()`).
