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

use egui::{Id, Rect, Response, Role, Sense, Ui, WidgetInfo};
use egui_pop_api::AppHandle;
use egui_pop_api::steps::Outcome;
use egui_pop_core::binding::SharedStore;
use egui_pop_core::kit::{Glyph, Kit, KitCx, Part, State};
use egui_pop_core::strings::{tr, tr_args};
use egui_pop_model::commands::Origin;
use egui_pop_model::ids::OwnerId;
use egui_pop_model::values::{Change, DeclareError, Descriptor, Set, Value};
use egui_table::{CellInfo, HeaderCellInfo, HeaderRow, PrefetchInfo, TableDelegate};
use serde_json::{Value as Json, json};

use crate::list::{self, Signal};
use crate::nav::{self, TypeAhead};
use crate::rows;
use crate::select::{Key, Mode, Proposal, Selection};
use crate::steps::{self, TABLE_RESIZE, TABLE_SELECT, TABLE_SORT};
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

    /// Draws a cell (its row's background is drawn).
    fn cell(&mut self, ui: &mut Ui, row: usize, column: usize, selected: bool);
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

    /// Brings a row into view at the next pass.
    pub fn reveal(&mut self, row: usize) {
        self.reveal = Some(row);
    }
}

/// What [`DataTable::show`] returns.
#[derive(Debug)]
pub struct TableOutput<K> {
    /// The table as one focusable widget.
    pub response: Response,
    /// Selection changes and vetoes.
    pub signals: Vec<Signal<K>>,
    /// The sort changed this pass: sort the rows again.
    pub sorted: bool,
    /// The widths changed this pass (a resize ended).
    pub resized: bool,
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
        }
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
        self,
        ui: &mut Ui,
        state: &mut TableState<K>,
        rows: &mut dyn TableRows<K>,
    ) -> TableOutput<K> {
        let id = Id::unique(("egui_pop_views::table", &self.id));
        self.read_store(state);
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
            .auto_size_mode(egui_table::AutoSizeMode::Never);
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
        let take_focus = delegate.take_focus;
        let sort_to = delegate.sort_to.take();
        state.page = delegate.page;
        let label = self
            .label
            .clone()
            .unwrap_or_else(|| tr(ui.ctx(), &s::TABLE));
        let response = ui.interact(inner.rect, id, Sense::focusable_noninteractive());
        egui_pop_core::a11y::name(&response, Role::Unknown, &label);
        if response.has_focus() {
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
        if !signals.is_empty() || sorted || resized {
            ui.ctx().request_repaint();
        }
        TableOutput {
            response,
            signals,
            sorted,
            resized,
        }
    }
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
}

impl<K: Key> TableDelegate for Delegate<'_, K> {
    fn prepare(&mut self, info: &PrefetchInfo) {
        let n = (info.visible_rows.end - info.visible_rows.start) as usize;
        self.page = n.saturating_sub(2).max(1);
    }

    fn header_cell_ui(&mut self, ui: &mut Ui, cell: &HeaderCellInfo) {
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
        let sense = if c.sortable {
            Sense::click()
        } else {
            Sense::hover()
        };
        let r = ui.interact(rect, ui.scope_id().with("sort"), sense);
        let kind = if c.sortable {
            Role::Button
        } else {
            Role::Label
        };
        r.widget_info(move || WidgetInfo::labeled(kind, true, &name));
        let state = State::REST
            .with(State::HOVER, c.sortable && r.hovered())
            .with(State::ACTIVE, c.sortable && r.is_pointer_button_down_on())
            .with(State::SELECTED, sorted.is_some());
        let font = egui::TextStyle::Body.resolve(ui.style());
        let galley = ui
            .painter()
            .layout_no_wrap(c.title.clone(), font, egui::Color32::PLACEHOLDER);
        let cell = &self.kit.table_header_cell;
        let cx = KitCx {
            painter: ui.painter(),
            ppp: ui.ctx().pixels_per_point(),
        };
        let part = Part {
            rect,
            room: rect,
            state,
            label: Some(&galley),
        };
        cell.paint_part(&cx, &part);
        // The sort direction: the kit's glyph, after the title.
        if let Some(s) = sorted {
            let glyph = self.kit.glyph(if s.descending {
                Glyph::SortDescending
            } else {
                Glyph::SortAscending
            });
            let label = cell.label_rect(&part, galley.size());
            let side = galley.size().y;
            let at = Rect::from_min_size(
                egui::pos2(label.right() + side * 0.25, label.top()),
                egui::vec2(side, side),
            );
            glyph.paint(ui.painter(), at, cell.style(state).text);
        }
        if c.sortable && r.clicked() {
            self.sort_to = Some(Sort::next(self.sort, &c.id));
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
        rows::paint(ui, &self.kit.list_row, rect, state);
        if selected {
            ui.visuals_mut().override_text_color =
                Some(rows::text_color(&self.kit.list_row, state));
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
        ui.add_space(ui.spacing().item_spacing.x);
        self.rows.cell(ui, row, cell.col_nr, selected);
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
}
