//! A drag across windows, on the bus (ADR 0029).
//!
//! egui keeps each window's input apart. While the button is held, the
//! system gives every pointer event to the window the drag began in
//! (Wayland's and X11's implicit grab, the capture on Windows), whose
//! pointer keeps following the drag past its edges; no other window sees
//! the drag, nor the release. egui's own drag payload lives in one
//! window's passes: it is gone before another window could take it.
//!
//! A **drag session** carries any typed payload from the window it began
//! in to the window it is let go over:
//!
//! - the source starts it ([`start`]), naming its kind; `pop.drag.start` is
//!   asked ([`DragStart`]), then `drag.started` said;
//! - each pass, the **drop targets** say where they are, by window and by
//!   id ([`target`]): over one, the target shows where the payload would
//!   go (per-frame painting, the ADR's exception: no event per pass);
//! - let go, the payload is delivered once, to the target under the
//!   pointer: in the window it began in, or in another window, found from
//!   the screen's coordinates where the system tells where windows are;
//!   where it hides them (Wayland), the first window that sees the pointer
//!   over one of its targets once the drag lets it go takes it, within a
//!   few passes. A delivery is `pop.drag.drop` ([`DragDrop`]), then
//!   `drag.dropped`; a target that refuses it says why, in `drag.refused`
//!   with its reason, as does a drag let go over no target and taken by
//!   nobody, or cancelled (`Escape`). A source handling a drop in its own
//!   window by itself (the workbench's tabs into its areas) claims it
//!   ([`claim`]): `drag.dropped` names what took it.
//!
//! The workbench's tabs and tab groups are carried by the session too, so
//! their drags are said on the bus; their drop is the workbench's dock
//! space's (egui_stack's one drag across its windows), which ends the
//! session where it lands it, or refuses it, at a dock of another window:
//! a release the space took is never delivered to a target of the
//! session, nor said twice.

use std::any::Any;
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use bus_contract::{Bus, ListenOpts, Point, PointId, PointKind};
use egui::{Context, Id, Pos2, Rect, Vec2, ViewportId};
use egui_pop_core::lock;
use serde::Serialize;
use serde_json::json;

use super::{OWNER, emit_with};
use crate::app::AppHandle;

/// How many passes a payload let go outside its window waits for another
/// window to see the pointer over one of its targets (a pass a frame while
/// it waits: about a fifth of a second), where the system hides window
/// positions.
const RELEASED_PASSES: u8 = 12;

/// A drag begins: its kind and the window it began in.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[non_exhaustive]
pub struct DragBegin {
    /// What is dragged, as the source names it (`tab`, `photos`).
    pub kind: String,
    /// The window it began in (`main`, an application window's id,
    /// `panel:<id>`).
    pub window: String,
}

/// A drag let go over a target that takes it.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[non_exhaustive]
pub struct DragDelivery {
    pub kind: String,
    /// The window it began in.
    pub source: String,
    /// The window it is delivered in.
    pub window: String,
    /// The target that takes it, by the id it registered with.
    pub target: String,
}

/// A drag begins (`pop.drag.start`): its listeners are told, each saying what
/// it did in the value.
#[derive(Debug)]
pub struct DragStart;
impl Point for DragStart {
    const KIND: PointKind = PointKind::Request;
    const ID: PointId = PointId::new("pop.drag.start");
    type Input = DragBegin;
    type Value = Vec<String>;
}

/// A drag is delivered to a target (`pop.drag.drop`): egui-pop's listener
/// [`TARGET`] says which, first.
#[derive(Debug)]
pub struct DragDrop;
impl Point for DragDrop {
    const KIND: PointKind = PointKind::Request;
    const ID: PointId = PointId::new("pop.drag.drop");
    type Input = DragDelivery;
    type Value = Vec<String>;
}

/// The drag points, in the order [`super::order_dump`] prints them.
pub const DRAG_POINTS: [&str; 2] = [DragStart::ID.as_str(), DragDrop::ID.as_str()];

/// egui-pop's listener of `pop.drag.drop`, first: the target that took it.
pub const TARGET: &str = "pop.drag.target";

