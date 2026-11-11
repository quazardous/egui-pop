---
egui-pop-core: major
---

An endless knob: `Knob::endless(Endless::Wrap)` for a value that wraps
around its soft range (an angle, a hue), `Endless::Unbounded` for one
with no bounds. Its face has no arc and no ends, twelve ticks turning
with the value, one turn over the soft range; it takes the same drag
modes, travel, wheel and keys (Home and End do nothing on it). Every
slider and knob change now says how far the value moved in
`Change::delta`: `to - from`, except on a wrapping knob, where it is the
motion unwrapped, for a host that maps changes to nudges. To move: a
struct literal of `slider::Change` gives `delta` too.
