//! The element picker behind devtools' inspect mode: while it is on, the
//! widget under the pointer is outlined with a small label, a click
//! picks it instead of reaching it, Tab and the arrows move between
//! widgets and Enter picks, Escape leaves. A pick says what egui knows of
//! the widget ([`Pick`]); the application side (`egui_pop_api`'s
//! devtools) adds what it knows and shows it.
//!
//! The hit test reads the widget rectangles of the pass just drawn
//! (egui's `ViewportState::this_pass`), in every window: the top-most
//! layer holding a rectangle under the point, then the smallest such
//! rectangle, the one drawn last on a tie. Every `Ui` registers a
//! rectangle, so what paints without one of its own (a custom-painted
//! cell, a mark on a canvas) is named by its nearest container, and the
//! pick says so (`kind: "container"`). Names, roles and values come from
//! the AccessKit tree of the same pass, switched on while the mode is.
//!
//! What draws a widget is recorded only while a context inspects:
//! [`region`] (a zone's block, a panel: what the application side
//! records around what it draws) and the kit's parts painted through a
//! component's handler (with the state their box was taken in).
//!
//! ```
//! use egui_pop_core::inspect;
//!
//! let ctx = egui::Context::default();
//! inspect::start(&ctx);
//! assert!(inspect::active(&ctx));
//! inspect::stop(&ctx);
//! assert!(!inspect::active(&ctx));
//! ```

use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};

use egui::emath::TSTransform;
use egui::{
    Color32, Context, Event, FontId, Id, Key, LayerId, Order, Painter, Pos2, Rect, Stroke,
    StrokeKind, Ui, ViewportId, WidgetRect,
};
use serde::Serialize;

use crate::kit::{BoxStyle, State as KitState};
use crate::state::{lock, shared};
use crate::theme::Tokens;

/// How many contexts are inspecting: what records parts and regions
/// checks it first, so that nothing is locked while none is.
static INSPECTING: AtomicUsize = AtomicUsize::new(0);

/// A part of the interface the application drew around what it holds
/// (a zone's block, a panel), recorded while a context inspects.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Region {
    /// `block` or `panel`.
    pub kind: String,
    /// The block's or the panel's id.
    pub id: String,
    /// The zone a block is drawn in.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub zone: Option<String>,
    /// The plugin that registered it.
    pub owner: String,
    #[serde(skip)]
    layer: Option<LayerId>,
    #[serde(skip)]
    rect: Rect,
}

/// A part painted through a kit component's handler.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct KitPart {
    /// The component (`button`, `list-row`).
    pub component: String,
    /// The handler that drew it (`box`, `underline`).
    pub shape: String,
    /// The states its box was taken in (`hover+focus`, `rest`).
    pub state: String,
    /// The box: fill, border, radius, text.
    pub fill: String,
    pub border: String,
    pub radius: f32,
    pub text: String,
    /// The part lies around the widget (a row around a cell) rather than
    /// being the widget's own box.
    pub around: bool,
    #[serde(skip)]
    layer: Option<LayerId>,
    #[serde(skip)]
    rect: Rect,
}

/// What the picker knows of a widget.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Pick {
    /// egui's id, in hexadecimal.
    pub id: String,
    /// `widget`, or `container` when nothing smaller registered a
    /// rectangle there: what is painted inside is named by the container
    /// holding it.
    pub kind: String,
    /// The AccessKit role (`Button`, `Label`), when the widget has a
    /// node.
    pub role: Option<String>,
    /// The accessible name.
    pub label: Option<String>,
    pub value: Option<String>,
    pub description: Option<String>,
    /// The nearest ancestor with a name, as `Role "name"`.
    pub within: Option<String>,
    /// What it senses: `click`, `drag`, `focusable`, `hover`.
    pub sense: String,
    pub enabled: bool,
    pub focused: bool,
    /// Its rectangle in the window, in points: x, y, width, height.
    pub rect: [f32; 4],
    /// The egui layer it is drawn on (`Middle 1A2B`).
    pub layer: String,
    #[serde(skip)]
    pub viewport: ViewportId,
    /// The application's parts around it, outermost first.
    pub regions: Vec<Region>,
    /// The kit part that drew its box, if one was painted through a
    /// handler.
    pub kit: Option<KitPart>,
    /// `pointer`, `keyboard` or `accesskit`.
    pub how: String,
}

