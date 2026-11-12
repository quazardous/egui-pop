---
egui-pop-api: patch
egui-pop-test: minor
---

Closing the main window while another is open moves that window's
content into it at the window's own size and place (the position where
the platform tells it), and the main window takes its own back when
raised. With the application's frame (`FramePlugin::own`), application
windows and content moved into the main window get the main window's
title bar: title, minimize, maximize, close. `MultiWindow` gains
`names`, `click`, `queue` and `set_outer_position`.
