---
egui-pop-core: minor
egui-pop: minor
---

Inputs keep to the width they are given. A horizontal slider with no size no longer reaches past what shows of its row (a panel's edge cutting a row wider than it): its bar ends at the edge, with its value inside. The slider measures its value as it draws it (in the numbers' font) before choosing to show its label or to drop the unit, and its value is never drawn past the bar's ends. `NumberInput::width` sets the field's width; with no range (an end at `f64::MAX`, as `from_descriptor` gives a descriptor without one) the field is now sized to the value shown instead of the widest field (160 points), and a bounded field is sized to the wider of its range's two ends. Behaviour change: sliders in a narrow room and number inputs without a range lay out narrower than before.
