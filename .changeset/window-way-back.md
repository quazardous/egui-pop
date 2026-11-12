---
egui-pop-api: minor
egui-pop: minor
---

A window moved into the main window can go back. When the main window
is closed while another window is open, that window's content moves into
it, as before; now raising the main window (`AppHandle::raise_window`
with `WindowId::main()`, or `window.switch { "window": "main" }`, which
accepts `main` meanwhile) sends that content back to a window of its own
(its id, title, saved geometry and keys) and draws the application's
shell in the main window again, with its title. The journal says
`window.moved` (`to` the window's own id) and `window.opened` (`main`).
`AppHandle::main_window` names the main window's own id whatever it
shows.

`AppHandle::quit` quits the application: the main window closes and the
other windows with it, none moving into the main window. `app.quit`
(File > Quit, Ctrl+Q) uses it, so quitting from another window ends the
application instead of moving that window into the main one.
`AppHandle::quitting` tells whether a quit is under way.
