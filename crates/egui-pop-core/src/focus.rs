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
//! the keyboard.
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

use egui::{Context, Id, LayerId, Pos2, Rect, Ui};
use egui_pop_model::when::WhenContext;
use serde::Serialize;

use crate::state::{lock, pass, shared};

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
}

fn state(ctx: &Context) -> std::sync::Arc<std::sync::Mutex<State>> {
    shared::<State>(ctx, "core.focus")
}

/// What the focus chain is pointed at.
fn target(ctx: &Context, pressed: Option<(LayerId, Pos2)>) -> Option<(LayerId, Pos2)> {
    if let Some(id) = ctx.memory(|m| m.focused())
        && let Some(r) = ctx.read_response(id)
    {
        return Some((r.layer_id, r.interact_rect.center()));
    }
    pressed
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
fn follow_pointer(ctx: &Context, s: &mut State) {
    if !crate::focus_mode::pointer_may_focus(ctx) {
        return;
    }
    let Some(pos) = ctx.input(|i| i.pointer.hover_pos()) else {
        return;
    };
    let Some(layer) = ctx.layer_id_at(pos) else {
        return;
    };
    let Some(under) = deepest(&s.last, layer, pos) else {
        return;
    };
    if let Some(id) = ctx.memory(|m| m.focused())
        && let Some(r) = ctx.read_response(id)
        && deepest(&s.last, r.layer_id, r.interact_rect.center()) != Some(under)
    {
        ctx.memory_mut(|m| m.surrender_focus(id));
    }
    s.pressed = Some((layer, pos));
}

fn rotate(ctx: &Context, s: &mut State) {
    let now = pass(ctx);
    if s.pass == now {
        return;
    }
    s.pass = now;
    s.last = std::mem::take(&mut s.building);
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
    }
    follow_pointer(ctx, s);
    s.chain.clear();
    if let Some((layer, pos)) = target(ctx, s.pressed) {
        let mut at = deepest(&s.last, layer, pos);
        while let Some(i) = at {
            s.chain.push(i);
            at = s.last[i].parent;
        }
        s.chain.reverse();
    }
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
    s.open.pop();
    inner
}

/// Installs the focus plugin on a context (once; later calls do
/// nothing): Escape leaves a text input before anything else sees it,
/// see [the order](self#escape-in-order).
pub fn install(ctx: &Context) {
    let st = state(ctx);
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
        escape(ctx, input);
    }
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
    if let Some(r) = ctx.read_response(id) {
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
