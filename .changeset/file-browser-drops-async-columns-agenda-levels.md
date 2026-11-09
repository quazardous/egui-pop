---
egui-pop-api: minor
egui-pop-i18n: patch
---

The file browser takes drops (`FileBrowser::with_drops`): files dragged
inside the app (`DraggedPaths`) or from the system go to a folder entry,
the folder shown or an app's own target, lit while hovering, refused or
accepted by the app, out as `BrowserOutput::dropped`. The columns mode
lists each column off the interface thread (`ColumnListings`, also the
picker's), streams entries with a busy row, and cancels listings the
focus left. The agenda groups by day, month or year (toolbar,
Ctrl+Up/Down), collapses groups (click, Ctrl+Left/Right), and computes
its dates once per listing.
