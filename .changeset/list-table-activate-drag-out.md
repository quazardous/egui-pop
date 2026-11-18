---
egui-pop-views: major
egui-pop: major
---

A list (`ListView`) and a data table (`DataTable`) activate a row on Enter (the cursor's row) or a double click, signalling `Signal::Activate { key }`, as the thumbnail grid does. Built with the new `drag_out(true)`, a drag starting on a row signals `Signal::DragOut { keys }` (the selection when the row is in it, else that row alone) and shows the carry cursor and the count while it lasts. `TableState::set_sort` and `TableState::set_widths` set the order shown in the header and the columns' widths without a value store, for an order the application decides elsewhere. `table::Signal` names the list's signals. Breaking: `list::Signal` has the new variants `Activate` and `DragOut`, and is now `#[non_exhaustive]`.