/// What the input hook took for the end of the pass.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Act {
    Click(Pos2),
    Next,
    Previous,
    Pick,
    Node(u64),
}

#[derive(Debug, Default)]
struct Window {
    acts: Vec<Act>,
    /// A press taken: its release is taken too.
    pressed: bool,
    regions: Vec<Region>,
    parts: Vec<KitPart>,
}

#[derive(Default)]
struct State {
    on: bool,
    /// The devtools setting is on: the platform's copy chord with Shift
    /// (egui-winit's `Copy` event) is given back as the key it was, so
    /// that Ctrl+Shift+C can be bound.
    armed: bool,
    /// Whether AccessKit was on when the mode started.
    accesskit_before: Option<bool>,
    windows: HashMap<ViewportId, Window>,
    /// The modifiers held in each window, while armed.
    held: HashMap<ViewportId, egui::Modifiers>,
    /// The keyboard's widget, while it is the one shown.
    cursor: Option<(ViewportId, Id)>,
    /// The pointer's place when the keyboard last moved.
    cursor_at: Option<Pos2>,
    /// Picks not yet taken by the application side.
    picked: Option<Pick>,
    last: Option<Pick>,
    left: bool,
}

impl std::fmt::Debug for State {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Inspect")
            .field("on", &self.on)
            .field("armed", &self.armed)
            .field("last", &self.last.as_ref().map(|p| p.id.as_str()))
            .finish_non_exhaustive()
    }
}

fn state(ctx: &Context) -> std::sync::Arc<std::sync::Mutex<State>> {
    shared::<State>(ctx, "core.inspect")
}

/// Starts the mode in every window of `ctx`.
pub fn start(ctx: &Context) {
    ctx.add_plugin(Hook);
    let accesskit = ctx.viewport(|vp| vp.this_pass.accesskit_state.is_some());
    let st = state(ctx);
    let mut s = lock(&st);
    if s.on {
        return;
    }
    s.on = true;
    s.left = false;
    s.cursor = None;
    s.accesskit_before = Some(accesskit);
    s.windows.clear();
    drop(s);
    INSPECTING.fetch_add(1, Ordering::SeqCst);
    // Names and roles come from the AccessKit tree.
    ctx.enable_accesskit();
    ctx.request_repaint();
}

/// Leaves the mode.
pub fn stop(ctx: &Context) {
    let st = state(ctx);
    let mut s = lock(&st);
    leave(ctx, &mut s);
}

fn leave(ctx: &Context, s: &mut State) {
    if !s.on {
        return;
    }
    s.on = false;
    s.left = true;
    s.cursor = None;
    s.windows.clear();
    INSPECTING.fetch_sub(1, Ordering::SeqCst);
    if s.accesskit_before.take() == Some(false) {
        ctx.disable_accesskit();
    }
    ctx.request_repaint();
}

/// Whether the mode is on.
pub fn active(ctx: &Context) -> bool {
    INSPECTING.load(Ordering::SeqCst) > 0 && lock(&state(ctx)).on
}

/// Gives the platform's copy chord with Shift back as a key (Ctrl+Shift+C
/// under egui-winit, which turns it into a copy event and swallows the
/// key) while `on`: the devtools setting is.
pub fn arm(ctx: &Context, on: bool) {
    let st = state(ctx);
    if lock(&st).armed == on {
        return;
    }
    ctx.add_plugin(Hook);
    lock(&st).armed = on;
}

