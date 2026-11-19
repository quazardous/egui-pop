---
egui-pop-core: patch
---

A focused slider, range slider or knob keeps its arrow keys: they step its value and no longer move the keyboard to a neighbouring widget after the first press (egui's arrow navigation, which a tab bar's grip beside a panel made reachable). It also says it reads its arrows, Page Up, Page Down, Home and End (`focus::reads_keys`), so they reach it before a global binding of theirs. And a thumbnail grid's rubber band senses only the grid's own width, no longer taking the clicks of widgets beside the grid in the same `Ui` (the scroll area clips its rows only up and down).
