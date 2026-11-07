---
egui-pop-core: patch
egui-pop-api: patch
egui-pop: patch
---

egui-pop moves to egui_stack 0.25.3, and a palette no longer gets narrower than its content. egui_stack's floating windows now stop at their least size and take drops over a dock, a folded one keeps the height of its band, and it adds `Section::max_height` and `max_share`, `DockSpace::no_drop_zone(s)` and `DockRules::accept_drops`. A palette whose content does not shrink (a row of tools that does not wrap) wider than its panels' least width could be resized below that content's width: its frame stopped shrinking while the buttons of its band kept moving left, away from its right edge. The palette now stops at the width its content takes, and its band's buttons stay at its right edge, while it is resized, once it is dropped and when it opens at its content's size.