/// The pick made since the last call, if any.
pub fn take_picked(ctx: &Context) -> Option<Pick> {
    lock(&state(ctx)).picked.take()
}

/// Whether the mode was left (Escape, a pick) since the last call.
pub fn take_left(ctx: &Context) -> bool {
    std::mem::take(&mut lock(&state(ctx)).left)
}

/// The last pick.
pub fn last(ctx: &Context) -> Option<Pick> {
    lock(&state(ctx)).last.clone()
}

/// Records a part of the interface around what `ui` drew (call it once
/// drawn): a zone's block or a panel, with the plugin owning it. Does
/// nothing unless the mode is on.
pub fn region(ui: &Ui, kind: &str, id: &str, zone: Option<&str>, owner: &str) {
    if INSPECTING.load(Ordering::Relaxed) == 0 {
        return;
    }
    let ctx = ui.ctx();
    let viewport = ctx.viewport_id();
    let st = state(ctx);
    let mut s = lock(&st);
    if !s.on {
        return;
    }
    s.windows.entry(viewport).or_default().regions.push(Region {
        kind: kind.into(),
        id: id.into(),
        zone: zone.map(String::from),
        owner: owner.into(),
        layer: Some(ui.layer_id()),
        rect: ui.min_rect(),
    });
}

/// Records a kit part painted (by `Component::paint`).
pub(crate) fn painted(
    painter: &Painter,
    component: &str,
    shape: &str,
    state: KitState,
    rect: Rect,
    b: &BoxStyle,
) {
    if INSPECTING.load(Ordering::Relaxed) == 0 {
        return;
    }
    let ctx = painter.ctx();
    let viewport = ctx.viewport_id();
    let st = self::state(ctx);
    let mut s = lock(&st);
    if !s.on {
        return;
    }
    s.windows.entry(viewport).or_default().parts.push(KitPart {
        component: component.into(),
        shape: shape.into(),
        state: state_name(state),
        fill: hex(b.fill),
        border: format!("{} {}", b.border_width, hex(b.border_color)),
        radius: b.radius,
        text: hex(b.text),
        around: false,
        layer: Some(painter.layer_id()),
        rect,
    });
}

/// A kit state's name: its flags joined with `+`, or `rest`.
pub fn state_name(state: KitState) -> String {
    let names: Vec<&str> = KitState::ORDER
        .iter()
        .filter(|(s, _)| state.contains(*s))
        .map(|(_, n)| *n)
        .collect();
    if names.is_empty() {
        "rest".into()
    } else {
        names.join("+")
    }
}

/// A colour as `#rrggbbaa`.
pub fn hex(c: Color32) -> String {
    let [r, g, b, a] = c.to_srgba_unmultiplied();
    format!("#{r:02x}{g:02x}{b:02x}{a:02x}")
}

/// The egui plugin: takes the input while the mode is on, and picks at
/// the end of each window's pass.
struct Hook;

