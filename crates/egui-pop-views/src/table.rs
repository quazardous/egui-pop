//! A data table: virtualised rows, a sticky header, columns sorted and
//! resized, cells drawn by the application.
//!
//! It wraps Rerun's `egui_table` (MIT OR Apache-2.0), which draws the
//! rows in view, the header and the resize handles, and adds what an
//! egui-pop view has: a selection by key with the keyboard and the
//! pointer, sorting from the header, and the sort and the widths kept in
//! the host's value store (declare them once with [`declare`]), every
//! change a step (`table.select`, `table.sort`, `table.resize`).
//!
//! Columns can be selected too ([`DataTable::select_columns`]): a header
//! click selects its column (Ctrl toggles it, Shift extends), the column
//! is tinted from its header to its last row, and the sort moves to the
//! header's context menu, as in a spreadsheet. The column under the
//! pointer is tinted as well and said in the output
//! ([`TableOutput::hovered_column`]). A right click on a header opens its
//! menu: the sort entries, then the application's
//! ([`DataTable::header_menu`]).
//!
//! The application owns the rows: it sorts them as [`TableState::sort`]
//! (or the store) says and gives them through [`TableRows`].
//!
//! ```
//! use egui_pop_views::table::{Column, DataTable, TableRows, TableState};
//!
//! struct Files(Vec<(u64, String, u64)>);
//!
//! impl TableRows<u64> for Files {
//!     fn len(&self) -> usize { self.0.len() }
//!     fn key(&self, row: usize) -> u64 { self.0[row].0 }
//!     fn label(&self, row: usize) -> String { self.0[row].1.clone() }
//!     fn cell(&mut self, ui: &mut egui::Ui, row: usize, column: usize, _selected: bool) {
//!         match column {
//!             0 => { ui.label(&self.0[row].1); }
//!             _ => { ui.label(format!("{} KB", self.0[row].2)); }
//!         }
//!     }
//! }
//!
//! fn ui(ui: &mut egui::Ui, state: &mut TableState<u64>, files: &mut Files) {
//!     let columns = [Column::new("name", "Name").width(200.0), Column::new("size", "Size")];
//!     let out = DataTable::new("files", &columns).show(ui, state, files);
//!     if out.sorted {
//!         // Sort the rows as `state.sort()` says.
//!     }
//! }
//! ```

use egui::{Id, Response, Role, Sense, Ui, WidgetInfo};
use egui_pop_api::AppHandle;
use egui_pop_api::selection::{SelectionItem, SelectionService};
use egui_pop_api::steps::Outcome;
use egui_pop_core::binding::SharedStore;
use egui_pop_core::kit::{Kit, KitCx, Part, State};
use egui_pop_core::strings::{tr, tr_args};
use egui_pop_core::widgets::SortHeader;
use egui_pop_model::commands::Origin;
use egui_pop_model::ids::OwnerId;
use egui_pop_model::values::{Change, DeclareError, Descriptor, Set, Value};
use egui_table::{CellInfo, HeaderCellInfo, HeaderRow, PrefetchInfo, TableDelegate};
use serde_json::{Value as Json, json};

use crate::list::{self, Signal};
use crate::nav::{self, TypeAhead};
use crate::rows;
use crate::select::{Key, Mode, Proposal, Selection};
use crate::steps::{self, TABLE_RESIZE, TABLE_SELECT, TABLE_SELECT_COLUMNS, TABLE_SORT};
use crate::strings as s;

/// A column as declared.
#[derive(Clone, Debug, PartialEq)]
pub struct Column {
    /// Its id in steps, signals and the store.
    pub id: String,
    pub title: String,
    /// Its width until resized.
    pub width: f32,
    pub min_width: f32,
    pub sortable: bool,
    pub resizable: bool,
}

impl Column {
    pub fn new(id: impl Into<String>, title: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            title: title.into(),
            width: 120.0,
            min_width: 32.0,
            sortable: true,
            resizable: true,
        }
    }

    pub fn width(mut self, width: f32) -> Self {
        self.width = width;
        self
    }

    pub fn min_width(mut self, width: f32) -> Self {
        self.min_width = width;
        self
    }

    pub fn sortable(mut self, on: bool) -> Self {
        self.sortable = on;
        self
    }

    pub fn resizable(mut self, on: bool) -> Self {
        self.resizable = on;
        self
    }
}

/// A table's order: by a column, up or down.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Sort {
    pub column: String,
    pub descending: bool,
}

impl Sort {
    /// As the store keeps it: `name`, `-name` (descending), empty
    /// (unsorted).
    pub fn encode(sort: Option<&Sort>) -> String {
        match sort {
            Some(s) if s.descending => format!("-{}", s.column),
            Some(s) => s.column.clone(),
            None => String::new(),
        }
    }

