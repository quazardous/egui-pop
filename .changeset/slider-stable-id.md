---
egui-pop-core: patch
---

A slider, a range slider, a knob or a scrub field keeps its drag when a widget appears before it mid-drag (a "modified" mark, a reset button shown once the value moves): it senses under an id keyed by its label in its `Ui`, no longer egui's automatic id, which changed with every widget added before it and made egui drop the drag after its first frame (the value moved once, then no end of the gesture, no undo step, and Escape restored nothing). Two such widgets with the same label in the same `Ui` take numbered ids.
