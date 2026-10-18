//! The overlay stack: dialogs, popovers and menus above the interface.
//!
//! [`OverlayStack`] is the model, pure and tested by its invariants:
//!
//! - Escape dismisses the **top** overlay only (if it allows it);
//! - a click outside dismisses the dismissable overlays above the one
//!   clicked (or all of them), stopping at a modal: nothing below a modal
//!   receives the click;
//! - closing an overlay closes every overlay opened above it;
//! - each overlay remembers the widget focused when it opened; when it
//!   closes, the focus goes back there.
//!
//! [`show`] is the egui side: it keeps the stack in the context, draws an
//! overlay (a modal with a backdrop, or an area placed next to an anchor
//! with [`crate::placement`]) and restores the focus.

use egui::{Context, Id, Rect, Ui};
use serde::Serialize;

use crate::placement::{Placement, Side, place};
use crate::state::{lock, pass, shared};

/// What kind of overlay.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    /// Blocks what is below; Escape closes it, a click outside does not.
    Modal,
    /// Floats next to an anchor; Escape or a click outside closes it.
    Popover,
    /// As a popover, for menus.
    Menu,
}

/// An overlay as declared.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct OverlaySpec {
    #[serde(skip)]
    pub id: Id,
    /// What it is, for the debug bridge and accessibility.
    pub label: String,
    pub kind: Kind,
    pub dismiss_on_escape: bool,
    pub dismiss_on_outside: bool,
    /// A modal's width, when the dialog width token is too narrow (a
    /// settings window).
    pub width: Option<f32>,
    /// A modal's offset from the window's centre, in points (a dialog
    /// moved by the user).
    #[serde(skip)]
    pub offset: egui::Vec2,
    /// How much of the theme's backdrop a modal dims the interface with
    /// (`backdrop.opacity`): 1 by default, 0 for none (the interface stays
    /// as it is, still blocked).
    pub dim: f32,
}

impl OverlaySpec {
    pub fn new(id: impl std::hash::Hash + std::fmt::Debug, label: &str, kind: Kind) -> Self {
        Self {
            id: Id::new(id),
            label: label.into(),
            kind,
            dismiss_on_escape: true,
            dismiss_on_outside: kind != Kind::Modal,
            width: None,
            offset: egui::Vec2::ZERO,
            dim: 1.0,
        }
    }

    /// A modal this far from the window's centre (points).
    pub fn offset(mut self, offset: egui::Vec2) -> Self {
        self.offset = offset;
        self
    }

    /// A modal dimming the interface with this share of the theme's
    /// backdrop (0 to 1).
    pub fn dim(mut self, dim: f32) -> Self {
        self.dim = dim.clamp(0.0, 1.0);
        self
    }

    /// A modal this wide (points; capped to the window).
    pub fn width(mut self, width: f32) -> Self {
        self.width = Some(width);
        self
    }

    /// Escape does not close it (a modal waiting for an answer).
    pub fn keep_on_escape(mut self) -> Self {
        self.dismiss_on_escape = false;
        self
    }
}

/// Why an overlay closed.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Reason {
    Escape,
    Outside,
    /// Closed by its owner (or because it was below).
    Closed,
    /// An overlay below it closed.
    Parent,
}

/// An overlay that closed, and where the focus goes.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Closed {
    #[serde(skip)]
    pub id: Id,
    pub label: String,
    pub reason: Reason,
    #[serde(skip)]
    pub restore_focus: Option<Id>,
}

#[derive(Clone, Debug, PartialEq)]
struct Entry {
    spec: OverlaySpec,
    restore_focus: Option<Id>,
    rect: Option<Rect>,
}

/// The stack, bottom first.
#[derive(Clone, Debug, Default)]
pub struct OverlayStack {
    entries: Vec<Entry>,
}

/// What opening did.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Opened {
    New,
    /// It was open already: what was above it closed, it is on top again.
    Raised,
}

impl OverlayStack {
    pub fn new() -> Self {
        Self::default()
    }

    /// Opens an overlay on top, remembering the focus to restore.
    pub fn open(&mut self, spec: OverlaySpec, focused: Option<Id>) -> (Opened, Vec<Closed>) {
        if let Some(i) = self.position(spec.id) {
            let closed = self.close_from(i + 1, Reason::Parent);
            self.entries[i].spec = spec;
            return (Opened::Raised, closed);
        }
        self.entries.push(Entry {
            spec,
            restore_focus: focused,
            rect: None,
        });
        (Opened::New, Vec::new())
    }