    pub fn decode(text: &str) -> Option<Sort> {
        match text.strip_prefix('-') {
            _ if text.is_empty() => None,
            Some(c) => Some(Sort {
                column: c.into(),
                descending: true,
            }),
            None => Some(Sort {
                column: text.into(),
                descending: false,
            }),
        }
    }

    /// What a click on `column`'s header asks for: up, then down, then
    /// unsorted.
    pub fn next(current: Option<&Sort>, column: &str) -> Option<Sort> {
        match current {
            Some(s) if s.column == column && !s.descending => Some(Sort {
                column: column.into(),
                descending: true,
            }),
            Some(s) if s.column == column => None,
            _ => Some(Sort {
                column: column.into(),
                descending: false,
            }),
        }
    }
}

/// Declares a table's values in the store: `<prefix>/sort` (text:
/// `name`, `-name` or empty) and `<prefix>/widths` (one per column).
/// Call it once, from the plugin owning the table.
pub fn declare(
    store: &SharedStore,
    owner: &OwnerId,
    prefix: &str,
    columns: &[Column],
) -> Result<(), DeclareError> {
    let mut s = egui_pop_core::lock(store);
    s.declare(
        owner,
        &format!("{prefix}/sort"),
        Descriptor::text("Sort", ""),
    )?;
    let widths: Vec<f64> = columns.iter().map(|c| f64::from(c.width)).collect();
    s.declare(
        owner,
        &format!("{prefix}/widths"),
        Descriptor::floats("Column widths", columns.len() as u32, 8.0, 4000.0, &widths),
    )
}

/// The rows a table shows; the application owns them, sorted as the
/// table's sort says.
pub trait TableRows<K: Key> {
    fn len(&self) -> usize;

    fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Row `row`'s key.
    fn key(&self, row: usize) -> K;

    /// Row `row`'s accessible name, searched by type-ahead.
    fn label(&self, row: usize) -> String;

    /// Draws a cell (its row's background is drawn). Its `Ui` truncates
    /// text: a label too long for the cell is elided with "…" and shows
    /// the full text on hover.
    fn cell(&mut self, ui: &mut Ui, row: usize, column: usize, selected: bool);

    /// Where row `row` starts, from the top of the first row, rows being
    /// `row_h` high unless the application says otherwise: rows of
    /// several heights (a row expanded to several lines) give their
    /// offsets here, `0.0` for row 0, and [`TableRows::len`] gives the
    /// height of them all. Called for the rows in view and a binary
    /// search over the rows, so keep it cheap.
    fn row_top(&self, row: usize, row_h: f32) -> f32 {
        row as f32 * row_h
    }
}

/// A table's state between passes.
#[derive(Debug)]
pub struct TableState<K: Key> {
    selection: Selection<K>,
    type_ahead: TypeAhead,
    sort: Option<Sort>,
    widths: Vec<f32>,
    /// The widths last given to `egui_table`.
    given: Option<Vec<f32>>,
    reveal: Option<usize>,
    page: usize,
    len: Option<usize>,
    /// The selected columns' ids, in the columns' order.
    columns: Vec<String>,
    /// The column a Shift+click extends from.
    column_anchor: Option<String>,
    /// The column under the pointer at the last pass, tinted at this one.
    hovered_column: Option<String>,
    /// The shared selection's revision last followed (columns).
    seen: Option<u64>,
}

impl<K: Key> Default for TableState<K> {
    fn default() -> Self {
        Self {
            selection: Selection::default(),
            type_ahead: TypeAhead::default(),
            sort: None,
            widths: Vec::new(),
            given: None,
            reveal: None,
            page: 10,
            len: None,
            columns: Vec::new(),
            column_anchor: None,
            hovered_column: None,
            seen: None,
        }
    }
}

impl<K: Key> TableState<K> {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn selection(&self) -> &Selection<K> {
        &self.selection
    }

    /// Sets the selection without running a step.
    pub fn set_selection(&mut self, keys: impl IntoIterator<Item = K>, cursor: Option<K>) {
        self.selection.set(keys, cursor);
    }

    /// The order the application sorts its rows in.
    pub fn sort(&self) -> Option<&Sort> {
        self.sort.as_ref()
    }

    /// The columns' widths as last shown.
    pub fn widths(&self) -> &[f32] {
        &self.widths
    }

    /// The selected columns' ids ([`DataTable::select_columns`]).
    pub fn selected_columns(&self) -> &[String] {
        &self.columns
    }

    /// Sets the selected columns without running a step.
    pub fn set_selected_columns(&mut self, columns: impl IntoIterator<Item = String>) {
        self.columns = columns.into_iter().collect();
    }

    /// The column under the pointer at the last pass, over its header or
    /// one of its cells.
    pub fn hovered_column(&self) -> Option<&str> {
        self.hovered_column.as_deref()
    }

