---
egui-pop-api: minor
---

Three shell hooks for applications that lay out their own workbench:

- `AppBuilder::layout` gives the layout an application starts with when
  none was saved (its panels' areas, tabs and splits, the closed ones,
  the folded areas), repaired as a saved layout is; the `Default`
  arrangement resets to it. Listing the shell's zone panels
  (`PANEL_SIDE_BAR`, `PANEL_MAIN`, `PANEL_INSPECTOR`) among the closed
  ones leaves them out, without setting a layout after `build`.
- `AppBuilder::frameless` draws the areas it names without a frame: no
  margin around the area, and its panels' bodies without margin, border
  or rounded corners, so they run to the window's edges with the
  splitter's line between them (`AppHandle::frameless` reads it).
- The `zones::TOP_BAR_END` zone puts widgets at the top bar's end,
  left to right, before the commands of `MENU_BAR_END`: a document's
  name, a readout. It keeps the width it took, and shows without the
  menu bar too.