    /// Replaces an open overlay's declaration where it is (its Escape
    /// rule, its offset), closing nothing; whether it is open.
    pub fn update(&mut self, spec: OverlaySpec) -> bool {
        match self.position(spec.id) {
            Some(i) => {
                self.entries[i].spec = spec;
                true
            }
            None => false,
        }
    }

    fn position(&self, id: Id) -> Option<usize> {
        self.entries.iter().position(|e| e.spec.id == id)
    }

    /// Closes from index `i` up, topmost first; the focus to restore is
    /// the lowest one's.
    fn close_from(&mut self, i: usize, reason: Reason) -> Vec<Closed> {
        let mut out = Vec::new();
        while self.entries.len() > i {
            let e = self.entries.pop().expect("an entry above i");
            let r = if self.entries.len() == i {
                reason
            } else {
                Reason::Parent
            };
            out.push(Closed {
                id: e.spec.id,
                label: e.spec.label,
                reason: r,
                restore_focus: e.restore_focus,
            });
        }
        out
    }

    /// Closes an overlay and everything above it. Closing one that is not
    /// open closes nothing (the list is empty).
    pub fn close(&mut self, id: Id) -> Vec<Closed> {
        match self.position(id) {
            Some(i) => self.close_from(i, Reason::Closed),
            None => Vec::new(),
        }
    }

    /// Escape: closes the top overlay if it allows it.
    pub fn escape(&mut self) -> Vec<Closed> {
        match self.entries.last() {
            Some(e) if e.spec.dismiss_on_escape => {
                self.close_from(self.entries.len() - 1, Reason::Escape)
            }
            _ => Vec::new(),
        }
    }

    /// A click on `hit` (an overlay, or `None` for the interface below):
    /// closes the dismissable overlays above it, stopping at a modal or
    /// at one that stays.
    pub fn click(&mut self, hit: Option<Id>) -> Vec<Closed> {
        let floor = hit.and_then(|h| self.position(h)).map_or(0, |i| i + 1);
        let mut cut = self.entries.len();
        while cut > floor {
            let e = &self.entries[cut - 1];
            if e.spec.kind == Kind::Modal || !e.spec.dismiss_on_outside {
                break;
            }
            cut -= 1;
        }
        if cut == self.entries.len() {
            Vec::new()
        } else {
            self.close_from(cut, Reason::Outside)
        }
    }

    /// The topmost modal, if any: everything below it is blocked.
    pub fn modal(&self) -> Option<Id> {
        self.entries
            .iter()
            .rev()
            .find(|e| e.spec.kind == Kind::Modal)
            .map(|e| e.spec.id)
    }

    /// Whether input to an overlay (or to the interface, `None`) is
    /// blocked by a modal above it.
    pub fn is_blocked(&self, target: Option<Id>) -> bool {
        let Some(m) = self
            .entries
            .iter()
            .rposition(|e| e.spec.kind == Kind::Modal)
        else {
            return false;
        };
        match target.and_then(|t| self.position(t)) {
            Some(i) => i < m,
            None => true,
        }
    }

    pub fn top(&self) -> Option<&OverlaySpec> {
        self.entries.last().map(|e| &e.spec)
    }

