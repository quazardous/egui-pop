---
egui-pop-api: patch
egui-pop: patch
---

A dialog opened from an application window (a confirmation, a form) is cancelled when that window closes, as a file choice asked from it is, instead of coming up again over the main window. A window that moves into the main window (the main window closed while it was open) still takes its dialogs there.
