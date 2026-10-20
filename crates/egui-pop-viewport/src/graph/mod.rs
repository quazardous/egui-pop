//! A node graph editor over an application's graph, on egui's
//! [`Scene`](egui::Scene) (pan and zoom).
//!
//! **The application keeps the graph.** It describes its nodes, their
//! typed ports, its wires and groups through [`GraphModel`]; the editor
//! draws them and hands back what the user did as [`Edit`]s (connect,
//! disconnect, move, delete, add, a parameter changed), which the
//! application applies to its graph or not. Each edit goes through the
//! [`on_edit`](GraphEditor::on_edit) hook first: allow, veto or redirect
//! (`egui-pop-imaging` runs them as the `graph.*` steps).
//!
//! - **Pan and zoom**: the wheel pans, Ctrl (Cmd) and the wheel or a
//!   pinch zoom around the pointer, a drag with the middle or secondary
//!   button pans, and so does a drag with Space held. A double click on
//!   the canvas fits the graph. [`GraphMinimap`] is the navigator's
//!   pattern: the whole graph, the part in view framed.
//! - **Levels**: the zoom picks what is drawn ([`Level`]): the
//!   overview (compact nodes, groups folded into stacks), the ports, and
//!   the detail, where each node's parameters are egui-pop sliders
//!   edited inline.
//! - **Nodes**: a click selects (Ctrl or Shift adds), a drag moves the
//!   selection (one [`Edit::Move`] at its end, snapped to the grid with
//!   [`snap`](GraphEditor::snap); Escape puts it back), the chevron or a
//!   double click on the title collapses, a slot in the title bar is the
//!   application's ([`header`](GraphEditor::header)). A drag on the
//!   canvas draws a rubber band.
//! - **Wires**: a drag from a port draws a wire, checked against the
//!   ports' types while it is drawn; dropped on a port that accepts it,
//!   it connects (replacing an input's wire); a drag off a connected
//!   input takes its wire along, dropped nowhere it disconnects. A click
//!   on a port, then on another, connects too.
//! - **Groups**: a frame around the group's nodes, whose title folds
//!   them into a stack (a click on the stack unfolds it).
//! - **Keyboard** ([`GraphKeys`]): Tab goes through the nodes and their
//!   ports; on a node the arrows go to the nearest node that way and
//!   Shift with the arrows moves the selection; on a port the arrows go
//!   to the node's other ports, Enter or Space picks it as a wire's end
//!   (twice: connected), Delete disconnects it and Escape goes back to
//!   the node. Delete removes the selection, Ctrl+D duplicates it,
//!   Ctrl+A selects everything, Home fits.
//! - **Accessibility**: the canvas is a group named by its label, each
//!   node a group (role description "node") with its ports as buttons,
//!   their type and wires as the value; refusals and wires are announced.
//!
//! ```
//! use egui_pop_viewport::graph::{Edge, Edit, GraphEditor, GraphModel, GraphState, Node};
//!
//! struct Pipeline {
//!     nodes: Vec<Node>,
//!     edges: Vec<Edge>,
//! }
//!
//! impl GraphModel for Pipeline {
//!     fn nodes(&self) -> Vec<Node> {
//!         self.nodes.clone()
//!     }
//!     fn edges(&self) -> Vec<Edge> {
//!         self.edges.clone()
//!     }
//! }
//!
//! fn ui(ui: &mut egui::Ui, graph: &mut Pipeline, state: &mut GraphState) {
//!     let out = GraphEditor::new(&*graph, state).snap(Some(10.0)).show(ui);
//!     for edit in out.edits {
//!         if let Edit::Connect { from, to, replaces } = edit {
//!             graph.edges.retain(|e| !replaces.contains(e));
//!             graph.edges.push(Edge::new(from, to));
//!         }
//!     }
//! }
//! ```

mod minimap;
mod model;
mod state;

use std::collections::{BTreeSet, HashMap};
use std::fmt;

use egui::accesskit::Role;
use egui::emath::TSTransform;
use egui::text::{LayoutJob, TextWrapping};
use egui::{
    Color32, CornerRadius, FontId, Id, Key, Modifiers, Pos2, Rangef, Rect, Response, Sense, Stroke,
    StrokeKind, Ui, UiBuilder, Vec2, pos2, vec2,
};
use egui_pop_core::a11y::{self, Politeness};
use egui_pop_core::icon::Icon;
use egui_pop_core::slider::{self, Slider};
use egui_pop_core::state::Slot;
use egui_pop_core::strings::{self, tr, tr_args};
use egui_pop_core::theme::Tokens;
use serde_json::{Value as Json, json};

pub use minimap::GraphMinimap;
pub use model::{
    Edge, Edit, GraphEvent, GraphModel, Group, Kind, Level, Moved, NewNode, Node, Param, Port,
    PortRef, PortType, Refusal, Request, Side, TYPE_TOKENS, check,
};
pub use state::GraphState;

use crate::change::{Cause, Verdict};
use crate::strings as s;
use state::{Moving, Wire};

/// A title bar's height, in graph units.
const HEADER: f32 = 28.0;
/// A row of ports.
const ROW: f32 = 20.0;
/// Inside a node's edges.
const PAD: f32 = 6.0;
/// A node's heights in the overview, the smallest that keeps its title
/// readable at the zoom (a few steps, so the glyph cache stays small).
const OVERVIEW_H: [f32; 3] = [48.0, 64.0, 90.0];
/// A port's radius.
const PORT_R: f32 = 5.0;
/// The square a port answers the pointer in.
const PORT_HIT: f32 = 16.0;
/// How near a port a dragged wire is dropped on it.
const DROP: f32 = 14.0;
/// Around a group's nodes.
const GROUP_PAD: f32 = 14.0;
/// A group's title band.
const GROUP_LABEL: f32 = 24.0;
/// The chevron's square in a title bar.
const CHEVRON: f32 = 16.0;

/// The node graph's colours, from the theme (`graph.*`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GraphTokens {
    pub background: Color32,
    pub grid: Color32,
    pub node: Color32,
    pub node_header: Color32,
    pub node_border: Color32,
    pub node_selected: Color32,
    pub wire: Color32,
    pub wire_preview: Color32,
    pub wire_refused: Color32,
    pub group: Color32,
    pub group_border: Color32,
    pub band: Color32,
    pub text: Color32,
    pub text_muted: Color32,
    pub focus: Color32,
}

impl GraphTokens {
    pub fn of(ctx: &egui::Context) -> GraphTokens {
        let t = Tokens::of(ctx);
        let c = |path: &str, fallback: Color32| t.color(path).unwrap_or(fallback);
        GraphTokens {
            background: c("graph.background", t.surface.canvas),
            grid: c("graph.grid", t.border.default),
            node: c("graph.node", t.surface.raised),
            node_header: c("graph.node-header", t.surface.overlay),
            node_border: c("graph.node-border", t.border.default),
            node_selected: c("graph.node-selected", t.accent.default),
            wire: c("graph.wire", t.border.strong),
            wire_preview: c("graph.wire-preview", t.accent.default),
            wire_refused: c("graph.wire-refused", t.status.danger),
            group: c("graph.group", t.accent.subtle),
            group_border: c("graph.group-border", t.border.default),
            band: c("graph.band", t.accent.default),
            text: t.text.default,
            text_muted: t.text.muted,
            focus: t.focus_ring,
        }
    }

    /// A port type's colour: its token, else the plain wire's.
    pub fn type_color(&self, ctx: &egui::Context, ty: &PortType) -> Color32 {
        Tokens::of(ctx).color(&ty.color).unwrap_or(self.wire)
    }
}

/// The node editors' keys, as data for input profiles ([`set_keys`]).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GraphKeys {
    /// Deletes the selection (on a port: disconnects it).
    pub delete: [Key; 2],
    /// With Ctrl (Cmd): duplicates the selection.
    pub duplicate: Key,
    /// With Ctrl (Cmd): selects every node.
    pub select_all: Key,
    /// Fits the graph in view.
    pub fit: Key,
    /// Held with the arrows on a node: moves the selection.
    pub nudge: Modifiers,
    /// Cancels a wire, a drag; on a port, back to its node.
    pub cancel: Key,
}

impl Default for GraphKeys {
    fn default() -> Self {
        GraphKeys {
            delete: [Key::Delete, Key::Backspace],
            duplicate: Key::D,
            select_all: Key::A,
            fit: Key::Home,
            nudge: Modifiers::SHIFT,
            cancel: Key::Escape,
        }
    }
}

const KEYS: Slot<GraphKeys> = Slot::new("viewport.graph.keys");

/// The node editors' keys in a context (the defaults until set).
pub fn keys(ctx: &egui::Context) -> GraphKeys {
    KEYS.get_or_default(ctx)
}

/// Remaps the node editors' keys in a context (an input profile).
pub fn set_keys(ctx: &egui::Context, keys: GraphKeys) {
    KEYS.set(ctx, keys);
}

type HookFn<'a> = dyn FnMut(&Edit, Cause) -> Verdict<Edit> + 'a;
type HeaderFn<'a> = dyn FnMut(&mut Ui, &Node) + 'a;

/// The node graph editor.
pub struct GraphEditor<'a> {
    model: &'a dyn GraphModel,
    state: &'a mut GraphState,
    id_salt: Option<Id>,
    label: Option<String>,
    size: Option<Vec2>,
    snap: Option<f32>,
    node_width: f32,
    overview_below: f32,
    detail_from: f32,
    zoom_range: Rangef,
    header: Option<Box<HeaderFn<'a>>>,
    hook: Option<Box<HookFn<'a>>>,
    requests: Vec<Request>,
    describe: bool,
}

