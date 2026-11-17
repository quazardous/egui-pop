---
egui-pop-model: major
egui-pop-core: major
---

Knobs drag on both axes by default: right and up both add (`KnobDrag::Both`), since some people drag a knob upward and others sideways, and neither should be ignored. `KnobDrag::default()` and every bundled pointer profile now use `both`; a knob or a profile can still ask for `vertical`, `horizontal` or `circular`.

**Breaking:** `KnobDrag` has a new variant, `Both`, and is the new default: a `match` on it adds an arm; a knob or profile that relied on the default and wants the old behaviour asks for `KnobDrag::Vertical`.