/// What a drop target sees of a drag of its payload type in a pass.
#[derive(Debug)]
#[non_exhaustive]
pub enum Seen<T> {
    /// No drag of this type, or not over the target.
    Nothing,
    /// Over it, at this point of its window.
    Over { payload: Arc<T>, at: Pos2 },
    /// Dragged out of its window where the system hides where windows are
    /// (Wayland): this window cannot tell whether the pointer is over the
    /// target until the drag lets it go. The best hint is an outline.
    Unknown(Arc<T>),
    /// Let go over it: it is the target's, now, once.
    Dropped(Arc<T>),
    /// Let go over it while it refused it: why (said on the bus).
    Refused { payload: Arc<T>, reason: String },
}

/// How a drag ended, for its source ([`ended`]).
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum End {
    /// Delivered to a target of a window.
    Dropped { window: String, target: String },
    /// Let go over a target that refused it.
    Refused {
        window: String,
        target: String,
        reason: String,
    },
    /// Let go over no target: in the window it began in (`inside`), or
    /// outside every window that could tell. The source may handle it
    /// ([`claim`]).
    Left { inside: bool },
    /// Cancelled (`Escape`).
    Cancelled,
}

type Payload = Arc<dyn Any + Send + Sync>;

/// A drag in progress.
struct Session {
    kind: String,
    payload: Payload,
    /// The window it began in, its viewport and its name on the bus.
    source: ViewportId,
    source_window: String,
    /// The source's pointer, in its points: it keeps following the drag
    /// past the window's edges.
    at: Pos2,
    /// The pointer left the source window.
    outside: bool,
    /// The source's top-left on the screen, in pixels, and its scale;
    /// `None` where the system hides window positions (Wayland).
    origin: Option<(Vec2, f32)>,
    /// The source's passes since the drag began.
    pass: u64,
    /// Let go outside: the source's passes left for a window to take it.
    released: Option<u8>,
}

/// A target registered in a window's pass.
#[derive(Clone)]
struct Registered {
    id: String,
    rect: Rect,
    refusal: Option<String>,
}

/// The target under the pointer in a window, at the end of its last pass.
#[derive(Clone)]
struct Under {
    window: String,
    target: Registered,
    /// The source's pass it was found in.
    pass: u64,
}

/// A drop waiting for its target's call.
struct Delivered {
    window: String,
    target: String,
    payload: Payload,
    refused: Option<String>,
    /// Passes of its window begun since: one is the target's.
    passes: u8,
}

/// What a drag ended as, kept for its source until the next drag.
struct Ended {
    kind: String,
    payload: Payload,
    source: String,
    end: End,
    /// What the source said took a drop it handled itself.
    claimed: Option<String>,
    /// Said on the bus.
    said: bool,
    /// Taken by its source ([`take_ended`]).
    taken: bool,
}

/// What the bus is told at the end of a window's pass.
enum Say {
    Start(DragBegin),
    Drop(DragDelivery),
    Refused(serde_json::Value),
}

#[derive(Default)]
struct State {
    session: Option<Session>,
    /// Each window's name on the bus, by viewport, from its pass.
    windows: BTreeMap<ViewportId, String>,
    /// The targets registered in each window's pass under way.
    targets: BTreeMap<ViewportId, Vec<Registered>>,
    /// The target under the pointer, per window, at its last pass's end.
    under: BTreeMap<ViewportId, Under>,
    delivered: Vec<Delivered>,
    ended: Option<Ended>,
    say: Vec<Say>,
}

impl State {
    fn window_of(&self, v: ViewportId) -> String {
        self.windows
            .get(&v)
            .cloned()
            .unwrap_or_else(|| crate::windows::WindowId::MAIN.to_string())
    }

    /// Ends the session as `end`, said on the bus now unless the source
    /// may still claim it (let go over no target).
    fn end(&mut self, end: End) {
        let Some(s) = self.session.take() else { return };
        self.under.clear();
        match &end {
            End::Dropped { window, target } => self.say.push(Say::Drop(DragDelivery {
                kind: s.kind.clone(),
                source: s.source_window.clone(),
                window: window.clone(),
                target: target.clone(),
            })),
            End::Refused {
                window,
                target,
                reason,
            } => self.say.push(Say::Refused(json!({
                "kind": s.kind,
                "source": s.source_window,
                "window": window,
                "target": target,
                "reason": reason,
            }))),
            End::Cancelled => self.say.push(Say::Refused(json!({
                "kind": s.kind,
                "source": s.source_window,
                "window": null,
                "target": null,
                "reason": "cancelled",
            }))),
            End::Left { .. } => {}
        }
        let said = !matches!(end, End::Left { .. });
        self.ended = Some(Ended {
            kind: s.kind,
            payload: s.payload,
            source: s.source_window,
            end,
            claimed: None,
            said,
            taken: false,
        });
    }

