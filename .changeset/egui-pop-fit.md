---
egui-pop-fit: minor
egui-pop: minor
---

`egui-pop-fit`, a new crate in the workspace: the slot layout engine formerly published on its own as `egui_fit` (0.1), with its history, its tests and an unchanged API (paths `egui_fit::` become `egui_pop_fit::`). It packs plain boxes into a grid in standard slot sizes (¼, ⅓, ½, ⅔, ¾ of a unit and whole multiples, counted in twelfths), with forms, orientations, pins, groups and their bands, growth and hysteresis, and has no egui dependency; its `serde` feature is kept. The facade re-exports it as `egui_pop::fit`, and its `fit-serde` feature turns on `serde` there. Experimental.