    pub fn contains(&self, id: Id) -> bool {
        self.position(id).is_some()
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// The overlays, bottom first.
    pub fn specs(&self) -> impl Iterator<Item = &OverlaySpec> {
        self.entries.iter().map(|e| &e.spec)
    }

    /// Records where an overlay was drawn (for outside clicks).
    pub fn set_rect(&mut self, id: Id, rect: Rect) {
        if let Some(i) = self.position(id) {
            self.entries[i].rect = Some(rect);
        }
    }

    /// The topmost overlay drawn at a point.
    pub fn hit(&self, pos: egui::Pos2) -> Option<Id> {
        self.entries
            .iter()
            .rev()
            .find(|e| e.rect.is_some_and(|r| r.contains(pos)))
            .map(|e| e.spec.id)
    }
}

#[derive(Debug, Default)]
struct State {
    pass: u64,
    stack: OverlayStack,
    /// Closed since the owners last looked.
    closed: Vec<Closed>,
    /// Shown this pass; those not shown again are closed.
    shown: Vec<Id>,
    shown_last: Vec<Id>,
    history: Vec<Closed>,
}

fn state(ctx: &Context) -> std::sync::Arc<std::sync::Mutex<State>> {
    shared::<State>(ctx, "overlay")
}

/// Once per pass, before anything reads the keyboard: Escape and outside
/// clicks close overlays; overlays no longer shown are closed. Returns
/// whether Escape was used (and consumed).
pub fn begin_pass(ctx: &Context) -> bool {
    let st = state(ctx);
    let mut s = lock(&st);
    let now = pass(ctx);
    if s.pass == now {
        return false;
    }
    s.pass = now;
    s.shown_last = std::mem::take(&mut s.shown);
    // A closing is news only to an owner still showing its overlay: one
    // left unseen would close the overlay the next time it opens.
    let State {
        closed, shown_last, ..
    } = &mut *s;
    closed.retain(|c| shown_last.contains(&c.id));
    // An overlay whose owner stopped showing it is closed.
    let gone: Vec<Id> = s
        .stack
        .specs()
        .map(|o| o.id)
        .filter(|id| !s.shown_last.contains(id))
        .collect();
    for id in gone {
        let closed = s.stack.close(id);
        settle(ctx, &mut s, closed);
    }
    let mut used_escape = false;
    if !s.stack.is_empty() && ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
        let closed = s.stack.escape();
        if !closed.is_empty() {
            used_escape = true;
            ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape));
        }
        settle(ctx, &mut s, closed);
    }
    if let Some(pos) = ctx.input(|i| {
        i.pointer
            .any_pressed()
            .then(|| i.pointer.interact_pos())
            .flatten()
    }) {
        let hit = s.stack.hit(pos);
        let closed = s.stack.click(hit);
        settle(ctx, &mut s, closed);
    }
    used_escape
}

fn settle(ctx: &Context, s: &mut State, closed: Vec<Closed>) {
    if let Some(target) = closed.last().and_then(|c| c.restore_focus) {
        ctx.memory_mut(|m| m.request_focus(target));
    }
    for c in &closed {
        log::debug!(target: "egui_pop", "overlay `{}` closed: {:?}", c.label, c.reason);
    }
    s.history.extend(closed.iter().cloned());
    let len = s.history.len();
    if len > 32 {
        s.history.drain(..len - 32);
    }
    s.closed.extend(closed);
}

/// Closes an overlay (and those above it) from code.
pub fn close(ctx: &Context, id: Id) {
    let st = state(ctx);
    let mut s = lock(&st);
    let closed = s.stack.close(id);
    settle(ctx, &mut s, closed);
}

/// The open overlays, bottom first.
pub fn stack(ctx: &Context) -> Vec<OverlaySpec> {
    let st = state(ctx);
    let s = lock(&st);
    s.stack.specs().cloned().collect()
}

/// The open overlays with where they were last drawn, bottom first (an
/// overlay not drawn yet is left out).
pub fn rects(ctx: &Context) -> Vec<(OverlaySpec, Rect)> {
    let st = state(ctx);
    let s = lock(&st);
    s.stack
        .entries
        .iter()
        .filter_map(|e| e.rect.map(|r| (e.spec.clone(), r)))
        .collect()
}

/// The overlays closed lately, oldest first.
pub fn recently_closed(ctx: &Context) -> Vec<Closed> {
    let st = state(ctx);
    lock(&st).history.clone()
}

/// Where an open overlay was last drawn.
pub fn rect(ctx: &Context, id: Id) -> Option<Rect> {
    let st = state(ctx);
    let s = lock(&st);
    s.stack
        .entries
        .iter()
        .find(|e| e.spec.id == id)
        .and_then(|e| e.rect)
}

/// Whether a modal overlay is open.
pub fn modal_open(ctx: &Context) -> bool {
    let st = state(ctx);
    lock(&st).stack.modal().is_some()
}

/// Where a floating overlay goes.
#[derive(Clone, Copy, Debug)]
pub struct Anchor {
    pub rect: Rect,
    pub side: Side,
}