    /// Brings a row into view at the next pass.
    pub fn reveal(&mut self, row: usize) {
        self.reveal = Some(row);
    }
}

/// What [`DataTable::show`] returns.
#[derive(Debug)]
#[non_exhaustive]
pub struct TableOutput<K> {
    /// The table as one focusable widget.
    pub response: Response,
    /// Selection changes and vetoes.
    pub signals: Vec<Signal<K>>,
    /// The sort changed this pass: sort the rows again.
    pub sorted: bool,
    /// The widths changed this pass (a resize ended).
    pub resized: bool,
    /// The column selection changed this pass
    /// ([`TableState::selected_columns`]).
    pub columns_selected: bool,
    /// The column under the pointer, over its header or one of its cells.
    pub hovered_column: Option<String>,
    /// The column whose header was clicked this pass (a primary click).
    pub header_clicked: Option<String>,
    /// The rows in view this pass (empty before the table's first pass).
    pub visible_rows: std::ops::Range<usize>,
}

/// What a header's context menu adds after the sort entries, given the
/// menu's `Ui` and the column's id.
type MenuFn<'a> = dyn FnMut(&mut Ui, &str) + 'a;

/// [`DataTable::header_menu`]'s closure.
struct HeaderMenu<'a>(Box<MenuFn<'a>>);

impl std::fmt::Debug for HeaderMenu<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("HeaderMenu")
    }
}

/// A column's item in the application-wide selection: `column:<id>`.
pub fn column_item(column: &str) -> SelectionItem {
    SelectionItem::new(format!("column:{column}"))
}

/// The data table widget.
#[derive(Debug)]
pub struct DataTable<'a> {
    id: String,
    label: Option<String>,
    columns: &'a [Column],
    mode: Mode,
    app: Option<&'a AppHandle>,
    store: Option<(&'a SharedStore, String)>,
    height: Option<f32>,
    row_height: Option<f32>,
    select_columns: bool,
    header_menu: Option<HeaderMenu<'a>>,
    shared_columns: Option<&'a SelectionService>,
    stick_to_bottom: bool,
}

impl<'a> DataTable<'a> {
    /// A table named `id` in steps and signals.
    pub fn new(id: impl Into<String>, columns: &'a [Column]) -> Self {
        Self {
            id: id.into(),
            label: None,
            columns,
            mode: Mode::Single,
            app: None,
            store: None,
            height: None,
            row_height: None,
            select_columns: false,
            header_menu: None,
            shared_columns: None,
            stick_to_bottom: false,
        }
    }

    /// Keeps the last row in view as rows are added, until the user
    /// scrolls up; scrolled back to the end, it follows again (a log's
    /// tail). Off by default.
    pub fn stick_to_bottom(mut self, on: bool) -> Self {
        self.stick_to_bottom = on;
        self
    }

    /// Lets a header click select its column (Ctrl+click toggles it,
    /// Shift+click extends from the last one clicked), through the
    /// `table.select_columns` step; the selected columns are tinted from
    /// header to last row. The sort is then in the header's context menu
    /// (a click no longer sorts), as in a spreadsheet. Off by default: a
    /// click sorts.
    pub fn select_columns(mut self, on: bool) -> Self {
        self.select_columns = on;
        self
    }

    /// Adds entries to a header's context menu (a right click), after the
    /// sort entries: `add(ui, column_id)`.
    pub fn header_menu(mut self, add: impl FnMut(&mut Ui, &str) + 'a) -> Self {
        self.header_menu = Some(HeaderMenu(Box::new(add)));
        self
    }

    /// Binds the column selection to the application-wide selection
    /// (`egui_pop_api::selection`), its items `column:<id>`
    /// ([`column_item`]): the columns selected or hovered anywhere are
    /// tinted, and what is selected here goes there (Ctrl changes only
    /// this table's columns). Needs [`DataTable::select_columns`] to
    /// select from the headers.
    pub fn shared_columns(mut self, selection: &'a SelectionService) -> Self {
        self.shared_columns = Some(selection);
        self
    }

