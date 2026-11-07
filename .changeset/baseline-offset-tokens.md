---
egui-pop-tokens: minor
egui-pop-core: minor
egui-pop: minor
---

A theme sets where text sits in its row: `font.baseline-offset.ui` and
`font.baseline-offset.mono` (numbers, 0 in every built-in theme) draw a
family's glyphs that share of the size lower (negative: higher),
through egui's `FontTweak::y_offset_factor` on each face of the family;
the row keeps its height. A theme matching another toolkit's text sets
one value. `Tokens::font.baseline_offsets` holds them;
`fonts::BaselineOffsets` and `fonts::install_with_offsets` install the
families with them, and `fonts::install` keeps the offsets installed
last.
