//! A live tree over a source the application implements.
//!
//! - **Lazy, asynchronous**: a branch's children are asked for the
//!   first time it is expanded ([`TreeSource::load`]); the source
//!   answers now or later, from any thread, through the [`Sink`] it is
//!   given. Meanwhile the branch shows a loading row; a failure shows
//!   the reason and a retry button.
//! - **Live**: the application inserts, removes, moves, renames or
//!   reloads nodes at any time through a [`TreeHandle`]; the selection
//!   and the expansion are keyed by node, so they survive.
//! - **Selection** (single or multiple), **keyboard** (arrows,
//!   Page Up/Down, Home/End, Left/Right to close, open or move to the
//!   parent, Space, Ctrl+A, Enter, F2, type-ahead), **drag and drop**
//!   (before, after or inside a node), **rename in place**.
//! - **Rows** carry an icon ([`TreeView::icon`]) and widgets of the
//!   application's at their right end ([`TreeView::row_widgets`]) that
//!   act on their row without selecting it, and a context menu for the
//!   row clicked ([`TreeView::context_menu`], [`TreeView::context_ui`]).
//! - **Virtualised**: rows are flattened once per change and only the
//!   rows in view are built, so 100 000 nodes cost a screenful.
//! - Each moment is a step (`tree.load`, `tree.expand`, `tree.select`,
//!   `tree.drop`, `tree.rename`, `tree.activate`, `tree.context`, see
//!   [`crate::steps`]) and a [`Signal`] in the output.
//!
//! ```
//! use egui_pop_views::tree::{Node, Sink, TreeSource, TreeState, TreeView};
//!
//! /// Folders named by their path, two levels deep.
//! struct Folders;
//!
//! impl TreeSource<String> for Folders {
//!     fn load(&self, parent: Option<&String>, sink: Sink<String>) {
//!         // A real source answers from a thread.
//!         let base = parent.cloned().unwrap_or_default();
//!         let depth = base.matches('/').count();
//!         sink.deliver(
//!             (0..3)
//!                 .map(|i| {
//!                     let key = format!("{base}/{i}");
//!                     if depth < 1 { Node::branch(key, format!("Folder {i}")) } else { Node::leaf(key, format!("File {i}")) }
//!                 })
//!                 .collect(),
//!         );
//!     }
//! }
//!
//! fn ui(ui: &mut egui::Ui, state: &mut TreeState<String>) {
//!     let out = TreeView::new("folders").label("Folders").show(ui, state, &Folders);
//!     for signal in out.signals {
//!         // Apply drops and renames to the application's data...
//!         let _ = signal;
//!     }
//! }
//! ```

pub mod model;

use std::sync::{Arc, Mutex};

use egui::{
    Context, Id, Modifiers, Rect, Response, Shape, Stroke, TextEdit, Ui, UiBuilder, WidgetInfo,
    WidgetType, pos2, vec2,
};
use egui_pop_api::steps::Outcome;
use egui_pop_api::{AppHandle, menus};
use egui_pop_core::icon::{Size, Source};
use egui_pop_core::strings::{self, tr, tr_args};
use egui_pop_core::theme::Tokens;
use egui_pop_model::ids::{MenuId, StepId};
use serde_json::{Value as Json, json};

pub use model::{Children, Edit, Model, Node, Position, Row};

use crate::nav::{self, Nav, TypeAhead};
use crate::rows::{self, Scroll};
use crate::select::{Key, Mode, Proposal, Selection};
use crate::steps::{
    self, TREE_ACTIVATE, TREE_CONTEXT, TREE_DROP, TREE_EXPAND, TREE_LOAD, TREE_RENAME, TREE_SELECT,
};
use crate::strings as s;

/// What the application implements: a branch's children, on demand.
pub trait TreeSource<K: Key> {
    /// Asks for the children of `parent` (`None`: the roots). Answer
    /// through `sink` now or later, from any thread. Not called again
    /// for that branch until it is reloaded.
    fn load(&self, parent: Option<&K>, sink: Sink<K>);
}

struct Inbox<K> {
    edits: Vec<Edit<K>>,
    ctx: Option<Context>,
}

/// The application's hand on a tree's view: edits from any thread,
/// applied at the next pass (which they wake).
pub struct TreeHandle<K> {
    inbox: Arc<Mutex<Inbox<K>>>,
}

impl<K> Clone for TreeHandle<K> {
    fn clone(&self) -> Self {
        Self {
            inbox: self.inbox.clone(),
        }
    }
}

impl<K> std::fmt::Debug for TreeHandle<K> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("TreeHandle")
    }
}

impl<K> TreeHandle<K> {
    fn push(&self, edit: Edit<K>) {
        let mut inbox = self.inbox.lock().unwrap_or_else(|e| e.into_inner());
        inbox.edits.push(edit);
        if let Some(ctx) = &inbox.ctx {
            ctx.request_repaint();
        }
    }

    /// A node under `parent` at `index` (`None`: last). Under a branch
    /// not loaded yet it is left out: it comes with the branch.
    pub fn insert(&self, parent: Option<K>, index: Option<usize>, node: Node<K>) {
        self.push(Edit::Insert {
            parent,
            index,
            node,
        });
    }

    /// A node and what is under it.
    pub fn remove(&self, key: K) {
        self.push(Edit::Remove(key));
    }

