---
egui-pop-api: minor
egui-pop-i18n: minor
---

The file browser gets two modes. `carousel` shows the focused entry large
in the middle and its neighbours smaller on each side; Left and Right, or a
click on a neighbour, bring an entry to the middle. `agenda` files the
entries by day, the newest first, one heading per day (when the entry was
taken, else when it last changed) and the entries without a time last; the
arrows move across the days. `ViewModes::built_in()` now has details,
thumbnails, filmstrip, columns, carousel and agenda, so its switcher is a
menu; the strings are in English and French.