impl fmt::Debug for GraphEditor<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("GraphEditor")
            .field("label", &self.label)
            .field("snap", &self.snap)
            .field("requests", &self.requests)
            .finish_non_exhaustive()
    }
}

/// What the editor did in a frame.
#[derive(Debug)]
pub struct GraphOutput {
    /// The canvas; `changed()` when an edit was proposed.
    pub response: Response,
    /// The edits for the application to apply, after the hook.
    pub edits: Vec<Edit>,
    pub events: Vec<GraphEvent>,
    pub level: Level,
    /// Screen points per graph unit.
    pub zoom: f32,
    /// The graph as drawn, when asked ([`GraphEditor::describe`]).
    pub snapshot: Option<Json>,
}

impl<'a> GraphEditor<'a> {
    pub fn new(model: &'a dyn GraphModel, state: &'a mut GraphState) -> GraphEditor<'a> {
        GraphEditor {
            model,
            state,
            id_salt: None,
            label: None,
            size: None,
            snap: None,
            node_width: 180.0,
            overview_below: 0.55,
            detail_from: 0.9,
            // upstream: text blurs in a Scene zoomed past 100 % (docs/upstream.md).
            zoom_range: Rangef::new(0.05, 1.5),
            header: None,
            hook: None,
            requests: Vec::new(),
            describe: false,
        }
    }

    /// Tells two editors in one `Ui` apart.
    pub fn id_salt(mut self, salt: impl std::hash::Hash + fmt::Debug) -> Self {
        self.id_salt = Some(Id::unique(salt));
        self
    }

    /// The accessible name (default "Node graph").
    pub fn label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }

    /// The size in points (default: what is available).
    pub fn size(mut self, size: Vec2) -> Self {
        self.size = Some(size);
        self
    }

    /// Snaps moved and added nodes to a grid of this many graph units
    /// (and draws it); `None` (the default) moves freely.
    pub fn snap(mut self, grid: Option<f32>) -> Self {
        self.snap = grid.filter(|g| *g > 0.0);
        self
    }

    /// A node's width in graph units (default 180).
    pub fn node_width(mut self, width: f32) -> Self {
        self.node_width = width.max(80.0);
        self
    }

    /// The zooms where the levels change: below `overview_below` the
    /// overview, from `detail_from` the detail (defaults 0.55 and 0.9).
    pub fn levels(mut self, overview_below: f32, detail_from: f32) -> Self {
        self.overview_below = overview_below;
        self.detail_from = detail_from.max(overview_below);
        self
    }

    /// How far the zoom goes (default 0.05 to 1.5; text blurs above 1).
    pub fn zoom_range(mut self, range: impl Into<Rangef>) -> Self {
        self.zoom_range = range.into();
        self
    }

    /// The application's slot in each title bar, at its right end (a
    /// status, a toggle), in the ports and detail views.
    pub fn header(mut self, header: impl FnMut(&mut Ui, &Node) + 'a) -> Self {
        self.header = Some(Box::new(header));
        self
    }

    /// Sees each edit before it is handed back: allow, veto or redirect
    /// (another edit). A slider's drag frames come too (`done: false`).
    pub fn on_edit(mut self, hook: impl FnMut(&Edit, Cause) -> Verdict<Edit> + 'a) -> Self {
        self.hook = Some(Box::new(hook));
        self
    }

    /// Requests done at this frame, as if by the user (an agent's
    /// command, a menu of the application's).
    pub fn requests(mut self, requests: impl IntoIterator<Item = Request>) -> Self {
        self.requests.extend(requests);
        self
    }

    /// Puts the graph as drawn in [`GraphOutput::snapshot`] (for agents
    /// and the debug bridge; see [`describe`]).
    pub fn describe(mut self, on: bool) -> Self {
        self.describe = on;
        self
    }

    /// The level a zoom shows.
    fn level_at(&self, zoom: f32) -> Level {
        if zoom < self.overview_below {
            Level::Overview
        } else if zoom >= self.detail_from {
            Level::Detail
        } else {
            Level::Ports
        }
    }

    /// Draws the editor; says what the user asked for.
    pub fn show(mut self, ui: &mut Ui) -> GraphOutput {
        let ctx = ui.ctx().clone();
        let id = ui.make_persistent_id(self.id_salt.unwrap_or(Id::unique("egui_pop_graph")));
        let nodes = self.model.nodes();
        let edges = self.model.edges();
        let groups = self.model.groups();
        let size = self.size.unwrap_or_else(|| ui.available_size());
        let outer = Rect::from_min_size(ui.available_rect_before_wrap().min, size);

        let mut requests = std::mem::take(&mut self.state.requests);
        requests.append(&mut self.requests);
        let mut edits = Vec::new();
        let mut events = Vec::new();
        let before_selection = self.state.selection.clone();

        // Requests: selections and views first, edits once checked.
        let base = Base {
            model: self.model,
            nodes: &nodes,
            edges: &edges,
        };
        for r in requests {
            match r {
                Request::Select(list) => {
                    self.state.selection = list
                        .into_iter()
                        .filter(|n| nodes.iter().any(|m| m.id == *n))
                        .collect();
                }
                Request::Fit => self.state.fit = true,
                Request::Level(level) => {
                    if self.state.has_view() {
                        let now = zoom_of(outer.size(), self.state.view, self.zoom_range);
                        let want = match level {
                            Level::Overview => self.overview_below * 0.7,
                            Level::Ports => (self.overview_below + self.detail_from) / 2.0,
                            Level::Detail => self.detail_from.max(1.0).min(self.zoom_range.max),
                        };
                        let c = self.state.view.center();
                        self.state.zoom_by(want / now.max(1e-6), c);
                    }
                }
                Request::Focus(node) => {
                    if let Some(n) = nodes.iter().find(|n| n.id == node) {
                        ctx.memory_mut(|m| m.request_focus(node_id(id, &n.id)));
                        let r = Rect::from_min_size(
                            pos2(n.pos[0], n.pos[1]),
                            vec2(self.node_width, HEADER),
                        );
                        self.state.center_on(r.center());
                    }
                }
                Request::Edit(e) => {
                    let e = match e {
                        Edit::Connect { from, to, .. } => {
                            let a = (from.clone(), Side::Output);
                            let b = (to.clone(), Side::Input);
                            match base.connect(&a, &b, None) {
                                Ok(e) => e,
                                Err(Refusal::Exists) => continue,
                                Err(refusal) => {
                                    refuse(&ctx, &base, &a.0, &b.0, &refusal, &mut events);
                                    continue;
                                }
                            }
                        }
                        e => e,
                    };
                    propose(&mut self.hook, e, Cause::Command, &mut edits);
                }
            }
        }

        if self.state.fit || !self.state.has_view() {
            self.state.view = fit_rect(&nodes, self.node_width);
            self.state.fit = false;
        }
        let zoom = zoom_of(outer.size(), self.state.view, self.zoom_range);
        let level = self.level_at(zoom);
        if self.state.level != Some(level) {
            if self.state.level.is_some() {
                events.push(GraphEvent::Level(level));
            }
            self.state.level = Some(level);
        }

        let label = self.label.clone().unwrap_or_else(|| tr(&ctx, &s::GRAPH));
        let mut view = self.state.view;
        let mut cx = Cx {
            ctx: ctx.clone(),
            id,
            t: GraphTokens::of(&ctx),
            keys: keys(&ctx),
            base,
            groups: &groups,
            level,
            zoom,
            node_width: self.node_width,
            snap: self.snap,
            state: &mut *self.state,
            hook: &mut self.hook,
            header: &mut self.header,
            edits: &mut edits,
            events: &mut events,
            lay: Layout::default(),
            focused: None,
            to_global: TSTransform::IDENTITY,
            reveal: None,
        };
        let scene = egui::Scene::new()
            .zoom_range(self.zoom_range)
            .drag_pan_buttons(
                egui::containers::DragPanButtons::MIDDLE
                    | egui::containers::DragPanButtons::SECONDARY,
            )
            .sense(Sense::click_and_drag());
        let inner = ui.scope_builder(UiBuilder::new().max_rect(outer), |ui| {
            scene.show(ui, &mut view, |ui| cx.content(ui))
        });
        let mut response = inner.inner.response;
        cx.canvas(&response, &label);
        let reveal = cx.reveal.take();
        let visible = cx.lay.visible;
        drop(cx);
        if view != self.state.view {
            self.state.view = view;
        }
        if let Some(r) = reveal
            && !visible.contains_rect(r)
        {
            self.state.center_on(r.center());
        }

        if self.state.selection != before_selection {
            events.push(GraphEvent::Selection(
                self.state.selection.iter().cloned().collect(),
            ));
        }
        if !edits.is_empty() {
            response.mark_changed();
        }
        let snapshot = self
            .describe
            .then(|| describe(self.model, &nodes, &edges, self.state));
        GraphOutput {
            response,
            edits,
            events,
            level,
            zoom,
            snapshot,
        }
    }
}

/// Screen points per graph unit when `view` is fitted in `size` (as
/// egui's `Scene` does).
fn zoom_of(size: Vec2, view: Rect, range: Rangef) -> f32 {
    if view.width() <= 0.0 || view.height() <= 0.0 {
        return 1.0;
    }
    range.clamp((size / view.size()).min_elem())
}

/// Around every node, with a margin.
fn fit_rect(nodes: &[Node], width: f32) -> Rect {
    let mut r = Rect::NOTHING;
    for n in nodes {
        r = r.union(Rect::from_min_size(
            pos2(n.pos[0], n.pos[1]),
            vec2(width, HEADER + ROW * 3.0),
        ));
    }
    if !r.is_positive() {
        r = Rect::from_min_size(Pos2::ZERO, vec2(width * 3.0, width * 2.0));
    }
    r.expand(40.0)
}

