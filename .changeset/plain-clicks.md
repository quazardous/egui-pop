---
egui-pop-core: patch
---

Enter and Space click a focused plain widget (a button, a checkbox, a
row sensing clicks only) only when pressed alone and once: with Ctrl,
Alt, Shift or Command held, or repeated while held down, the key
dispatcher takes them out of the input after the keymap had its turn, as
a native button does. A binding on such a key still runs; a widget
sensing drags (a slider, a splitter, a text field) reads its own keys as
before.
