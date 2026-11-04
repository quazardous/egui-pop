---
egui-pop-core: patch
---

A plain key bound to a command whose text is a symbol (`/`, the key `Slash`)
no longer types that symbol into the widget that gets the focus: the
dispatcher takes its text as it does a letter's, also when it arrives a pass
or two after the press.
