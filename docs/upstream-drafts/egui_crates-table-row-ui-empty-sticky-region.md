# egui_table: `TableDelegate::row_ui` is called for the empty sticky region

> **Before submitting (delete this block).** Repository: rerun-io/egui_crates
> (egui_table moved there; rerun-io/egui_table is archived). No issue
> template, no CONTRIBUTING and no AI policy found in that repository; its
> PR template asks for small, focused PRs, opened as drafts until
> self-reviewed and passing CI. No duplicate found (searched issues and PRs
> for row_ui / header_cell_ui / sticky in egui_crates and egui_table,
> 2026-10-20). Checked against `main` at 24310aa. In egui_table 0.10,
> `header_cell_ui` had the same problem; on `main` the header is drawn once
> (`top_ui`), so only `row_ui` is left.

With no sticky columns (`num_sticky_cols(0)`, the default), `SplitScroll`
still runs the sticky region (`left_bottom_ui`, unconditionally in
`split_scroll.rs`), and `region_ui` still calls
`TableDelegate::row_ui` for every visible row there, with an empty column
range:

- `left_bottom_ui`: <https://github.com/rerun-io/egui_crates/blob/24310aab9dbcc5bb2b589944c78f9a2b2747bf1e/crates/egui_table/src/table.rs#L875-L877>
- the column range becomes `0..0` but the row range does not:
  <https://github.com/rerun-io/egui_crates/blob/24310aab9dbcc5bb2b589944c78f9a2b2747bf1e/crates/egui_table/src/table.rs#L700-L720>
- `row_ui` is called with a row rect spanning all the columns:
  <https://github.com/rerun-io/egui_crates/blob/24310aab9dbcc5bb2b589944c78f9a2b2747bf1e/crates/egui_table/src/table.rs#L751-L769>

The doc says `row_ui` "might be called multiple times per row (e.g. for
sticky and non-sticky columns)", which is fine when both regions have
columns. For the empty one, though, the delegate runs a second time with a
full-width `max_rect` inside a zero-width clip rect: a row that makes itself
clickable (`ui.interact(ui.max_rect(), ...)` for selection) registers a
second widget, and a second AccessKit node with the full row's bounds, for
every row.

Found while building a desktop framework on egui
(<https://github.com/quazardous/egui-pop>), whose data table is built on
egui_table.

**Reproduce**

```rust
struct Delegate {
    row_ui_calls: usize,
}
impl egui_table::TableDelegate for Delegate {
    fn header_cell_ui(&mut self, _ui: &mut egui::Ui, _cell: &egui_table::HeaderCellInfo) {}
    fn cell_ui(&mut self, _ui: &mut egui::Ui, _cell: &egui_table::CellInfo) {}
    fn row_ui(&mut self, _ui: &mut egui::Ui, _row_nr: u64) {
        self.row_ui_calls += 1;
    }
}

#[test]
fn row_ui_once_per_row_without_sticky_columns() {
    let mut delegate = Delegate { row_ui_calls: 0 };
    let ctx = egui::Context::default();
    let _ = ctx.run_ui(Default::default(), |ui| {
        egui_table::Table::new()
            .num_rows(3)
            .columns(vec![egui_table::Column::new(100.0); 2])
            .show(ui, &mut delegate);
    });
    // Fails: row_ui also runs once per row in the empty sticky region.
    assert_eq!(delegate.row_ui_calls, 3);
}
```

**Expected**

`row_ui` is called once per visible row and per region that has columns.

**Suggested fix**

Skip the region when it has no columns, at the top of `region_ui`:

```rust
if region_cols.is_empty() {
    return;
}
```

(or in `left_bottom_ui` only, if the scrolling region must keep running for
a table with only sticky columns, for its prefetch).
