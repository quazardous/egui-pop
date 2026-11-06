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
optionally, how a row is drawn (by default, its label). A row is a
[list item](list-items.md): `ListRows::icon` and `ListRows::trailing`
give it an icon before what `ListRows::ui` draws and weak text on its
right.

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

### Rows: their look, their heights, the tail

A table's rows are the kit's `table-row` component: `selected`, `hover`
(never while selected: the selection stays under the pointer) and
`focus` for the cursor of a focused table. It is based on `list-row`, so
a table looks like a list until a kit gives `table-row` tokens of its
own; the Rerun-like kits select quietly, a faint fill and a thin marker
on the left, and outline the row under the pointer. Whatever the state,
the content keeps its place: the first cell keeps the room of the
row's widest border (`Component::border_room`).

`TableRows::row_top(row, row_h)` gives rows of several heights (a row
unfolded to several lines): where each row starts, `0.0` for the first
(by default `row * row_h`). `DataTable::stick_to_bottom(true)` keeps the
last row in view as rows come in, until the user scrolls up, and again
once back at the end. `TableOutput::visible_rows` says which rows were
in view.

## The text log

`egui_pop::views::text_log` is a log view on the data table, after the
Rerun viewer's text log view (`re_view_text_log`, MIT OR Apache-2.0: its
design, none of its code): a row per entry, with its time, entity path,
level and body.

| Part | What it does |
|---|---|
| Time | written in a timeline's scale (`TextLog::time_scale`, or a bound timeline's: `1:02.500`, a timecode, bars and beats), else as a plain number (a frame) |
| Entity | the entity path, tinted where it is the one highlighted elsewhere (`TextLog::highlight_entity`) |
| Level | a chip in the level's colour (`palette.log.*`, six levels from trace to critical) |
| Body | its first line; a multi-line body shows how many lines it hides and unfolds with its arrow (`TextLogState::set_expanded`) |
| Level chips | a toggle chip per level, with how many lines of it match the text (`TextLogState::set_level_shown`) |
| Filter | shows the lines whose body or entity path holds the text, case aside (`TextLogState::set_filter`) |
| Follow | on by default: the last line stays in view as lines come in, until the user scrolls up; scrolled back to the end it follows again (`TextLogOutput::following`) |
| Time cursor | the rows at the latest time not after the cursor are the table's selection, drawn by `table-row`, and come into view when the cursor moves elsewhere; a click or the arrow keys move the cursor to a row's time (`TextLogOutput::cursor_moved`) |

The application owns the entries: `LogSource` (implemented for a slice
of `LogEntry`), in time order. The filter keeps its result between
passes and scans only the entries added at the end, so a log of a
hundred thousand lines costs a screenful of rows a pass;
`TextLogState::refilter` starts it over (the entries changed otherwise,
or the application's own filter, `TextLog::keep`, did). Its strings
(the columns, the chips' names, the count, the disclosure) are
translated.

```rust
use egui_pop::egui;
use egui_pop::views::text_log::{Level, LogEntry, TextLog, TextLogState};
use egui_pop::views::timeline::{TimeScale, TimelineState};

fn show_log(ui: &mut egui::Ui, log: &[LogEntry], state: &mut TextLogState, timeline: &mut TimelineState) {
    // The timeline's playhead is the time cursor: a row picked moves it.
    let out = TextLog::new("robot.log", log)
        .label("Robot log")
        .timeline(timeline)
        .show(ui, state);
    if let Some(entry) = out.picked {
        let _ = &log[entry].entity;
    }
}

let log = vec![
    LogEntry::new(0.0, "/robot", Level::Info, "started"),
    LogEntry::new(1.5, "/robot/arm", Level::Warn, "joint 2 near its limit"),
    LogEntry::new(2.0, "/robot", Level::Critical, "emergency stop\nreason: no answer"),
];
let mut state = TextLogState::new();
let mut timeline = TimelineState::new(TimeScale::Seconds, 10.0);
let ctx = egui::Context::default();
let mut frame = ctx.run_ui(egui::RawInput::default(), |ui| {
    show_log(ui, &log, &mut state, &mut timeline)
});
frame.textures_delta.clear();
assert_eq!(state.shown().len(), 3);
```

The gallery's `text-log` page shows a robot's minute under a timeline,
and its playground streams lines in or swaps in a hundred thousand; the
[Rerun-like viewer](rerun-like.md)'s text log is this view.

## Columns

A table can select columns as well as rows, opt-in with
`DataTable::select_columns(true)`, as in a spreadsheet:

| Gesture | What it does |
|---|---|
| Click a header | Selects its column alone |
| Ctrl+click a header | Adds or removes its column |
| Shift+click a header | Selects the columns from the last one clicked to it (with Ctrl, adds them) |
| Right-click a header | Its menu: Sort ascending, Sort descending, Unsorted, then the application's entries |

A header click then selects instead of sorting: the sort is in the
header's menu, which is there whether columns are selected or not.
Without `select_columns`, a click sorts as before.

Each change runs the `table.select_columns` step; the output's
`columns_selected` is set and `TableState::selected_columns` gives the
columns' ids, in the columns' order (`set_selected_columns` sets them
without a step). A selected column is tinted from its header to its last
row, and so is the column under the pointer, over its header or any of
its cells: the table row's `selected` and `hover` fills at half strength
(`table-row` in the kit), so a selected row shows through.
`TableOutput::hovered_column` names the column under the pointer and
`header_clicked` a header clicked this pass (with or without column
selection). `DataTable::header_menu(|ui, column| …)` adds the
application's entries to every header's menu.

`DataTable::shared_columns(&service)` binds the column selection to the
application-wide selection (`egui_pop_api::selection`): a column is the
item `column:<id>` (`table::column_item`), selected and hovered there
like any other, so a column chosen in a table shows in a panel
listening to the selection, and a column selected elsewhere is tinted
here. A plain click replaces the whole selection, Ctrl changes only the
table's columns.

```rust
use egui_pop::egui;
use egui_pop::views::table::{Column, DataTable, TableRows, TableState};

# struct Rows;
# impl TableRows<u32> for Rows {
#     fn len(&self) -> usize { 0 }
#     fn key(&self, row: usize) -> u32 { row as u32 }
#     fn label(&self, row: usize) -> String { row.to_string() }
#     fn cell(&mut self, _: &mut egui::Ui, _: usize, _: usize, _: bool) {}
# }
fn ui(ui: &mut egui::Ui, state: &mut TableState<u32>, rows: &mut Rows) {
    let columns = [Column::new("time", "Time"), Column::new("speed", "Speed")];
    let out = DataTable::new("samples", &columns)
        .select_columns(true)
        .header_menu(|ui, column| {
            if ui.button(format!("Plot {column}")).clicked() {
                ui.close();
            }
        })
        .show(ui, state, rows);
    if out.columns_selected {
        let _plotted: &[String] = state.selected_columns();
    }
}
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
| `table.select_columns` | `table`, `columns` | sets the column selection (a header clicked, `select_columns`); the output's `columns_selected` is set |

The gallery's `table` page shows 10 000 photos sorted and resized
through the store, cells drawn by the page (stars for the rating), a
veto on sorting by one column, columns selected from their headers
(shared with the application-wide selection), a header menu, and a list
of keywords.
