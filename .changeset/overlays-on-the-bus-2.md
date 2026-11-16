---
egui-pop-core: minor
egui-pop-api: patch
egui-pop-views: patch
egui-pop-viewport: patch
---

The last egui popups are on the bus (ADR 0029, slice 6 done): a toolbar's overflow (More tools), a palette group's menu, the breadcrumb's and the status bar's overflow, the viewport's zoom menu, the library filter's add menu, a table column's menu and the file chooser's folder menu each ask their `popup.open` and `popup.close`, and appear in `windows.list` while open. New in the core: `overlay::follow_menu`, following the menu egui opens from a widget.
