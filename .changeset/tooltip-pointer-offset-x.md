---
egui-pop-tokens: minor
egui-pop-core: minor
egui-pop: minor
---

Tooltips at the pointer (`tooltip.placement: pointer`) take a horizontal
offset, `tooltip.offset-x` (0 when not given, so today's themes keep
their look): their corner sits `offset-x` right of and `offset` below the
pointer (13 and 13 for GPUI's). Where the window lacks room on the right
or below, the tooltip goes to the pointer's other side, the same distance
away, instead of being pushed back under the pointer.
