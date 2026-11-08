---
egui-pop-api: patch
---

The file browser's mode switcher no longer panics in a context without
the icon font (a test harness, the first pass): its buttons show their
labels alone until the font is in use.