    /// Its accessible name.
    pub fn label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }

    pub fn mode(mut self, mode: Mode) -> Self {
        self.mode = mode;
        self
    }

    /// Runs its moments as steps of this application.
    pub fn app(mut self, app: &'a AppHandle) -> Self {
        self.app = Some(app);
        self
    }

    /// Keeps the sort and the widths in `store` under `prefix` (see
    /// [`declare`]).
    pub fn store(mut self, store: &'a SharedStore, prefix: impl Into<String>) -> Self {
        self.store = Some((store, prefix.into()));
        self
    }

    /// Its height (default: what is available).
    pub fn height(mut self, height: f32) -> Self {
        self.height = Some(height);
        self
    }

    /// Each row's height (default: the theme's interactive height).
    pub fn row_height(mut self, height: f32) -> Self {
        self.row_height = Some(height);
        self
    }

    fn read_store<K: Key>(&self, state: &mut TableState<K>) {
        let n = self.columns.len();
        if state.widths.len() != n {
            state.widths = self.columns.iter().map(|c| c.width).collect();
        }
        let Some((store, prefix)) = &self.store else {
            return;
        };
        let s = egui_pop_core::lock(store);
        if let Some(Value::Text(t)) = s.get(&format!("{prefix}/sort")).map(|e| &e.value) {
            state.sort = Sort::decode(t);
        }
        if let Some(Value::Floats(w)) = s.get(&format!("{prefix}/widths")).map(|e| &e.value)
            && w.len() == n
        {
            state.widths = w.iter().map(|&w| w as f32).collect();
        }
    }

    fn write_store(&self, path: &str, value: Value) -> Result<(), String> {
        let Some((store, prefix)) = &self.store else {
            return Ok(());
        };
        egui_pop_core::lock(store)
            .set(
                // A person sorted or resized: the framework writes it.
                Set::new(&format!("{prefix}/{path}"), Change::To(value), Origin::User)
                    .by(&OwnerId::FRAMEWORK)
                    .untracked(),
            )
            .map(|_| ())
            .map_err(|e| format!("{e:?}"))
    }

    pub fn show<K: Key>(
        mut self,
        ui: &mut Ui,
        state: &mut TableState<K>,
        rows: &mut dyn TableRows<K>,
    ) -> TableOutput<K> {
        let id = Id::unique(("egui_pop_views::table", &self.id));
        self.read_store(state);
        state
            .columns
            .retain(|c| self.columns.iter().any(|col| &col.id == c));
        if let Some(service) = self.shared_columns
            && state.seen != Some(service.revision())
        {
            state.seen = Some(service.revision());
            let items: std::collections::HashSet<SelectionItem> =
                service.items().into_iter().collect();
            state.columns = self
                .columns
                .iter()
                .filter(|c| items.contains(&column_item(&c.id)))
                .map(|c| c.id.clone())
                .collect();
        }
        let shared_hover = self.shared_columns.and_then(|service| {
            self.columns
                .iter()
                .find(|c| service.is_hovered(&column_item(&c.id)))
                .map(|c| c.id.clone())
        });
        let len = rows.len();
        list::retain(&mut state.selection, &mut state.len, len, &|i| rows.key(i));
        let row_h = self.row_height.unwrap_or_else(|| rows::height(ui));
        let focused = ui.memory(|m| m.has_focus(id));
        let mut proposals = Vec::new();
        if focused {
            let (p, reveal) = nav::flat(
                ui,
                &state.selection,
                self.mode,
                len,
                &|i| rows.key(i),
                &|i| rows.label(i),
                state.page,
                &mut state.type_ahead,
            );
            proposals = p;
            if reveal.is_some() {
                state.reveal = reveal;
            }
        }

        let columns: Vec<egui_table::Column> = self
            .columns
            .iter()
            .zip(&state.widths)
            .map(|(c, w)| {
                egui_table::Column::new(*w)
                    .range(c.min_width..=f32::INFINITY)
                    .resizable(c.resizable)
                    .id(Id::unique(("column", &c.id)))
            })
            .collect();
        let mut table = egui_table::Table::new()
            .id_salt(id)
            .num_rows(len as u64)
            .columns(columns.clone())
            .headers([HeaderRow::new(row_h + 4.0)])
            .auto_size_mode(egui_table::AutoSizeMode::Never)
            .stick_to_bottom(self.stick_to_bottom);
        if let Some(r) = state.reveal.take() {
            table = table.scroll_to_row(r as u64, None);
        }
        // The widths the store holds go to `egui_table`'s own state, when
        // they changed (not during a resize).
        let mut delegate = Delegate {
            rows,
            columns: self.columns,
            selection: &state.selection,
            sort: state.sort.as_ref(),
            mode: self.mode,
            len,
            focused,
            proposals: &mut proposals,
            sort_to: None,
            page: state.page,
            take_focus: false,
            row_h,
            kit: Kit::of(ui.ctx()),
            select_columns: self.select_columns,
            selected_columns: &state.columns,
            tinted: state.hovered_column.clone().or(shared_hover),
            hovered_column: None,
            header_clicked: None,
            column_click: None,
            header_menu: self.header_menu.take(),
            visible_rows: 0..0,
        };
        let widths = &state.widths;
        let given = &mut state.given;
        let draw = |ui: &mut Ui| {
            // `egui_table` keeps its state under the drawing ui's id.
            let table_id = table.get_id(ui);
            if given.as_ref() != Some(widths) {
                let mut ts = egui_table::TableState::load(ui.ctx(), table_id).unwrap_or_default();
                for (i, c) in columns.iter().enumerate() {
                    ts.col_widths.insert(c.id_for(i), widths[i]);
                }
                ts.parent_width = Some(ui.available_width());
                ts.store(ui.ctx(), table_id);
                *given = Some(widths.clone());
            }
            (table.show(ui, &mut delegate), table_id)
        };
        let (inner, table_id) = match self.height {
            Some(h) => {
                ui.allocate_ui(egui::vec2(ui.available_width(), h), |ui| {
                    ui.set_min_height(h);
                    draw(ui)
                })
                .inner
            }
            None => draw(ui),
        };
        // upstream: egui_table says nothing of its scroll area, so what overflows
        // comes from the sizes it was given.
        let wide = state.widths.iter().sum::<f32>();
        let tall = delegate.rows.row_top(len, row_h) + row_h + 4.0;
        egui_pop_core::ui::contain_wheel_in(
            ui,
            inner.rect,
            [
                wide > inner.rect.width() + 0.5,
                tall > inner.rect.height() + 0.5,
            ],
        );
        let take_focus = delegate.take_focus;
        let sort_to = delegate.sort_to.take();
        let hovered_column = delegate.hovered_column.take();
        let header_clicked = delegate.header_clicked.take();
        let column_click = delegate.column_click.take();
        let visible_rows = delegate.visible_rows.clone();
        let page = delegate.page;
        drop(delegate);
        if hovered_column != state.hovered_column {
            state.hovered_column = hovered_column.clone();
            ui.ctx().request_repaint();
        }
        if let Some(service) = self.shared_columns {
            service.hover(
                &self.id,
                hovered_column.as_deref().map(column_item).as_ref(),
            );
        }
        state.page = page;
        let label = self
            .label
            .clone()
            .unwrap_or_else(|| tr(ui.ctx(), &s::TABLE));
        let response = ui.interact(inner.rect, id, Sense::focusable_noninteractive());
        egui_pop_core::a11y::name(&response, Role::Unknown, &label);
        if response.has_focus() {
            // Its cursor row shows it.
            egui_pop_core::focus::own_ring(ui.ctx(), id);
            ui.memory_mut(|m| {
                m.set_focus_lock_filter(
                    id,
                    egui::EventFilter {
                        horizontal_arrows: true,
                        vertical_arrows: true,
                        ..Default::default()
                    },
                )
            });
        }
        if take_focus {
            response.request_focus();
        }
        // upstream: egui_table's resize handles have no name (see
        // docs/upstream.md).
        for (i, c) in columns.iter().enumerate() {
            if let Some(r) = ui
                .ctx()
                .read_response(table_id.with(c.id_for(i)).with("resize"))
            {
                let name = format!("{} ↔", self.columns[i].title);
                r.widget_info(move || WidgetInfo::labeled(Role::Unknown, true, &name));
            }
        }

        let mut signals = list::run(
            self.app,
            &TABLE_SELECT,
            "table",
            &self.id,
            &mut state.selection,
            proposals,
        );
        let mut sorted = false;
        if let Some(to) = sort_to {
            let mut done = to.clone();
            let outcome = steps::run(
                self.app,
                &TABLE_SORT,
                json!({
                    "table": self.id,
                    "column": to.as_ref().map(|s| &s.column),
                    "descending": to.as_ref().is_some_and(|s| s.descending),
                }),
                |args| {
                    done = args["column"].as_str().map(|c| Sort {
                        column: c.into(),
                        descending: args["descending"].as_bool().unwrap_or(false),
                    });
                    self.write_store("sort", Value::Text(Sort::encode(done.as_ref())))?;
                    Ok(Json::String(Sort::encode(done.as_ref())))
                },
            );
            match outcome {
                Outcome::Done(Ok(_)) => {
                    state.sort = done;
                    sorted = true;
                }
                Outcome::Done(Err(reason)) | Outcome::Vetoed { reason, .. } => {
                    signals.push(Signal::Vetoed {
                        step: TABLE_SORT,
                        reason,
                    })
                }
            }
        }

        // A resize ends: its widths go through the step to the store.
        let mut resized = false;
        let now: Vec<f32> = egui_table::TableState::load(ui.ctx(), table_id)
            .map(|ts| {
                columns
                    .iter()
                    .enumerate()
                    .map(|(i, c)| {
                        ts.col_widths
                            .get(&c.id_for(i))
                            .copied()
                            .unwrap_or(c.current)
                    })
                    .collect()
            })
            .unwrap_or_else(|| state.widths.clone());
        let dragging = ui.input(|i| i.pointer.any_down());
        if !dragging && now != state.widths {
            let mut done = now.clone();
            let outcome = steps::run(
                self.app,
                &TABLE_RESIZE,
                json!({ "table": self.id, "widths": now }),
                |args| {
                    done = serde_json::from_value(args["widths"].clone())
                        .map_err(|e| e.to_string())?;
                    if done.len() != self.columns.len() {
                        return Err("one width per column".into());
                    }
                    self.write_store(
                        "widths",
                        Value::Floats(done.iter().map(|&w| f64::from(w)).collect()),
                    )?;
                    Ok(json!(done))
                },
            );
            match outcome {
                Outcome::Done(Ok(_)) => {
                    state.widths = done;
                    resized = true;
                }
                Outcome::Done(Err(reason)) | Outcome::Vetoed { reason, .. } => {
                    signals.push(Signal::Vetoed {
                        step: TABLE_RESIZE,
                        reason,
                    })
                }
            }
            // Given again: what was decided, not what was dragged.
            state.given = None;
        }
        // A header click selecting columns.
        let mut columns_selected = false;
        if let Some((column, mods)) = column_click {
            let before = state.columns.clone();
            let to = column_selection(
                self.columns,
                &state.columns,
                state.column_anchor.as_deref(),
                &column,
                mods,
            );
            let mut done = to.clone();
            let outcome = steps::run(
                self.app,
                &TABLE_SELECT_COLUMNS,
                json!({ "table": self.id, "columns": to }),
                |args| {
                    done = serde_json::from_value(args["columns"].clone())
                        .map_err(|e| e.to_string())?;
                    Ok(json!(done))
                },
            );
            match outcome {
                Outcome::Done(Ok(_)) => {
                    if !mods.shift {
                        state.column_anchor = Some(column);
                    }
                    if done != before {
                        state.columns = done;
                        columns_selected = true;
                        if let Some(service) = self.shared_columns {
                            let item_of = |c: &String| column_item(c);
                            crate::shared::write(
                                service,
                                &self.id,
                                &before,
                                &state.columns,
                                mods.command,
                                &item_of,
                            );
                            state.seen = Some(service.revision());
                        }
                    }
                }
                Outcome::Done(Err(reason)) | Outcome::Vetoed { reason, .. } => {
                    signals.push(Signal::Vetoed {
                        step: TABLE_SELECT_COLUMNS,
                        reason,
                    })
                }
            }
        }
        if !signals.is_empty() || sorted || resized || columns_selected {
            ui.ctx().request_repaint();
        }
        TableOutput {
            response,
            signals,
            sorted,
            resized,
            columns_selected,
            hovered_column,
            header_clicked,
            visible_rows,
        }
    }
}

