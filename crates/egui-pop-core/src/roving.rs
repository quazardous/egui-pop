//! Roving groups: one Tab stop, arrows inside ([`tab::roving`](crate::tab::roving)),
//! and a widget's parts ([`tab::parts`](crate::tab::parts)).
//!
//! A toolbar of twenty tools used to be twenty Tab stops, and so were the
//! activity bar and the status bar. A roving group is one stop, as ARIA's
//! toolbar pattern and every desktop toolkit have it:
//!
//! - **Tab from a widget of the group** leaves it, past its last widget (Shift+Tab
//!   before its first), whichever of them holds the focus.
//! - **Tab coming in** lands on the widget focused there last, the first the first
//!   time, whichever way it came from.
//! - **The arrows along it** (Left and Right in a row, Up and Down in a column) go
//!   from widget to widget in the order they are placed, round from one end to the
//!   other; Home and End go to the ends. The other arrows are left as they are.
//!   With [`Axis::Own`] the widgets move the focus with the arrows themselves (a
//!   graph's nodes, by place).
//!
//! The group's widgets are every widget drawn inside it that can take the focus,
//! on whatever layer (a scene draws on a layer of its own), so a disabled tool is
//! passed over. An open menu or popup keeps its own arrows, and a text field inside
//! keeps every key.
//!
//! A widget's **parts** (a graph node's chevron, ports and sliders) are a group
//! inside one: Enter on the widget goes into them, Tab and Shift+Tab go round them,
//! Escape comes back to the widget. Tab coming into the roving group lands on the
//! widget, not on its parts.
//!
//! upstream: egui has no way to keep a widget out of Tab's order while it stays
//! focusable by other means, nor to say where Tab lands in a container (its order
//! is the order of drawing), so the group puts egui right: Tab from a widget of the
//! group goes on from its last widget, and Tab landing on one that is not the
//! remembered moves on to it before anything is drawn (docs/upstream.md, "egui:
//! no roving Tab stop").

use std::collections::HashMap;
use std::ops::Range;

use egui::{Context, FocusDirection, Id, Key, LayerId, Modifiers, Rect, Ui};

use crate::state::{lock, pass, shared_in_window};
use crate::tab::Axis;

/// How long after Tab (in passes) the focus it moved lands: in its own pass,
/// with Shift the next.
const TAB_PASSES: u64 = 2;

#[derive(Clone, Copy, Debug, PartialEq)]
enum Kind {
    Roving(Axis),
    /// The parts of a widget, its owner.
    Parts(Id),
}

#[derive(Clone, Debug)]
struct Group {
    name: String,
    kind: Kind,
    /// Its widgets, as their order in each layer it drew on, its own first.
    spans: Vec<(LayerId, Range<usize>)>,
}

impl Group {
    fn len(&self) -> usize {
        self.spans.iter().map(|(_, r)| r.len()).sum()
    }

    fn holds(&self, at: (LayerId, usize)) -> bool {
        self.spans
            .iter()
            .any(|(l, r)| *l == at.0 && r.contains(&at.1))
    }
}

#[derive(Debug, Default)]
struct State {
    pass: u64,
    building: Vec<Group>,
    last: Vec<Group>,
    /// The widget focused last in each group, by name.
    remembered: HashMap<String, Id>,
    /// The focus as the last pass began.
    seen: Option<Id>,
    /// When Tab was last pressed.
    tabbed: Option<u64>,
}

fn state(ctx: &Context) -> std::sync::Arc<std::sync::Mutex<State>> {
    shared_in_window::<State>(ctx, "core.tab.roving")
}

fn rotate(ctx: &Context, s: &mut State) {
    let now = pass(ctx);
    if s.pass != now {
        s.pass = now;
        s.last = std::mem::take(&mut s.building);
    }
}

/// How many widgets this pass drew so far, in each layer.
fn counts(ctx: &Context) -> HashMap<LayerId, usize> {
    ctx.viewport(|v| {
        v.this_pass
            .widgets
            .layers()
            .map(|(l, w)| (*l, w.len()))
            .collect()
    })
}

fn open<R>(ui: &mut Ui, name: &str, kind: Kind, add: impl FnOnce(&mut Ui) -> R) -> R {
    let before = counts(ui.ctx());
    let inner = add(ui);
    let own = ui.layer_id();
    let mut spans: Vec<(LayerId, Range<usize>)> = counts(ui.ctx())
        .into_iter()
        .filter_map(|(l, end)| {
            let start = before.get(&l).copied().unwrap_or(0);
            // A menu or popover opened from the group floats above it and
            // keeps its own Tab ring; a scene's layer has the group's order.
            (end > start && l.order <= own.order).then_some((l, start..end))
        })
        .collect();
    spans.sort_by_key(|(l, _)| (*l != own, l.order, l.id.value()));
    let st = state(ui.ctx());
    let mut s = lock(&st);
    rotate(ui.ctx(), &mut s);
    s.building.push(Group {
        name: name.into(),
        kind,
        spans,
    });
    inner
}

