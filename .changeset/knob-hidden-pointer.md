---
egui-pop-core: major
---

A knob's drag can hide the pointer and put it back where the press was
when it ends, so a long drag is not stopped by the edge of the screen:
`Knob::hide_pointer(true)` for one knob, the sliders' new
`Gestures::knob_hide_pointer` for all (off by default). Only the linear
drag modes hide it. Windows, macOS and X11 hide and move it back;
Wayland hides it but refuses to move it, so it reappears where the drag
ended; on the web it stays visible. To move: a struct literal of
`slider::Gestures` gives `knob_hide_pointer` too (or uses
`..Default::default()`).
