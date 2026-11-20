---
egui-pop-api: minor
egui-pop-i18n: patch
egui-pop: minor
---

`FramePlugin::logo` puts the application's logo in its own title bar,
between the system menu button and its name, a toolbar icon's size and
named for screen readers ("Helios logo"): the frame's menu block draws
it, so an application no longer replaces that block to show its logo.
The gallery's title bar shows one.
