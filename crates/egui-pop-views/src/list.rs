//! A list of rows the application draws, virtualised.
//!
//! The application gives the rows ([`ListRows`]): how many, each one's
//! key (the selection is kept by key, so it survives the rows being
//! sorted, filtered or changed) and label (its accessible name and what
//! type-ahead searches), and draws each row in view. The list does the
//! selection (single or multiple, Ctrl, Shift), the keyboard (arrows,
//! Page Up/Down, Home/End, Space, Ctrl+A, type-ahead) and scrolling;
//! a selection runs the `list.select` step.
//!
//! ```
//! use egui_pop_views::list::{ListRows, ListState, ListView};
//! use egui_pop_views::Mode;
//!
//! struct Recent(Vec<(u64, String)>);
//!
//! impl ListRows<u64> for Recent {
//!     fn len(&self) -> usize { self.0.len() }
//!     fn key(&self, row: usize) -> u64 { self.0[row].0 }
//!     fn label(&self, row: usize) -> String { self.0[row].1.clone() }
//! }
//!
//! fn ui(ui: &mut egui::Ui, state: &mut ListState<u64>, rows: &mut Recent) {
//!     let out = ListView::new("recent").mode(Mode::Multiple).show(ui, state, rows);
//!     let _ = out.signals;
//! }
//! ```

use std::collections::HashSet;

use egui::{Id, Response, Role, Ui, UiBuilder, WidgetInfo};
use egui_pop_api::AppHandle;
use egui_pop_api::selection::{SelectionItem, SelectionService};
use egui_pop_core::icon::Source;
use egui_pop_core::strings::tr;
use egui_pop_model::ids::StepId;

use crate::item::{self, Disclosure};
use crate::nav::{self, TypeAhead};
use crate::rows::{self, Scroll};
use crate::select::{self, Key, Mode, Proposal, Selection};
use crate::shared;
use crate::steps::LIST_SELECT;
use crate::strings as s;

/// The rows a list shows; the application owns them.
pub trait ListRows<K: Key> {
    fn len(&self) -> usize;

    fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Row `row`'s key.
    fn key(&self, row: usize) -> K;

    /// Row `row`'s accessible name, searched by type-ahead.
    fn label(&self, row: usize) -> String;

    /// Row `row`'s item in the application-wide selection
    /// ([`ListView::shared`]): by default its key's
    /// ([`SelectionItem::from_key`]). Give the id other widgets showing
    /// the same thing use.
    fn item(&self, row: usize) -> SelectionItem {
        SelectionItem::from_key(&self.key(row))
    }

    /// Row `row`'s icon, drawn before what [`ListRows::ui`] draws, in
    /// the row's text colour (default: none).
    fn icon(&self, row: usize) -> Option<Source> {
        let _ = row;
        None
    }

    /// Row `row`'s weak text on its right (a count, a type; default:
    /// none), in the row's muted tone; what [`ListRows::ui`] draws stops
    /// before it.
    fn trailing(&self, row: usize) -> Option<String> {
        let _ = row;
        None
    }

    /// Draws row `row` (its background is drawn, with its icon and its
    /// trailing text, [a list item](crate::item)'s; the row is named by
    /// its label). By default, its label.
    fn ui(&mut self, ui: &mut Ui, row: usize, selected: bool) {
        let _ = selected;
        let rect = ui.max_rect();
        ui.painter().text(
            egui::pos2(rect.left(), rect.center().y),
            egui::Align2::LEFT_CENTER,
            self.label(row),
            egui::TextStyle::Body.resolve(ui.style()),
            ui.visuals()
                .override_text_color
                .unwrap_or_else(|| ui.visuals().text_color()),
        );
    }
}

/// A list's state between passes.
#[derive(Debug)]
pub struct ListState<K: Key> {
    selection: Selection<K>,
    scroll: Scroll,
    type_ahead: TypeAhead,
    len: Option<usize>,
    /// The shared selection's revision and the row count last followed.
    seen: Option<(u64, u64)>,
}

