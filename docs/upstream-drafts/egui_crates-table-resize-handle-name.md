# egui_table: column resize handles have no accessible name

> **Before submitting (delete this block).** Repository: rerun-io/egui_crates
> (egui_table moved there; rerun-io/egui_table is archived). No issue
> template, no CONTRIBUTING and no AI policy found in that repository; its
> PR template asks for small, focused PRs, opened as drafts until
> self-reviewed and passing CI. No duplicate found (searched issues and PRs
> for accessibility / accesskit / resize in egui_crates and egui_table,
> 2026-10-20). Checked against `main` at 24310aa.

The column resize handles are interactive widgets (`Sense::click_and_drag`)
with no widget info, so AccessKit exposes them as unnamed nodes with no
role. A screen reader announces nothing useful, and a kittest test cannot
find a handle by label:
<https://github.com/rerun-io/egui_crates/blob/24310aab9dbcc5bb2b589944c78f9a2b2747bf1e/crates/egui_table/src/table.rs#L910-L919>

egui's own resize handles (panels, windows) report `Role::Splitter` with a
name since egui 0.37 (`containers/panel.rs`, `containers/window.rs`).

Found while building a desktop framework on egui
(<https://github.com/quazardous/egui-pop>), whose data table is built on
egui_table.

**Reproduce**

```rust
use egui_kittest::{kittest::Queryable as _, Harness};

struct Delegate;
impl egui_table::TableDelegate for Delegate {
    fn header_cell_ui(&mut self, ui: &mut egui::Ui, cell: &egui_table::HeaderCellInfo) {
        ui.label(format!("Column {}", cell.col_range.start));
    }
    fn cell_ui(&mut self, ui: &mut egui::Ui, cell: &egui_table::CellInfo) {
        ui.label(format!("{}", cell.row_nr));
    }
}

#[test]
fn resize_handles_are_named() {
    let harness = Harness::new_ui(|ui| {
        egui_table::Table::new()
            .num_rows(3)
            .columns(vec![egui_table::Column::new(100.0).resizable(true); 2])
            .headers([egui_table::HeaderRow::new(20.0)])
            .show(ui, &mut Delegate);
    });
    // Fails: no node with role Splitter (the handles have no role or name).
    harness.get_by_role(egui::accesskit::Role::Splitter);
}
```

**Expected**

Each handle is a `Role::Splitter` node with a name, for example "Resize
column 2" by default.

**Suggested fix**

Right after the `ui.interact` call:

```rust
resize_response.widget_info(|| {
    egui::WidgetInfo::labeled(
        egui::accesskit::Role::Splitter,
        ui.is_enabled(),
        format!("Resize column {}", col_nr + 1),
    )
});
```

and, if the delegate should name it after the column's title, an optional
`TableDelegate::column_resize_name(&self, col_nr: usize) -> Option<String>`
used before the default.