    /// Delivers the session to `target` of `window`: taken, or refused.
    fn deliver(&mut self, window: String, target: &Registered) {
        let Some(s) = self.session.as_ref() else {
            return;
        };
        self.delivered.push(Delivered {
            window: window.clone(),
            target: target.id.clone(),
            payload: s.payload.clone(),
            refused: target.refusal.clone(),
            passes: 0,
        });
        let end = match &target.refusal {
            Some(reason) => End::Refused {
                window,
                target: target.id.clone(),
                reason: reason.clone(),
            },
            None => End::Dropped {
                window,
                target: target.id.clone(),
            },
        };
        self.end(end);
    }
}

fn state(ctx: &Context) -> Arc<Mutex<State>> {
    ctx.data_mut(|d| {
        d.get_temp_mut_or_default::<Arc<Mutex<State>>>(Id::unique("egui_pop.drag"))
            .clone()
    })
}

/// Starts a drag of `payload` in the window being drawn (`ctx`'s
/// viewport), as `kind`: a source calls it when its drag starts
/// (`Response::drag_started`). A drag still in progress is replaced.
///
/// <div class="warning">Experimental: may change in any release
/// (docs/stability.md).</div>
pub fn start<T: Any + Send + Sync>(ctx: &Context, kind: &str, payload: T) {
    let st = state(ctx);
    let mut st = lock(&st);
    let source = ctx.viewport_id();
    let source_window = st.window_of(source);
    let at = ctx.input(|i| i.pointer.latest_pos()).unwrap_or(Pos2::ZERO);
    st.session = None;
    st.under.clear();
    st.ended = None;
    st.say.push(Say::Start(DragBegin {
        kind: kind.to_string(),
        window: source_window.clone(),
    }));
    st.session = Some(Session {
        kind: kind.to_string(),
        payload: Arc::new(payload),
        source,
        source_window,
        at,
        outside: false,
        origin: origin(ctx),
        pass: 0,
        released: None,
    });
}

/// The payload of the drag in progress, when it is a `T`.
///
/// <div class="warning">Experimental: may change in any release
/// (docs/stability.md).</div>
pub fn payload<T: Any + Send + Sync>(ctx: &Context) -> Option<Arc<T>> {
    let st = state(ctx);
    let st = lock(&st);
    st.session
        .as_ref()
        .and_then(|s| s.payload.clone().downcast::<T>().ok())
}

/// Whether a drag is in progress (held, or let go and waiting for a
/// window to take it).
///
/// <div class="warning">Experimental: may change in any release
/// (docs/stability.md).</div>
pub fn active(ctx: &Context) -> bool {
    lock(&state(ctx)).session.is_some()
}

/// Whether the drag in progress was let go and waits for another window
/// to see the pointer.
pub(crate) fn waiting(ctx: &Context) -> bool {
    lock(&state(ctx))
        .session
        .as_ref()
        .is_some_and(|s| s.released.is_some())
}

/// Where the pointer of the drag of a `T` in progress (held, or let go and
/// waiting for a window to take it) is in the window being drawn (`ctx`'s
/// viewport), in its points, when it can be known.
pub(crate) fn pointer_here<T: Any + Send + Sync>(ctx: &Context) -> Option<Pos2> {
    let st = state(ctx);
    let st = lock(&st);
    let s = st.session.as_ref().filter(|s| s.payload.is::<T>())?;
    pointer_in(s, ctx)
}

/// How the last drag of a `T` ended, and its payload, while no other drag
/// began: for its source, to handle a drop over no target itself.
///
/// <div class="warning">Experimental: may change in any release
/// (docs/stability.md).</div>
pub fn ended<T: Any + Send + Sync>(ctx: &Context) -> Option<(Arc<T>, End)> {
    let st = state(ctx);
    let st = lock(&st);
    let e = st.ended.as_ref().filter(|e| !e.taken)?;
    Some((e.payload.clone().downcast::<T>().ok()?, e.end.clone()))
}

