---
egui-pop-api: minor
egui-pop-i18n: minor
---

The file browser gets a filmstrip, `filmstrip`: one virtualised row of
thumbnails under a large view of the focused entry, Left and Right along
the strip, the large view's thumbnail asked first. Quick look opens a
large view of the focused entry over the entries with Ctrl+Y, or Space
when an entry has the focus; the arrows move behind it, and Escape, Space
or Ctrl+Y close it. `ViewModes::built_in()` now has details, thumbnails,
filmstrip and columns; the strings are in English and French.