/// Draws `add` as a roving group, see [`tab::roving`](crate::tab::roving).
pub(crate) fn show<R>(ui: &mut Ui, name: &str, axis: Axis, add: impl FnOnce(&mut Ui) -> R) -> R {
    open(ui, name, Kind::Roving(axis), add)
}

/// Draws `add` as a widget's parts, see [`tab::parts`](crate::tab::parts).
pub(crate) fn parts<R>(ui: &mut Ui, name: &str, owner: Id, add: impl FnOnce(&mut Ui) -> R) -> R {
    open(ui, name, Kind::Parts(owner), add)
}

/// The names of the roving groups the last pass drew, and their widgets that can
/// take the focus, in the order drawn.
pub(crate) fn groups(ctx: &Context) -> Vec<(String, Vec<Id>)> {
    let st = state(ctx);
    let mut s = lock(&st);
    rotate(ctx, &mut s);
    s.last
        .iter()
        .filter(|g| matches!(g.kind, Kind::Roving(_)))
        .map(|g| {
            (
                g.name.clone(),
                members(ctx, g).iter().map(|m| m.0).collect(),
            )
        })
        .collect()
}

/// A group's widgets of the last pass that can take the focus, with their
/// rectangles, in the order drawn.
fn members(ctx: &Context, g: &Group) -> Vec<(Id, Rect)> {
    ctx.viewport(|v| {
        let widgets = &v.prev_pass.widgets;
        g.spans
            .iter()
            .flat_map(|(l, r)| widgets.get_layer(*l).skip(r.start).take(r.len()))
            .filter(|w| crate::place::can_focus(w))
            .map(|w| (w.id, w.rect))
            .collect()
    })
}

/// The groups of the last pass holding a widget, innermost first.
fn groups_of(ctx: &Context, groups: &[Group], id: Id) -> Vec<usize> {
    let Some(at) = ctx.viewport(|v| v.prev_pass.widgets.order(id)) else {
        return Vec::new();
    };
    let mut held: Vec<usize> = (0..groups.len()).filter(|&i| groups[i].holds(at)).collect();
    held.sort_by_key(|&i| groups[i].len());
    held
}

/// Whether a group of the last pass holds a widget.
fn holds(ctx: &Context, g: &Group, id: Id) -> bool {
    ctx.viewport(|v| v.prev_pass.widgets.order(id))
        .is_some_and(|at| g.holds(at))
}

/// A press of `key`, plain or with Shift only: whether Shift was held.
fn pressed(ctx: &Context, key: Key) -> Option<bool> {
    ctx.input(|i| {
        i.events.iter().find_map(|e| match e {
            egui::Event::Key {
                key: k,
                pressed: true,
                modifiers,
                ..
            } if *k == key && (modifiers.is_none() || modifiers.shift_only()) => {
                Some(modifiers.shift)
            }
            _ => None,
        })
    })
}

/// What a plain key moving inside a group asks for.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Step {
    Back,
    On,
    First,
    Last,
}

fn step_pressed(ctx: &Context, axis: Axis) -> Option<(Key, Step)> {
    let (back, on) = match axis {
        Axis::Horizontal => (Key::ArrowLeft, Key::ArrowRight),
        Axis::Vertical => (Key::ArrowUp, Key::ArrowDown),
        Axis::Own => return None,
    };
    ctx.input(|i| {
        i.events.iter().find_map(|e| match e {
            egui::Event::Key {
                key,
                pressed: true,
                modifiers,
                ..
            } if modifiers.is_none() => match *key {
                k if k == back => Some((k, Step::Back)),
                k if k == on => Some((k, Step::On)),
                Key::Home => Some((Key::Home, Step::First)),
                Key::End => Some((Key::End, Step::Last)),
                _ => None,
            },
            _ => None,
        })
    })
}

/// The members in the order they are placed along the group (as drawn, for
/// [`Axis::Own`]).
fn placed(mut members: Vec<(Id, Rect)>, axis: Axis) -> Vec<Id> {
    let key = |r: &Rect| match axis {
        Axis::Horizontal => r.center().x,
        Axis::Vertical => r.center().y,
        Axis::Own => 0.0,
    };
    members.sort_by(|a, b| key(&a.1).total_cmp(&key(&b.1)));
    members.into_iter().map(|m| m.0).collect()
}

fn give(ctx: &Context, s: &mut State, to: Id, key: Key, shift: bool) {
    let mods = if shift {
        Modifiers::SHIFT
    } else {
        Modifiers::NONE
    };
    ctx.input_mut(|i| i.consume_key(mods, key));
    ctx.memory_mut(|m| {
        m.request_focus(to);
        m.move_focus(FocusDirection::None);
    });
    s.seen = Some(to);
}

