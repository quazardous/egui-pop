---
egui-pop-agent: patch
egui-pop-test: patch
---

A `ui` action that closes the application window it was sent to (`Escape`, a close button) is answered once the window is gone, with `"window_closed": true`, instead of the application being reported gone; a live scenario's next steps then go to the main window.