/// Takes how the last drag of a `T` ended: [`ended`], once.
///
/// <div class="warning">Experimental: may change in any release
/// (docs/stability.md).</div>
pub fn take_ended<T: Any + Send + Sync>(ctx: &Context) -> Option<(Arc<T>, End)> {
    let st = state(ctx);
    let mut st = lock(&st);
    let e = st.ended.as_mut().filter(|e| !e.taken)?;
    let payload = e.payload.clone().downcast::<T>().ok()?;
    e.taken = true;
    Some((payload, e.end.clone()))
}

/// The source handled a drag let go over no target ([`End::Left`]): `what`
/// took it (the workbench's areas), said as the target of `drag.dropped`.
///
/// <div class="warning">Experimental: may change in any release
/// (docs/stability.md).</div>
pub fn claim(ctx: &Context, what: &str) {
    let st = state(ctx);
    let mut st = lock(&st);
    if let Some(e) = st.ended.as_mut().filter(|e| !e.said) {
        e.claimed = Some(what.to_string());
    }
}

/// A drop the workbench's dock space took, or refused at a target of its
/// own (a tab or a tab group let go over a dock of another window): the
/// drag of a `T` in progress, or the one just let go over no target of
/// the session and not said yet, ends there, delivered to no target of
/// the session (the claim rule: a release the space took is never
/// delivered twice). `drag.dropped` names the window and the target the
/// space took it in; `drag.refused` the reason, when refused.
pub(crate) fn taken<T: Any + Send + Sync>(
    ctx: &Context,
    window: &str,
    target: &str,
    refusal: Option<String>,
) {
    let st = state(ctx);
    let mut st = lock(&st);
    let end = match refusal {
        Some(reason) => End::Refused {
            window: window.to_string(),
            target: target.to_string(),
            reason,
        },
        None => End::Dropped {
            window: window.to_string(),
            target: target.to_string(),
        },
    };
    if st.session.as_ref().is_some_and(|s| s.payload.is::<T>()) {
        st.end(end);
        return;
    }
    let Some(e) = st
        .ended
        .as_mut()
        .filter(|e| !e.said && e.payload.is::<T>() && matches!(e.end, End::Left { .. }))
    else {
        return;
    };
    e.said = true;
    let say = match &end {
        End::Refused {
            window,
            target,
            reason,
        } => Say::Refused(json!({
            "kind": e.kind,
            "source": e.source,
            "window": window,
            "target": target,
            "reason": reason,
        })),
        _ => Say::Drop(DragDelivery {
            kind: e.kind.clone(),
            source: e.source.clone(),
            window: window.to_string(),
            target: target.to_string(),
        }),
    };
    e.end = end;
    st.say.push(say);
}

/// The window's top-left on the screen, in pixels, and its scale, where
/// the system tells it.
fn origin(ctx: &Context) -> Option<(Vec2, f32)> {
    let ppp = ctx.pixels_per_point();
    ctx.input(|i| i.viewport().inner_rect)
        .map(|r| (r.min.to_vec2() * ppp, ppp))
}

/// Where the session's pointer is in the window being drawn (`ctx`), in
/// its points, when it can be known: the window's own pointer; in the
/// source window, the drag's; in another window, the drag's carried over
/// the screen when both windows know where they are (not on Wayland).
fn pointer_in(s: &Session, ctx: &Context) -> Option<Pos2> {
    if let Some(p) = ctx.input(|i| i.pointer.hover_pos()) {
        return Some(p);
    }
    if ctx.viewport_id() == s.source {
        return Some(s.at);
    }
    let (origin, ppp) = s.origin?;
    let inner = ctx.input(|i| i.viewport().inner_rect)?;
    let screen = origin + s.at.to_vec2() * ppp;
    Some((screen / ctx.pixels_per_point() - inner.min.to_vec2()).to_pos2())
}

