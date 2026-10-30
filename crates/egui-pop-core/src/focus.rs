//! Focus scopes and the focus chain (ADR 0007).
//!
//! A [`scope`] is a named region of the interface (a panel, a dialog, a
//! list) that sets context keys while the focus is inside it. Scopes nest
//! the way they are drawn; each pass records them, and the **focus
//! chain** is the scopes containing the focused widget (or, when nothing
//! has the keyboard focus, the place last clicked), outermost first. The
//! [`when_context`] built from it gives each scope's keys the scope's
//! depth, so key resolution prefers the binding nearest the focus.
//!
//! In hover mode ([`focus_mode`](crate::focus_mode)), the place the
//! pointer moved to stands for the place last clicked, and a widget
//! focused in another scope lets go of the focus.
//!
//! The chain is computed from the previous pass's scopes (egui decides
//! focus between passes), so it is ready before this pass's widgets read
//! the keyboard. Each window has its own scopes and chain (egui keeps a
//! focus per window): in a detached panel's window, [`when_context`]
//! says what has the keyboard there.
//!
//! A focused widget belongs to the scopes it was drawn in, wherever it lies:
//! a row scrolled out of its panel is still that panel's. The place a pointer
//! pressed has no widget to ask, so it goes by the scopes' rectangles.
//!
//! [`take_moved`] says when the focus went somewhere else, once per change,
//! for what keeps a notion of its own of where the user is (the workbench's
//! focused panel); [`prefer`] names the widget a scope's focus should land on
//! when something hands the scope the keyboard.
//!
//! # Escape, in order
//!
//! With the focus plugin [`install`]ed, one press of Escape goes to the
//! first of these that takes it:
//!
//! 1. a focused **text input** lets go of the focus, which stays in the
//!    input's surface (its scope still heads the focus chain); nothing
//!    else sees that press (unless the input lets Escape through, see
//!    [`escape_passes`]); an input that cancels on Escape (a slider's
//!    typed entry) learns of that press through [`escape_left`];
//! 2. egui aborts a **drag** in progress;
//! 3. the **top overlay** closes, if it closes on Escape (popovers,
//!    menus, dialogs: "Escape dismisses"), see `overlay::begin_pass`;
//! 4. the **key bindings** of Escape, the innermost focused surface's
//!    first, then the outer surfaces', then the global ones (ADR 0007);
//! 5. what is left reaches egui's widgets.
//!
//! So in a text field of a panel, the first Escape leaves the field and
//! the second reaches the panel's own Escape (cancelling a gesture,
//! closing the panel's overlay).
//!
//! While a text input owns the keyboard ([`text_input_owns_keyboard`]:
//! a text field has the focus, or an open input claimed it with
//! [`claim_keyboard`]), the key dispatcher leaves Escape and Enter to it.
//!
//! ```
//! use egui_pop_core::focus;
//!
//! fn panel(ui: &mut egui::Ui) {
//!     focus::scope(ui, "list", &[("listMode", "grid")], |ui| {
//!         let _ = ui.button("Item");
//!     });
//! }
//! ```

use std::ops::Range;

use egui::{Context, Id, LayerId, Pos2, Rect, Ui};
use egui_pop_model::when::WhenContext;
use serde::Serialize;

use crate::state::{lock, pass, shared, shared_in_window};

/// One scope as drawn in a pass.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct ScopeRecord {
    pub name: String,
    pub keys: Vec<(String, String)>,
    /// The enclosing scope, by index in the pass.
    pub parent: Option<usize>,
    /// 1 for a top-level scope.
    pub depth: u32,
    #[serde(skip)]
    pub layer: LayerId,
    #[serde(skip)]
    pub rect: Rect,
}