/// The edit as the hook leaves it, kept.
fn propose(hook: &mut Option<Box<HookFn<'_>>>, edit: Edit, cause: Cause, edits: &mut Vec<Edit>) {
    match hook.as_mut().map_or(Verdict::Allow, |h| h(&edit, cause)) {
        Verdict::Allow => edits.push(edit),
        Verdict::Veto => {}
        Verdict::Redirect(e) => edits.push(e),
    }
}

/// Why a wire is refused, in words.
fn reason(ctx: &egui::Context, r: &Refusal) -> String {
    match r {
        Refusal::Types { from, to } => {
            tr_args(ctx, &s::GRAPH_TYPES_DIFFER, &[("from", from), ("to", to)])
        }
        Refusal::SameNode => tr(ctx, &s::GRAPH_SAME_NODE),
        Refusal::SameSide => tr(ctx, &s::GRAPH_SAME_SIDE),
        Refusal::Exists => tr(ctx, &s::GRAPH_EXISTS),
        Refusal::Other(text) => text.clone(),
    }
}

/// Reports a refused wire: an event and an announcement.
fn refuse(
    ctx: &egui::Context,
    base: &Base<'_>,
    from: &PortRef,
    to: &PortRef,
    refusal: &Refusal,
    events: &mut Vec<GraphEvent>,
) {
    let why = reason(ctx, refusal);
    a11y::announce(
        ctx,
        tr_args(
            ctx,
            &s::GRAPH_REFUSED,
            &[
                ("from", &base.port_name(ctx, from)),
                ("to", &base.port_name(ctx, to)),
                ("reason", &why),
            ],
        ),
        Politeness::Assertive,
    );
    events.push(GraphEvent::Refused {
        from: from.clone(),
        to: to.clone(),
        reason: why,
    });
}

fn node_id(id: Id, node: &str) -> Id {
    id.with(("node", node))
}

fn port_id(id: Id, port: &PortRef, side: Side) -> Id {
    id.with(("port", &port.node, &port.port, side))
}

/// The graph and the application's checks.
#[derive(Clone, Copy)]
struct Base<'g> {
    model: &'g dyn GraphModel,
    nodes: &'g [Node],
    edges: &'g [Edge],
}

impl<'g> Base<'g> {
    fn node(&self, id: &str) -> Option<&'g Node> {
        self.nodes.iter().find(|n| n.id == id)
    }

    fn port(&self, p: &PortRef, side: Side) -> Option<(&'g Node, &'g Port)> {
        let n = self.node(&p.node)?;
        Some((n, n.port(side, &p.port)?))
    }

    /// "Blur, input Image" (or the ids when the port is gone).
    fn port_name(&self, ctx: &egui::Context, p: &PortRef) -> String {
        for side in [Side::Output, Side::Input] {
            if let Some((n, port)) = self.port(p, side) {
                return port_label(ctx, n, port, side);
            }
        }
        format!("{}.{}", p.node, p.port)
    }

    /// The wires at a port.
    fn wires_at(&self, p: &PortRef, side: Side) -> Vec<Edge> {
        self.edges
            .iter()
            .filter(|e| match side {
                Side::Input => e.to == *p,
                Side::Output => e.from == *p,
            })
            .cloned()
            .collect()
    }

    /// Whether a wire from `a` may end at `b` (either way round).
    fn check(&self, a: &(PortRef, Side), b: &(PortRef, Side)) -> Result<(), Refusal> {
        if a.1 == b.1 {
            return Err(Refusal::SameSide);
        }
        let (out, inp) = if a.1 == Side::Output { (a, b) } else { (b, a) };
        let from = self
            .port(&out.0, Side::Output)
            .ok_or_else(|| Refusal::Other(format!("no output `{}`", out.0.port)))?;
        let to = self
            .port(&inp.0, Side::Input)
            .ok_or_else(|| Refusal::Other(format!("no input `{}`", inp.0.port)))?;
        if from.0.id == to.0.id {
            return Err(Refusal::SameNode);
        }
        self.model.can_connect(from, to)
    }

    /// The edit a wire from `a` to `b` makes (`detached`: the wire it was
    /// dragged off).
    fn connect(
        &self,
        a: &(PortRef, Side),
        b: &(PortRef, Side),
        detached: Option<&Edge>,
    ) -> Result<Edit, Refusal> {
        self.check(a, b)?;
        let (out, inp) = if a.1 == Side::Output { (a, b) } else { (b, a) };
        let mut replaces: Vec<Edge> = detached.into_iter().cloned().collect();
        let multiple = self
            .port(&inp.0, Side::Input)
            .is_some_and(|(_, p)| p.multiple);
        if !multiple {
            for e in self.wires_at(&inp.0, Side::Input) {
                if !replaces.contains(&e) {
                    replaces.push(e);
                }
            }
        }
        let edge = Edge::new(out.0.clone(), inp.0.clone());
        replaces.retain(|e| *e != edge);
        if replaces.is_empty() && self.edges.contains(&edge) {
            return Err(Refusal::Exists);
        }
        Ok(Edit::Connect {
            from: edge.from,
            to: edge.to,
            replaces,
        })
    }
}

/// A port's accessible name: "Blur, input Image".
fn port_label(ctx: &egui::Context, node: &Node, port: &Port, side: Side) -> String {
    let key = match side {
        Side::Input => &s::GRAPH_PORT_INPUT,
        Side::Output => &s::GRAPH_PORT_OUTPUT,
    };
    tr_args(ctx, key, &[("node", &node.title), ("port", &port.label)])
}

/// A box drawn: a node, or a group folded into a stack.
#[derive(Clone, Debug)]
struct Placed {
    rect: Rect,
    /// The node's index, or `None` for a stack.
    node: Option<usize>,
    /// A stack's group index.
    group: Option<usize>,
    collapsed: bool,
}

/// Where everything is this frame.
#[derive(Clone, Debug)]
struct Layout {
    boxes: Vec<Placed>,
    /// Each node's box index (`None`: not drawn; in a stack, the
    /// stack's).
    box_of: Vec<Option<usize>>,
    /// Ports' centres, by node index, side and port index.
    ports: HashMap<(usize, Side, usize), Pos2>,
    /// The frames of unfolded groups: group index and rect (title band
    /// included).
    frames: Vec<(usize, Rect)>,
    /// The part of the graph in view.
    visible: Rect,
}

impl Default for Layout {
    fn default() -> Self {
        Layout {
            boxes: Vec::new(),
            box_of: Vec::new(),
            ports: HashMap::new(),
            frames: Vec::new(),
            visible: Rect::NOTHING,
        }
    }
}

/// One frame of the editor.
struct Cx<'a, 'g, 'h> {
    ctx: egui::Context,
    id: Id,
    t: GraphTokens,
    keys: GraphKeys,
    base: Base<'g>,
    groups: &'g [Group],
    level: Level,
    zoom: f32,
    node_width: f32,
    snap: Option<f32>,
    state: &'a mut GraphState,
    hook: &'a mut Option<Box<HookFn<'h>>>,
    header: &'a mut Option<Box<HeaderFn<'h>>>,
    edits: &'a mut Vec<Edit>,
    events: &'a mut Vec<GraphEvent>,
    lay: Layout,
    /// The node with the keyboard focus, by index.
    focused: Option<usize>,
    to_global: TSTransform,
    /// A rect to bring into view (what gained the focus).
    reveal: Option<Rect>,
}