/// A drop target for a payload of type `T`, in the window being drawn
/// (`ctx`'s viewport), by its `id` (unique in its window) and its `rect`:
/// what it sees of a drag this pass. `refusal` says why it does not take
/// a payload, asked only while one hovers it.
///
/// A drop is delivered to the target once: as [`Seen::Dropped`] (or
/// [`Seen::Refused`]) in the pass it is let go over it, or in this
/// window's next pass when the release was resolved before the target
/// was drawn (in another window, from the screen's coordinates).
///
/// <div class="warning">Experimental: may change in any release
/// (docs/stability.md).</div>
pub fn target<T: Any + Send + Sync>(
    ctx: &Context,
    id: &str,
    rect: Rect,
    refusal: impl FnOnce(&T) -> Option<String>,
) -> Seen<T> {
    let st = state(ctx);
    let mut st = lock(&st);
    let viewport = ctx.viewport_id();
    let window = st.window_of(viewport);
    // A drop already resolved for this target.
    if let Some(i) = st
        .delivered
        .iter()
        .position(|d| d.window == window && d.target == id && d.payload.is::<T>())
    {
        let d = st.delivered.remove(i);
        let Ok(payload) = d.payload.downcast::<T>() else {
            return Seen::Nothing;
        };
        return match d.refused {
            Some(reason) => Seen::Refused { payload, reason },
            None => Seen::Dropped(payload),
        };
    }
    let Some(s) = st.session.as_ref() else {
        return Seen::Nothing;
    };
    let Ok(payload) = s.payload.clone().downcast::<T>() else {
        return Seen::Nothing;
    };
    let pointer = pointer_in(s, ctx);
    let own = viewport == s.source;
    let Some(at) = pointer.filter(|p| rect.contains(*p)) else {
        return match pointer {
            None if s.outside && !own => Seen::Unknown(payload),
            _ => Seen::Nothing,
        };
    };
    let registered = Registered {
        id: id.to_string(),
        rect,
        refusal: refusal(&payload),
    };
    // Let go outside its window, where no window could tell it was under
    // the pointer: the first that sees it over a target takes it.
    if s.released.is_some() && ctx.input(|i| i.pointer.hover_pos()).is_some() {
        st.deliver(window, &registered);
        let d = st.delivered.pop().expect("just delivered");
        return match d.refused {
            Some(reason) => Seen::Refused { payload, reason },
            None => Seen::Dropped(payload),
        };
    }
    if s.released.is_some() {
        return Seen::Nothing;
    }
    st.targets.entry(viewport).or_default().push(registered);
    Seen::Over { payload, at }
}

/// A window's pass begins (`window`: its name on the bus). In the window
/// a drag began in: its pointer followed; let go, the drag is delivered
/// to the target found under the pointer in the last passes (in this
/// window when it is let go inside it, else in the window the screen's
/// coordinates place it in), else waits a few passes for another window
/// to see the pointer; `Escape` cancels it.
pub(crate) fn begin_pass(ctx: &Context, window: &str) {
    let st = state(ctx);
    let mut st = lock(&st);
    let viewport = ctx.viewport_id();
    st.windows.insert(viewport, window.to_string());
    st.targets.remove(&viewport);
    // Drops their target did not take in a whole pass of their window:
    // forgotten.
    st.delivered.retain_mut(|d| {
        if d.window != window {
            return true;
        }
        d.passes += 1;
        d.passes < 2
    });
    let escape = ctx.input(|i| i.key_pressed(egui::Key::Escape));
    let (latest, released, down) = ctx.input(|i| {
        (
            i.pointer.latest_pos(),
            i.pointer.any_released(),
            i.pointer.any_down(),
        )
    });
    let viewport_rect = ctx.input(|i| i.viewport_rect());
    let here = origin(ctx);
    let (pass, outside) = {
        let Some(s) = st.session.as_mut().filter(|s| s.source == viewport) else {
            return;
        };
        s.pass += 1;
        match s.released {
            Some(0) => None,
            Some(n) => {
                s.released = Some(n - 1);
                ctx.request_repaint();
                return;
            }
            None => {
                if let Some(p) = latest {
                    s.at = p;
                }
                s.outside = !viewport_rect.contains(s.at);
                s.origin = here;
                Some((s.pass, s.outside))
            }
        }
        .unwrap_or((0, true))
    };
    if pass == 0 {
        // Let go outside, and no window took it in time.
        st.end(End::Left { inside: false });
        return;
    }
    if escape {
        st.end(End::Cancelled);
        return;
    }
    if !released && down {
        return;
    }
    let fresh = |u: &&Under| u.pass + 1 >= pass;
    let under = if outside {
        st.under
            .iter()
            .filter(|(v, _)| **v != viewport)
            .map(|(_, u)| u)
            .find(fresh)
            .cloned()
    } else {
        st.under.get(&viewport).filter(fresh).cloned()
    };
    match under {
        Some(u) => {
            st.deliver(u.window, &u.target);
            ctx.request_repaint();
        }
        // No other window to see the pointer.
        None if outside && st.windows.len() <= 1 => st.end(End::Left { inside: false }),
        None if outside => {
            if let Some(s) = st.session.as_mut() {
                s.released = Some(RELEASED_PASSES);
            }
            ctx.request_repaint();
        }
        None => st.end(End::Left { inside: true }),
    }
}