    /// A node to another place (a move into itself is refused).
    pub fn move_to(&self, key: K, parent: Option<K>, index: Option<usize>) {
        self.push(Edit::Move { key, parent, index });
    }

    pub fn rename(&self, key: K, label: impl Into<String>) {
        self.push(Edit::Rename {
            key,
            label: label.into(),
        });
    }

    /// A node's label, name and kind.
    pub fn update(&self, node: Node<K>) {
        self.push(Edit::Update(node));
    }

    /// Loads a branch again (`None`: the roots); its children stay on
    /// screen until the new ones come.
    pub fn reload(&self, parent: Option<K>) {
        self.push(Edit::Reload(parent));
    }

    /// Sets a branch's children directly (a source that replaced the
    /// `tree.load` step answers this way).
    pub fn set_children(&self, parent: Option<K>, result: Result<Vec<Node<K>>, String>) {
        self.push(Edit::Children {
            parent,
            ticket: None,
            result,
        });
    }
}

/// Where a source answers one request.
#[derive(Debug)]
pub struct Sink<K> {
    handle: TreeHandle<K>,
    parent: Option<K>,
    ticket: u64,
}

impl<K> Sink<K> {
    /// The branch asked for (`None`: the roots).
    pub fn parent(&self) -> Option<&K> {
        self.parent.as_ref()
    }

    /// The branch's children, in order.
    pub fn deliver(self, children: Vec<Node<K>>) {
        self.answer(Ok(children));
    }

    /// The branch could not be loaded.
    pub fn fail(self, reason: impl Into<String>) {
        self.answer(Err(reason.into()));
    }

    fn answer(self, result: Result<Vec<Node<K>>, String>) {
        self.handle.push(Edit::Children {
            parent: self.parent,
            ticket: Some(self.ticket),
            result,
        });
    }
}

#[derive(Clone, Debug)]
struct Renaming<K> {
    key: K,
    text: String,
    focus: bool,
}

#[derive(Clone, Debug)]
struct Drag<K> {
    keys: Vec<K>,
    /// The drop the pointer points at, if allowed.
    target: Option<(K, Position)>,
}

/// A tree's state between passes: the cache, the selection, where it is
/// scrolled.
pub struct TreeState<K: Key> {
    model: Model<K>,
    selection: Selection<K>,
    inbox: Arc<Mutex<Inbox<K>>>,
    scroll: Scroll,
    type_ahead: TypeAhead,
    renaming: Option<Renaming<K>>,
    drag: Option<Drag<K>>,
    /// The row last right-clicked, and the node its menu is for.
    context: Option<(K, K)>,
    /// A selection waiting for nodes not loaded yet.
    pending: Option<(Vec<K>, Option<K>)>,
    /// A node to bring into view at the next pass.
    reveal: Option<K>,
}

impl<K: Key> Default for TreeState<K> {
    fn default() -> Self {
        Self {
            model: Model::default(),
            selection: Selection::default(),
            inbox: Arc::new(Mutex::new(Inbox {
                edits: Vec::new(),
                ctx: None,
            })),
            scroll: Scroll::default(),
            type_ahead: TypeAhead::default(),
            renaming: None,
            drag: None,
            context: None,
            pending: None,
            reveal: None,
        }
    }
}

impl<K: Key> std::fmt::Debug for TreeState<K> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TreeState")
            .field("nodes", &self.model.len())
            .field("rows", &self.model.rows_now().len())
            .field("selection", &self.selection)
            .finish()
    }
}

impl<K: Key> TreeState<K> {
    pub fn new() -> Self {
        Self::default()
    }

    /// The application's hand on the view: edits from any thread.
    pub fn handle(&self) -> TreeHandle<K> {
        TreeHandle {
            inbox: self.inbox.clone(),
        }
    }

    pub fn selection(&self) -> &Selection<K> {
        &self.selection
    }

    /// Sets the selection without running a step (restoring a saved
    /// state, following another view). Nodes the view has not loaded yet
    /// (under a branch not opened, or still loading) are queued: the
    /// selection takes each as it loads, and once all have come the
    /// cursor's row is brought into view. Open their ancestors
    /// ([`TreeState::set_expanded`]) so that they load. The user
    /// selecting, or another call, drops what was queued.
    pub fn set_selection(&mut self, keys: impl IntoIterator<Item = K>, cursor: Option<K>) {
        self.pending = Some((keys.into_iter().collect(), cursor));
        self.settle(false);
    }

    /// The selection queued by [`TreeState::set_selection`] for nodes
    /// not loaded yet.
    pub fn pending_selection(&self) -> Option<(&[K], Option<&K>)> {
        self.pending
            .as_ref()
            .map(|(k, c)| (k.as_slice(), c.as_ref()))
    }

    /// What the view knows: nodes, children, expansion, rows.
    pub fn model(&self) -> &Model<K> {
        &self.model
    }

    /// Opens or closes a branch without running a step (restoring a
    /// saved state).
    pub fn set_expanded(&mut self, key: &K, expanded: bool) {
        self.model.set_expanded(key, expanded);
    }

    /// Starts renaming a node in place, from its plain name
    /// ([`Node::name`]).
    pub fn rename(&mut self, key: &K) {
        if let Some(n) = self.model.node(key) {
            self.renaming = Some(Renaming {
                key: key.clone(),
                text: n.name().to_string(),
                focus: true,
            });
        }
    }