/// Shows an overlay while `*open`; sets `*open` to false when it closes
/// (Escape, a click outside, or [`close`]), restoring the focus. A modal
/// is centred (moved by its `offset`) over a backdrop (`dim`); other
/// kinds are placed next to `anchor` (or at the pointer).
pub fn show<R>(
    ctx: &Context,
    spec: &OverlaySpec,
    open: &mut bool,
    anchor: Option<Anchor>,
    add: impl FnOnce(&mut Ui) -> R,
) -> Option<R> {
    begin_pass(ctx);
    let st = state(ctx);
    {
        let mut s = lock(&st);
        if let Some(i) = s.closed.iter().position(|c| c.id == spec.id) {
            s.closed.remove(i);
            *open = false;
            return None;
        }
        if !*open {
            if s.stack.contains(spec.id) {
                let closed = s.stack.close(spec.id);
                settle(ctx, &mut s, closed);
                s.closed.retain(|c| c.id != spec.id);
            }
            return None;
        }
        if !s.stack.contains(spec.id) {
            let focused = ctx.memory(|m| m.focused());
            let (_, closed) = s.stack.open(spec.clone(), focused);
            settle(ctx, &mut s, closed);
        } else if s.stack.specs().any(|o| o.id == spec.id && o != spec) {
            // Declared differently this pass (Escape kept, moved).
            s.stack.update(spec.clone());
        }
        s.shown.push(spec.id);
    }
    let kit = crate::kit::Kit::of(ctx);
    let rest = crate::kit::State::REST;
    let (inner, rect) = match spec.kind {
        Kind::Modal => {
            let backdrop = &kit.backdrop;
            let alpha = (backdrop.data.opacity * spec.dim).clamp(0.0, 1.0);
            let dialog = &kit.dialog;
            let padding = dialog.data.padding_x;
            let r = egui::Modal::new(spec.id)
                .area(
                    egui::Modal::default_area(spec.id)
                        .anchor(egui::Align2::CENTER_CENTER, spec.offset),
                )
                .backdrop_color(backdrop.style(rest).fill.gamma_multiply(alpha))
                .frame(egui::Frame::NONE)
                .show(ctx, |ui| {
                    let shadow = ui.visuals().window_shadow;
                    dialog
                        .show(ui, rest, shadow, |ui| {
                            let width = spec.width.map_or(dialog.data.width, |w| {
                                w.min(ctx.content_rect().width() - 4.0 * padding)
                            });
                            ui.set_max_width(width);
                            if spec.width.is_some() {
                                ui.set_min_width(width);
                            }
                            add(ui)
                        })
                        .inner
                });
            (r.inner, r.response.rect)
        }
        Kind::Popover | Kind::Menu => {
            let screen = ctx.content_rect();
            let last = ctx.memory(|m| m.area_rect(spec.id)).map(|r| r.size());
            let size = last.unwrap_or(egui::vec2(200.0, 100.0));
            let anchor = anchor.unwrap_or_else(|| {
                let p = ctx
                    .input(|i| i.pointer.interact_pos())
                    .unwrap_or(screen.center());
                Anchor {
                    rect: Rect::from_min_size(p, egui::Vec2::ZERO),
                    side: Side::Bottom,
                }
            });
            let placed = place(&Placement::new(anchor.rect, size, screen).side(anchor.side));
            let r = egui::Area::new(spec.id)
                .order(egui::Order::Foreground)
                .fixed_pos(placed.rect.min)
                .show(ctx, |ui| {
                    let shadow = ui.visuals().popup_shadow;
                    kit.popover.show(ui, rest, shadow, add).inner
                });
            (r.inner, r.response.rect)
        }
    };
    lock(&st).stack.set_rect(spec.id, rect);
    Some(inner)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec(name: &str, kind: Kind) -> OverlaySpec {
        OverlaySpec::new(name, name, kind)
    }

    fn labels(c: &[Closed]) -> Vec<&str> {
        c.iter().map(|c| c.label.as_str()).collect()
    }

    #[test]
    fn escape_closes_only_the_top_overlay_and_focus_goes_back() {
        let mut s = OverlayStack::new();
        let f1 = Id::new("field");
        let f2 = Id::new("button in dialog");
        s.open(spec("dialog", Kind::Modal), Some(f1));
        s.open(spec("menu", Kind::Menu), Some(f2));
        let c = s.escape();
        assert_eq!(labels(&c), ["menu"]);
        assert_eq!(c[0].restore_focus, Some(f2));
        assert_eq!(s.len(), 1);
        let c = s.escape();
        assert_eq!(labels(&c), ["dialog"]);
        assert_eq!(c[0].restore_focus, Some(f1));
        assert!(s.escape().is_empty(), "nothing left");
        s.open(spec("ask", Kind::Modal).keep_on_escape(), None);
        assert!(s.escape().is_empty(), "it stays");
    }

    #[test]
    fn an_overlay_declared_again_changes_in_place() {
        let mut s = OverlayStack::new();
        s.open(spec("dialog", Kind::Modal), Some(Id::new("field")));
        s.open(spec("menu", Kind::Menu), None);
        // Escape kept for a while (a dialog moved by the keys): nothing
        // above it closes.
        assert!(s.update(spec("dialog", Kind::Modal).keep_on_escape()));
        assert_eq!(s.len(), 2);
        assert_eq!(labels(&s.escape()), ["menu"]);
        assert!(s.escape().is_empty(), "the dialog keeps Escape");
        assert!(s.update(spec("dialog", Kind::Modal)));
        let c = s.escape();
        assert_eq!(labels(&c), ["dialog"]);
        assert_eq!(c[0].restore_focus, Some(Id::new("field")));
        assert!(!s.update(spec("ghost", Kind::Modal)));
    }

    #[test]
    fn closing_an_overlay_closes_those_above_it_and_restores_the_lowest_focus() {
        let mut s = OverlayStack::new();
        s.open(spec("a", Kind::Popover), Some(Id::new(1)));
        s.open(spec("b", Kind::Menu), Some(Id::new(2)));
        s.open(spec("c", Kind::Menu), Some(Id::new(3)));
        let c = s.close(Id::new("b"));
        assert_eq!(labels(&c), ["c", "b"]);
        assert_eq!(c[0].reason, Reason::Parent);
        assert_eq!(c[1].reason, Reason::Closed);
        assert_eq!(c.last().unwrap().restore_focus, Some(Id::new(2)));
        assert_eq!(s.len(), 1);
        assert!(s.close(Id::new("ghost")).is_empty());
    }

    #[test]
    fn a_click_outside_closes_popovers_above_what_it_hit_but_never_a_modal() {
        let mut s = OverlayStack::new();
        s.open(spec("menu", Kind::Menu), None);
        s.open(spec("submenu", Kind::Menu), None);
        // A click in the first menu closes the submenu only.
        assert_eq!(labels(&s.click(Some(Id::new("menu")))), ["submenu"]);
        // A click on the interface closes the menu.
        assert_eq!(labels(&s.click(None)), ["menu"]);
        s.open(spec("dialog", Kind::Modal), None);
        s.open(spec("popover", Kind::Popover), None);
        assert_eq!(labels(&s.click(None)), ["popover"]);
        assert!(s.click(None).is_empty(), "the modal stays");
        assert_eq!(s.len(), 1);
    }

    #[test]
    fn a_modal_blocks_what_is_below_it() {
        let mut s = OverlayStack::new();
        s.open(spec("popover", Kind::Popover), None);
        assert!(!s.is_blocked(None));
        s.open(spec("dialog", Kind::Modal), None);
        s.open(spec("menu", Kind::Menu), None);
        assert_eq!(s.modal(), Some(Id::new("dialog")));
        assert!(s.is_blocked(None));
        assert!(s.is_blocked(Some(Id::new("popover"))));
        assert!(!s.is_blocked(Some(Id::new("dialog"))));
        assert!(!s.is_blocked(Some(Id::new("menu"))));
    }

    #[test]
    fn opening_an_open_overlay_raises_it() {
        let mut s = OverlayStack::new();
        s.open(spec("a", Kind::Popover), None);
        s.open(spec("b", Kind::Popover), None);
        let (o, closed) = s.open(spec("a", Kind::Popover), None);
        assert_eq!(o, Opened::Raised);
        assert_eq!(labels(&closed), ["b"]);
        assert_eq!(s.top().unwrap().label, "a");
    }

    #[test]
    fn invariants_hold_over_random_operations() {
        let mut s = OverlayStack::new();
        let mut seed: u64 = 7;
        let mut next = || {
            seed = seed
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            (seed >> 33) as usize
        };
        for _ in 0..2_000 {
            let n = next() % 6;
            let kind = [Kind::Modal, Kind::Popover, Kind::Menu][next() % 3];
            let before = s.len();
            match next() % 4 {
                0 => {
                    s.open(OverlaySpec::new(n, &n.to_string(), kind), None);
                }
                1 => {
                    let c = s.escape();
                    assert!(c.len() <= 1);
                }
                2 => {
                    let c = s.close(Id::new(n));
                    assert_eq!(s.len() + c.len(), before);
                }
                _ => {
                    let modal_before = s.modal();
                    s.click(None);
                    // A modal never closes from a click.
                    assert_eq!(s.modal(), modal_before);
                }
            }
            // Ids are unique in the stack.
            let ids: Vec<Id> = s.specs().map(|o| o.id).collect();
            let mut dedup = ids.clone();
            dedup.sort_by_key(|i| i.value());
            dedup.dedup();
            assert_eq!(ids.len(), dedup.len());
        }
    }
}
