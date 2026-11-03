---
egui-pop-api: minor
---

More keys in the in-app file picker (`docs/files.md`): `Alt+Left` and
`Alt+Right` go back and forward through the folders shown, `Ctrl+H` shows or
hides hidden files, `F5` reads the folder again, and `Alt+P` is now a command
too. All are commands of the keymap (`file.back`, `file.forward`,
`file.toggle_hidden`, `file.reload`, `file.toggle_preview`), so they can be
rebound, and the check boxes' tips say their keys. `FilePicker::go_back`,
`go_forward` and `reload` do the same for a picker drawn on its own. `Back` and `Forward`
hold in a modal (the picker's, a dialog's), where the context stack's
`nav.back` (also `Alt+Left`) no longer applies (`!modalOpen`).