    /// Whether a node is being renamed.
    pub fn renaming(&self) -> Option<&K> {
        self.renaming.as_ref().map(|r| &r.key)
    }

    /// Selects what is known of the queued selection; once all of it
    /// is known, it is no longer queued (and its cursor is brought into
    /// view if `reveal`).
    fn settle(&mut self, reveal: bool) {
        let Some((keys, cursor)) = &self.pending else {
            return;
        };
        let model = &self.model;
        let known: Vec<K> = keys.iter().filter(|k| model.contains(k)).cloned().collect();
        let at = cursor.clone().filter(|c| model.contains(c));
        let complete = known.len() == keys.len() && at.is_some() == cursor.is_some();
        self.selection.set(known, at.clone());
        if complete {
            self.pending = None;
            if reveal {
                self.reveal = at;
            }
        }
    }

    /// Applies the edits waiting; returns whether there were any.
    fn drain(&mut self) -> bool {
        let edits = std::mem::take(&mut self.inbox.lock().unwrap_or_else(|e| e.into_inner()).edits);
        let any = !edits.is_empty();
        let cursor_at = self
            .selection
            .cursor()
            .and_then(|c| self.row_of(c))
            .unwrap_or(0);
        for e in edits {
            self.model.apply(e);
        }
        if any {
            let model = &self.model;
            let had_cursor = self.selection.cursor().is_some();
            self.selection.retain(|k| model.contains(k));
            // Nodes a queued selection waited for may have come.
            let queued = self.pending.is_some();
            self.settle(true);
            if self
                .renaming
                .as_ref()
                .is_some_and(|r| !self.model.contains(&r.key))
            {
                self.renaming = None;
            }
            // The cursor's node went: the cursor stays where it was.
            if had_cursor && !queued && self.selection.cursor().is_none() {
                let rows = self.model.rows();
                let near = (0..rows.len())
                    .map(|d| cursor_at.saturating_sub(d).min(rows.len() - 1))
                    .find_map(|i| rows[i].key().cloned());
                self.selection.set_cursor(near);
            }
        }
        any
    }

    fn row_of(&self, key: &K) -> Option<usize> {
        self.model
            .rows_now()
            .iter()
            .position(|r| r.key() == Some(key))
    }
}

/// What happened in a tree this pass.
#[derive(Clone, Debug, PartialEq)]
pub enum Signal<K> {
    /// A branch's children were asked for (`None`: the roots).
    Load {
        key: Option<K>,
    },
    Expand {
        key: K,
        expanded: bool,
    },
    Select {
        keys: Vec<K>,
        cursor: Option<K>,
    },
    /// Nodes dropped; the view moved them, the application moves them in
    /// its data.
    Drop {
        keys: Vec<K>,
        target: K,
        position: Position,
    },
    /// A node renamed (`from` and `to` are plain names, [`Node::name`]);
    /// the view shows the new name (a node with a name of its own keeps
    /// its label until the application updates it), the application
    /// renames it in its data.
    Rename {
        key: K,
        from: String,
        to: String,
    },
    /// A leaf opened (Enter, double click).
    Activate {
        key: K,
    },
    /// A row's context menu opened (a right click), for that node; the
    /// selection is left as it is.
    Context {
        key: K,
    },
    /// A step was vetoed.
    Vetoed {
        step: StepId,
        reason: String,
    },
}

/// What [`TreeView::show`] returns.
#[derive(Debug)]
pub struct TreeOutput<K> {
    /// The tree as one focusable widget.
    pub response: Response,
    pub signals: Vec<Signal<K>>,
}

enum Action<K> {
    Load(Option<K>),
    Expand(K, bool),
    Select(Proposal<K>),
    Drop(Vec<K>, K, Position),
    Rename(K, String),
    Activate(K),
}

/// What draws a row's icon.
type IconFn<'a, K> = Box<dyn Fn(&Node<K>) -> Option<Source> + 'a>;
/// What adds a row's widgets, or fills its context menu.
type RowFn<'a, K> = Box<dyn FnMut(&mut Ui, &Node<K>) + 'a>;

/// A row's context menu.
enum ContextMenu<'a, K> {
    /// Declared, with its commands.
    Declared(&'a AppHandle, MenuId),
    /// Drawn by the application.
    Ui(RowFn<'a, K>),
}

/// The tree widget.
pub struct TreeView<'a, K: Key> {
    id: String,
    label: Option<String>,
    mode: Mode,
    app: Option<&'a AppHandle>,
    drag_drop: bool,
    rename: bool,
    height: Option<f32>,
    icon: Option<IconFn<'a, K>>,
    row_widgets: Option<RowFn<'a, K>>,
    context: Option<ContextMenu<'a, K>>,
}

impl<K: Key> std::fmt::Debug for TreeView<'_, K> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TreeView")
            .field("id", &self.id)
            .field("label", &self.label)
            .field("mode", &self.mode)
            .field("app", &self.app.is_some())
            .field("drag_drop", &self.drag_drop)
            .field("rename", &self.rename)
            .field("height", &self.height)
            .field("icon", &self.icon.is_some())
            .field("row_widgets", &self.row_widgets.is_some())
            .field("context", &self.context.is_some())
            .finish()
    }
}