/// The columns selected after a header click on `column`: alone, or
/// toggled with Ctrl, or the run from `anchor` with Shift (added to the
/// selection with Ctrl too), in the columns' order.
pub fn column_selection(
    columns: &[Column],
    current: &[String],
    anchor: Option<&str>,
    column: &str,
    mods: egui::Modifiers,
) -> Vec<String> {
    let index = |id: &str| columns.iter().position(|c| c.id == id);
    let mut out: Vec<String> = if mods.shift {
        let to = index(column);
        let from = anchor.and_then(index).or(to);
        let mut run = match (from, to) {
            (Some(a), Some(b)) => columns[a.min(b)..=a.max(b)]
                .iter()
                .map(|c| c.id.clone())
                .collect(),
            _ => vec![column.to_string()],
        };
        if mods.command {
            run.extend(current.iter().cloned());
        }
        run
    } else if mods.command {
        let mut v = current.to_vec();
        match v.iter().position(|c| c == column) {
            Some(i) => {
                v.remove(i);
            }
            None => v.push(column.to_string()),
        }
        v
    } else {
        vec![column.to_string()]
    };
    // In the columns' order, once each.
    out.sort_by_key(|c| index(c).unwrap_or(usize::MAX));
    out.dedup();
    out
}

struct Delegate<'a, K: Key> {
    rows: &'a mut dyn TableRows<K>,
    columns: &'a [Column],
    selection: &'a Selection<K>,
    sort: Option<&'a Sort>,
    mode: Mode,
    len: usize,
    focused: bool,
    proposals: &'a mut Vec<Proposal<K>>,
    sort_to: Option<Option<Sort>>,
    page: usize,
    take_focus: bool,
    row_h: f32,
    /// Fetched once per pass, for every row and header.
    kit: std::sync::Arc<Kit>,
    select_columns: bool,
    selected_columns: &'a [String],
    /// The column tinted as hovered: under the pointer at the last pass,
    /// or hovered elsewhere (the shared selection).
    tinted: Option<String>,
    /// The column under the pointer this pass.
    hovered_column: Option<String>,
    header_clicked: Option<String>,
    /// A header click selecting its column, with the modifiers held.
    column_click: Option<(String, egui::Modifiers)>,
    header_menu: Option<HeaderMenu<'a>>,
    /// The rows in view, as `egui_table` prepared them.
    visible_rows: std::ops::Range<usize>,
}