/// Runs at the start of each pass, once egui has read the keys and the place
/// has put back a focus Tab goes on from, before anything is drawn.
pub(crate) fn begin_pass(ctx: &Context) {
    let now = pass(ctx);
    let st = state(ctx);
    let mut s = lock(&st);
    rotate(ctx, &mut s);
    let mut tab = pressed(ctx, Key::Tab);
    if tab.is_some() {
        s.tabbed = Some(now);
    }
    let focused = ctx.memory(|m| m.focused());
    // Tab after Escape goes on from the place: out of its group too.
    if let Some((anchor, back)) = crate::place::going_on_from(ctx)
        && focused == Some(anchor)
    {
        tab = Some(back);
    }
    let seen = std::mem::replace(&mut s.seen, focused);
    let Some(f) = focused else {
        // Escape from a part goes back to its widget (egui let go of the focus).
        if pressed(ctx, Key::Escape) == Some(false)
            && let Some(p) = seen
            && let Some(&inner) = groups_of(ctx, &s.last, p).first()
            && let Kind::Parts(owner) = s.last[inner].kind
        {
            log::debug!(target: "egui_pop", "Focus: Escape goes back from {} to its widget", s.last[inner].name);
            ctx.memory_mut(|m| m.request_focus(owner));
            s.seen = Some(owner);
        }
        return;
    };
    if egui::TextEdit::load_state(ctx, f).is_some() {
        return;
    }
    let chain = groups_of(ctx, &s.last, f);
    // Tab inside a widget's parts goes round them.
    if let Some(&inner) = chain.first()
        && let Kind::Parts(_) = s.last[inner].kind
        && let Some(back) = tab
    {
        let order: Vec<Id> = members(ctx, &s.last[inner]).iter().map(|m| m.0).collect();
        if let Some(at) = order.iter().position(|&m| m == f) {
            let n = order.len();
            let to = if back {
                order[(at + n - 1) % n]
            } else {
                order[(at + 1) % n]
            };
            give(ctx, &mut s, to, Key::Tab, back);
            return;
        }
    }
    // Enter on a widget goes into its parts.
    if pressed(ctx, Key::Enter) == Some(false)
        && !egui::Popup::is_any_open(ctx)
        && let Some(g) = s.last.iter().find(|g| g.kind == Kind::Parts(f))
        && let Some(&(first, _)) = members(ctx, g).first()
    {
        log::debug!(target: "egui_pop", "Focus: Enter goes into {}", g.name);
        give(ctx, &mut s, first, Key::Enter, false);
        return;
    }
    let Some(&gi) = chain
        .iter()
        .find(|&&i| matches!(s.last[i].kind, Kind::Roving(_)))
    else {
        return;
    };
    let g = s.last[gi].clone();
    let Kind::Roving(axis) = g.kind else {
        return;
    };
    let direct = chain.first() == Some(&gi);
    let members = members(ctx, &g);
    if !members.iter().any(|m| m.0 == f) {
        return;
    }
    // Tab came in: on to the widget focused there last, else the first.
    let came_in = seen.is_none_or(|p| !holds(ctx, &g, p));
    if came_in && tab.is_none() && s.tabbed.is_some_and(|at| now <= at + TAB_PASSES) {
        s.tabbed = None;
        let remembered = s.remembered.get(&g.name).copied();
        let target = remembered
            .filter(|r| members.iter().any(|m| m.0 == *r))
            .or_else(|| placed(members.clone(), axis).first().copied());
        if let Some(t) = target
            && t != f
        {
            log::debug!(target: "egui_pop", "Focus: Tab comes into {} on {t:?}", g.name);
            ctx.memory_mut(|m| m.request_focus(t));
            s.seen = Some(t);
            s.remembered.insert(g.name, t);
            return;
        }
    }
    if direct {
        s.remembered.insert(g.name.clone(), f);
    }
    // Tab leaves: egui goes on from the group's end.
    if let Some(back) = tab {
        let end = if back {
            members.first()
        } else {
            members.last()
        };
        if let Some(&(end, _)) = end {
            ctx.memory_mut(|m| {
                m.request_focus(end);
                m.move_focus(if back {
                    FocusDirection::Previous
                } else {
                    FocusDirection::Next
                });
            });
        }
        return;
    }
    if !direct || egui::Popup::is_any_open(ctx) {
        return;
    }
    let Some((key, step)) = step_pressed(ctx, axis) else {
        return;
    };
    let order = placed(members, axis);
    let Some(at) = order.iter().position(|&m| m == f) else {
        return;
    };
    let n = order.len();
    let to = match step {
        Step::Back => order[(at + n - 1) % n],
        Step::On => order[(at + 1) % n],
        Step::First => order[0],
        Step::Last => order[n - 1],
    };
    give(ctx, &mut s, to, key, false);
    s.remembered.insert(g.name, to);
}
