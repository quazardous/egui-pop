---
egui-pop-model: major
egui-pop-core: major
egui-pop-api: major
egui-pop-viewport: major
egui-pop-i18n: major
egui-pop-platform: major
egui-pop-views: major
egui-pop-editors: major
egui-pop-imaging: major
egui-pop-agent: major
egui-pop-test: major
egui-pop: major
---

A knob turns in one drag mode, chosen by the application, never by where
the press lands: `KnobDrag` (`Vertical`, the default: up is more;
`Horizontal`: right is more; `Both`: right and up both add; `Circular`:
around the centre, relative to the press, so the value never jumps, and
still near the centre). `Knob::drag` sets it for one knob; the pointer
profile's new `options.knob_drag` (`vertical` in every shipped profile)
for the others, carried in the sliders' `Gestures::knob_drag`. The
centre and rim zones are gone: a press on the rim drags vertically like
one in the middle. `Knob::travel` is the points of drag over the soft
range in every linear mode, whatever the knob's size (200 by default).
To move: a struct literal of `pointer::Options` or `slider::Gestures`
gives `knob_drag` too (or uses `..Default::default()`); a knob that
relied on its rim to turn around takes `.drag(KnobDrag::Circular)`.
