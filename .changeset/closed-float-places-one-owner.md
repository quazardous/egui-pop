---
egui-pop-api: patch
egui-pop: patch
---

A detached window's closed place has one owner, egui-pop's (the screen it was on and its rect): the copy egui_stack keeps when it closes a float (`ZoneLayout::closed`) is dropped at each pass, so a panel detached again opens where egui-pop kept it, never moved by a stale copy.
