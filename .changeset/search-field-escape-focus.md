---
egui-pop-views: patch
egui-pop: patch
---

`SearchField` with suggestions keeps the keyboard through its Escape
sequence in an application's window: the focus plugin made the field let
go on the first Escape, so the list closed but the next Escapes (clear the
text, leave the field) never reached it. While Escape has a step to do,
the field now lets it through the focus plugin and claims it from the key
dispatcher.
