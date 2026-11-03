---
egui-pop-api: patch
---

The `overlayOpen` context key is true while an egui popup is open (a
menu, a combo box's list), not only while egui-pop's overlay stack holds
something. Tab with a menu open and nothing focused no longer hides the
floating palettes behind it, and Escape there closes the menu rather
than leave zen mode or pop a pushed mode.
