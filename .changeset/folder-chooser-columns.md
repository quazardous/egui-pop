---
egui-pop-api: major
egui-pop: major
---

The in-app folder chooser offers Finder's column view: its mode switcher
has `details` and `columns` (and `thumbnails` with previews). The path's
folders stand side by side, the highlighted folder's subfolders after
them; Right goes into a folder, Left back to the parent, a click in
another column goes there, and the folder selected (else the folder
shown) is the answer. `PickerView` gets `Columns` and is now
`#[non_exhaustive]`; `FilePicker::views` lists the views a picker offers.