#[derive(Debug, Default)]
struct State {
    pass: u64,
    /// This pass's scopes.
    building: Vec<ScopeRecord>,
    open: Vec<usize>,
    /// The previous pass's scopes, the chain computed from them.
    last: Vec<ScopeRecord>,
    chain: Vec<usize>,
    /// Where the pointer last pressed, on which layer (or where the
    /// focused widget was when Escape took its focus away).
    pressed: Option<(LayerId, Pos2)>,
    /// The focus plugin is installed.
    installed: bool,
    /// Text inputs that let Escape through, this pass and the previous.
    passing: Vec<Id>,
    passing_last: Vec<Id>,
    /// Text inputs that claimed the keyboard, with the pass of their
    /// last claim ([`claim_keyboard`]).
    claims: Vec<(Id, u64)>,
    /// The text input Escape took the focus from, and in which pass.
    escaped: Option<(Id, u64)>,
    /// `building` and `last`'s other half.
    drawn: Vec<Drawn>,
    drawn_last: Vec<Drawn>,
    /// The widget holding the focus when last looked at.
    seen: Option<Id>,
    /// Whether the window had the system's focus when last looked at.
    window: Option<bool>,
    /// What changed at the start of this pass, until asked.
    moved: Option<Moved>,
}

/// What a scope is besides its record, by the same index: not part of
/// [`ScopeRecord`], which the debug bridge shows.
#[derive(Clone, Debug, Default)]
struct Drawn {
    /// The widgets drawn inside it, as their order in its layer.
    widgets: Range<usize>,
    /// The widget it wants to hand the focus to ([`prefer`]).
    preferred: Option<Id>,
}

/// How the place the user works in changed, from [`take_moved`].
#[derive(Clone, Debug, PartialEq)]
pub enum Moved {
    /// The pointer pressed there (or, in hover mode, moved there): the
    /// scopes at that place, outermost first.
    Pressed(Vec<ScopeRecord>),
    /// A widget took the focus, or the window holding it got the system's
    /// focus back: the focus chain.
    Focused(Vec<ScopeRecord>),
    /// The widget holding the focus let it go and nothing took it.
    Left,
}

/// The focus state of the window being drawn: each window (a detached
/// panel's included) has its own scopes and chain, as egui keeps a
/// focus per window.
fn state(ctx: &Context) -> std::sync::Arc<std::sync::Mutex<State>> {
    shared_in_window::<State>(ctx, "core.focus")
}

/// A widget's response from this pass (else the last), read without
/// touching the focus.
///
/// `Context::read_response` runs egui's "clicked elsewhere" rule: on a
/// click that did not land on the widget, a focused widget loses the
/// focus. Called every pass for the focused widget (to find its layer,
/// its place), it drops the focus a widget took when a click landed on
/// one of its children (a tree focused through its clicked row). This
/// reads the response with that rule off.
///
/// upstream: egui has no side-effect free way to read a widget's
/// response or layer (see docs/upstream.md).
pub fn peek_response(ctx: &Context, id: Id) -> Option<egui::Response> {
    use egui::SurrenderFocusOn;
    let was = ctx.options_mut(|o| {
        std::mem::replace(
            &mut o.input_options.surrender_focus_on,
            SurrenderFocusOn::Never,
        )
    });
    let r = ctx.read_response(id);
    ctx.options_mut(|o| o.input_options.surrender_focus_on = was);
    r
}

/// The deepest scope drawn around a widget of the last pass: the one whose
/// widgets include it. Its place would not do: a row scrolled out of its
/// panel lies over the next panel, or nowhere.
fn holding(ctx: &Context, s: &State, id: Id) -> Option<Option<usize>> {
    let (layer, order) = ctx.viewport(|v| v.prev_pass.widgets.order(id))?;
    let at = s
        .last
        .iter()
        .zip(&s.drawn_last)
        .enumerate()
        .filter(|(_, (r, d))| r.layer == layer && d.widgets.contains(&order))
        .max_by_key(|(i, (r, _))| (r.depth, *i))
        .map(|(i, _)| i);
    Some(at)
}

/// The deepest scope the focus chain is pointed at.
fn target(ctx: &Context, s: &State) -> Option<usize> {
    if let Some(id) = ctx.memory(|m| m.focused()) {
        if let Some(at) = holding(ctx, s, id) {
            return at;
        }
        if let Some(r) = peek_response(ctx, id) {
            return deepest(&s.last, r.layer_id, r.interact_rect.center());
        }
    }
    let (layer, pos) = s.pressed?;
    deepest(&s.last, layer, pos)
}

/// A scope and those around it, outermost first.
fn outward(scopes: &[ScopeRecord], mut at: Option<usize>) -> Vec<usize> {
    let mut chain = Vec::new();
    while let Some(i) = at {
        chain.push(i);
        at = scopes[i].parent;
    }
    chain.reverse();
    chain
}