impl<K: Key> Delegate<'_, K> {
    /// The fill a column's parts take when it is selected or hovered:
    /// the table row's in that state (selected wins), `None` otherwise.
    fn column_fill(&self, column: &str) -> Option<egui::Color32> {
        let state = if self.selected_columns.iter().any(|c| c == column) {
            State::SELECTED
        } else if self.tinted.as_deref() == Some(column) {
            State::HOVER
        } else {
            return None;
        };
        Some(self.kit.table_row.style(state).fill)
    }

    /// Tints a cell of a selected or hovered column, at half strength so
    /// a selected row still shows through.
    fn tint(&self, ui: &Ui, column: &str, rect: egui::Rect) {
        if let Some(fill) = self.column_fill(column) {
            ui.painter()
                .rect_filled(rect, 0.0, fill.gamma_multiply(0.5));
        }
    }

    /// Marks a selected or hovered column's header with a bar along its
    /// bottom, in the full fill, leaving its title as it is.
    fn mark_header(&self, ui: &Ui, column: &str, rect: egui::Rect) {
        if let Some(fill) = self.column_fill(column) {
            let h = (rect.height() * 0.1).clamp(2.0, 4.0);
            let bar =
                egui::Rect::from_min_max(egui::pos2(rect.left(), rect.bottom() - h), rect.max);
            ui.painter().rect_filled(bar, 0.0, fill);
        }
    }
}

