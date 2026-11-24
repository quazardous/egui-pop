---
egui-pop-core: patch
egui-pop-views: patch
egui-pop-api: patch
egui-pop: patch
---

Screen readers read more of egui-pop (found by Orca walking the gallery):
each window is named after its title, so the focus landing on the window
no longer says a bare "frame"; a tree view says it is a tree, a list a list
box and a data table a table (with its rows and columns), not an unknown
node; a table's column resize handles are splitters; and the arrows in a
tree, a list and the command palette now read the row they move to.
