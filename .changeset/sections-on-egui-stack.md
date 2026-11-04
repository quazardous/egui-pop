---
egui-pop-core: major
egui-pop-model: minor
egui-pop: major
egui-pop-api: major
egui-pop-editors: major
egui-pop-i18n: major
egui-pop-imaging: major
egui-pop-platform: major
egui-pop-test: major
egui-pop-viewport: major
egui-pop-views: major
---

Collapsible sections (`core::sections`) run on egui_stack's `sections` module
(egui_stack 0.22.2): the kit paints them as its `DockPainter`, the strings
name their boundaries, the pointer map gives the pull's keys, and the look and
the behaviour are unchanged. `Section`, `Stack`, `Laid`, `Remembered`,
`laid_out` and `row_height` are egui_stack's, re-exported; `Laid` is now
`#[non_exhaustive]`, so it can no longer be built outside the crate. The
arithmetic is re-exported as `core::sections::plan` (with `DragMode`), and
`model::sections` is deprecated, kept unchanged for one release. Layouts are
remembered under egui_stack's memory key; one stored under egui-pop's old key
moves there the first time its stack is shown or loaded.

Migration: build a `Laid` from a real stack (`laid_out`) instead of a struct
literal; take `DragMode` and the plan from `core::sections`, not
`model::sections`.
