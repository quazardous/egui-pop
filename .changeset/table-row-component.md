---
egui-pop-tokens: minor
egui-pop-core: minor
egui-pop-views: minor
egui-pop: minor
---

A data table's rows have a kit component of their own, and the table grows what a log needs.

- **`table-row`**: the rows of `DataTable` are drawn by a new kit component (`Kit::table_row`): `selected`, `hover` (never while selected), `focus` for the cursor of a focused table. It is based on `list-row`, so every theme and kit looks as before until it gives `table-row` tokens of its own; a column's tint follows it too. The first cell keeps the room of the row's widest border, so a state's border never moves the content. The Rerun-like kits give Rerun's quiet selection (a faint fill, the strong text, a 2-point marker on the left in the accent) and an outline under the pointer.
- **Rows of several heights**: `TableRows::row_top(row, row_h)` says where each row starts (by default `row * row_h`).
- **The tail**: `DataTable::stick_to_bottom(true)` keeps the last row in view as rows come in, until the user scrolls up, and again once back at the end.
- `TableOutput::visible_rows` gives the rows in view.