impl egui::plugin::Plugin for Hook {
    fn debug_name(&self) -> &'static str {
        "egui_pop_core::inspect"
    }

    fn input_hook(&mut self, ctx: &Context, input: &mut egui::RawInput) {
        let st = state(ctx);
        let mut s = lock(&st);
        if s.armed {
            let mut held = s.held.get(&input.viewport_id).copied().unwrap_or_default();
            give_back_the_chord(input, &mut held);
            s.held.insert(input.viewport_id, held);
        }
        if !s.on {
            return;
        }
        let viewport = input.viewport_id;
        let mut leaving = false;
        let w = s.windows.entry(viewport).or_default();
        input.events.retain(|e| match e {
            Event::PointerButton {
                pos,
                button,
                pressed,
                ..
            } => {
                if *pressed {
                    if *button == egui::PointerButton::Primary {
                        w.acts.push(Act::Click(*pos));
                    }
                    w.pressed = true;
                    false
                } else {
                    // A release whose press reached egui goes on.
                    !std::mem::take(&mut w.pressed)
                }
            }
            Event::Key {
                key,
                pressed,
                modifiers,
                ..
            } => {
                let act = match key {
                    Key::Escape => {
                        leaving |= *pressed;
                        return false;
                    }
                    Key::Tab if modifiers.shift => Act::Previous,
                    Key::Tab | Key::ArrowDown | Key::ArrowRight => Act::Next,
                    Key::ArrowUp | Key::ArrowLeft => Act::Previous,
                    Key::Enter => Act::Pick,
                    _ => return true,
                };
                if *pressed {
                    w.acts.push(act);
                }
                false
            }
            Event::AccessKitActionRequest(r) if r.action == egui::accesskit::Action::Click => {
                w.acts.push(Act::Node(r.target_node.0));
                false
            }
            _ => true,
        });
        if leaving {
            leave(ctx, &mut s);
        }
    }

    fn on_begin_pass(&mut self, ui: &mut Ui) {
        let viewport = ui.ctx().viewport_id();
        let st = state(ui.ctx());
        let mut s = lock(&st);
        if let Some(w) = s.windows.get_mut(&viewport) {
            w.regions.clear();
            w.parts.clear();
        }
    }

    fn on_end_pass(&mut self, ui: &mut Ui) {
        end_pass(ui.ctx());
    }
}

/// egui-winit sends the copy chord as a copy event, its key swallowed;
/// with Shift held it is given back as the key. `held`: the modifiers
/// held, followed from event to event (they arrive as events).
fn give_back_the_chord(input: &mut egui::RawInput, held: &mut egui::Modifiers) {
    let mut out = Vec::with_capacity(input.events.len());
    for e in input.events.drain(..) {
        match &e {
            Event::ModifiersChanged(m) => *held = *m,
            Event::Key { modifiers, .. } | Event::PointerButton { modifiers, .. } => {
                *held = *modifiers;
            }
            Event::WindowFocused(false) => *held = egui::Modifiers::NONE,
            _ => {}
        }
        if e == Event::Copy && held.command && held.shift {
            for pressed in [true, false] {
                out.push(Event::Key {
                    key: Key::C,
                    physical_key: Some(Key::C),
                    pressed,
                    repeat: false,
                    modifiers: *held,
                });
            }
        } else {
            out.push(e);
        }
    }
    input.events = out;
}

/// An AccessKit node's role, name, value and description.
type Node = (String, Option<String>, Option<String>, Option<String>);

/// What a window's pass drew, read once at its end.
struct Drawn {
    widgets: Vec<(usize, WidgetRect)>,
    nodes: HashMap<Id, Node>,
    parents: HashMap<Id, Id>,
    order: Vec<LayerId>,
    transforms: HashMap<LayerId, TSTransform>,
}

impl Drawn {
    fn read(ctx: &Context) -> Drawn {
        let (widgets, nodes, parents) = ctx.viewport(|vp| {
            let mut widgets = Vec::new();
            for (_, list) in vp.this_pass.widgets.layers() {
                for (i, w) in list.iter().enumerate() {
                    widgets.push((i, *w));
                }
            }
            let mut nodes = HashMap::new();
            let mut parents = HashMap::new();
            if let Some(a) = vp.this_pass.accesskit_state.as_ref() {
                for (id, n) in &a.nodes {
                    nodes.insert(
                        *id,
                        (
                            format!("{:?}", n.role()),
                            n.label().map(String::from),
                            n.value().map(String::from),
                            n.description().map(String::from),
                        ),
                    );
                }
                for (id, p) in &a.parent_map {
                    parents.insert(*id, *p);
                }
            }
            (widgets, nodes, parents)
        });
        let order: Vec<LayerId> = ctx.memory(|m| m.layer_ids().collect());
        let mut transforms = HashMap::new();
        for (_, w) in &widgets {
            if let std::collections::hash_map::Entry::Vacant(e) = transforms.entry(w.layer_id)
                && let Some(t) = ctx.layer_transform_to_global(w.layer_id)
            {
                e.insert(t);
            }
        }
        Drawn {
            widgets,
            nodes,
            parents,
            order,
            transforms,
        }
    }

