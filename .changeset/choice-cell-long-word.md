---
egui-pop-core: patch
---

`compact::ChoiceCell` keeps its slot when its word is longer than the slot: the word is cut to the slot's width instead of widening the cell (egui's combo box truncated to the room left in the row, not to its width).
