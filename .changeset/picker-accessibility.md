---
egui-pop-api: patch
---

The in-app file picker reads better with a screen reader (`docs/files.md`):
its entries, in the list or the grid, are a list box named by the folder (`Files in Photos`)
(several selectable when choosing several files), each entry an option of it,
the highlighted one its current option. The preview pane is a group named
Preview, and another folder is announced with how many entries it shows.
French included. Tests finding an entry as a `Button` find it as a
`ListBoxOption` now.
