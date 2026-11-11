---
egui-pop-core: minor
---

`Knob::wheel(KnobWheel::Hover)` makes the wheel turn a knob whenever the
pointer is over it, focused or not, for knobs in places that do not
scroll (a strip of controls, a HUD over an image); `KnobWheel::Profile`,
the default, follows the pointer profile as before. A touchpad's scroll
over a slider, a range or a knob is now summed into steps, one a line's
length, instead of one step a frame whatever its length; a mouse wheel
still steps once a notch.