impl<K: Key> TableDelegate for Delegate<'_, K> {
    fn prepare(&mut self, info: &PrefetchInfo) {
        // upstream: the range is reversed for a table clipped away, scrolled out
        // of a scroll area around it.
        let n = info
            .visible_rows
            .end
            .saturating_sub(info.visible_rows.start) as usize;
        self.page = n.saturating_sub(2).max(1);
        let start = (info.visible_rows.start as usize).min(self.len);
        let end = (info.visible_rows.end as usize).clamp(start, self.len);
        self.visible_rows = start..end;
    }

    fn header_cell_ui(&mut self, ui: &mut Ui, cell: &HeaderCellInfo) {
        // upstream: called for an empty sticky region too (see
        // docs/upstream.md).
        if ui.clip_rect().width() <= 0.0 {
            return;
        }
        let Some(c) = self.columns.get(cell.col_range.start) else {
            return;
        };
        let rect = ui.max_rect();
        let sorted = self.sort.filter(|s| s.column == c.id);
        let name = match sorted {
            Some(s) if s.descending => tr_args(ui.ctx(), &s::SORTED_DOWN, &[("column", &c.title)]),
            Some(_) => tr_args(ui.ctx(), &s::SORTED_UP, &[("column", &c.title)]),
            None => c.title.clone(),
        };
        let r = SortHeader {
            title: &c.title,
            name: &name,
            descending: sorted.map(|s| s.descending),
            // Clickable when a click does something, and for its menu.
            sortable: c.sortable || self.select_columns || self.header_menu.is_some(),
        }
        .show(ui, &self.kit, rect, ui.scope_id().with("sort"));
        self.mark_header(ui, &c.id, rect);
        if self.select_columns {
            let selected = self.selected_columns.contains(&c.id);
            r.widget_info(|| WidgetInfo::selected(Role::Button, true, selected, &name));
        }
        if r.hovered() {
            self.hovered_column = Some(c.id.clone());
        }
        if r.clicked() {
            self.header_clicked = Some(c.id.clone());
            if self.select_columns {
                self.column_click = Some((c.id.clone(), ui.input(|i| i.modifiers)));
            } else if c.sortable {
                self.sort_to = Some(Sort::next(self.sort, &c.id));
            }
        }
        let current = self.sort.filter(|s| s.column == c.id).cloned();
        let menu = &mut self.header_menu;
        let mut sort_to = None;
        r.context_menu(|ui| {
            if c.sortable {
                let entry = |ui: &mut Ui, key, to: Option<Sort>| {
                    let on = current == to;
                    egui_pop_core::widgets::selectable_label(ui, on, tr(ui.ctx(), key))
                        .clicked()
                        .then_some(to)
                };
                let up = Some(Sort {
                    column: c.id.clone(),
                    descending: false,
                });
                let down = Some(Sort {
                    column: c.id.clone(),
                    descending: true,
                });
                for (key, to) in [
                    (&s::SORT_ASCENDING, up),
                    (&s::SORT_DESCENDING, down),
                    (&s::SORT_CLEAR, None),
                ] {
                    if let Some(to) = entry(ui, key, to) {
                        sort_to = Some(to);
                        ui.close();
                    }
                }
            }
            if let Some(HeaderMenu(add)) = menu {
                if c.sortable {
                    ui.separator();
                }
                add(ui, &c.id);
            }
        });
        if let Some(to) = sort_to
            && to.as_ref() != self.sort
        {
            self.sort_to = Some(to);
        }
    }

    fn row_ui(&mut self, ui: &mut Ui, row_nr: u64) {
        let row = row_nr as usize;
        // upstream: called for an empty sticky region too (see
        // docs/upstream.md).
        if ui.clip_rect().width() <= 0.0 || row >= self.len {
            return;
        }
        let key = self.rows.key(row);
        let rect = ui.max_rect();
        let r = ui.interact(rect, ui.scope_id().with("select"), Sense::CLICK);
        let selected = self.selection.contains(&key);
        let cursor = self.focused && self.selection.cursor() == Some(&key);
        let state = rows::state(selected, r.hovered(), cursor);
        if row % 2 == 1 && !selected {
            self.kit.table_stripe.paint(
                &KitCx {
                    painter: ui.painter(),
                    ppp: ui.ctx().pixels_per_point(),
                },
                &Part {
                    rect,
                    room: rect,
                    state: State::REST,
                    label: None,
                },
            );
        }
        rows::paint(ui, &self.kit.table_row, rect, state);
        if selected {
            ui.visuals_mut().override_text_color =
                Some(rows::text_color(&self.kit.table_row, state));
        }
        let name = self.rows.label(row);
        r.widget_info(move || WidgetInfo::selected(Role::Button, true, selected, &name));
        if r.clicked() {
            self.take_focus = true;
            let mods = ui.input(|i| i.modifiers);
            let rows = &*self.rows;
            self.proposals.push(self.selection.click(
                self.mode,
                self.len,
                &|i| rows.key(i),
                row,
                mods,
            ));
        }
    }

    fn cell_ui(&mut self, ui: &mut Ui, cell: &CellInfo) {
        let row = cell.row_nr as usize;
        if row >= self.len {
            return;
        }
        let selected = self.selection.contains(&self.rows.key(row));
        if let Some(c) = self.columns.get(cell.col_nr) {
            let rect = ui.max_rect();
            self.tint(ui, &c.id, rect);
            if ui.rect_contains_pointer(rect) {
                self.hovered_column = Some(c.id.clone());
            }
        }
        // The row's border, whatever its state, never moves the content:
        // the first cell keeps the room of its widest one.
        let room = if cell.col_nr == 0 {
            self.kit.table_row.border_room(ui.ctx().pixels_per_point())
        } else {
            0.0
        };
        ui.add_space(ui.spacing().item_spacing.x + room);
        // A text too long for its cell is elided with "…" (an egui label
        // then shows the full text on hover), not cut at the cell's edge.
        ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Truncate);
        // A selectable label senses clicks and sits above the row: a
        // click on a cell's text would select the text instead of the
        // row. An elided label still shows its full text on hover.
        ui.style_mut().interaction.selectable_labels = false;
        self.rows.cell(ui, row, cell.col_nr, selected);
    }

    fn row_top_offset(&self, _ctx: &egui::Context, _table_id: Id, row_nr: u64) -> f32 {
        self.rows.row_top(row_nr as usize, self.row_h)
    }

    fn default_row_height(&self) -> f32 {
        self.row_h
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_header_click_sorts_up_then_down_then_not() {
        let up = Sort::next(None, "name");
        assert_eq!(Sort::encode(up.as_ref()), "name");
        let down = Sort::next(up.as_ref(), "name");
        assert_eq!(Sort::encode(down.as_ref()), "-name");
        assert_eq!(Sort::next(down.as_ref(), "name"), None);
        assert_eq!(Sort::next(down.as_ref(), "size"), Sort::decode("size"));
        assert_eq!(Sort::decode(""), None);
    }

    #[test]
    fn a_header_click_selects_toggles_or_extends_columns() {
        use egui::Modifiers;
        let cols: Vec<Column> = ["a", "b", "c", "d"]
            .iter()
            .map(|id| Column::new(*id, *id))
            .collect();
        let sel = |current: &[&str], anchor, column, mods| {
            let current: Vec<String> = current.iter().map(|s| s.to_string()).collect();
            column_selection(&cols, &current, anchor, column, mods)
        };
        assert_eq!(sel(&["a"], Some("a"), "c", Modifiers::NONE), ["c"]);
        assert_eq!(sel(&["c"], Some("c"), "a", Modifiers::COMMAND), ["a", "c"]);
        assert_eq!(sel(&["a", "c"], Some("a"), "c", Modifiers::COMMAND), ["a"]);
        assert_eq!(
            sel(&["b"], Some("b"), "d", Modifiers::SHIFT),
            ["b", "c", "d"]
        );
        assert_eq!(
            sel(
                &["a"],
                Some("c"),
                "d",
                Modifiers::SHIFT | Modifiers::COMMAND
            ),
            ["a", "c", "d"]
        );
        // No anchor yet: the column alone.
        assert_eq!(sel(&[], None, "b", Modifiers::SHIFT), ["b"]);
        assert_eq!(column_item("size").as_str(), "column:size");
    }
}
