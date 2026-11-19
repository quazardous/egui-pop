---
egui-pop-api: patch
egui-pop-core: patch
---

Focusing a panel (`workbench.focus`, `<panel>.focus`, `AppHandle::focus_panel`) focuses the panel, no longer its first focusable widget: the panel's own stop holds the keyboard and Tab enters its widgets, as in classic applications. A slider first in a panel was focused that way, so the wheel over it stepped its value instead of scrolling the panel. A panel that wants a widget to take the keyboard names it with `focus::prefer` (a canvas under a toolbar, a search field a "go to the search" key lands in), as before. F6's regions still land on a region's first focusable widget.
