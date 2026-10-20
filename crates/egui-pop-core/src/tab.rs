//! Tab order and focus scopes for the keyboard: Tab moves through a
//! scope's stops in reading order, or as explicit indices and groups
//! say ([`egui_pop_model::tab`]); F6 moves between panels.
//!
//! A [`scope`] is a region whose Tab order egui-pop decides: a panel
//! ([`ScopeKind::Panel`]) or a modal ([`ScopeKind::Modal`]). Inside it,
//! a widget joins the order as a **stop** ([`stop`]); egui-pop's inputs
//! and sliders do so themselves, any other widget with
//! `tab::stop(ui, &response, TabStop::new())`.
//!
//! - **Tab and Shift+Tab** from a stop go to the next or previous stop
//!   of its scope: reading order by default (rows from top to bottom,
//!   left to right), stops with an explicit [`TabStop::index`] first,
//!   the stops of a [`TabStop::group`] together.
//! - **Disabled fields are skipped** (they cannot take the focus);
//!   read-only ones are not.
//! - **In a modal**, Tab cycles: after the last stop comes the first,
//!   and Shift+Tab goes round the other way. Every control of a modal
//!   scope should then be a stop, or Tab never reaches it. (egui-pop's
//!   modal overlays keep the focus inside them anyway: egui only lets
//!   the widgets of the top modal layer take it.)
//! - **In a panel**, Tab past the last stop leaves the order to egui,
//!   which goes on in the order widgets were drawn (to the next panel).
//! - **F6 and Shift+F6** move to the next or previous panel scope that
//!   has stops, to the stop focused there last (or its first). F6 does
//!   nothing while a modal scope or a modal overlay is open.
//! - The focus on a widget that is not a stop is left to egui (its own
//!   Tab order is the order widgets were drawn), and a focused widget
//!   that takes the keys itself (a terminal) keeps Tab and F6.
//!
//! ```
//! use egui_pop_core::tab::{self, ScopeKind, TabStop};
//!
//! fn form(ui: &mut egui::Ui, name: &mut String, city: &mut String) {
//!     tab::scope(ui, "form", ScopeKind::Panel, |ui| {
//!         let r = ui.text_edit_singleline(city);
//!         tab::stop(ui, &r, TabStop::new().index(2));
//!         let r = ui.text_edit_singleline(name);
//!         tab::stop(ui, &r, TabStop::new().index(1));
//!     });
//! }
//! ```

use std::collections::HashMap;

use egui::{Context, FocusDirection, Id, Key, LayerId, Modifiers, Rect, Response, Ui};
use egui_pop_model::tab::{self as model, TabEntry};
use serde::Serialize;

use crate::state::{lock, pass, shared};

/// What a scope is, which decides the ends of its Tab order and F6.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ScopeKind {
    /// A panel: F6 comes here; Tab past its ends goes on with egui.
    Panel,
    /// A modal: Tab cycles inside; F6 does nothing while it is open.
    Modal,
}

/// How a widget joins the Tab order.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TabStop {
    index: Option<i32>,
    group: Option<String>,
    skip: bool,
}

impl TabStop {
    pub fn new() -> Self {
        Self::default()
    }

    /// An explicit place: stops with an index come first, by index.
    pub fn index(mut self, index: i32) -> Self {
        self.index = Some(index);
        self
    }

    /// The stops of a group follow each other.
    pub fn group(mut self, group: impl Into<String>) -> Self {
        self.group = Some(group.into());
        self
    }

    /// Out of the order (a disabled field).
    pub fn skip(mut self, skip: bool) -> Self {
        self.skip = skip;
        self
    }

    /// The group, if any.
    pub fn group_name(&self) -> Option<&str> {
        self.group.as_deref()
    }

    /// The explicit index, if any.
    pub fn explicit_index(&self) -> Option<i32> {
        self.index
    }
}

/// One stop as drawn.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct StopRecord {
    #[serde(skip)]
    pub id: Id,
    pub entry: TabEntry,
}

/// One scope as drawn, with its stops.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct ScopeRecord {
    pub name: String,
    pub kind: ScopeKind,
    #[serde(skip)]
    pub layer: LayerId,
    #[serde(skip)]
    pub rect: Rect,
    pub stops: Vec<StopRecord>,
}

impl ScopeRecord {
    /// Its stops' ids in Tab order.
    pub fn order(&self) -> Vec<Id> {
        let entries: Vec<TabEntry> = self.stops.iter().map(|s| s.entry.clone()).collect();
        model::order(&entries, false)
            .into_iter()
            .map(|i| self.stops[i].id)
            .collect()
    }

    fn has_stops(&self) -> bool {
        self.stops.iter().any(|s| !s.entry.skip)
    }
}

#[derive(Debug, Default)]
struct State {
    pass: u64,
    building: Vec<ScopeRecord>,
    open: Vec<usize>,
    last: Vec<ScopeRecord>,
    /// The stop focused last in each panel, by scope name.
    remembered: HashMap<String, Id>,
    installed: bool,
}

fn state(ctx: &Context) -> std::sync::Arc<std::sync::Mutex<State>> {
    shared::<State>(ctx, "core.tab")
}

fn rotate(ctx: &Context, s: &mut State) {
    let now = pass(ctx);
    if s.pass != now {
        s.pass = now;
        s.last = std::mem::take(&mut s.building);
        s.open.clear();
    }
}

