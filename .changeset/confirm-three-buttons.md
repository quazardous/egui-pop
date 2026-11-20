---
egui-pop-api: major
egui-pop-agent: major
egui-pop-editors: major
egui-pop-i18n: major
egui-pop-imaging: major
egui-pop-platform: major
egui-pop-test: major
egui-pop-views: major
egui-pop: major
---

A confirmation takes a second action, for the classic three-button
question ("Save", "Don't save", "Cancel"):
`Confirm::secondary(Secondary::new(verb, command))`, `.danger()` for a
destructive one, drawn in the danger tone and never the default: Enter
takes the primary button, Escape cancels. Its command runs through the
same `dialog.answer` step, answered as the choice of index 1. The
buttons follow the platform's order (`ButtonOrder`: the primary button
last on macOS and GNOME, first on Windows and KDE Plasma), or the one
given with `Confirm::order`; `Confirm::cancel` renames Cancel.
Breaking: `Confirm` has three new public fields (`secondary`, `cancel`,
`order`).