impl<'a, K: Key> TreeView<'a, K> {
    /// A tree named `id` in steps and signals.
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            label: None,
            mode: Mode::Single,
            app: None,
            drag_drop: false,
            rename: false,
            height: None,
            icon: None,
            row_widgets: None,
            context: None,
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

    /// Nodes may be dragged and dropped (`tree.drop` decides).
    pub fn drag_drop(mut self, on: bool) -> Self {
        self.drag_drop = on;
        self
    }

    /// Nodes may be renamed in place (F2).
    pub fn rename(mut self, on: bool) -> Self {
        self.rename = on;
        self
    }

    /// Its height (default: what is available).
    pub fn height(mut self, height: f32) -> Self {
        self.height = Some(height);
        self
    }

    /// Each row's icon, between the triangle and the label (`None`: no
    /// icon on that row), drawn in the row's text colour at the inline
    /// icon size.
    pub fn icon(mut self, icon: impl Fn(&Node<K>) -> Option<Source> + 'a) -> Self {
        self.icon = Some(Box::new(icon));
        self
    }

    /// Widgets at each row's right end, added right to left (a
    /// visibility toggle, a lock): they act on their row without
    /// selecting it, and the label is cut before them. Called for the
    /// rows in view only, in an `Ui` whose ids are the row's own.
    pub fn row_widgets(mut self, widgets: impl FnMut(&mut Ui, &Node<K>) + 'a) -> Self {
        self.row_widgets = Some(Box::new(widgets));
        self
    }

    /// A declared menu (`Ctx::menu_item`, commands) opened on a row's
    /// right click, for that row: `{"tree": id, "key": key}` is merged
    /// into its commands' arguments and seen by their `when` clauses as
    /// `args.key`, whatever is selected. It runs `tree.context`, then
    /// `menu.open`.
    pub fn context_menu(mut self, app: &'a AppHandle, menu: MenuId) -> Self {
        self.context = Some(ContextMenu::Declared(app, menu));
        self
    }

    /// A context menu the application draws, opened on a row's right
    /// click and given that row's node, whatever is selected. It runs
    /// `tree.context`.
    pub fn context_ui(mut self, menu: impl FnMut(&mut Ui, &Node<K>) + 'a) -> Self {
        self.context = Some(ContextMenu::Ui(Box::new(menu)));
        self
    }

