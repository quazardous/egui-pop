---
egui-pop-api: minor
egui-pop-core: patch
egui-pop-i18n: patch
---

Devtools' keys no longer clash with the browsers' own, so a web build sees
them: the devtools panel (`app.devtools`) moves from F12 to Shift+F12, the
inspect mode (`devtools.inspect`) from Ctrl+Shift+C to Ctrl+Shift+F12.
Firefox, Chrome and Safari take neither, on Linux, Windows or macOS.
Ctrl+Shift+C can still be bound to the inspect mode in the keyboard
shortcuts; egui-winit's copy event is given back as the key only then, so
a text field copies with it otherwise. New `DevtoolsPlugin::on_by_default`:
an application shown to developers (the gallery) turns the
`developer.devtools` setting on by default; it stays off otherwise.