    fn global(&self, w: &WidgetRect) -> Rect {
        match self.transforms.get(&w.layer_id) {
            Some(t) => *t * w.interact_rect,
            None => w.interact_rect,
        }
    }

    /// Back to front: the layer's order, then its place among the areas.
    fn rank(&self, layer: LayerId) -> (Order, i64) {
        let at = self.order.iter().position(|l| *l == layer);
        (layer.order, at.map_or(-1, |i| i as i64))
    }

    fn pickable(layer: LayerId) -> bool {
        !matches!(layer.order, Order::Tooltip | Order::Debug)
    }

    /// The widget at `pos`: the top-most layer with a rectangle there,
    /// then the smallest, the last drawn on a tie.
    fn at(&self, pos: Pos2) -> Option<WidgetRect> {
        let mut best: Option<((Order, i64), f32, usize, WidgetRect)> = None;
        for (i, w) in &self.widgets {
            if !Self::pickable(w.layer_id) {
                continue;
            }
            let r = self.global(w);
            if !r.is_positive() || !r.contains(pos) {
                continue;
            }
            let rank = self.rank(w.layer_id);
            let area = r.area();
            let better = match &best {
                None => true,
                Some((br, ba, bi, _)) => {
                    rank > *br || (rank == *br && (area < *ba || (area == *ba && i > bi)))
                }
            };
            if better {
                best = Some((rank, area, *i, *w));
            }
        }
        best.map(|b| b.3)
    }

    /// The widgets the keyboard steps through: interactive ones, back to
    /// front, in the order drawn.
    fn stops(&self) -> Vec<WidgetRect> {
        let mut v: Vec<&(usize, WidgetRect)> = self
            .widgets
            .iter()
            .filter(|(_, w)| {
                Self::pickable(w.layer_id)
                    && w.enabled
                    && w.sense.interactive()
                    && self.global(w).is_positive()
            })
            .collect();
        v.sort_by_key(|(i, w)| (self.rank(w.layer_id), *i));
        v.into_iter().map(|(_, w)| *w).collect()
    }

    fn by_id(&self, id: Id) -> Option<WidgetRect> {
        self.widgets.iter().map(|(_, w)| *w).find(|w| w.id == id)
    }

    fn named(&self, id: Id) -> Option<String> {
        let (role, label, _, _) = self.nodes.get(&id)?;
        label
            .as_ref()
            .filter(|l| !l.is_empty())
            .map(|l| format!("{role} {l:?}"))
    }

    /// The nearest ancestor with a name.
    fn within(&self, id: Id) -> Option<String> {
        let mut at = id;
        for _ in 0..64 {
            at = *self.parents.get(&at)?;
            if let Some(n) = self.named(at) {
                return Some(n);
            }
        }
        None
    }

    fn short(&self, w: &WidgetRect) -> String {
        let r = self.global(w);
        let what = self.named(w.id).or_else(|| {
            self.nodes
                .get(&w.id)
                .map(|(role, ..)| role.clone())
                .filter(|r| r != "GenericContainer")
        });
        let size = format!("{:.0}×{:.0}", r.width(), r.height());
        match what {
            Some(w) => format!("{w}  {size}"),
            None => format!("{}  {size}", sense(w)),
        }
    }
}

fn sense(w: &WidgetRect) -> String {
    let s = w.sense;
    let mut v = Vec::new();
    if s.senses_click() {
        v.push("click");
    }
    if s.senses_drag() {
        v.push("drag");
    }
    if s.is_focusable() {
        v.push("focusable");
    }
    if v.is_empty() {
        "hover".into()
    } else {
        v.join("+")
    }
}