    /// Draws the tree; runs what the user did as steps.
    pub fn show(
        mut self,
        ui: &mut Ui,
        state: &mut TreeState<K>,
        source: &dyn TreeSource<K>,
    ) -> TreeOutput<K> {
        let id = Id::new(("egui_pop_views::tree", &self.id));
        state
            .inbox
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .ctx
            .get_or_insert_with(|| ui.ctx().clone());
        let mut signals = Vec::new();
        state.drain();
        // Branches shown expanded and never loaded are asked for, once.
        for _ in 0..4 {
            let wanted = state.model.take_wanted();
            if wanted.is_empty() {
                break;
            }
            for parent in wanted {
                self.run(state, source, Action::Load(parent), &mut signals);
            }
            state.drain();
        }

        let focused = ui.memory(|m| m.has_focus(id));
        let mut actions = Vec::new();
        let row_h = rows::height(ui);
        if focused && state.renaming.is_none() {
            self.keys(ui, state, row_h, &mut actions);
        }

        if let Some(key) = state.reveal.take() {
            state.model.rows();
            state.scroll.reveal = state.row_of(&key);
        }
        let label = self.label.clone().unwrap_or_else(|| tr(ui.ctx(), &s::TREE));
        let icon = self.icon.take();
        let mut row_widgets = self.row_widgets.take();
        let mut context_menu = self.context.take();
        let tree = self.id.as_str();
        let app = self.app;
        state.model.rows();
        let TreeState {
            model,
            selection,
            scroll,
            renaming,
            drag,
            context,
            ..
        } = state;
        let count = model.rows_now().len();
        let mut take_focus = false;
        let (_, response, inner) = rows::show(
            ui,
            id,
            &label,
            scroll,
            count,
            row_h,
            self.height,
            |ui, visible| {
                let indent = ui.spacing().indent;
                let pointer = ui.input(|i| i.pointer.interact_pos());
                for i in visible.range.clone() {
                    let rect = (visible.rect_of)(i);
                    let row = model.rows_now()[i].clone();
                    let x = rect.left() + row.depth() as f32 * indent;
                    match row {
                        Row::Node { key, .. } => {
                            let node = match model.node(&key) {
                                Some(n) => n.clone(),
                                None => continue,
                            };
                            let r = ui.interact(rect, id.with(("row", &key)), rows::ROW_SENSE);
                            if r.secondary_clicked() {
                                *context = open_context(app, tree, &key, &mut signals);
                            }
                            // The menu, for the node the step settled on.
                            if let Some((_, target)) =
                                context.as_ref().filter(|(row, _)| *row == key)
                            {
                                match context_menu.as_mut() {
                                    Some(ContextMenu::Declared(app, menu)) => {
                                        menus::context_menu_with(
                                            &r,
                                            app,
                                            menu,
                                            json!({ "tree": tree, "key": target }),
                                        );
                                    }
                                    Some(ContextMenu::Ui(add)) => {
                                        if let Some(n) = model.node(target) {
                                            r.context_menu(|ui| add(ui, n));
                                        }
                                    }
                                    None => {}
                                }
                            }
                            let selected = selection.contains(&key);
                            let cursor = (focused && selection.cursor() == Some(&key))
                                || r.context_menu_opened();
                            let hovered = r.contains_pointer() && drag.is_none();
                            rows::paint(ui, rect, selected, hovered, cursor);
                            let color = rows::text_color(ui, selected);
                            let toggle = Rect::from_min_size(
                                pos2(x, rect.top()),
                                vec2(indent, rect.height()),
                            );
                            if node.branch {
                                disclosure(ui, toggle, model.is_expanded(&key), color);
                            }
                            let mut left = toggle.right();
                            if let Some(source) = icon.as_ref().and_then(|f| f(&node)) {
                                let side = Size::Inline.points(&Tokens::of(ui.ctx()));
                                let at = Rect::from_center_size(
                                    pos2(left + side / 2.0, rect.center().y),
                                    vec2(side, side),
                                );
                                source.paint(ui, at, color);
                                left += side + ui.spacing().icon_spacing;
                            }
                            // The row's widgets, from the right end; the
                            // label stops before them.
                            let mut right = rect.right();
                            if let Some(add) = row_widgets.as_mut() {
                                let mut child = ui.new_child(
                                    UiBuilder::new()
                                        .id_salt(("row_widgets", &key))
                                        .max_rect(Rect::from_min_max(
                                            pos2(left, rect.top()),
                                            rect.max,
                                        ))
                                        .layout(egui::Layout::right_to_left(egui::Align::Center)),
                                );
                                add(&mut child, &node);
                                let used = child.min_rect();
                                if used.width() > 0.0 {
                                    right = used.left() - ui.spacing().item_spacing.x;
                                }
                            }
                            let text_rect = Rect::from_min_max(
                                pos2(left, rect.top()),
                                pos2(right.max(left), rect.bottom()),
                            );
                            let editing = renaming.as_ref().is_some_and(|r| r.key == key);
                            if editing {
                                let ren = renaming.as_mut().expect("renaming");
                                let mut child = ui.new_child(
                                    UiBuilder::new()
                                        .max_rect(text_rect.shrink2(vec2(0.0, 1.0)))
                                        .layout(egui::Layout::left_to_right(egui::Align::Center)),
                                );
                                let edit = child.add(
                                    TextEdit::singleline(&mut ren.text)
                                        .id(id.with("rename"))
                                        .desired_width(text_rect.width() - 4.0),
                                );
                                egui_pop_core::a11y::name(
                                    &edit,
                                    WidgetType::TextEdit,
                                    &tr_args(ui.ctx(), &s::RENAME, &[("name", node.name())]),
                                );
                                if ren.focus {
                                    edit.request_focus();
                                    ren.focus = false;
                                } else if edit.lost_focus() {
                                    if ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                                        actions.push(Action::Rename(key.clone(), ren.text.clone()));
                                    }
                                    *renaming = None;
                                    take_focus = true;
                                }
                            } else {
                                ui.painter()
                                    .with_clip_rect(text_rect.intersect(ui.clip_rect()))
                                    .text(
                                        pos2(text_rect.left(), text_rect.center().y),
                                        egui::Align2::LEFT_CENTER,
                                        &node.label,
                                        egui::TextStyle::Body.resolve(ui.style()),
                                        color,
                                    );
                            }
                            let name = node.label.clone();
                            r.widget_info(move || {
                                WidgetInfo::selected(
                                    WidgetType::SelectableLabel,
                                    true,
                                    selected,
                                    &name,
                                )
                            });
                            if r.double_clicked() {
                                if node.branch {
                                    actions.push(Action::Expand(
                                        key.clone(),
                                        !model.is_expanded(&key),
                                    ));
                                } else {
                                    actions.push(Action::Activate(key.clone()));
                                }
                            } else if r.clicked() {
                                take_focus = true;
                                let on_toggle = pointer.is_some_and(|p| toggle.contains(p));
                                if node.branch && on_toggle {
                                    actions.push(Action::Expand(
                                        key.clone(),
                                        !model.is_expanded(&key),
                                    ));
                                } else if let Some(j) = node_index(model, i) {
                                    let nodes = model.node_rows();
                                    let key_at = |j: usize| {
                                        model.rows_now()[nodes[j]]
                                            .key()
                                            .cloned()
                                            .expect("a node row")
                                    };
                                    let mods = ui.input(|i| i.modifiers);
                                    actions.push(Action::Select(selection.click(
                                        self.mode,
                                        nodes.len(),
                                        &key_at,
                                        j,
                                        mods,
                                    )));
                                }
                            }
                            if self.drag_drop && r.drag_started() && renaming.is_none() {
                                let keys = if selected && self.mode == Mode::Multiple {
                                    // In the order they are shown.
                                    model
                                        .node_rows()
                                        .iter()
                                        .filter_map(|&j| model.rows_now()[j].key())
                                        .filter(|k| selection.contains(k))
                                        .cloned()
                                        .collect()
                                } else {
                                    vec![key.clone()]
                                };
                                *drag = Some(Drag { keys, target: None });
                            }
                        }
                        Row::Loading { .. } => {
                            pseudo(ui, rect, x + indent, |ui| {
                                ui.weak(tr(ui.ctx(), &s::LOADING));
                            });
                        }
                        Row::Empty { .. } => {
                            pseudo(ui, rect, x + indent, |ui| {
                                ui.weak(tr(ui.ctx(), &s::EMPTY));
                            });
                        }
                        Row::Failed { parent, reason, .. } => {
                            pseudo(ui, rect, x + indent, |ui| {
                                ui.colored_label(
                                    Tokens::of(ui.ctx()).status.danger,
                                    tr_args(ui.ctx(), &s::FAILED, &[("reason", &reason)]),
                                );
                                if ui.small_button(tr(ui.ctx(), &strings::RETRY)).clicked() {
                                    actions.push(Action::Load(parent.clone()));
                                }
                            });
                        }
                    }
                }
                if let Some(d) = drag.as_mut() {
                    dragging(ui, d, model, &visible, pointer, indent, row_h);
                }
            },
        );
        // A drag ends: dropped where it points, or nowhere.
        if let Some(d) = drag.as_ref() {
            let (released, down) =
                ui.input(|i| (i.pointer.primary_released(), i.pointer.primary_down()));
            let inside = ui
                .input(|i| i.pointer.interact_pos())
                .is_some_and(|p| inner.contains(p));
            if released || !down {
                if released
                    && inside
                    && let Some((target, position)) = d.target.clone()
                {
                    actions.push(Action::Drop(d.keys.clone(), target, position));
                }
                *drag = None;
            } else if let Some(p) = ui.input(|i| i.pointer.interact_pos()) {
                // Near an edge, the view scrolls.
                let edge = row_h;
                let max = (count as f32 * row_h - inner.height()).max(0.0);
                if p.y < inner.top() + edge && scroll.offset > 0.0 {
                    scroll.to = Some((scroll.offset - row_h / 2.0).max(0.0));
                } else if p.y > inner.bottom() - edge && scroll.offset < max {
                    scroll.to = Some((scroll.offset + row_h / 2.0).min(max));
                }
                if scroll.to.is_some() {
                    ui.ctx().request_repaint();
                }
            }
        }
        if take_focus {
            response.request_focus();
        }
        let ran = !actions.is_empty();
        for a in actions {
            self.run(state, source, a, &mut signals);
        }
        if ran {
            state.drain();
            ui.ctx().request_repaint();
        }
        TreeOutput { response, signals }
    }

    /// The keys of a focused tree.
    fn keys(&self, ui: &Ui, state: &mut TreeState<K>, row_h: f32, actions: &mut Vec<Action<K>>) {
        let keys = nav::read(ui);
        if keys.is_empty() {
            return;
        }
        let model = &mut state.model;
        model.rows();
        let nodes = model.node_rows().to_vec();
        let rows = model.rows_now();
        let key_at = |j: usize| rows[nodes[j]].key().cloned().expect("a node row");
        let cursor = state
            .selection
            .cursor()
            .and_then(|c| nodes.iter().position(|&i| rows[i].key() == Some(c)));
        let page = rows::page(&state.scroll, row_h);
        let now = ui.input(|i| i.time);
        for (k, mods) in keys {
            let mut go = |j: usize, mods: Modifiers, actions: &mut Vec<Action<K>>| {
                actions.push(Action::Select(state.selection.move_to(
                    self.mode,
                    nodes.len(),
                    &key_at,
                    j,
                    mods,
                )));
                state.scroll.reveal = Some(nodes[j]);
            };
            match k {
                Nav::Up | Nav::Down | Nav::PageUp | Nav::PageDown | Nav::Home | Nav::End => {
                    if let Some(j) = nav::target(&k, cursor, nodes.len(), page) {
                        go(j, mods, actions);
                    }
                }
                Nav::Left | Nav::Right => {
                    let Some(c) = cursor else { continue };
                    let key = key_at(c);
                    let Some(node) = model.node(&key) else {
                        continue;
                    };
                    let open = node.branch && model.is_expanded(&key);
                    match (k, open) {
                        (Nav::Left, true) => actions.push(Action::Expand(key, false)),
                        (Nav::Left, false) => {
                            if let Some(Some(p)) = model.parent(&key)
                                && let Some(j) =
                                    nodes.iter().position(|&i| rows[i].key() == Some(p))
                            {
                                go(j, Modifiers::NONE, actions);
                            }
                        }
                        (_, false) if node.branch => actions.push(Action::Expand(key, true)),
                        (_, true) => {
                            if let Some(first) = model.loaded(Some(&key)).and_then(|v| v.first())
                                && let Some(j) =
                                    nodes.iter().position(|&i| rows[i].key() == Some(first))
                            {
                                go(j, Modifiers::NONE, actions);
                            }
                        }
                        _ => {}
                    }
                }
                Nav::Space => {
                    if mods.command {
                        if let Some(p) = state.selection.toggle_cursor(self.mode) {
                            actions.push(Action::Select(p));
                        }
                    } else if let Some(c) = cursor {
                        go(c, Modifiers::NONE, actions);
                    }
                }
                Nav::All if self.mode == Mode::Multiple => {
                    actions.push(Action::Select(state.selection.all(nodes.len(), &key_at)));
                }
                Nav::Activate => {
                    let Some(c) = cursor else { continue };
                    let key = key_at(c);
                    match model.node(&key) {
                        Some(n) if n.branch => {
                            actions.push(Action::Expand(key.clone(), !model.is_expanded(&key)))
                        }
                        Some(_) => actions.push(Action::Activate(key)),
                        None => {}
                    }
                }
                Nav::Rename if self.rename => {
                    if let Some(c) = cursor
                        && let Some(n) = model.node(&key_at(c))
                    {
                        state.renaming = Some(Renaming {
                            key: n.key.clone(),
                            text: n.name().to_string(),
                            focus: true,
                        });
                        return;
                    }
                }
                Nav::Text(t) => {
                    let search = state.type_ahead.push(&t, now).to_string();
                    let label_at = |j: usize| {
                        model
                            .node(&key_at(j))
                            .map(|n| n.name().to_string())
                            .unwrap_or_default()
                    };
                    if let Some(j) = nav::find(&search, cursor, nodes.len(), &label_at) {
                        go(j, Modifiers::NONE, actions);
                    }
                }
                _ => {}
            }
        }
    }

    /// Runs one action as its step.
    fn run(
        &self,
        state: &mut TreeState<K>,
        source: &dyn TreeSource<K>,
        action: Action<K>,
        signals: &mut Vec<Signal<K>>,
    ) {
        let tree = self.id.as_str();
        let (step, outcome, signal) = match action {
            Action::Load(parent) => {
                let ticket = state.model.begin_load(parent.as_ref());
                let handle = state.handle();
                let asked = parent.clone();
                let outcome = steps::run(
                    self.app,
                    &TREE_LOAD,
                    json!({ "tree": tree, "key": parent }),
                    |args| {
                        let parent: Option<K> = parse(&args["key"])?;
                        let ticket = if parent == asked {
                            ticket
                        } else {
                            state.model.begin_load(parent.as_ref())
                        };
                        source.load(
                            parent.as_ref(),
                            Sink {
                                handle,
                                parent: parent.clone(),
                                ticket,
                            },
                        );
                        Ok(json!("loading"))
                    },
                );
                if let Some(reason) = outcome.vetoed() {
                    state.model.fail(parent.as_ref(), reason);
                }
                (TREE_LOAD, outcome, Signal::Load { key: parent })
            }
            Action::Expand(key, expanded) => {
                let mut done = (key.clone(), expanded);
                let outcome = steps::run(
                    self.app,
                    &TREE_EXPAND,
                    json!({ "tree": tree, "key": key, "expanded": expanded }),
                    |args| {
                        let key: K = parse(&args["key"])?;
                        let expanded = args["expanded"].as_bool().unwrap_or(expanded);
                        state.model.set_expanded(&key, expanded);
                        done = (key, expanded);
                        Ok(Json::Bool(expanded))
                    },
                );
                (
                    TREE_EXPAND,
                    outcome,
                    Signal::Expand {
                        key: done.0,
                        expanded: done.1,
                    },
                )
            }
            Action::Select(p) => {
                // The user's selection wins over one still queued.
                state.pending = None;
                let mut done = p.clone();
                let selection = &mut state.selection;
                let outcome = steps::run(self.app, &TREE_SELECT, p.args("tree", tree), |args| {
                    done = p.from_args(args)?;
                    selection.apply(done.clone());
                    Ok(json!(done.keys.len()))
                });
                (
                    TREE_SELECT,
                    outcome,
                    Signal::Select {
                        keys: done.keys,
                        cursor: done.cursor,
                    },
                )
            }
            Action::Drop(keys, target, position) => {
                let mut done = (keys.clone(), target.clone(), position);
                let model = &mut state.model;
                let outcome = steps::run(
                    self.app,
                    &TREE_DROP,
                    json!({ "tree": tree, "keys": keys, "target": target, "position": position }),
                    |args| {
                        let keys: Vec<K> = parse(&args["keys"])?;
                        let target: K = parse(&args["target"])?;
                        let position: Position = parse(&args["position"])?;
                        drop_into(model, &keys, &target, position)?;
                        done = (keys, target, position);
                        Ok(json!(done.0.len()))
                    },
                );
                (
                    TREE_DROP,
                    outcome,
                    Signal::Drop {
                        keys: done.0,
                        target: done.1,
                        position: done.2,
                    },
                )
            }
            Action::Rename(key, to) => {
                let Some(node) = state.model.node(&key).cloned() else {
                    return;
                };
                let from = node.name().to_string();
                if to.trim().is_empty() || to == from {
                    return;
                }
                let mut done = to.clone();
                let model = &mut state.model;
                let outcome = steps::run(
                    self.app,
                    &TREE_RENAME,
                    json!({ "tree": tree, "key": key, "from": from, "to": to }),
                    |args| {
                        let to = args["to"].as_str().ok_or("`to` is not a text")?.to_string();
                        // A node with a name of its own keeps its label
                        // (decorations the application rebuilds) until
                        // the application updates it.
                        let mut renamed = node;
                        match &mut renamed.name {
                            Some(name) => *name = to.clone(),
                            None => renamed.label = to.clone(),
                        }
                        model.apply(Edit::Update(renamed));
                        done = to;
                        Ok(json!(done))
                    },
                );
                (
                    TREE_RENAME,
                    outcome,
                    Signal::Rename {
                        key,
                        from,
                        to: done,
                    },
                )
            }
            Action::Activate(key) => {
                let outcome = steps::run(
                    self.app,
                    &TREE_ACTIVATE,
                    json!({ "tree": tree, "key": key }),
                    |_| Ok(Json::Null),
                );
                (TREE_ACTIVATE, outcome, Signal::Activate { key })
            }
        };
        match outcome {
            Outcome::Done(Ok(_)) => signals.push(signal),
            Outcome::Done(Err(reason)) | Outcome::Vetoed { reason, .. } => {
                signals.push(Signal::Vetoed { step, reason })
            }
        }
    }
}

