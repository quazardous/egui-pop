---
egui-pop-api: patch
egui-pop-test: minor
---

Application windows close one way, whatever asks (their close button, their content, `AppHandle::close_window`, `window.close`, the Settings app closing): the window is marked closed, its geometry kept, and the main window is asked for its next pass at once, which stops declaring its viewport so egui's backend destroys the system window. The paint guard now watches every application window, not only the main one: a window whose passes paint nothing under its title bar for longer than the delay logs one error naming it and its state, publishes `window.painted_nothing` (with `window`) and shows the fallback page with a Close button; a pass of a window that is already closed asks for its close and the main window's pass, and an error names it if it keeps running; a closed window the backend still lists past the delay is said (`window.outlived`) and shown hidden for one pass. `MultiWindow::hold_listed` keeps a window listed after it is no longer declared, as a backend that failed to destroy it would.
