---
egui-pop-api: patch
---

The file browser's carousel follows the thumbnail size control: the
entry in the middle takes that size, made smaller only when its cell
would not fit the room. Its neighbours are spaced by the width each
cell really takes at its own size (`EntryDrawer::cell_size`), so a
drawer whose cells are wider than their thumbnails (a frame, a least
width for a long name) no longer draws the middle cell over them.