/// Runs `tree.context` for a right-clicked row: the row and the node
/// its menu is for, unless vetoed.
fn open_context<K: Key>(
    app: Option<&AppHandle>,
    tree: &str,
    key: &K,
    signals: &mut Vec<Signal<K>>,
) -> Option<(K, K)> {
    let mut target = key.clone();
    let outcome = steps::run(
        app,
        &TREE_CONTEXT,
        json!({ "tree": tree, "key": key }),
        |args| {
            target = parse(&args["key"])?;
            Ok(Json::Bool(true))
        },
    );
    match outcome {
        Outcome::Done(Ok(_)) => {
            signals.push(Signal::Context {
                key: target.clone(),
            });
            Some((key.clone(), target))
        }
        Outcome::Done(Err(reason)) | Outcome::Vetoed { reason, .. } => {
            signals.push(Signal::Vetoed {
                step: TREE_CONTEXT,
                reason,
            });
            None
        }
    }
}

fn parse<T: serde::de::DeserializeOwned>(v: &Json) -> Result<T, String> {
    serde_json::from_value(v.clone()).map_err(|e| e.to_string())
}

/// The position of row `row` among the node rows.
fn node_index<K: Key>(model: &Model<K>, row: usize) -> Option<usize> {
    model.node_rows().binary_search(&row).ok()
}