/// Draws `add` as a Tab scope named `name` (see [the rules](self)).
pub fn scope<R>(ui: &mut Ui, name: &str, kind: ScopeKind, add: impl FnOnce(&mut Ui) -> R) -> R {
    install(ui.ctx());
    let st = state(ui.ctx());
    let index = {
        let mut s = lock(&st);
        rotate(ui.ctx(), &mut s);
        s.building.push(ScopeRecord {
            name: name.into(),
            kind,
            layer: ui.layer_id(),
            rect: Rect::NOTHING,
            stops: Vec::new(),
        });
        let i = s.building.len() - 1;
        s.open.push(i);
        i
    };
    let (inner, rect) = ui
        .scope(|ui| {
            let inner = add(ui);
            (inner, ui.min_rect())
        })
        .inner;
    let mut s = lock(&st);
    if let Some(r) = s.building.get_mut(index) {
        r.rect = rect;
    }
    s.open.pop();
    inner
}

/// Puts a widget in the Tab order of the innermost scope being drawn
/// (outside any scope, it stays in egui's order). A widget that cannot
/// take the focus (disabled) is skipped whatever `stop` says.
pub fn stop(ui: &Ui, response: &Response, stop: TabStop) {
    let st = state(ui.ctx());
    let mut s = lock(&st);
    rotate(ui.ctx(), &mut s);
    let Some(&i) = s.open.last() else {
        return;
    };
    let r = response.rect;
    let entry = TabEntry {
        rect: [r.left(), r.top(), r.width(), r.height()],
        index: stop.index,
        group: stop.group,
        skip: stop.skip || !response.enabled(),
    };
    s.building[i].stops.push(StopRecord {
        id: response.id,
        entry,
    });
}

/// The scopes drawn in the last pass, with their stops.
pub fn scopes(ctx: &Context) -> Vec<ScopeRecord> {
    let st = state(ctx);
    let mut s = lock(&st);
    rotate(ctx, &mut s);
    s.last.clone()
}

/// A scope's stops in Tab order, as drawn in the last pass.
pub fn order(ctx: &Context, name: &str) -> Vec<Id> {
    scopes(ctx)
        .iter()
        .find(|r| r.name == name)
        .map(ScopeRecord::order)
        .unwrap_or_default()
}

/// Installs the Tab plugin on a context (once; [`scope`] does it).
pub fn install(ctx: &Context) {
    let st = state(ctx);
    let mut s = lock(&st);
    if !s.installed {
        s.installed = true;
        drop(s);
        ctx.add_plugin(TabPlugin);
    }
}

/// Moves the focus on Tab and F6, after egui has read the input and
/// before anything is drawn.
#[derive(Debug)]
struct TabPlugin;

impl egui::plugin::Plugin for TabPlugin {
    fn debug_name(&self) -> &'static str {
        "egui_pop_core::tab"
    }

    fn on_begin_pass(&mut self, ui: &mut Ui) {
        begin_pass(ui.ctx());
    }
}

/// A press of `key`, plain or with Shift only: whether it goes back.
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

fn focus(ctx: &Context, target: Id, key: Key, back: bool) {
    ctx.memory_mut(|m| {
        m.request_focus(target);
        m.move_focus(FocusDirection::None);
    });
    let mods = if back {
        Modifiers::SHIFT
    } else {
        Modifiers::NONE
    };
    ctx.input_mut(|i| i.consume_key(mods, key));
}

fn begin_pass(ctx: &Context) {
    let st = state(ctx);
    let mut s = lock(&st);
    rotate(ctx, &mut s);
    let focused = ctx.memory(|m| m.focused());
    // Where the focus is: a scope and its stop.
    let at = focused.and_then(|f| {
        s.last
            .iter()
            .enumerate()
            .find_map(|(i, sc)| sc.stops.iter().position(|x| x.id == f).map(|j| (i, j)))
    });
    if let Some((i, _)) = at
        && s.last[i].kind == ScopeKind::Panel
        && let Some(f) = focused
    {
        let name = s.last[i].name.clone();
        s.remembered.insert(name, f);
    }
    if let Some(back) = pressed(ctx, Key::Tab)
        && let Some((i, j)) = at
    {
        let sc = &s.last[i];
        let entries: Vec<TabEntry> = sc.stops.iter().map(|x| x.entry.clone()).collect();
        let order = model::order(&entries, false);
        let cycle = sc.kind == ScopeKind::Modal;
        if let Some(k) = model::next(&order, Some(j), back, cycle) {
            let target = sc.stops[k].id;
            drop(s);
            focus(ctx, target, Key::Tab, back);
            return;
        }
    }
    if let Some(back) = pressed(ctx, Key::F6) {
        let modal = s
            .last
            .iter()
            .any(|sc| sc.kind == ScopeKind::Modal && sc.has_stops())
            || crate::overlay::modal_open(ctx);
        if modal {
            return;
        }
        let panels: Vec<usize> = (0..s.last.len())
            .filter(|&i| s.last[i].kind == ScopeKind::Panel && s.last[i].has_stops())
            .collect();
        if panels.is_empty() {
            return;
        }
        let current = at.map(|(i, _)| i).or_else(|| {
            let f = focused?;
            let r = ctx.read_response(f)?;
            panels.iter().copied().find(|&p| {
                s.last[p].layer == r.layer_id && s.last[p].rect.contains(r.rect.center())
            })
        });
        let pos = current.and_then(|c| panels.iter().position(|&p| p == c));
        let n = panels.len();
        let next = match pos {
            None if back => panels[n - 1],
            None => panels[0],
            Some(p) if back => panels[(p + n - 1) % n],
            Some(p) => panels[(p + 1) % n],
        };
        let sc = &s.last[next];
        let order = sc.order();
        let target = s
            .remembered
            .get(&sc.name)
            .copied()
            .filter(|r| order.contains(r))
            .or_else(|| order.first().copied());
        if let Some(target) = target {
            drop(s);
            focus(ctx, target, Key::F6, back);
        }
    }
}