fn end_pass(ctx: &Context) {
    if INSPECTING.load(Ordering::Relaxed) == 0 {
        return;
    }
    let viewport = ctx.viewport_id();
    let st = state(ctx);
    let (acts, regions, parts, cursor, cursor_at) = {
        let mut s = lock(&st);
        if !s.on {
            return;
        }
        let w = s.windows.entry(viewport).or_default();
        (
            std::mem::take(&mut w.acts),
            w.regions.clone(),
            w.parts.clone(),
            s.cursor,
            s.cursor_at,
        )
    };
    let drawn = Drawn::read(ctx);
    let pointer = ctx.input(|i| i.pointer.hover_pos());
    let mut cursor = cursor
        .filter(|(v, _)| *v == viewport)
        .and_then(|(_, id)| drawn.by_id(id))
        // The pointer moving gives the outline back to it.
        .filter(|_| pointer.is_none() || pointer == cursor_at);
    let mut picked: Option<(WidgetRect, &str)> = None;
    for act in acts {
        match act {
            Act::Click(pos) => {
                if let Some(w) = drawn.at(pos) {
                    picked = Some((w, "pointer"));
                }
            }
            Act::Node(node) => {
                if let Some(w) = drawn
                    .widgets
                    .iter()
                    .map(|(_, w)| *w)
                    .find(|w| w.id.value() == node)
                {
                    picked = Some((w, "accesskit"));
                }
            }
            Act::Next | Act::Previous => {
                let stops = drawn.stops();
                if stops.is_empty() {
                    continue;
                }
                let from = cursor
                    .or_else(|| pointer.and_then(|p| drawn.at(p)))
                    .or_else(|| ctx.memory(|m| m.focused()).and_then(|id| drawn.by_id(id)));
                let at = from.and_then(|f| stops.iter().position(|w| w.id == f.id));
                let n = stops.len();
                let next = match (at, act == Act::Next) {
                    (Some(i), true) => (i + 1) % n,
                    (Some(i), false) => (i + n - 1) % n,
                    (None, true) => 0,
                    (None, false) => n - 1,
                };
                cursor = Some(stops[next]);
            }
            Act::Pick => {
                let target = cursor.or_else(|| pointer.and_then(|p| drawn.at(p)));
                if let Some(w) = target {
                    picked = Some((w, "keyboard"));
                }
            }
        }
    }
    if let Some((w, how)) = picked {
        let pick = describe(ctx, &drawn, &w, viewport, &regions, &parts, how);
        let mut s = lock(&st);
        s.last = Some(pick.clone());
        s.picked = Some(pick);
        leave(ctx, &mut s);
        return;
    }
    {
        let mut s = lock(&st);
        s.cursor = cursor.map(|w| (viewport, w.id));
        if cursor.is_some() {
            s.cursor_at = pointer;
        }
    }
    let shown = cursor.or_else(|| pointer.and_then(|p| drawn.at(p)));
    if let Some(w) = shown {
        outline(ctx, drawn.global(&w), &drawn.short(&w));
    }
}

/// Draws the outline and its label over everything.
fn outline(ctx: &Context, rect: Rect, label: &str) {
    let t = Tokens::of(ctx);
    let painter = ctx.layer_painter(LayerId::new(Order::Debug, Id::unique("egui_pop.inspect")));
    painter.rect(
        rect,
        0.0,
        t.accent.default.gamma_multiply(0.15),
        Stroke::new(1.5, t.focus_ring),
        StrokeKind::Outside,
    );
    let galley = painter.layout_no_wrap(
        label.to_string(),
        FontId::proportional(t.font.small),
        t.text.on_accent,
    );
    let pad = egui::vec2(t.space.xs, t.space.xs / 2.0);
    let size = galley.size() + 2.0 * pad;
    let screen = ctx.content_rect();
    let mut at = rect.left_top() - egui::vec2(0.0, size.y + 2.0);
    if at.y < screen.top() {
        at.y = rect.bottom() + 2.0;
    }
    at.x =
        at.x.clamp(screen.left(), (screen.right() - size.x).max(screen.left()));
    let bg = Rect::from_min_size(at, size);
    painter.rect_filled(bg, t.radius.widget, t.accent.default);
    painter.galley(bg.min + pad, galley, t.text.on_accent);
}