/// The deepest scope drawn at a place.
fn deepest(scopes: &[ScopeRecord], layer: LayerId, pos: Pos2) -> Option<usize> {
    scopes
        .iter()
        .enumerate()
        .filter(|(_, r)| r.layer == layer && r.rect.contains(pos))
        .max_by_key(|(i, r)| (r.depth, *i))
        .map(|(i, _)| i)
}

/// In hover mode (`focus_mode`), the surface under the moving pointer
/// gets the keys: the chain points there, and a widget holding the
/// focus in another surface lets go of it.
///
/// The place it moved to, if it did.
fn follow_pointer(ctx: &Context, s: &mut State) -> Option<(LayerId, Pos2)> {
    if !crate::focus_mode::pointer_may_focus(ctx) {
        return None;
    }
    let pos = ctx.input(|i| i.pointer.hover_pos())?;
    let layer = ctx.layer_id_at(pos)?;
    let under = deepest(&s.last, layer, pos)?;
    if let Some(id) = ctx.memory(|m| m.focused())
        && let Some(r) = peek_response(ctx, id)
        && deepest(&s.last, r.layer_id, r.interact_rect.center()) != Some(under)
    {
        ctx.memory_mut(|m| m.surrender_focus(id));
    }
    s.pressed = Some((layer, pos));
    Some((layer, pos))
}

fn rotate(ctx: &Context, s: &mut State) {
    let now = pass(ctx);
    if s.pass == now {
        return;
    }
    s.pass = now;
    let mut press = None;
    s.last = std::mem::take(&mut s.building);
    s.drawn_last = std::mem::take(&mut s.drawn);
    s.passing_last = std::mem::take(&mut s.passing);
    s.open.clear();
    if let Some(pos) = ctx.input(|i| {
        i.pointer
            .any_pressed()
            .then(|| i.pointer.interact_pos())
            .flatten()
    }) && let Some(layer) = ctx.layer_id_at(pos)
    {
        s.pressed = Some((layer, pos));
        press = Some((layer, pos));
    }
    press = follow_pointer(ctx, s).or(press);
    s.chain = outward(&s.last, target(ctx, s));
    s.moved = what_moved(ctx, s, press);
}

/// What changed since the last pass, given where the pointer pressed in this
/// one. A press says where it landed even when the focus stays elsewhere
/// until it is released.
fn what_moved(ctx: &Context, s: &mut State, press: Option<(LayerId, Pos2)>) -> Option<Moved> {
    let focused = ctx.memory(|m| m.focused());
    let window = ctx.input(|i| i.viewport().focused);
    let widget_changed = std::mem::replace(&mut s.seen, focused) != focused;
    let came_back = std::mem::replace(&mut s.window, window) == Some(false) && window == Some(true);
    let records = |s: &State, chain: &[usize]| -> Vec<ScopeRecord> {
        chain.iter().map(|&i| s.last[i].clone()).collect()
    };
    if let Some((layer, pos)) = press {
        let at = outward(&s.last, deepest(&s.last, layer, pos));
        return Some(Moved::Pressed(records(s, &at)));
    }
    if widget_changed {
        return Some(match focused {
            Some(_) => Moved::Focused(records(s, &s.chain)),
            None => Moved::Left,
        });
    }
    came_back.then(|| Moved::Focused(records(s, &s.chain)))
}

/// Draws `add` inside a named scope that sets `keys` (and `<name>Focus`)
/// while the focus is in it.
pub fn scope<R>(
    ui: &mut Ui,
    name: &str,
    keys: &[(&str, &str)],
    add: impl FnOnce(&mut Ui) -> R,
) -> R {
    let st = state(ui.ctx());
    let index = {
        let mut s = lock(&st);
        rotate(ui.ctx(), &mut s);
        let parent = s.open.last().copied();
        let depth = parent.map_or(1, |p| s.building[p].depth + 1);
        let mut all: Vec<(String, String)> = vec![(format!("{name}Focus"), "true".into())];
        all.extend(keys.iter().map(|(k, v)| (k.to_string(), v.to_string())));
        s.building.push(ScopeRecord {
            name: name.into(),
            keys: all,
            parent,
            depth,
            layer: ui.layer_id(),
            rect: Rect::NOTHING,
        });
        let first = widgets_in(ui);
        s.drawn.push(Drawn {
            widgets: first..first,
            preferred: None,
        });
        let i = s.building.len() - 1;
        s.open.push(i);
        i
    };
    // The region the scope may use (a panel's whole area), as visible.
    let (inner, rect) = ui
        .scope(|ui| {
            let inner = add(ui);
            let rect = ui.max_rect().union(ui.min_rect()).intersect(ui.clip_rect());
            (inner, rect)
        })
        .inner;
    let mut s = lock(&st);
    if let Some(r) = s.building.get_mut(index) {
        r.rect = rect;
    }
    let end = widgets_in(ui);
    if let Some(d) = s.drawn.get_mut(index) {
        d.widgets.end = end;
    }
    s.open.pop();
    inner
}

