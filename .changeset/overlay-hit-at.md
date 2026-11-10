---
egui-pop-core: patch
---

`overlay::hit_at` says which overlay of the stack a press at a point lands in, given the egui layer on top there (an overlay's own popups count), or none, for code that dismisses on a press away while sharing the stack's notion of inside.
