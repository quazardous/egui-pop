---
egui-pop-core: minor
egui-pop: minor
---

A flat tree's row at its depth: `TreeRow::depth(n)` indents a row by `n`
times the theme's `spacing.indent` before its toggle (or a leaf's room
for one), and says it is at level `n + 1` to assistive technology, on its
toggle and on its row. For a tree kept as a flat list, whose rows are not
nested in their parents' bodies; what the caller draws in the row is
unchanged. The gallery's folds page shows one.