/// The names of the scopes this pass drew around a widget, innermost first.
/// Called at the end of the pass; empty if no scope was drawn in it.
pub(crate) fn scopes_holding(ctx: &Context, id: Id) -> Vec<String> {
    let st = state(ctx);
    let s = lock(&st);
    if s.pass != pass(ctx) {
        return Vec::new();
    }
    let Some((layer, order)) = ctx.viewport(|v| v.this_pass.widgets.order(id)) else {
        return Vec::new();
    };
    let at = s
        .building
        .iter()
        .zip(&s.drawn)
        .enumerate()
        .filter(|(_, (r, d))| r.layer == layer && d.widgets.contains(&order))
        .max_by_key(|(i, (r, _))| (r.depth, *i))
        .map(|(i, _)| i);
    let mut names: Vec<String> = outward(&s.building, at)
        .into_iter()
        .map(|i| s.building[i].name.clone())
        .collect();
    names.reverse();
    names
}

/// The widgets this pass drew in each of these scopes, in the same order, as
/// their order in the layer. A scope not drawn in this layer is left out.
pub(crate) fn spans_of(ctx: &Context, layer: LayerId, names: &[String]) -> Vec<Range<usize>> {
    let st = state(ctx);
    let s = lock(&st);
    if s.pass != pass(ctx) {
        return Vec::new();
    }
    names
        .iter()
        .filter_map(|name| {
            let at = s
                .building
                .iter()
                .position(|r| r.layer == layer && r.name == *name)?;
            Some(s.drawn.get(at)?.widgets.clone())
        })
        .collect()
}

/// How many widgets this pass drew so far in `ui`'s layer.
fn widgets_in(ui: &Ui) -> usize {
    let layer = ui.layer_id();
    ui.ctx()
        .viewport(|v| v.this_pass.widgets.get_layer(layer).count())
}

/// Names the widget the scopes open around it hand the focus to when asked
/// (call it each pass, in the scope, where it is drawn): the canvas under a
/// panel's toolbar, the list under its search field. Without one, a panel
/// given the keyboard goes to its first focusable widget. The first widget
/// to ask in a scope wins.
pub fn prefer(ctx: &Context, id: Id) {
    let st = state(ctx);
    let mut s = lock(&st);
    rotate(ctx, &mut s);
    let open = s.open.clone();
    for i in open {
        if let Some(d) = s.drawn.get_mut(i) {
            d.preferred.get_or_insert(id);
        }
    }
}

/// The widget named by [`prefer`] so far in the scope being drawn.
pub fn preferred(ctx: &Context) -> Option<Id> {
    let st = state(ctx);
    let s = lock(&st);
    let at = *s.open.last()?;
    s.drawn.get(at)?.preferred
}

/// What moved since the last time this was asked, at most once per change
/// and per window: where the pointer pressed, where a widget took the focus,
/// or that the focus went nowhere. Read before the pass's drawing, it says
/// what the end of the previous pass settled.
pub fn take_moved(ctx: &Context) -> Option<Moved> {
    let st = state(ctx);
    let mut s = lock(&st);
    rotate(ctx, &mut s);
    s.moved.take()
}

/// Installs the focus plugin on a context (once; later calls do
/// nothing): Escape leaves a text input before anything else sees it,
/// see [the order](self#escape-in-order), the keys typed with an AccessKit
/// `Focus` action reach the field it focuses, a widget the keyboard
/// focuses comes into view in the scroll areas around it, nested ones
/// included (the pointer's focus leaves them where they are), and the focus
/// keeps its place: Tab after Escape or after the focused widget went away
/// goes on from there, and a widget removed hands the focus to its neighbour.
pub fn install(ctx: &Context) {
    // Once per context, whichever window asks.
    let st = shared::<State>(ctx, "core.focus");
    let mut s = lock(&st);
    if !s.installed {
        s.installed = true;
        drop(s);
        ctx.add_plugin(FocusPlugin);
    }
}

