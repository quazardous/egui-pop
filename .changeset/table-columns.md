---
egui-pop-views: major
---

The data table selects columns: `DataTable::select_columns(true)` makes a
header click select its column (Ctrl toggles, Shift extends), through the
new `table.select_columns` step, and moves the sort to the header's menu.
Selected columns and the column under the pointer are tinted from header
to last row. A right click on a header opens its menu (the sort entries,
then the application's, `DataTable::header_menu`). `TableOutput` says
`columns_selected`, `hovered_column` and `header_clicked`;
`TableState::selected_columns` gives the columns. `DataTable::shared_columns`
binds the column selection to the application-wide selection, a column
being the item `column:<id>`.

Breaking: `TableOutput` has new fields and is now `#[non_exhaustive]`
(it is only ever built by the table), so later fields are additive.
