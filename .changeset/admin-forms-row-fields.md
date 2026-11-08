---
egui-pop-model: minor
egui-pop-api: minor
---

Administration app, slice 5. Row fields (`RowField`): a record's value
as a toggle, a text or a choice in a list or table, written through a
command naming the row and the value (`${value}`). Actions may declare a
confirmation (`Confirmation`), asked with the dialogs' Confirm before
their command runs; a `confirm` hook is asked too. Meter columns
(`CellKind::Meter`). Row clauses read the record (`row.<key>`), an object
answer is one record, and a deep link marks and scrolls to a record.