/// Lets Escape through a text input (call it each pass it is drawn):
/// Escape then goes on to the overlay around it, as for a search field
/// that is the whole of a popup (the command palette), instead of
/// leaving the field first.
pub fn escape_passes(ctx: &Context, id: Id) {
    let st = state(ctx);
    let mut s = lock(&st);
    rotate(ctx, &mut s);
    s.passing.push(id);
}

/// Claims the keyboard for an open text input (call it each pass it
/// stays open): while it is claimed, [`text_input_owns_keyboard`] is
/// true, so key dispatchers leave Escape and Enter to the input. The
/// claim holds for the pass it is made in and the next one, since a
/// dispatcher runs before the input is drawn. A slider's typed entry
/// claims it: Escape cancels the entry, Enter commits it, and neither
/// runs a key binding.
pub fn claim_keyboard(ctx: &Context, id: Id) {
    let now = pass(ctx);
    let st = state(ctx);
    let mut s = lock(&st);
    s.claims.retain(|(c, p)| *c != id && p + 1 >= now);
    s.claims.push((id, now));
}

/// Whether a text input owns the keyboard: a text field has the focus,
/// or an open input claimed it ([`claim_keyboard`]) this pass or the
/// last. egui-pop's key dispatcher then leaves Escape and Enter (without
/// Ctrl or Alt) to the input; a host with a dispatcher of its own, or
/// one that reads Escape before the widgets are drawn, should ask this
/// first and do the same.
///
/// The claim matters for Escape: egui lets go of the focus when Escape
/// is pressed, before the pass is drawn, so on that pass no text field
/// has the focus any more.
pub fn text_input_owns_keyboard(ctx: &Context) -> bool {
    if ctx.text_edit_focused() {
        return true;
    }
    let now = pass(ctx);
    let st = state(ctx);
    let s = lock(&st);
    s.claims.iter().any(|(_, p)| p + 1 >= now)
}

/// Whether Escape just took the focus from this text input (the focus
/// plugin swallows that press, see [the order](self#escape-in-order)):
/// an input that cancels on Escape reads it here, since the key itself
/// is gone. Answered once: the press is spent by the first call that
/// sees it.
pub fn escape_left(ctx: &Context, id: Id) -> bool {
    let now = pass(ctx);
    let st = state(ctx);
    let mut s = lock(&st);
    match s.escaped {
        Some((e, p)) if e == id && p + 1 >= now => {
            s.escaped = None;
            true
        }
        _ => false,
    }
}

/// Runs before egui reads each pass's input.
#[derive(Debug)]
struct FocusPlugin;

impl egui::plugin::Plugin for FocusPlugin {
    fn debug_name(&self) -> &'static str {
        "egui_pop_core::focus"
    }

    fn input_hook(&mut self, ctx: &Context, input: &mut egui::RawInput) {
        hold_after_accesskit_focus(ctx, input);
        escape(ctx, input);
    }

    fn on_begin_pass(&mut self, ui: &mut Ui) {
        crate::reveal::begin_pass(ui.ctx());
        crate::place::begin_pass(ui.ctx());
    }

    fn on_end_pass(&mut self, ui: &mut Ui) {
        crate::place::end_pass(ui.ctx());
    }
}

/// Events held back for each window's next pass, see
/// [`hold_after_accesskit_focus`].
#[derive(Debug, Default)]
struct Held(Vec<(egui::ViewportId, Vec<egui::Event>)>);