/// A window's pass ends: the target under the pointer in it kept for the
/// release (the last registered: drawn over the others), and what the
/// bus is to be told said.
pub(crate) fn end_pass(app: &AppHandle, ctx: &Context) {
    let st = state(ctx);
    let say = {
        let mut st = lock(&st);
        let viewport = ctx.viewport_id();
        let window = st.window_of(viewport);
        let targets = st.targets.remove(&viewport).unwrap_or_default();
        let found = st.session.as_ref().and_then(|s| {
            let at = pointer_in(s, ctx)?;
            let t = targets.iter().rev().find(|t| t.rect.contains(at))?;
            Some(Under {
                window,
                target: t.clone(),
                pass: s.pass,
            })
        });
        match found {
            Some(u) => {
                st.under.insert(viewport, u);
            }
            None => {
                st.under.remove(&viewport);
            }
        }
        // A drag let go over no target, in the source's pass: what the
        // source claimed, or nobody.
        let source_pass = st
            .ended
            .as_ref()
            .is_some_and(|e| !e.said && st.window_of(viewport) == e.source);
        let said = if source_pass && let Some(e) = st.ended.as_mut() {
            e.said = true;
            let inside = matches!(e.end, End::Left { inside: true });
            let say = match e.claimed.clone() {
                Some(what) => Say::Drop(DragDelivery {
                    kind: e.kind.clone(),
                    source: e.source.clone(),
                    window: if inside {
                        e.source.clone()
                    } else {
                        String::new()
                    },
                    target: what,
                }),
                None => Say::Refused(json!({
                    "kind": e.kind,
                    "source": e.source,
                    "window": if inside { json!(e.source) } else { json!(null) },
                    "target": null,
                    "reason": if inside {
                        "nothing takes it here"
                    } else {
                        "let go outside every window"
                    },
                })),
            };
            Some(say)
        } else {
            None
        };
        st.say.extend(said);
        std::mem::take(&mut st.say)
    };
    for s in say {
        match s {
            Say::Start(begin) => {
                let did = emit_with::<DragStart>(app, begin.clone(), Vec::new());
                app.publish(
                    "egui_pop",
                    None,
                    "drag.started",
                    json!({ "kind": begin.kind, "window": begin.window, "did": did }),
                );
            }
            Say::Drop(d) => {
                let did = emit_with::<DragDrop>(app, d.clone(), Vec::new());
                app.publish(
                    "egui_pop",
                    None,
                    "drag.dropped",
                    json!({
                        "kind": d.kind,
                        "source": d.source,
                        "window": d.window,
                        "target": d.target,
                        "did": did,
                    }),
                );
            }
            Say::Refused(data) => {
                log::info!(target: "egui_pop", "drag refused: {data}");
                app.publish("egui_pop", None, "drag.refused", data);
            }
        }
    }
}

/// Registers egui-pop's listener of `pop.drag.drop`.
pub(super) fn install(kernel: &dyn Bus) {
    let listened = kernel.listen::<DragDrop>(
        TARGET,
        ListenOpts::new().owner(OWNER).priority(1000),
        |d, did| {
            did.push(format!("{TARGET}: `{}` in `{}`", d.target, d.window));
            Ok(())
        },
    );
    debug_assert!(listened.is_ok(), "{listened:?}");
}
