---
egui-pop-core: minor
egui-pop: minor
---

The slider's other forms. `Slider::vertical` turns a slider into a
fader (up is more, the value at the top, the label at the bottom, the
same gestures and history). `knob::Knob` is a rotary knob with the
slider's terms and history: drag up or around, Shift finer, the wheel,
keys, typed entry, double click to reset. A slider now shrinks to its
room (at least `slider::MIN_WIDTH`, 24 points): the label goes to the
tooltip, then the unit (`Slider::compact`); `Slider::size` sets it.
