---
egui-pop-core: minor
egui-pop-test: minor
egui-pop: minor
---

Input bricks name themselves: a `NumberInput`, a `Slider`, a text `Input` and a descriptor's drop-down (`param`) give assistive technologies their label as their own name, so a caller forgetting `.labelled_by` no longer leaves a field a screen reader cannot name (`NumberInput`'s field and the drop-down had none of their own). `a11y::input` names a custom input brick the same way and records it; `a11y::inputs` lists the bricks drawn in the last pass. A brick built with a blank label stops a debug build. The test kit's `unnamed_inputs` and `assert_inputs_named` list the bricks drawn without a name.