impl<K: Key> Default for ListState<K> {
    fn default() -> Self {
        Self {
            selection: Selection::default(),
            scroll: Scroll::default(),
            type_ahead: TypeAhead::default(),
            len: None,
            seen: None,
        }
    }
}

impl<K: Key> ListState<K> {
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

    /// Brings a row into view at the next pass.
    pub fn reveal(&mut self, row: usize) {
        self.scroll.reveal = Some(row);
    }
}

/// What happened in a list (or a table) this pass.
#[derive(Clone, Debug, PartialEq)]
pub enum Signal<K> {
    Select {
        keys: Vec<K>,
        cursor: Option<K>,
    },
    /// A step was vetoed.
    Vetoed {
        step: StepId,
        reason: String,
    },
}

/// What [`ListView::show`] returns.
#[derive(Debug)]
pub struct ListOutput<K> {
    /// The list as one focusable widget.
    pub response: Response,
    pub signals: Vec<Signal<K>>,
}

/// The list widget.
#[derive(Debug)]
pub struct ListView<'a> {
    id: String,
    label: Option<String>,
    mode: Mode,
    app: Option<&'a AppHandle>,
    height: Option<f32>,
    row_height: Option<f32>,
    shared: Option<&'a SelectionService>,
}

impl<'a> ListView<'a> {
    /// A list named `id` in steps and signals.
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            label: None,
            mode: Mode::Single,
            app: None,
            height: None,
            row_height: None,
            shared: None,
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

    /// Binds the list to the application-wide selection
    /// (`egui_pop_api::selection`, the service under `SELECTION`): its
    /// rows ([`ListRows::item`]) show selected and hovered when they are
    /// so anywhere, and what it selects (still through `list.select`)
    /// goes there: a plain click or move replaces the whole selection,
    /// Ctrl changes only the list's items. The list is then in the
    /// selection's focus surface, where Alt+Left and Alt+Right walk its
    /// history.
    pub fn shared(mut self, selection: &'a SelectionService) -> Self {
        self.shared = Some(selection);
        self
    }

    pub fn show<K: Key>(
        self,
        ui: &mut Ui,
        state: &mut ListState<K>,
        rows: &mut dyn ListRows<K>,
    ) -> ListOutput<K> {
        shared::surface(ui, self.shared.is_some(), |ui| self.draw(ui, state, rows))
    }

