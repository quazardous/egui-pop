---
egui-pop-core: minor
egui-pop: minor
---

`Slider::travel(points)`: points of drag over the soft range, whatever the bar's length, as `Knob::travel` has. A scrub field (`Slider::scrub`) moves at a set speed, a value's sensitivity, rather than across its width; the fine and coarse modifiers still scale it.