impl Cx<'_, '_, '_> {
    fn propose(&mut self, edit: Edit, cause: Cause) {
        propose(self.hook, edit, cause, self.edits);
    }

    /// Gives an accessible node its bounds on screen: egui gives those
    /// of the scene's layer, before its transform.
    // upstream: AccessKit bounds ignore layer transforms (docs/upstream.md).
    fn bounds(&self, id: Id, rect: Rect) {
        let r = self.to_global * rect;
        self.ctx.accesskit_node_builder(id, |node| {
            node.set_bounds(egui::accesskit::Rect {
                x0: r.min.x.into(),
                y0: r.min.y.into(),
                x1: r.max.x.into(),
                y1: r.max.y.into(),
            });
        });
    }

    /// The pointer, in graph units.
    fn pointer(&self) -> Option<Pos2> {
        self.ctx
            .input(|i| i.pointer.latest_pos())
            .map(|p| self.to_global.inverse() * p)
    }

    /// Whether a group is folded into a stack at this level.
    fn folded(&self, group: &str) -> bool {
        if self.level == Level::Overview {
            !self.state.unfolded.contains(group)
        } else {
            self.state.folded.contains(group)
        }
    }

    fn toggle_fold(&mut self, group: &str) {
        let set = if self.level == Level::Overview {
            &mut self.state.unfolded
        } else {
            &mut self.state.folded
        };
        if !set.remove(group) {
            set.insert(group.to_string());
        }
    }

    /// A node's place, moved with the drag under way.
    fn pos_of(&self, n: &Node) -> Pos2 {
        let p = pos2(n.pos[0], n.pos[1]);
        match &self.state.moving {
            Some(m) if m.nodes.iter().any(|(id, _)| *id == n.id) => p + m.offset,
            _ => p,
        }
    }

    /// Lays the graph out.
    fn layout(&mut self) {
        let nodes = self.base.nodes;
        let mut lay = Layout {
            box_of: vec![None; nodes.len()],
            visible: self.lay.visible,
            ..Layout::default()
        };
        let folded: Vec<bool> = self.groups.iter().map(|g| self.folded(&g.id)).collect();
        let group_index =
            |id: &str| -> Option<usize> { self.groups.iter().position(|g| g.id == id) };
        // Stacks: one box per folded group with members, where its
        // first-placed member is.
        let mut stack_box: HashMap<usize, usize> = HashMap::new();
        for (i, n) in nodes.iter().enumerate() {
            let Some(g) = n.group.as_deref().and_then(group_index) else {
                continue;
            };
            if !folded[g] {
                continue;
            }
            let p = self.pos_of(n);
            let b = *stack_box.entry(g).or_insert_with(|| {
                lay.boxes.push(Placed {
                    rect: Rect::from_min_size(p, Vec2::ZERO),
                    node: None,
                    group: Some(g),
                    collapsed: true,
                });
                lay.boxes.len() - 1
            });
            let r = &mut lay.boxes[b].rect;
            r.min = r.min.min(p);
            lay.box_of[i] = Some(b);
        }
        let stack_h = if self.level == Level::Overview {
            self.overview_h()
        } else {
            HEADER + ROW
        };
        for b in &mut lay.boxes {
            b.rect = Rect::from_min_size(b.rect.min, vec2(self.node_width, stack_h));
        }
        for (i, n) in nodes.iter().enumerate() {
            if lay.box_of[i].is_some() {
                continue;
            }
            let collapsed = self.state.collapsed.contains(&n.id);
            let rows = n.inputs.len().max(n.outputs.len());
            let params = if self.level == Level::Detail && !collapsed {
                self.base.model.params(&n.id).len()
            } else {
                0
            };
            let h = match self.level {
                Level::Overview => self.overview_h(),
                _ if collapsed => HEADER,
                _ => HEADER + rows as f32 * ROW + params as f32 * param_h(&self.ctx) + PAD,
            };
            let rect = Rect::from_min_size(self.pos_of(n), vec2(self.node_width, h));
            lay.boxes.push(Placed {
                rect,
                node: Some(i),
                group: None,
                collapsed,
            });
            lay.box_of[i] = Some(lay.boxes.len() - 1);
        }
        // Ports: on their rows, or at the box's sides when it is
        // compact.
        for (i, n) in nodes.iter().enumerate() {
            let Some(b) = lay.box_of[i] else { continue };
            let placed = &lay.boxes[b];
            let r = placed.rect;
            let compact =
                placed.node.is_none() || placed.collapsed || self.level == Level::Overview;
            for (side, list) in [(Side::Input, &n.inputs), (Side::Output, &n.outputs)] {
                let x = if side == Side::Input {
                    r.left()
                } else {
                    r.right()
                };
                for k in 0..list.len() {
                    let y = if compact {
                        r.center().y
                    } else {
                        r.top() + HEADER + (k as f32 + 0.5) * ROW
                    };
                    lay.ports.insert((i, side, k), pos2(x, y));
                }
            }
        }
        // Frames around unfolded groups.
        for (g, group) in self.groups.iter().enumerate() {
            if folded[g] {
                continue;
            }
            let mut r = Rect::NOTHING;
            for (i, n) in nodes.iter().enumerate() {
                if n.group.as_deref() == Some(group.id.as_str())
                    && let Some(b) = lay.box_of[i]
                {
                    r = r.union(lay.boxes[b].rect);
                }
            }
            if r.is_positive() {
                let mut r = r.expand(GROUP_PAD);
                r.min.y -= GROUP_LABEL;
                lay.frames.push((g, r));
            }
        }
        self.lay = lay;
    }

    /// Where a port is drawn, by reference.
    fn port_pos(&self, p: &PortRef, side: Side) -> Option<Pos2> {
        let i = self.base.nodes.iter().position(|n| n.id == p.node)?;
        let n = &self.base.nodes[i];
        let k = match side {
            Side::Input => n.inputs.iter().position(|q| q.id == p.port)?,
            Side::Output => n.outputs.iter().position(|q| q.id == p.port)?,
        };
        self.lay.ports.get(&(i, side, k)).copied()
    }

    /// Whether ports are drawn and answer.
    fn ports_shown(&self, b: &Placed) -> bool {
        self.level != Level::Overview && b.node.is_some() && !b.collapsed
    }

    /// The scene's content.
    fn content(&mut self, ui: &mut Ui) {
        self.to_global = self
            .ctx
            .layer_transform_to_global(ui.layer_id())
            .unwrap_or(TSTransform::IDENTITY);
        self.lay.visible = ui.clip_rect();
        self.layout();
        let painter = ui.painter().clone();
        painter.rect_filled(self.lay.visible, 0.0, self.t.background);
        self.paint_grid(&painter);
        self.groups_ui(ui, &painter);
        self.paint_wires(&painter);
        for b in 0..self.lay.boxes.len() {
            let placed = self.lay.boxes[b].clone();
            match placed.node {
                Some(i) => self.node_ui(ui, &painter, &placed, i),
                None => self.stack_ui(ui, &painter, &placed),
            }
        }
        self.wire_ui(ui, &painter);
        if let Some([a, b]) = self.state.band {
            let r = Rect::from_two_pos(a, b);
            painter.rect_filled(r, 0.0, self.t.band.gamma_multiply(0.12));
            painter.rect_stroke(
                r,
                0.0,
                Stroke::new(1.0 / self.zoom, self.t.band),
                StrokeKind::Inside,
            );
        }
        self.state.drawn = state::Drawn {
            boxes: self
                .lay
                .boxes
                .iter()
                .map(|b| {
                    let sel = b
                        .node
                        .is_some_and(|i| self.state.selection.contains(&self.base.nodes[i].id));
                    (b.rect, sel)
                })
                .collect(),
            bounds: self
                .lay
                .boxes
                .iter()
                .fold(Rect::NOTHING, |r, b| r.union(b.rect)),
            visible: self.lay.visible,
            zoom: self.zoom,
        };
    }

    fn paint_grid(&self, painter: &egui::Painter) {
        let mut g = self.snap.unwrap_or(24.0);
        while g * self.zoom < 10.0 {
            g *= 4.0;
        }
        let v = self.lay.visible;
        let stroke = Stroke::new(1.0 / self.zoom, self.t.grid.gamma_multiply(0.5));
        let mut x = (v.left() / g).floor() * g;
        while x <= v.right() {
            painter.vline(x, v.y_range(), stroke);
            x += g;
        }
        let mut y = (v.top() / g).floor() * g;
        while y <= v.bottom() {
            painter.hline(v.x_range(), y, stroke);
            y += g;
        }
    }

    /// A wire's stroke width in graph units: about two points on screen.
    fn wire_width(&self) -> f32 {
        (2.0 / self.zoom).clamp(1.5, 12.0)
    }

    fn bezier(&self, painter: &egui::Painter, a: Pos2, b: Pos2, color: Color32) {
        let dx = ((b.x - a.x).abs() * 0.5).max(40.0);
        painter.add(egui::epaint::CubicBezierShape::from_points_stroke(
            [a, a + vec2(dx, 0.0), b - vec2(dx, 0.0), b],
            false,
            Color32::TRANSPARENT,
            Stroke::new(self.wire_width(), color),
        ));
    }

    fn paint_wires(&self, painter: &egui::Painter) {
        let mut seen: BTreeSet<[i64; 4]> = BTreeSet::new();
        for e in self.base.edges {
            if let Some(w) = &self.state.wire
                && w.detached.as_ref() == Some(e)
            {
                continue;
            }
            let (Some(a), Some(b)) = (
                self.port_pos(&e.from, Side::Output),
                self.port_pos(&e.to, Side::Input),
            ) else {
                continue;
            };
            // Inside one stack, or drawn twice between stacks.
            let same_box = self.box_of(&e.from.node).is_some()
                && self.box_of(&e.from.node) == self.box_of(&e.to.node);
            let key = [a.x, a.y, b.x, b.y].map(|v| (v * 4.0).round() as i64);
            if same_box || !seen.insert(key) {
                continue;
            }
            let color = self
                .base
                .port(&e.from, Side::Output)
                .map_or(self.t.wire, |(_, p)| self.t.type_color(&self.ctx, &p.ty));
            self.bezier(painter, a, b, color);
        }
    }

    fn box_of(&self, node: &str) -> Option<usize> {
        let i = self.base.nodes.iter().position(|n| n.id == node)?;
        self.lay.box_of[i]
    }

    /// The nodes of a group.
    fn members(&self, g: usize) -> Vec<String> {
        let id = &self.groups[g].id;
        self.base
            .nodes
            .iter()
            .filter(|n| n.group.as_deref() == Some(id.as_str()))
            .map(|n| n.id.clone())
            .collect()
    }

    /// Starts moving these nodes.
    fn start_move(&mut self, nodes: Vec<String>) {
        let nodes = nodes
            .into_iter()
            .filter_map(|id| {
                let n = self.base.node(&id)?;
                Some((id, n.pos))
            })
            .collect();
        self.state.moving = Some(Moving {
            nodes,
            offset: Vec2::ZERO,
        });
    }

    /// A drag of a box or a frame: moves, then one edit at its end.
    fn drag_move(&mut self, ui: &Ui, r: &Response) {
        if r.dragged_by(egui::PointerButton::Primary)
            && let Some(m) = &mut self.state.moving
        {
            m.offset += r.drag_delta();
        }
        if r.drag_stopped() {
            let cancelled = ui.input(|i| i.key_pressed(self.keys.cancel));
            if let Some(m) = self.state.moving.take()
                && !cancelled
                && m.offset != Vec2::ZERO
            {
                let moves = m
                    .nodes
                    .into_iter()
                    .map(|(node, from)| Moved {
                        to: self.snapped(pos2(from[0], from[1]) + m.offset),
                        node,
                        from,
                    })
                    .collect();
                self.propose(Edit::Move { moves }, Cause::Drag);
            }
        }
    }

    fn snapped(&self, p: Pos2) -> [f32; 2] {
        match self.snap {
            Some(g) => [(p.x / g).round() * g, (p.y / g).round() * g],
            None => [p.x, p.y],
        }
    }

    fn groups_ui(&mut self, ui: &mut Ui, painter: &egui::Painter) {
        let radius = CornerRadius::same(6);
        for (g, r) in self.lay.frames.clone() {
            painter.rect_filled(r, radius, self.t.group.gamma_multiply(0.45));
            painter.rect_stroke(
                r,
                radius,
                Stroke::new(1.0 / self.zoom.max(0.5), self.t.group_border),
                StrokeKind::Inside,
            );
            let band = Rect::from_min_size(r.min, vec2(r.width(), GROUP_LABEL));
            let label = self.groups[g].label.clone();
            painter.text(
                band.left_center() + vec2(PAD + 2.0, 0.0),
                egui::Align2::LEFT_CENTER,
                &label,
                FontId::proportional(Tokens::of(&self.ctx).font.small),
                self.t.text_muted,
            );
            let gid = self.id.with(("group", &self.groups[g].id));
            let resp = ui.interact(band, gid, Sense::click_and_drag());
            let name = tr_args(&self.ctx, &s::GRAPH_FOLD, &[("group", &label)]);
            a11y::name(&resp, egui::Role::Button, &name);
            self.bounds(gid, band);
            if resp.has_focus() {
                self.ring(painter, band);
            }
            if resp.clicked() {
                let id = self.groups[g].id.clone();
                self.toggle_fold(&id);
            }
            if resp.drag_started_by(egui::PointerButton::Primary) {
                let members = self.members(g);
                self.start_move(members);
            }
            self.drag_move(ui, &resp);
        }
    }

    fn ring(&self, painter: &egui::Painter, r: Rect) {
        painter.rect_stroke(
            r.expand(3.0 / self.zoom),
            CornerRadius::same(7),
            Stroke::new(2.0 / self.zoom.max(0.25), self.t.focus),
            StrokeKind::Outside,
        );
    }

    /// A title, cut to its width, at a size readable at this zoom.
    fn title(&self, painter: &egui::Painter, rect: Rect, text: &str, size: f32, color: Color32) {
        let mut job =
            LayoutJob::simple_singleline(text.to_string(), FontId::proportional(size), color);
        job.wrap = TextWrapping {
            max_width: rect.width().max(1.0),
            max_rows: 1,
            break_anywhere: true,
            overflow_character: Some('…'),
        };
        let galley = painter.layout_job(job);
        let pos = pos2(rect.left(), rect.center().y - galley.size().y / 2.0);
        painter.galley(pos, galley, color);
    }

    /// A node's height in the overview.
    fn overview_h(&self) -> f32 {
        let want = Tokens::of(&self.ctx).font.body * 1.8 / self.zoom.max(0.01);
        OVERVIEW_H
            .into_iter()
            .find(|h| *h >= want)
            .unwrap_or(OVERVIEW_H[2])
    }

    /// The title's font size: in the overview, near half the node's
    /// height (readable when zoomed out), else the body's.
    fn title_size(&self) -> f32 {
        if self.level == Level::Overview {
            (self.overview_h() * 0.45).round()
        } else {
            Tokens::of(&self.ctx).font.body
        }
    }

    fn stack_ui(&mut self, ui: &mut Ui, painter: &egui::Painter, b: &Placed) {
        let Some(g) = b.group else { return };
        let r = b.rect;
        let radius = CornerRadius::same(6);
        // The stack's depth: two edges behind.
        for k in [2.0, 1.0] {
            let off = vec2(6.0, 6.0) * k;
            painter.rect_filled(r.translate(off), radius, self.t.node_header);
            painter.rect_stroke(
                r.translate(off),
                radius,
                Stroke::new(1.0 / self.zoom.max(0.5), self.t.node_border),
                StrokeKind::Inside,
            );
        }
        painter.rect_filled(r, radius, self.t.node);
        let members = self.members(g);
        let selected = members.iter().any(|m| self.state.selection.contains(m));
        painter.rect_stroke(
            r,
            radius,
            Stroke::new(
                if selected { 2.0 } else { 1.0 } / self.zoom.max(0.5),
                if selected {
                    self.t.node_selected
                } else {
                    self.t.node_border
                },
            ),
            StrokeKind::Inside,
        );
        let label = self.groups[g].label.clone();
        let count = members.len().to_string();
        let size = self.title_size();
        let badge = format!("+{count}");
        let text_r = Rect::from_min_max(
            r.min + vec2(PAD * 2.0, 0.0),
            pos2(r.right() - PAD * 2.0 - size * 2.0, r.max.y),
        );
        self.title(painter, text_r, &label, size, self.t.text);
        painter.text(
            r.right_center() - vec2(PAD * 2.0, 0.0),
            egui::Align2::RIGHT_CENTER,
            badge,
            FontId::proportional(size * 0.85),
            self.t.text_muted,
        );
        let sid = self.id.with(("stack", &self.groups[g].id));
        let resp = ui.interact(r, sid, Sense::click_and_drag());
        let name = tr_args(
            &self.ctx,
            &s::GRAPH_UNFOLD,
            &[("group", &label), ("count", &count)],
        );
        a11y::name(&resp, egui::Role::Button, &name);
        let role = tr(&self.ctx, &s::GRAPH_ROLE_STACK);
        self.ctx.accesskit_node_builder(sid, |node| {
            node.set_role_description(role);
        });
        self.bounds(sid, r);
        if resp.has_focus() {
            self.ring(painter, r);
        }
        if resp.clicked() {
            let id = self.groups[g].id.clone();
            self.toggle_fold(&id);
        }
        if resp.drag_started_by(egui::PointerButton::Primary) {
            self.start_move(members);
        }
        self.drag_move(ui, &resp);
    }

    fn node_ui(&mut self, ui: &mut Ui, painter: &egui::Painter, b: &Placed, i: usize) {
        let base = self.base;
        let n = &base.nodes[i];
        let r = b.rect;
        let selected = self.state.selection.contains(&n.id);
        let radius = CornerRadius::same(6);
        let header = Rect::from_min_size(
            r.min,
            vec2(
                r.width(),
                if self.level == Level::Overview {
                    r.height()
                } else {
                    HEADER
                },
            ),
        );
        painter.rect_filled(r, radius, self.t.node);
        if self.level != Level::Overview {
            painter.rect_filled(
                header,
                CornerRadius {
                    nw: 6,
                    ne: 6,
                    sw: if b.collapsed { 6 } else { 0 },
                    se: if b.collapsed { 6 } else { 0 },
                },
                self.t.node_header,
            );
        }
        // The colour of what it gives, as a band on the left.
        if let Some(p) = n.outputs.first().or(n.inputs.first()) {
            let band = Rect::from_min_size(r.min, vec2(4.0, header.height()));
            painter.rect_filled(
                band,
                CornerRadius {
                    nw: 6,
                    sw: if self.level == Level::Overview || b.collapsed {
                        6
                    } else {
                        0
                    },
                    ne: 0,
                    se: 0,
                },
                self.t.type_color(&self.ctx, &p.ty),
            );
        }
        painter.rect_stroke(
            r,
            radius,
            Stroke::new(
                if selected { 2.5 } else { 1.0 } / self.zoom.max(0.5),
                if selected {
                    self.t.node_selected
                } else {
                    self.t.node_border
                },
            ),
            StrokeKind::Inside,
        );

        // The node: a click selects, a drag moves the selection.
        let nid = node_id(self.id, &n.id);
        let resp = ui.interact(r, nid, Sense::click_and_drag());
        let wires = base.edges.iter().filter(|e| e.touches(&n.id)).count();
        let value = tr_args(
            &self.ctx,
            &s::GRAPH_NODE_VALUE,
            &[
                ("inputs", &n.inputs.len().to_string()),
                ("outputs", &n.outputs.len().to_string()),
                ("wires", &wires.to_string()),
            ],
        );
        let enabled = ui.is_enabled();
        let title = n.title.clone();
        resp.widget_info(|| {
            let mut info = egui::WidgetInfo::labeled(egui::Role::Unknown, enabled, &title);
            info.current_text_value = Some(value.clone());
            info.selected = Some(selected);
            info
        });
        let role = tr(&self.ctx, &s::GRAPH_ROLE_NODE);
        self.ctx.accesskit_node_builder(nid, |node| {
            node.set_role(Role::Group);
            node.set_role_description(role);
            node.set_selected(selected);
        });
        self.bounds(nid, r);
        if resp.has_focus() {
            self.focused = Some(i);
            self.ring(painter, r);
            lock_keys(ui, nid, false);
        }
        if resp.gained_focus() {
            self.reveal = Some(r);
        }
        if resp.clicked() {
            resp.request_focus();
            self.click_select(ui, &n.id);
            if self.state.wire.as_ref().is_some_and(|w| !w.by_pointer) {
                self.state.wire = None;
            }
        }
        if resp.double_clicked() && self.level != Level::Overview {
            self.toggle_collapsed(&n.id);
        }
        if resp.drag_started_by(egui::PointerButton::Primary) {
            if !selected {
                self.state.selection = BTreeSet::from([n.id.clone()]);
            }
            let list = self.state.selection.iter().cloned().collect();
            self.start_move(list);
        }
        self.drag_move(ui, &resp);
        self.node_menu(&resp, n);

        // The title bar: the chevron, the title, the application's slot.
        let mut text_left = r.left() + PAD * 2.0;
        if self.level != Level::Overview {
            let c = Rect::from_center_size(
                pos2(r.left() + PAD + CHEVRON / 2.0 + 2.0, header.center().y),
                Vec2::splat(CHEVRON),
            );
            let icon = if b.collapsed {
                Icon::ChevronRight
            } else {
                Icon::ChevronDown
            };
            let cid = nid.with("chevron");
            let cr = ui.interact(c, cid, Sense::click());
            icon.paint(
                painter,
                c,
                if cr.hovered() {
                    self.t.text
                } else {
                    self.t.text_muted
                },
            );
            let key = if b.collapsed {
                &s::GRAPH_EXPAND
            } else {
                &s::GRAPH_COLLAPSE
            };
            a11y::name(
                &cr,
                egui::Role::Button,
                &tr_args(&self.ctx, key, &[("node", &n.title)]),
            );
            self.bounds(cid, c);
            if cr.has_focus() {
                self.ring(painter, c);
            }
            if cr.clicked() {
                self.toggle_collapsed(&n.id);
            }
            text_left = c.right() + PAD;
        }
        let slot_w = if self.header.is_some() && self.level != Level::Overview {
            72.0
        } else {
            0.0
        };
        let title_r = Rect::from_min_max(
            pos2(text_left, header.top()),
            pos2(header.right() - PAD - slot_w, header.bottom()),
        );
        self.title(painter, title_r, &n.title, self.title_size(), self.t.text);
        if slot_w > 0.0
            && let Some(h) = self.header.as_mut()
        {
            let slot = Rect::from_min_max(
                pos2(header.right() - PAD - slot_w, header.top()),
                pos2(header.right() - PAD, header.bottom()),
            );
            ui.scope_builder(
                UiBuilder::new()
                    .max_rect(slot)
                    .id_salt(("header", &n.id))
                    .layout(egui::Layout::right_to_left(egui::Align::Center)),
                |ui| h(ui, n),
            );
        }

        if !self.ports_shown(b) {
            return;
        }
        self.ports_ui(ui, painter, i, r);
        if self.level == Level::Detail {
            self.params_ui(ui, i, r);
        }
    }

    fn click_select(&mut self, ui: &Ui, node: &str) {
        let m = ui.input(|i| i.modifiers);
        if m.command || m.shift {
            if !self.state.selection.remove(node) {
                self.state.selection.insert(node.to_string());
            }
        } else {
            self.state.selection = BTreeSet::from([node.to_string()]);
        }
    }

    fn toggle_collapsed(&mut self, node: &str) {
        if !self.state.collapsed.remove(node) {
            self.state.collapsed.insert(node.to_string());
        }
    }

    fn node_menu(&mut self, resp: &Response, n: &Node) {
        let ctx = self.ctx.clone();
        let mut chosen = None;
        let _ = resp.context_menu(|ui| {
            if ui.button(tr(&ctx, &s::GRAPH_DUPLICATE)).clicked() {
                chosen = Some(0);
                ui.close();
            }
            if ui.button(tr(&ctx, &s::GRAPH_DELETE)).clicked() {
                chosen = Some(1);
                ui.close();
            }
        });
        if let Some(c) = chosen {
            if !self.state.selection.contains(&n.id) {
                self.state.selection = BTreeSet::from([n.id.clone()]);
            }
            if c == 0 {
                self.duplicate(Cause::Command);
            } else {
                self.delete(Cause::Command);
            }
        }
    }

    fn ports_ui(&mut self, ui: &mut Ui, painter: &egui::Painter, i: usize, r: Rect) {
        let base = self.base;
        let n = &base.nodes[i];
        let small = FontId::proportional(Tokens::of(&self.ctx).font.small);
        let order: Vec<(Side, usize)> = (0..n.inputs.len())
            .map(|k| (Side::Input, k))
            .chain((0..n.outputs.len()).map(|k| (Side::Output, k)))
            .collect();
        for (o, &(side, k)) in order.iter().enumerate() {
            let port = match side {
                Side::Input => &n.inputs[k],
                Side::Output => &n.outputs[k],
            };
            let Some(&at) = self.lay.ports.get(&(i, side, k)) else {
                continue;
            };
            let pref = PortRef::new(n.id.clone(), port.id.clone());
            let color = self.t.type_color(&self.ctx, &port.ty);
            // While a wire is drawn: lit where it may end, dimmed where
            // it may not.
            let fit = self.state.wire.as_ref().map(|w| {
                w.from == pref && w.side == side
                    || base
                        .check(&(w.from.clone(), w.side), &(pref.clone(), side))
                        .is_ok()
            });
            let fill = if fit == Some(false) {
                color.gamma_multiply(0.3)
            } else {
                color
            };
            let wired = !base.wires_at(&pref, side).is_empty();
            painter.circle(
                at,
                PORT_R,
                if wired { fill } else { self.t.node },
                Stroke::new(1.5, fill),
            );
            if fit == Some(true) {
                painter.circle_stroke(at, PORT_R + 3.0, Stroke::new(1.5, self.t.wire_preview));
            }
            let (text_at, align) = match side {
                Side::Input => (at + vec2(PORT_R + 5.0, 0.0), egui::Align2::LEFT_CENTER),
                Side::Output => (at - vec2(PORT_R + 5.0, 0.0), egui::Align2::RIGHT_CENTER),
            };
            painter.text(
                text_at,
                align,
                &port.label,
                small.clone(),
                self.t.text_muted,
            );

            let pid = port_id(self.id, &pref, side);
            let hit = Rect::from_center_size(at, Vec2::splat(PORT_HIT));
            let pr = ui.interact(hit, pid, Sense::click_and_drag());
            let name = port_label(&self.ctx, n, port, side);
            let value = tr_args(
                &self.ctx,
                &s::GRAPH_PORT_VALUE,
                &[
                    ("type", &port.ty.name),
                    ("wires", &base.wires_at(&pref, side).len().to_string()),
                ],
            );
            let enabled = ui.is_enabled();
            pr.widget_info(|| {
                let mut info = egui::WidgetInfo::labeled(egui::Role::Button, enabled, &name);
                info.current_text_value = Some(value.clone());
                info
            });
            let role = tr(
                &self.ctx,
                match side {
                    Side::Input => &s::GRAPH_ROLE_INPUT,
                    Side::Output => &s::GRAPH_ROLE_OUTPUT,
                },
            );
            self.ctx.accesskit_node_builder(pid, |node| {
                node.set_role_description(role);
            });
            self.bounds(pid, hit);
            if pr.hovered() && self.state.wire.is_none() {
                painter.circle_stroke(at, PORT_R + 3.0, Stroke::new(1.0, color));
            }
            if pr.has_focus() {
                painter.circle_stroke(
                    at,
                    PORT_R + 4.0,
                    Stroke::new(2.0 / self.zoom.max(0.25), self.t.focus),
                );
                lock_keys(ui, pid, true);
                self.port_keys(ui, i, &order, o, &pref, side);
            }
            if pr.gained_focus() {
                self.reveal = Some(r);
            }
            if pr.drag_started_by(egui::PointerButton::Primary) {
                self.start_wire(&pref, side, port.multiple, true);
            }
            if pr.clicked() {
                self.click_port(&pref, side, &name);
            }
            self.port_menu(&pr, &pref, side);
        }
    }

    /// A drag begins at a port: a wire from it, or the wire of a
    /// connected single input, taken off it.
    fn start_wire(&mut self, p: &PortRef, side: Side, multiple: bool, by_pointer: bool) {
        let mut wire = Wire {
            from: p.clone(),
            side,
            by_pointer,
            detached: None,
        };
        if side == Side::Input
            && !multiple
            && let Some(e) = self.base.wires_at(p, side).into_iter().next()
        {
            wire.from = e.from.clone();
            wire.side = Side::Output;
            wire.detached = Some(e);
        }
        self.state.wire = Some(wire);
    }

    /// A click on a port (or Enter, Space, an assistive technology's
    /// click): begins a wire, or ends the one begun.
    fn click_port(&mut self, p: &PortRef, side: Side, name: &str) {
        match self.state.wire.take() {
            Some(w) if w.from == *p && w.side == side => {}
            Some(w) if !w.by_pointer => self.finish_wire(&w, p, side, Cause::Click),
            _ => {
                self.state.wire = Some(Wire {
                    from: p.clone(),
                    side,
                    by_pointer: false,
                    detached: None,
                });
                a11y::announce(
                    &self.ctx,
                    tr_args(&self.ctx, &s::GRAPH_WIRE_STARTED, &[("port", name)]),
                    Politeness::Polite,
                );
            }
        }
    }

    /// Ends a wire at a port: an edit, or a refusal.
    fn finish_wire(&mut self, w: &Wire, p: &PortRef, side: Side, cause: Cause) {
        let a = (w.from.clone(), w.side);
        let b = (p.clone(), side);
        match self.base.connect(&a, &b, w.detached.as_ref()) {
            Ok(edit) => {
                if let Edit::Connect { from, to, .. } = &edit {
                    a11y::announce(
                        &self.ctx,
                        tr_args(
                            &self.ctx,
                            &s::GRAPH_CONNECTED,
                            &[
                                ("from", &self.base.port_name(&self.ctx, from)),
                                ("to", &self.base.port_name(&self.ctx, to)),
                            ],
                        ),
                        Politeness::Polite,
                    );
                }
                self.propose(edit, cause);
            }
            // The same wire again: nothing to do.
            Err(Refusal::Exists) => {}
            Err(refusal) => {
                let (from, to) = if a.1 == Side::Output {
                    (a.0, b.0)
                } else {
                    (b.0, a.0)
                };
                refuse(&self.ctx, &self.base, &from, &to, &refusal, self.events);
            }
        }
    }

    fn port_menu(&mut self, pr: &Response, p: &PortRef, side: Side) {
        let wires = self.base.wires_at(p, side);
        if wires.is_empty() {
            return;
        }
        let ctx = self.ctx.clone();
        let mut chosen = false;
        let _ = pr.context_menu(|ui| {
            if ui.button(tr(&ctx, &s::GRAPH_DISCONNECT)).clicked() {
                chosen = true;
                ui.close();
            }
        });
        if chosen {
            self.propose(Edit::Disconnect { edges: wires }, Cause::Command);
        }
    }

    /// Keys on a focused port: the arrows go through the node's ports,
    /// Escape cancels a wire or goes back to the node, Delete
    /// disconnects.
    fn port_keys(
        &mut self,
        ui: &Ui,
        i: usize,
        order: &[(Side, usize)],
        o: usize,
        p: &PortRef,
        side: Side,
    ) {
        let base = self.base;
        let n = &base.nodes[i];
        let (next, prev, cancel, delete) = ui.input_mut(|inp| {
            let next = inp.consume_key(Modifiers::NONE, Key::ArrowDown)
                || inp.consume_key(Modifiers::NONE, Key::ArrowRight);
            let prev = inp.consume_key(Modifiers::NONE, Key::ArrowUp)
                || inp.consume_key(Modifiers::NONE, Key::ArrowLeft);
            let cancel = inp.consume_key(Modifiers::NONE, self.keys.cancel);
            let delete = self
                .keys
                .delete
                .iter()
                .any(|k| inp.consume_key(Modifiers::NONE, *k));
            (next, prev, cancel, delete)
        });
        let len = order.len();
        let go = if next {
            Some((o + 1) % len)
        } else if prev {
            Some((o + len - 1) % len)
        } else {
            None
        };
        if let Some(t) = go {
            let (s2, k) = order[t];
            let port = match s2 {
                Side::Input => &n.inputs[k],
                Side::Output => &n.outputs[k],
            };
            let target = port_id(self.id, &PortRef::new(n.id.clone(), port.id.clone()), s2);
            ui.memory_mut(|m| m.request_focus(target));
        }
        if cancel && self.state.wire.take().is_none() {
            ui.memory_mut(|m| m.request_focus(node_id(self.id, &n.id)));
        }
        if delete {
            let wires = base.wires_at(p, side);
            if !wires.is_empty() {
                self.propose(Edit::Disconnect { edges: wires }, Cause::Key);
            }
        }
    }

    fn params_ui(&mut self, ui: &mut Ui, i: usize, r: Rect) {
        let base = self.base;
        let n = &base.nodes[i];
        let rows = n.inputs.len().max(n.outputs.len());
        let h = param_h(&self.ctx);
        let mut y = r.top() + HEADER + rows as f32 * ROW;
        for p in base.model.params(&n.id) {
            let row = Rect::from_min_size(
                pos2(r.left() + PAD * 2.0, y + 2.0),
                vec2(r.width() - PAD * 4.0, h - 4.0),
            );
            y += h;
            let mut v = p.value;
            let hook = &mut *self.hook;
            let node = n.id.clone();
            let param = p.id.clone();
            let out = ui
                .scope_builder(
                    UiBuilder::new()
                        .max_rect(row)
                        .id_salt(("param", &n.id, &p.id)),
                    |ui| {
                        let mut sl = Slider::new(&mut v, &p.label, p.range.clone());
                        if let Some(step) = p.step {
                            sl = sl.step(step);
                        }
                        if let Some(u) = &p.unit {
                            sl = sl.unit(u);
                        }
                        if let Some(d) = p.default {
                            sl = sl.default(d);
                        }
                        sl.on_change(|c: &slider::Change| {
                            let edit = Edit::Param {
                                node: node.clone(),
                                param: param.clone(),
                                from: c.from,
                                to: c.to,
                                done: c.done,
                            };
                            let cause = Cause::of_slider(c.cause);
                            match hook.as_mut().map_or(Verdict::Allow, |h| h(&edit, cause)) {
                                Verdict::Allow => slider::Verdict::Go,
                                Verdict::Veto => slider::Verdict::Veto,
                                Verdict::Redirect(Edit::Param { to, .. }) => {
                                    slider::Verdict::Redirect(to)
                                }
                                Verdict::Redirect(_) => slider::Verdict::Veto,
                            }
                        })
                        .show(ui)
                    },
                )
                .inner;
            self.bounds(out.response.id, out.response.rect);
            for c in out.changes {
                // Already through the hook.
                self.edits.push(Edit::Param {
                    node: n.id.clone(),
                    param: p.id.clone(),
                    from: c.from,
                    to: c.to,
                    done: c.done,
                });
            }
        }
    }

    /// The wire being drawn, and its end.
    fn wire_ui(&mut self, ui: &Ui, painter: &egui::Painter) {
        let Some(w) = self.state.wire.clone() else {
            return;
        };
        if ui.input(|i| i.key_pressed(self.keys.cancel)) {
            self.state.wire = None;
            return;
        }
        let Some(start) = self.port_pos(&w.from, w.side) else {
            self.state.wire = None;
            return;
        };
        let pointer = self.pointer();
        let target = pointer.and_then(|p| self.port_near(p));
        let color = match &target {
            Some((q, side))
                if self
                    .base
                    .check(&(w.from.clone(), w.side), &(q.clone(), *side))
                    .is_err() =>
            {
                self.t.wire_refused
            }
            _ => self.t.wire_preview,
        };
        let hovering = ui.rect_contains_pointer(self.lay.visible);
        if let Some(p) = pointer
            && (w.by_pointer || hovering)
        {
            let end = target
                .as_ref()
                .and_then(|(q, side)| self.port_pos(q, *side))
                .unwrap_or(p);
            let (a, b) = if w.side == Side::Output {
                (start, end)
            } else {
                (end, start)
            };
            self.bezier(painter, a, b, color);
        }
        if w.by_pointer && ui.input(|i| i.pointer.any_released()) {
            self.state.wire = None;
            match target {
                Some((q, side)) if q != w.from || side != w.side => {
                    self.finish_wire(&w, &q, side, Cause::Drag);
                }
                Some(_) => {}
                None => {
                    if let Some(e) = w.detached {
                        self.propose(Edit::Disconnect { edges: vec![e] }, Cause::Drag);
                    }
                }
            }
        }
    }

    /// The port under a point, if one is near enough.
    fn port_near(&self, p: Pos2) -> Option<(PortRef, Side)> {
        let mut best: Option<(f32, PortRef, Side)> = None;
        for (&(i, side, k), &at) in &self.lay.ports {
            let Some(b) = self.lay.box_of[i] else {
                continue;
            };
            if !self.ports_shown(&self.lay.boxes[b]) {
                continue;
            }
            let d = at.distance(p);
            if d <= DROP && best.as_ref().is_none_or(|(bd, _, _)| d < *bd) {
                let n = &self.base.nodes[i];
                let port = match side {
                    Side::Input => &n.inputs[k],
                    Side::Output => &n.outputs[k],
                };
                best = Some((d, PortRef::new(n.id.clone(), port.id.clone()), side));
            }
        }
        best.map(|(_, q, s)| (q, s))
    }

    fn duplicate(&mut self, cause: Cause) {
        let off = self.snap.unwrap_or(10.0).max(10.0) * 2.0;
        let nodes: Vec<NewNode> = self
            .state
            .selection
            .iter()
            .filter_map(|id| self.base.node(id))
            .map(|n| NewNode {
                kind: n.kind.clone(),
                at: self.snapped(pos2(n.pos[0] + off, n.pos[1] + off)),
                copy_of: Some(n.id.clone()),
            })
            .collect();
        if !nodes.is_empty() {
            self.propose(Edit::Add { nodes }, cause);
        }
    }

    fn delete(&mut self, cause: Cause) {
        let nodes: Vec<String> = self
            .state
            .selection
            .iter()
            .filter(|id| self.base.node(id).is_some())
            .cloned()
            .collect();
        if nodes.is_empty() {
            return;
        }
        let edges = self
            .base
            .edges
            .iter()
            .filter(|e| nodes.iter().any(|n| e.touches(n)))
            .cloned()
            .collect();
        self.propose(Edit::Delete { nodes, edges }, cause);
    }

    /// The canvas: its name, the rubber band, a click, a double click,
    /// its menu, and the keys on it or on a node.
    fn canvas(&mut self, response: &Response, label: &str) {
        let ctx = self.ctx.clone();
        let value = tr_args(
            &ctx,
            &s::GRAPH_VALUE,
            &[
                ("nodes", &self.base.nodes.len().to_string()),
                ("wires", &self.base.edges.len().to_string()),
                (
                    "level",
                    &tr(
                        &ctx,
                        match self.level {
                            Level::Overview => &s::GRAPH_LEVEL_OVERVIEW,
                            Level::Ports => &s::GRAPH_LEVEL_PORTS,
                            Level::Detail => &s::GRAPH_LEVEL_DETAIL,
                        },
                    ),
                ),
            ],
        );
        let label = label.to_string();
        response.widget_info(|| {
            let mut info = egui::WidgetInfo::labeled(egui::Role::Unknown, true, &label);
            info.current_text_value = Some(value.clone());
            info
        });
        ctx.accesskit_node_builder(response.id, |node| node.set_role(Role::Group));
        self.bounds(response.id, self.lay.visible);
        let to_scene = self.to_global.inverse();
        let pointer = ctx.input(|i| i.pointer.latest_pos()).map(|p| to_scene * p);
        let panning = ctx.input(|i| i.key_down(Key::Space));
        // Space held: a drag pans.
        if panning && response.dragged_by(egui::PointerButton::Primary) {
            self.state.pan(-response.drag_delta());
        } else if response.drag_started_by(egui::PointerButton::Primary)
            && let Some(p) = pointer
        {
            self.state.band = Some([p, p]);
        }
        if let Some(band) = &mut self.state.band
            && let Some(p) = pointer
        {
            band[1] = p;
        }
        if response.drag_stopped()
            && let Some([a, b]) = self.state.band.take()
        {
            let r = Rect::from_two_pos(a, b);
            let add = ctx.input(|i| i.modifiers.shift || i.modifiers.command);
            if !add {
                self.state.selection.clear();
            }
            for placed in &self.lay.boxes {
                if !placed.rect.intersects(r) {
                    continue;
                }
                match (placed.node, placed.group) {
                    (Some(i), _) => {
                        self.state.selection.insert(self.base.nodes[i].id.clone());
                    }
                    (None, Some(g)) => {
                        let members = self.members(g);
                        self.state.selection.extend(members);
                    }
                    _ => {}
                }
            }
        }
        if response.clicked() {
            response.request_focus();
            if !ctx.input(|i| i.modifiers.shift || i.modifiers.command) {
                self.state.selection.clear();
            }
            self.state.wire = None;
        }
        if response.double_clicked() {
            self.state.fit = true;
        }
        if response.secondary_clicked() {
            self.state.menu_at = pointer;
        }
        let kinds = self.base.model.kinds();
        if !kinds.is_empty() {
            let mut chosen = None;
            let _ = response.context_menu(|ui| {
                ui.label(tr(&ctx, &s::GRAPH_ADD_NODE));
                for k in &kinds {
                    if ui.button(&k.title).clicked() {
                        chosen = Some(k.id.clone());
                        ui.close();
                    }
                }
            });
            if let Some(kind) = chosen {
                let at = self
                    .state
                    .menu_at
                    .unwrap_or_else(|| self.lay.visible.center());
                self.propose(
                    Edit::Add {
                        nodes: vec![NewNode {
                            kind,
                            at: self.snapped(at),
                            copy_of: None,
                        }],
                    },
                    Cause::Command,
                );
            }
        }
        if response.has_focus() {
            ctx.memory_mut(|m| {
                m.set_focus_lock_filter(
                    response.id,
                    egui::EventFilter {
                        horizontal_arrows: true,
                        vertical_arrows: true,
                        ..Default::default()
                    },
                );
            });
            let arrow = ctx.input_mut(|i| {
                [
                    Key::ArrowLeft,
                    Key::ArrowRight,
                    Key::ArrowUp,
                    Key::ArrowDown,
                ]
                .into_iter()
                .any(|k| i.consume_key(Modifiers::NONE, k))
            });
            if arrow {
                // Into the graph: the node nearest the view's centre.
                let c = self.lay.visible.center();
                if let Some(i) = self.nearest(c, None) {
                    ctx.memory_mut(|m| m.request_focus(node_id(self.id, &self.base.nodes[i].id)));
                }
            }
        }
        if response.has_focus() || self.focused.is_some() {
            self.graph_keys();
        }
    }

    /// The node nearest a point (that way from it, when a direction is
    /// given), other than the one there.
    fn nearest(&self, from: Pos2, dir: Option<Vec2>) -> Option<usize> {
        let mut best: Option<(f32, usize)> = None;
        for b in &self.lay.boxes {
            let Some(i) = b.node else { continue };
            let d = b.rect.center() - from;
            if d.length() < 1.0 && dir.is_some() {
                continue;
            }
            let score = match dir {
                Some(dir) => {
                    let along = d.dot(dir);
                    if along <= 0.0 {
                        continue;
                    }
                    let across = (d - dir * along).length();
                    along + 2.0 * across
                }
                None => d.length(),
            };
            if best.is_none_or(|(s, _)| score < s) {
                best = Some((score, i));
            }
        }
        best.map(|(_, i)| i)
    }

    /// The keys on the canvas or on a node.
    fn graph_keys(&mut self) {
        let ctx = self.ctx.clone();
        let k = self.keys;
        let (delete, duplicate, all, fit) = ctx.input_mut(|i| {
            (
                k.delete
                    .iter()
                    .any(|key| i.consume_key(Modifiers::NONE, *key)),
                i.consume_key(Modifiers::COMMAND, k.duplicate),
                i.consume_key(Modifiers::COMMAND, k.select_all),
                i.consume_key(Modifiers::NONE, k.fit),
            )
        });
        let base = self.base;
        if let Some(i) = self.focused {
            let id = &base.nodes[i].id;
            if (delete || duplicate) && !self.state.selection.contains(id) {
                self.state.selection = BTreeSet::from([id.clone()]);
            }
            // The arrows: Shift moves the selection, alone they go to
            // the nearest node that way.
            let dirs = [
                (Key::ArrowLeft, vec2(-1.0, 0.0)),
                (Key::ArrowRight, vec2(1.0, 0.0)),
                (Key::ArrowUp, vec2(0.0, -1.0)),
                (Key::ArrowDown, vec2(0.0, 1.0)),
            ];
            for (key, dir) in dirs {
                let nudged = ctx.input_mut(|inp| inp.consume_key(k.nudge, key));
                if nudged {
                    if !self.state.selection.contains(id) {
                        self.state.selection = BTreeSet::from([id.clone()]);
                    }
                    let step = self.snap.unwrap_or(10.0);
                    let moves = self
                        .state
                        .selection
                        .iter()
                        .filter_map(|s| base.node(s))
                        .map(|n| Moved {
                            node: n.id.clone(),
                            from: n.pos,
                            to: self.snapped(pos2(n.pos[0], n.pos[1]) + dir * step),
                        })
                        .collect();
                    self.propose(Edit::Move { moves }, Cause::Key);
                    continue;
                }
                if ctx.input_mut(|inp| inp.consume_key(Modifiers::NONE, key)) {
                    let from = self.lay.box_of[i].map(|b| self.lay.boxes[b].rect.center());
                    if let Some(from) = from
                        && let Some(t) = self.nearest(from, Some(dir))
                    {
                        let target = node_id(self.id, &base.nodes[t].id);
                        ctx.memory_mut(|m| m.request_focus(target));
                    }
                }
            }
        }
        if delete {
            self.delete(Cause::Key);
        }
        if duplicate {
            self.duplicate(Cause::Key);
        }
        if all {
            self.state.selection = base.nodes.iter().map(|n| n.id.clone()).collect();
        }
        if fit {
            self.state.fit = true;
        }
    }
}

