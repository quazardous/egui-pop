---
egui-pop-model: minor
egui-pop-core: minor
---

Knobs drag on both axes by default: right and up both add (`KnobDrag::Both`), since some people drag a knob upward and others sideways, and neither should be ignored. `KnobDrag::default()` and every bundled pointer profile now use `both`; a knob or a profile can still ask for `vertical`, `horizontal` or `circular`.
