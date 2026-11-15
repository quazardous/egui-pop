---
egui-pop-core: minor
egui-pop-api: minor
egui-pop-i18n: minor
---

A failure stays in its frame, and the main window never stays empty without a word (`paint_guard`). A wizard stored as a focus screen whose page is not drawn (the Settings app closed under it, its block replaced) no longer leaves the title bar alone: the shell is drawn again, a warning says so once and `window.focus_hole` is published (`AppHandle::focus_screen_hole`). A main window that paints nothing (no text, no image, no overlay; title bar included in zen mode or under a window's content moved in) for longer than `AppHandle::set_empty_window_delay` (2 s by default, `None` turns it off) logs once what holds the screen (`AppHandle::screen_state`: the place stack, the Settings app and its wizard, the overlays, zen mode, the layout's areas), publishes `window.painted_nothing` and shows a fallback page, "Nothing to show here", with "Reset the layout" and "Back to the start"; the page leaves when the window paints again. The first pass of the main window is logged at info with the same state. A panel that panics while drawing is logged with its id and owner and published as `panel.fault` (`fault::boundary_reporting`); its region shows the fault with Retry, as before.