/// Moves dropped nodes in the view's model.
fn drop_into<K: Key>(
    model: &mut Model<K>,
    keys: &[K],
    target: &K,
    position: Position,
) -> Result<(), String> {
    if keys.iter().any(|k| model.is_within(target, k)) {
        return Err("a node cannot go into itself".into());
    }
    if !model.contains(target) {
        return Err("the target is gone".into());
    }
    model.move_many(keys, target, position);
    Ok(())
}

/// Tracks the drop target under the pointer and marks it.
fn dragging<K: Key>(
    ui: &Ui,
    d: &mut Drag<K>,
    model: &Model<K>,
    visible: &rows::Visible<'_>,
    pointer: Option<egui::Pos2>,
    indent: f32,
    row_h: f32,
) {
    d.target = None;
    let Some(p) = pointer else { return };
    let Some(i) = visible
        .range
        .clone()
        .find(|&i| (visible.rect_of)(i).contains(p))
    else {
        return;
    };
    let Row::Node { key, depth } = &model.rows_now()[i] else {
        return;
    };
    let Some(node) = model.node(key) else { return };
    if d.keys.iter().any(|k| model.is_within(key, k)) {
        return;
    }
    let rect = (visible.rect_of)(i);
    let f = (p.y - rect.top()) / row_h;
    let position = if node.branch {
        if f < 0.25 {
            Position::Before
        } else if f > 0.75 {
            Position::After
        } else {
            Position::Inside
        }
    } else if f < 0.5 {
        Position::Before
    } else {
        Position::After
    };
    let color = Tokens::of(ui.ctx()).accent.default;
    let x = rect.left() + (*depth as f32 + 1.0) * indent;
    let stroke = Stroke::new(2.0, color);
    match position {
        Position::Inside => {
            ui.painter()
                .rect_stroke(rect.shrink(1.0), 2.0, stroke, egui::StrokeKind::Inside);
        }
        Position::Before => {
            ui.painter().hline(x..=rect.right(), rect.top(), stroke);
        }
        Position::After => {
            ui.painter().hline(x..=rect.right(), rect.bottom(), stroke);
        }
    }
    d.target = Some((key.clone(), position));
}

/// A row that is not a node: loading, empty, failed.
fn pseudo(ui: &mut Ui, rect: Rect, x: f32, add: impl FnOnce(&mut Ui)) {
    let mut child = ui.new_child(
        UiBuilder::new()
            .max_rect(Rect::from_min_max(pos2(x, rect.top()), rect.max))
            .layout(egui::Layout::left_to_right(egui::Align::Center)),
    );
    add(&mut child);
}

/// The open or closed triangle of a branch.
fn disclosure(ui: &Ui, rect: Rect, open: bool, color: egui::Color32) {
    let c = rect.center();
    let r = (rect.height().min(rect.width()) * 0.18).max(3.0);
    let points = if open {
        vec![
            pos2(c.x - r, c.y - r * 0.6),
            pos2(c.x + r, c.y - r * 0.6),
            pos2(c.x, c.y + r * 0.8),
        ]
    } else {
        vec![
            pos2(c.x - r * 0.6, c.y - r),
            pos2(c.x + r * 0.8, c.y),
            pos2(c.x - r * 0.6, c.y + r),
        ]
    };
    ui.painter()
        .add(Shape::convex_polygon(points, color, Stroke::NONE));
}
