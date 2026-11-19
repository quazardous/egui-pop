---
egui-pop-api: minor
egui-pop: minor
---

The in-app picker draws its entries with the file browser's own rows and cells: its list and grid code is gone. The browser's `PlainDrawer` now draws a selectable row whose columns fit its width (name, when it last changed, size, type; facts said to screen readers as the row's description) and a thumbnail cell with the kit's icons and placeholder, every mark (badges and chips) an indicator agents read as `files.badge:<path>:<text>`. The browser's details rows gain the picker's look, and its cells are as wide as their thumbnail. A group's entry carries one chip per kind of member (`+XMP`, `+JPEG`), and `EntryMarks::label` says a shared chip name once. In the picker, a rubber band in the grid selects the files it crosses when choosing several; the grid takes the browser's thumbnails (the next row asked ahead, a group's member standing in). A folder row is named without its trailing `/` for screen readers, as the browser names it.
