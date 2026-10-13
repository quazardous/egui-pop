# Lists and tables

`egui-pop-views` (the facade's `views` feature, re-exported as
`egui_pop::views`) has a list and a data table, both virtualised (only
the rows in view are built) over rows the application owns and draws.

```sh
cargo add egui-pop --features views
```

Both keep the selection by key, so it survives the rows being sorted,
filtered or changed (rows that are gone leave it), take the keyboard
focus as one widget and run each selection as a step.

| Key | What it does |
|---|---|
| Up, Down, Page Up, Page Down, Home, End | Move the cursor and select; with Shift, extend the selection; with Ctrl, move the cursor alone |
| Space, Ctrl+Space | Select the cursor's row, toggle it |
| Ctrl+A | Select every row (`Mode::Multiple`) |
| Letters | Type-ahead: the next row whose label starts with what was typed in the last second |

A click selects, Ctrl+click toggles, Shift+click extends. Key bindings
of the application come first: a key bound to a command runs the
command.

## The list

The application implements `ListRows`: how many rows, each one's key
and label (its accessible name, what type-ahead searches), and,
optionally, how a row is drawn (by default, its label).

```rust
use egui_pop::egui;
use egui_pop::views::Mode;
use egui_pop::views::list::{ListRows, ListState, ListView, Signal};

struct Keywords(Vec<String>);

impl ListRows<usize> for Keywords {
    fn len(&self) -> usize {
        self.0.len()
    }
    fn key(&self, row: usize) -> usize {
        row
    }
    fn label(&self, row: usize) -> String {
        self.0[row].clone()
    }
    fn ui(&mut self, ui: &mut egui::Ui, row: usize, _selected: bool) {
        ui.label(format!("#{}", self.0[row]));
    }
}

fn keywords(ui: &mut egui::Ui, state: &mut ListState<usize>, rows: &mut Keywords) {
    let out = ListView::new("keywords").label("Keywords").mode(Mode::Multiple).show(ui, state, rows);
    for s in out.signals {
        if let Signal::Select { keys, .. } = s {
            let _ = keys; // Filter the photos by these keywords...
        }
    }
}

let mut state = ListState::new();
let mut rows = Keywords(vec!["beach".into(), "snow".into()]);
let ctx = egui::Context::default();
// An application runs eframe; a bare context drops its textures.
let mut frame = ctx.run_ui(egui::RawInput::default(), |ui| keywords(ui, &mut state, &mut rows));
frame.textures_delta.clear();
```

## The data table

`DataTable` wraps Rerun's `egui_table` (sticky header, resizable
columns, rows in view only) and adds the selection, the keyboard,
sorting from the header and the value store. Columns are declared
with `Column`; the application implements `TableRows` and draws each
cell.

A click on a sortable header sorts up, then down, then not at all. The
table only says so: the application sorts its rows as
`TableState::sort` says, when `TableOutput::sorted` is set. With
`DataTable::store`, the sort (`<prefix>/sort`: `name`, `-name` or empty)
and the widths (`<prefix>/widths`) live in the host's value store, so
they are saved with the settings, read by agents, and set from
anywhere; `table::declare` declares them once.

```rust
use egui_pop::egui;
use egui_pop::model::ids::OwnerId;
use egui_pop::prelude::*;
use egui_pop::views::table::{self, Column, DataTable, Sort, TableRows, TableState};
use egui_pop::views::{Mode, ViewsPlugin};

struct Files(Vec<(u64, String, u64)>);

impl Files {
    fn sort(&mut self, sort: Option<&Sort>) {
        match sort.map(|s| s.column.as_str()) {
            Some("size") => self.0.sort_by_key(|f| f.2),
            _ => self.0.sort_by(|a, b| a.1.cmp(&b.1)),
        }
        if sort.is_some_and(|s| s.descending) {
            self.0.reverse();
        }
    }
}

impl TableRows<u64> for Files {
    fn len(&self) -> usize {
        self.0.len()
    }
    fn key(&self, row: usize) -> u64 {
        self.0[row].0
    }
    fn label(&self, row: usize) -> String {
        self.0[row].1.clone()
    }
    fn cell(&mut self, ui: &mut egui::Ui, row: usize, column: usize, _selected: bool) {
        match column {
            0 => ui.label(&self.0[row].1),
            _ => ui.label(format!("{} KB", self.0[row].2)),
        };
    }
}

fn columns() -> Vec<Column> {
    vec![Column::new("name", "Name").width(200.0), Column::new("size", "Size").width(80.0)]
}

let ctx = egui::Context::default();
let app = App::builder().plugin(ViewsPlugin).build(&ctx).unwrap();
let h = app.handle();
let store = h.store();
table::declare(&store, &OwnerId::FRAMEWORK, "files", &columns()).unwrap();

// Sorted by size, descending, from anywhere (an agent, a saved setting):
store
    .lock()
    .unwrap()
    .set(egui_pop::model::values::Set::new(
        "files/sort",
        egui_pop::model::values::Change::To(egui_pop::model::values::Value::Text("-size".into())),
        Origin::User,
    ))
    .unwrap();

let mut state = TableState::new();
let mut files = Files(vec![(1, "a.txt".into(), 3), (2, "b.txt".into(), 9)]);
let mut frame = ctx.run_ui(egui::RawInput::default(), |ui| {
    let columns = columns();
    let out = DataTable::new("files", &columns)
        .mode(Mode::Multiple)
        .app(h)
        .store(&store, "files")
        .show(ui, &mut state, &mut files);
    let _ = out.sorted;
});
frame.textures_delta.clear();
files.sort(state.sort());
assert_eq!(files.0[0].1, "b.txt");
```

## Steps and signals

Given the application (`.app(…)`) with `ViewsPlugin` connected, each
moment is a step ([ADR 0011](adr/0011-lifecycle-steps.md)) a plugin may
veto, redirect, replace or follow; the selection also comes back as
`Signal::Select` in the output, and a stopped step as `Signal::Vetoed`.

| Step | Arguments | Default |
|---|---|---|
| `list.select` | `list`, `keys`, `cursor` | sets the selection |
| `table.select` | `table`, `keys`, `cursor` | sets the selection |
| `table.sort` | `table`, `column` (`null`: unsorted), `descending` | writes the sort to the store; the output's `sorted` is set |
| `table.resize` | `table`, `widths` | writes the widths to the store (when a resize ends); vetoed, the columns go back |

The gallery's `table` page shows 10 000 photos sorted and resized
through the store, cells drawn by the page (stars for the rating), a
veto on sorting by one column, and a list of keywords.
