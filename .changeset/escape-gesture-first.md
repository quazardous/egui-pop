---
egui-pop-core: minor
egui-pop-viewport: patch
egui-pop-api: patch
egui-pop: minor
---

Escape goes to the innermost gesture first. Pressed during a drag (a
slider, a knob, a scrub, a viewport or graph drag), Escape cancels the
drag and nothing else: the focus plugin takes the press out of the pass's
input, so a dialog, the Settings window or a popover around the gesture
no longer closes on it; the next Escape does. A widget cancelling on
Escape reads it with the new `focus::escape_cancelled` or
`focus::cancel_pressed` (the key itself is gone from the input in that
pass). `binding::edit` rolls the value store gesture back when Escape
ends the drag, egui's own widgets included, and cancels a gesture whose
widget stops being drawn while it is open (its host closed mid-drag),
so no gesture is left open.