/// An AccessKit `Focus` action (a screen reader, an agent) gives a text
/// field the keyboard from the next pass only: the keys typed with it
/// would be lost. The keyboard input that comes after the action in the
/// same pass is held back for the next one, in order, where the field
/// has the focus.
///
/// upstream: egui hands the focus over while the field is drawn, after
/// the field has read its input (docs/upstream.md, "egui: input in the
/// pass of an AccessKit focus is lost").
fn hold_after_accesskit_focus(ctx: &Context, input: &mut egui::RawInput) {
    let window = input.viewport_id;
    let st = shared::<Held>(ctx, "core.focus.held");
    let mut held = lock(&st);
    if let Some(i) = held.0.iter().position(|(w, _)| *w == window) {
        let (_, mut events) = held.0.swap_remove(i);
        events.append(&mut input.events);
        input.events = events;
    }
    let Some(at) = input.events.iter().position(is_accesskit_focus) else {
        return;
    };
    if !input.events[at + 1..].iter().any(is_keyboard_input) {
        return;
    }
    let rest = input.events.split_off(at + 1);
    held.0.push((window, rest));
    drop(held);
    ctx.request_repaint_of(window);
}

/// Whether an event asks, through AccessKit, to focus a widget.
fn is_accesskit_focus(e: &egui::Event) -> bool {
    matches!(
        e,
        egui::Event::AccessKitActionRequest(r) if r.action == egui::accesskit::Action::Focus
    )
}

/// Whether an event is input a focused text field reads.
fn is_keyboard_input(e: &egui::Event) -> bool {
    matches!(
        e,
        egui::Event::Text(_)
            | egui::Event::Key { .. }
            | egui::Event::Paste(_)
            | egui::Event::Copy
            | egui::Event::Cut
            | egui::Event::Ime(_)
    )
}

/// Whether an event is a press of Escape without modifiers.
fn is_escape(e: &egui::Event) -> bool {
    matches!(
        e,
        egui::Event::Key {
            key: egui::Key::Escape,
            pressed: true,
            modifiers,
            ..
        } if !modifiers.any()
    )
}

/// Step 1 of the Escape order: a focused text input lets go of the
/// focus and swallows the press. Whatever had the focus, its place is
/// kept for the focus chain (egui drops the focus on Escape before this
/// pass draws), so the surface around it still gets the keys.
fn escape(ctx: &Context, input: &mut egui::RawInput) {
    let Some(at) = input.events.iter().position(is_escape) else {
        return;
    };
    let Some(id) = ctx.memory(|m| m.focused()) else {
        return;
    };
    if let Some(r) = peek_response(ctx, id) {
        let st = state(ctx);
        lock(&st).pressed = Some((r.layer_id, r.interact_rect.center()));
    }
    let passes = {
        let st = state(ctx);
        let s = lock(&st);
        s.passing.contains(&id) || s.passing_last.contains(&id)
    };
    if ctx.text_edit_focused() && !passes {
        input.events.remove(at);
        ctx.memory_mut(|m| m.surrender_focus(id));
        lock(&state(ctx)).escaped = Some((id, pass(ctx)));
        log::debug!(target: "egui_pop", "Escape: left the text input {id:?}");
    }
}

/// The scopes of the focus chain, outermost first.
pub fn chain(ctx: &Context) -> Vec<ScopeRecord> {
    let st = state(ctx);
    let mut s = lock(&st);
    rotate(ctx, &mut s);
    s.chain.iter().map(|&i| s.last[i].clone()).collect()
}

/// The names in the focus chain, outermost first.
pub fn chain_names(ctx: &Context) -> Vec<String> {
    chain(ctx).into_iter().map(|r| r.name).collect()
}

/// The context keys: `base` (depth 0), each scope of the chain at its
/// depth, then `textInputFocus` at the deepest depth when a text field
/// has the focus, and `widgetFocus` whenever a widget has it.
pub fn when_context(ctx: &Context, base: &WhenContext) -> WhenContext {
    let mut cx = base.clone();
    let chain = chain(ctx);
    let mut depth = 0;
    for r in &chain {
        depth = r.depth;
        for (k, v) in &r.keys {
            cx.set_at(k, v, r.depth);
        }
    }
    if ctx.text_edit_focused() {
        cx.flag_at("textInputFocus", true, depth + 1);
    }
    if ctx.memory(|m| m.focused()).is_some() {
        cx.flag_at("widgetFocus", true, depth + 1);
    }
    cx
}

/// The focused widget, if any.
pub fn focused(ctx: &Context) -> Option<Id> {
    ctx.memory(|m| m.focused())
}

/// The scopes recorded in the previous pass (for the debug bridge).
pub fn scopes(ctx: &Context) -> Vec<ScopeRecord> {
    let st = state(ctx);
    let mut s = lock(&st);
    rotate(ctx, &mut s);
    s.last.clone()
}