fn describe(
    ctx: &Context,
    drawn: &Drawn,
    w: &WidgetRect,
    viewport: ViewportId,
    regions: &[Region],
    parts: &[KitPart],
    how: &str,
) -> Pick {
    let r = drawn.global(w);
    let node = drawn.nodes.get(&w.id);
    let role = node.map(|n| n.0.clone());
    let container = !w.sense.interactive()
        && role
            .as_deref()
            .is_none_or(|r| r == "GenericContainer" || r == "Unknown");
    let local = w.interact_rect;
    let centre = local.center();
    // The application's parts around it, on its layer, outermost first.
    let mut around: Vec<Region> = regions
        .iter()
        .filter(|g| g.layer == Some(w.layer_id) && g.rect.contains(centre))
        .cloned()
        .collect();
    // A block fills the panel it is drawn in: the panel goes outside.
    around.sort_by(|a, b| {
        b.rect
            .area()
            .total_cmp(&a.rect.area())
            .then_with(|| (a.kind == "block").cmp(&(b.kind == "block")))
    });
    // Its own box: the largest part painted within it; else the
    // smallest painted around it.
    let on_layer = parts.iter().filter(|p| p.layer == Some(w.layer_id));
    let own = on_layer
        .clone()
        .filter(|p| w.rect.expand(4.0).contains_rect(p.rect))
        .max_by(|a, b| a.rect.area().total_cmp(&b.rect.area()))
        .cloned();
    let kit = own.or_else(|| {
        on_layer
            .filter(|p| p.rect.contains(centre))
            .min_by(|a, b| a.rect.area().total_cmp(&b.rect.area()))
            .cloned()
            .map(|mut p| {
                p.around = true;
                p
            })
    });
    let none = |s: &Option<String>| s.clone().filter(|s| !s.is_empty());
    Pick {
        id: format!("{:016x}", w.id.value()),
        kind: if container { "container" } else { "widget" }.into(),
        role,
        label: node.and_then(|n| none(&n.1)),
        value: node.and_then(|n| none(&n.2)),
        description: node.and_then(|n| none(&n.3)),
        within: drawn.within(w.id),
        sense: sense(w),
        enabled: w.enabled,
        focused: ctx.memory(|m| m.focused()) == Some(w.id),
        rect: [r.min.x, r.min.y, r.width(), r.height()],
        layer: format!("{:?} {:?}", w.layer_id.order, w.layer_id.id),
        viewport,
        regions: around,
        kit,
        how: how.into(),
    }
}

