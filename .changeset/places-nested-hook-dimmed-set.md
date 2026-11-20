---
egui-pop-api: minor
egui-pop-i18n: minor
egui-pop: minor
---

`PlacesPanel` draws a library's sources, each part opt-in so that a panel
using none looks as before:

- **Nested items.** `PlaceItem::children` makes a section a tree: rows
  indented by depth under a disclosure toggle named after the item ("Show
  or hide Animals"), open at first with `PlaceItem::open_at_first` or kept
  by the application with `PlaceItem::open`. Right opens and Left folds a
  branch from its row, Up and Down go through the rows shown, each row is
  said at its level and a branch as expanded or not;
  `PlacesOutput::expanded` reports what the user opened or folded.
- **A hook on each row.** `PlacesPanel::on_row` is called with each row's
  `Response`, nested ones included: the application attaches a drop
  target or a drag source through the drag session, or a menu of its own.
- **Dimmed items.** `PlaceItem::dimmed` draws an item muted (a folder of
  an offline volume) and says it is unavailable; it stays choosable, its
  detail still in its tip.
- **A selection in several sections.** `PlacesPanel::selected_set` draws
  every id it is given selected, beside `selected`: a folder, an album and
  a keyword chosen together.