/// A parameter row's height.
fn param_h(ctx: &egui::Context) -> f32 {
    ctx.global_style().spacing.interact_size.y + 4.0
}

/// Keeps the arrows (and Escape, on a port) on a focused part of the
/// graph: egui would move the focus, or drop it.
fn lock_keys(ui: &Ui, id: Id, escape: bool) {
    ui.memory_mut(|m| {
        m.set_focus_lock_filter(
            id,
            egui::EventFilter {
                horizontal_arrows: true,
                vertical_arrows: true,
                escape,
                ..Default::default()
            },
        );
    });
}

/// The graph as drawn: nodes (place, ports and their wires, group,
/// collapsed, selected, parameters), wires, groups, the selection, the
/// view and the level. What `graph.read` answers.
pub fn describe(
    model: &dyn GraphModel,
    nodes: &[Node],
    edges: &[Edge],
    state: &GraphState,
) -> Json {
    let port = |p: &Port| {
        json!({
            "id": p.id,
            "label": p.label,
            "type": p.ty.name,
            "multiple": p.multiple,
        })
    };
    let v = state.drawn.visible;
    json!({
        "nodes": nodes
            .iter()
            .map(|n| json!({
                "id": n.id,
                "kind": n.kind,
                "title": n.title,
                "pos": [n.pos[0], n.pos[1]],
                "group": n.group,
                "collapsed": state.collapsed.contains(&n.id),
                "selected": state.selection.contains(&n.id),
                "inputs": n.inputs.iter().map(port).collect::<Vec<_>>(),
                "outputs": n.outputs.iter().map(port).collect::<Vec<_>>(),
                "params": model
                    .params(&n.id)
                    .iter()
                    .map(|p| json!({ "id": p.id, "label": p.label, "value": p.value }))
                    .collect::<Vec<_>>(),
            }))
            .collect::<Vec<_>>(),
        "edges": edges.iter().map(Edge::to_json).collect::<Vec<_>>(),
        "groups": model
            .groups()
            .iter()
            .map(|g| json!({
                "id": g.id,
                "label": g.label,
                "folded": state.folded.contains(&g.id),
                "unfolded": state.unfolded.contains(&g.id),
            }))
            .collect::<Vec<_>>(),
        "kinds": model
            .kinds()
            .iter()
            .map(|k| json!({ "id": k.id, "title": k.title }))
            .collect::<Vec<_>>(),
        "selection": state.selection.iter().collect::<Vec<_>>(),
        "level": state.level.map(Level::name),
        "zoom": state.drawn.zoom,
        "visible": [v.min.x, v.min.y, v.max.x, v.max.y],
        "wiring": state.wire.as_ref().map(|w| json!({ "from": w.from.to_json(), "side": w.side.name() })),
    })
}

/// The number of a zoom in percent, as the interface writes numbers.
pub(crate) fn zoom_text(ctx: &egui::Context, zoom: f32) -> String {
    strings::number(ctx, f64::from(zoom) * 100.0, 0)
}
