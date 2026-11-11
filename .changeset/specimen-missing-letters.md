---
egui-pop-core: minor
egui-pop-api: patch
egui-pop: minor
---

A letter a font specimen lacks is drawn as a box, never a panic.
`fonts::sample_label` draws text in a font with the letters no face of its
family covers drawn as boxes, whatever egui's `MissingGlyphPolicy` (under
kittest's default, `Panic`, laying them out panicked), and
`fonts::uncovered` lists those letters. The settings window's font picker
draws a family's name in its own face only when the face covers it, in the
interface's font otherwise.