    fn draw<K: Key>(
        self,
        ui: &mut Ui,
        state: &mut ListState<K>,
        rows: &mut dyn ListRows<K>,
    ) -> ListOutput<K> {
        let id = Id::unique(("egui_pop_views::list", &self.id));
        let len = rows.len();
        retain(&mut state.selection, &mut state.len, len, &|i| rows.key(i));
        if let Some(service) = self.shared {
            shared::follow(
                service,
                &mut state.selection,
                &mut state.seen,
                len as u64,
                (0..len).map(|i| (rows.key(i), rows.item(i))),
            );
        }
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
                rows::page(&state.scroll, row_h),
                &mut state.type_ahead,
            );
            proposals = p;
            if reveal.is_some() {
                state.scroll.reveal = reveal;
            }
        }
        let label = self.label.clone().unwrap_or_else(|| tr(ui.ctx(), &s::LIST));
        let selection = &state.selection;
        let mode = self.mode;
        let mut take_focus = false;
        let shared = self.shared;
        let mut hovering: Option<SelectionItem> = None;
        let (_, response, _) = rows::show(
            ui,
            id,
            &label,
            &mut state.scroll,
            len,
            row_h,
            self.height,
            |ui, visible| {
                for i in visible.range.clone() {
                    let rect = (visible.rect_of)(i);
                    let key = rows.key(i);
                    let r = ui.interact(rect, id.with(("row", &key)), rows::ROW_SENSE);
                    let selected = selection.contains(&key);
                    let cursor = focused && selection.cursor() == Some(&key);
                    // Hovered here, or the same item hovered elsewhere.
                    let mut hovered = r.hovered();
                    if let Some(service) = shared {
                        let item = rows.item(i);
                        if hovered {
                            hovering = Some(item);
                        } else {
                            hovered = service.is_hovered(&item);
                        }
                    }
                    let state = rows::state(selected, hovered, cursor);
                    let icon = rows.icon(i);
                    let placed = item::Frame {
                        kit: visible.kit,
                        state,
                        depth: 0,
                        guides: false,
                        disclosure: Disclosure::None,
                        handle: false,
                        icon: icon.as_ref(),
                    }
                    .paint(ui, rect, false);
                    let right = match rows.trailing(i) {
                        Some(text) => item::paint_trailing(
                            ui,
                            visible.kit,
                            state,
                            rect,
                            placed.left,
                            rect.right(),
                            &text,
                        ),
                        None => rect.right() - ui.spacing().item_spacing.x,
                    };
                    let mut child = ui.new_child(
                        UiBuilder::new()
                            .max_rect(egui::Rect::from_min_max(
                                egui::pos2(placed.left, rect.top()),
                                egui::pos2(right.max(placed.left), rect.bottom()),
                            ))
                            .layout(egui::Layout::left_to_right(egui::Align::Center)),
                    );
                    if selected {
                        child.visuals_mut().override_text_color = Some(placed.color);
                    }
                    rows.ui(&mut child, i, selected);
                    let name = rows.label(i);
                    r.widget_info(move || {
                        WidgetInfo::selected(Role::Button, true, selected, &name)
                    });
                    if r.clicked() {
                        take_focus = true;
                        let mods = ui.input(|i| i.modifiers);
                        proposals.push(selection.click(mode, len, &|i| rows.key(i), i, mods));
                    }
                }
            },
        );
        if take_focus {
            response.request_focus();
        }
        if let Some(service) = shared {
            service.hover(&self.id, hovering.as_ref());
        }
        let before = state.selection.keys().to_vec();
        let signals = run(
            self.app,
            &LIST_SELECT,
            "list",
            &self.id,
            &mut state.selection,
            proposals,
        );
        if let Some(service) = shared
            && let Some(Signal::Select { keys, .. }) = signals
                .iter()
                .rev()
                .find(|s| matches!(s, Signal::Select { .. }))
        {
            let items: std::collections::HashMap<K, SelectionItem> =
                (0..len).map(|i| (rows.key(i), rows.item(i))).collect();
            let item_of = |k: &K| {
                items
                    .get(k)
                    .cloned()
                    .unwrap_or_else(|| SelectionItem::from_key(k))
            };
            let keep_others = ui.input(|i| i.modifiers.command);
            shared::write(service, &self.id, &before, keys, keep_others, &item_of);
            state.seen = Some((service.revision(), len as u64));
        }
        if !signals.is_empty() {
            ui.ctx().request_repaint();
        }
        ListOutput { response, signals }
    }
}

/// Drops the selected keys no longer among the rows, when their count
/// changed.
pub(crate) fn retain<K: Key>(
    selection: &mut Selection<K>,
    last: &mut Option<usize>,
    len: usize,
    key_at: &dyn Fn(usize) -> K,
) {
    if *last == Some(len) {
        return;
    }
    *last = Some(len);
    if selection.is_empty() && selection.cursor().is_none() {
        return;
    }
    let keys: HashSet<K> = (0..len).map(key_at).collect();
    selection.retain(|k| keys.contains(k));
}

/// Runs the selection steps of a flat view; returns its signals.
pub(crate) fn run<K: Key>(
    app: Option<&AppHandle>,
    step: &StepId,
    field: &str,
    view: &str,
    selection: &mut Selection<K>,
    proposals: Vec<Proposal<K>>,
) -> Vec<Signal<K>> {
    let mut out = Vec::new();
    for p in proposals {
        if p.keys == selection.keys() && p.cursor.as_ref() == selection.cursor() {
            continue;
        }
        out.push(match select::run(app, step, field, view, selection, p) {
            Ok(done) => Signal::Select {
                keys: done.keys,
                cursor: done.cursor,
            },
            Err(reason) => Signal::Vetoed {
                step: step.clone(),
                reason,
            },
        });
    }
    out
}
