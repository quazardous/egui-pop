---
egui-pop-api: minor
egui-pop-i18n: minor
---

The file browser gets Finder's column view, `columns`: one column per
folder of the path, from the browser's root (`FileBrowser::with_root`)
down to the place, then the focused folder's entries or the focused
file's thumbnail. Up and Down move in a column, Right enters the focused
folder, Left goes back to the parent, a click in another column goes to
that folder. `ViewModes::folders()` gives the folder chooser's modes,
details and columns; `ViewModes::built_in()` every built-in mode.

A mode that needs the path sets `ModeNeeds::hierarchy`: the browser lists
the place's ancestors and the focused folder, hands them over in
`ModeContext::ancestors` and `ModeContext::peek` (with `ModeContext::place`),
and follows the mode's `ModeOutput::reach` and `ViewMode::travel`.
`BrowserOutput::place_changed` says the place changed. The mode's name is
translated (`egui-pop-files-mode-columns`, in English and French).
