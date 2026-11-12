---
egui-pop-api: minor
---

The Settings app's window mode (`admin.window`, "Pop out") is an
application window (`admin::APP_WINDOW`, id `settings`) rather than an
immediate viewport: it repaints on its own, draws the application's
title bar when the application draws its frame, keeps its size and
place, closes alone (its close button, Escape, Close, `admin.close`),
and moves into the main window when the main window is closed while it
is open. Where the backend has no windows of its own (the web,
kittest), the overlay as before.
