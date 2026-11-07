---
egui-pop-core: minor
egui-pop-api: patch
egui-pop: minor
---

A dialog's footer stays whole in the window. The in-app file chooser
laid its toolbar, list and footer one after the other in a room of a
set size, so a short window (or an empty folder's "nothing matches"
line) pushed its buttons below the dialog's edge; its footer now keeps
its whole height at the bottom and what is above it scrolls when the
room runs short. A custom dialog's body is held to what the window
leaves under its header and above its buttons, and the settings form to
the modal's room. `overlay::dialog_chrome` is what a dialog's frame
takes around its content (its padding and its widest border of any
state, which a resizable modal did not count), and
`overlay::DialogLayout` lays a dialog's content out footer first, for
an application's own dialogs.