/// Which windows have taken a press whose release is still owed (for
/// tests of the hook).
#[cfg(test)]
fn pressed(ctx: &Context) -> std::collections::HashSet<ViewportId> {
    lock(&state(ctx))
        .windows
        .iter()
        .filter(|(_, w)| w.pressed)
        .map(|(v, _)| *v)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(ctx: &Context, events: Vec<Event>, ui: impl FnMut(&mut Ui)) {
        let input = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, egui::vec2(400.0, 300.0))),
            events,
            ..Default::default()
        };
        let mut out = ctx.run_ui(input, ui);
        out.textures_delta.clear();
    }

    fn click(pos: Pos2) -> Vec<Event> {
        vec![
            Event::PointerMoved(pos),
            Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: Default::default(),
            },
            Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: Default::default(),
            },
        ]
    }

    #[test]
    fn a_click_picks_the_button_and_does_not_press_it() {
        let ctx = Context::default();
        let clicked = std::cell::Cell::new(0);
        let at = std::cell::Cell::new(Rect::NOTHING);
        let mut ui = |ui: &mut Ui| {
            let r = ui.button("Save");
            at.set(r.rect);
            if r.clicked() {
                clicked.set(clicked.get() + 1);
            }
        };
        frame(&ctx, vec![], &mut ui);
        start(&ctx);
        frame(&ctx, vec![], &mut ui);
        frame(&ctx, click(at.get().center()), &mut ui);
        assert_eq!(clicked.get(), 0);
        assert!(!active(&ctx));
        assert!(pressed(&ctx).is_empty());
        let pick = take_picked(&ctx).expect("picked");
        assert_eq!(pick.role.as_deref(), Some("Button"));
        assert_eq!(pick.label.as_deref(), Some("Save"));
        assert_eq!(pick.kind, "widget");
        assert_eq!(pick.how, "pointer");
        // Off again, a click reaches it.
        frame(&ctx, click(at.get().center()), &mut ui);
        assert_eq!(clicked.get(), 1);
    }

    #[test]
    fn keys_step_through_widgets_and_enter_picks() {
        let ctx = Context::default();
        let mut ui = |ui: &mut Ui| {
            let _ = ui.button("One");
            let _ = ui.button("Two");
        };
        frame(&ctx, vec![], &mut ui);
        start(&ctx);
        frame(&ctx, vec![], &mut ui);
        let key = |key| Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Default::default(),
        };
        frame(&ctx, vec![key(Key::Tab), key(Key::Tab)], &mut ui);
        frame(&ctx, vec![key(Key::Enter)], &mut ui);
        let pick = take_picked(&ctx).expect("picked");
        assert_eq!(pick.label.as_deref(), Some("Two"));
        assert_eq!(pick.how, "keyboard");
    }

    #[test]
    fn escape_leaves() {
        let ctx = Context::default();
        frame(&ctx, vec![], |_| {});
        start(&ctx);
        frame(
            &ctx,
            vec![Event::Key {
                key: Key::Escape,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: Default::default(),
            }],
            |_| {},
        );
        assert!(!active(&ctx));
        assert!(take_left(&ctx));
        assert!(take_picked(&ctx).is_none());
    }

    #[test]
    fn the_shifted_copy_chord_comes_back_as_its_key() {
        let chord = egui::Modifiers::COMMAND | egui::Modifiers::SHIFT;
        let mut input = egui::RawInput {
            events: vec![Event::ModifiersChanged(chord), Event::Copy],
            ..Default::default()
        };
        let mut held = egui::Modifiers::NONE;
        give_back_the_chord(&mut input, &mut held);
        assert!(matches!(
            input.events[1],
            Event::Key {
                key: Key::C,
                pressed: true,
                ..
            }
        ));
        let mut plain = egui::RawInput {
            events: vec![
                Event::ModifiersChanged(egui::Modifiers::COMMAND),
                Event::Copy,
            ],
            ..Default::default()
        };
        give_back_the_chord(&mut plain, &mut held);
        assert_eq!(plain.events[1], Event::Copy);
    }

    #[test]
    fn a_painted_mark_is_named_by_its_container() {
        let ctx = Context::default();
        let at = std::cell::Cell::new(Rect::NOTHING);
        let mut ui = |ui: &mut Ui| {
            ui.scope(|ui| {
                ui.set_min_size(egui::vec2(120.0, 60.0));
                at.set(ui.min_rect());
                ui.painter()
                    .circle_filled(at.get().center(), 4.0, Color32::RED);
            });
        };
        frame(&ctx, vec![], &mut ui);
        start(&ctx);
        frame(&ctx, vec![], &mut ui);
        frame(&ctx, click(at.get().center()), &mut ui);
        let pick = take_picked(&ctx).expect("picked");
        assert_eq!(pick.kind, "container");
    }
}
