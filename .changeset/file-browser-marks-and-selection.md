---
egui-pop-api: minor
---

The file browser marks entries as the application sees them
(`FileBrowser::with_marks`): `EntryMarks` gives badges, drawn on a
cell's thumbnail and after a row's name and said with the name to
assistive technology, and dims an entry (a photo already imported).
`FileBrowser::set_selection` sets the selection from the application
(an import's "select this day"). `ModeContext` and `PlainDrawer` take
the marks (`marks`).
