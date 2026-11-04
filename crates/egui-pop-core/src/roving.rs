//! Roving groups: one Tab stop, arrows inside ([`tab::roving`](crate::tab::roving)).
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
//!
//! The group's widgets are every widget drawn inside it that can take the focus,
//! so a disabled tool is passed over. An open menu or popup keeps its own arrows,
//! and a text field inside keeps every key.
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

#[derive(Clone, Debug)]
struct Group {
    name: String,
    axis: Axis,
    layer: LayerId,
    /// Its widgets, as their order in its layer.
    widgets: Range<usize>,
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

fn widgets_in(ui: &Ui) -> usize {
    let layer = ui.layer_id();
    ui.ctx()
        .viewport(|v| v.this_pass.widgets.get_layer(layer).count())
}

/// Draws `add` as a roving group, see [`tab::roving`](crate::tab::roving).
pub(crate) fn show<R>(ui: &mut Ui, name: &str, axis: Axis, add: impl FnOnce(&mut Ui) -> R) -> R {
    let first = widgets_in(ui);
    let inner = add(ui);
    let end = widgets_in(ui);
    let st = state(ui.ctx());
    let mut s = lock(&st);
    rotate(ui.ctx(), &mut s);
    s.building.push(Group {
        name: name.into(),
        axis,
        layer: ui.layer_id(),
        widgets: first..end,
    });
    inner
}

/// The names of the groups the last pass drew, and their widgets that can take
/// the focus, in the order drawn.
pub(crate) fn groups(ctx: &Context) -> Vec<(String, Vec<Id>)> {
    let st = state(ctx);
    let mut s = lock(&st);
    rotate(ctx, &mut s);
    s.last
        .iter()
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
        v.prev_pass
            .widgets
            .get_layer(g.layer)
            .skip(g.widgets.start)
            .take(g.widgets.len())
            .filter(|w| crate::place::can_focus(w))
            .map(|w| (w.id, w.rect))
            .collect()
    })
}

/// The innermost group of the last pass holding a widget.
fn group_of(ctx: &Context, groups: &[Group], id: Id) -> Option<usize> {
    let (layer, order) = ctx.viewport(|v| v.prev_pass.widgets.order(id))?;
    groups
        .iter()
        .enumerate()
        .filter(|(_, g)| g.layer == layer && g.widgets.contains(&order))
        .min_by_key(|(_, g)| g.widgets.len())
        .map(|(i, _)| i)
}

/// A press of Tab, plain or with Shift only: whether it goes back.
fn tab_pressed(ctx: &Context) -> Option<bool> {
    ctx.input(|i| {
        i.events.iter().find_map(|e| match e {
            egui::Event::Key {
                key: Key::Tab,
                pressed: true,
                modifiers,
                ..
            } if modifiers.is_none() || modifiers.shift_only() => Some(modifiers.shift),
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

/// The members in the order they are placed along the group.
fn placed(mut members: Vec<(Id, Rect)>, axis: Axis) -> Vec<Id> {
    let key = |r: &Rect| match axis {
        Axis::Horizontal => r.center().x,
        Axis::Vertical => r.center().y,
    };
    members.sort_by(|a, b| key(&a.1).total_cmp(&key(&b.1)));
    members.into_iter().map(|m| m.0).collect()
}

/// Runs at the start of each pass, once egui has read the keys and the place
/// has put back a focus Tab goes on from, before anything is drawn.
pub(crate) fn begin_pass(ctx: &Context) {
    let now = pass(ctx);
    let st = state(ctx);
    let mut s = lock(&st);
    rotate(ctx, &mut s);
    let mut tab = tab_pressed(ctx);
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
        return;
    };
    let Some(gi) = group_of(ctx, &s.last, f) else {
        return;
    };
    let g = s.last[gi].clone();
    let members = members(ctx, &g);
    if !members.iter().any(|m| m.0 == f) || egui::TextEdit::load_state(ctx, f).is_some() {
        return;
    }
    // Tab came in: on to the widget focused there last, else the first.
    let came_in = seen.is_none_or(|p| group_of(ctx, &s.last, p) != Some(gi));
    if came_in && tab.is_none() && s.tabbed.is_some_and(|at| now <= at + TAB_PASSES) {
        s.tabbed = None;
        let remembered = s.remembered.get(&g.name).copied();
        let target = remembered
            .filter(|r| members.iter().any(|m| m.0 == *r))
            .or_else(|| placed(members.clone(), g.axis).first().copied());
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
    s.remembered.insert(g.name.clone(), f);
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
    if egui::Popup::is_any_open(ctx) {
        return;
    }
    let Some((key, step)) = step_pressed(ctx, g.axis) else {
        return;
    };
    let order = placed(members, g.axis);
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
    ctx.input_mut(|i| i.consume_key(Modifiers::NONE, key));
    ctx.memory_mut(|m| {
        m.request_focus(to);
        m.move_focus(FocusDirection::None);
    });
    s.seen = Some(to);
    s.remembered.insert(g.name, to);
}
