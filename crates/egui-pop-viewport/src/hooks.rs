//! The application's way into each step of the viewport's lifecycle.
//!
//! The widget does the plumbing; at each step an application may:
//!
//! - **observe**: every change is a [`Signal`](crate::Signal) in
//!   [`ViewportOutput::signals`](crate::ViewportOutput) (forward them to
//!   the host's journal);
//! - **veto or redirect**: a *before* hook answers a [`Verdict`] (clamp
//!   the zoom, keep the view inside the image, snap a handle);
//! - **replace**: a *run* hook takes the place of the default action and
//!   may call it ([`Hooks::on_double_click`]);
//! - **extend**: an *after* hook adds to what was done
//!   ([`Hooks::under_overlays`] draws over the image and under the
//!   overlays, [`Hooks::after_paint`] over everything).
//!
//! Every hook is optional; without one the default applies. What hooks
//! decided is kept in the state's [`trace`](crate::ViewportState::trace)
//! and shown by the debug bridge's `viewport` collector.

use std::fmt;

use egui::{Painter, Pos2};

use crate::display::{DisplayTarget, Resolution};
use crate::overlay::HandleKind;
use crate::source::Readout;
use crate::view::{Mapping, View};
use crate::widget::Compare;

/// What a *before* hook decides.
#[derive(Clone, Debug, PartialEq)]
pub enum Verdict<T> {
    /// Go ahead as proposed.
    Allow,
    /// Do nothing: the step does not happen.
    Veto,
    /// Go ahead with this value instead (a clamped zoom, a snapped point).
    Redirect(T),
}

/// What made the view change.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cause {
    Pan,
    Wheel,
    Pinch,
    Key,
    DoubleClick,
    /// A click the pointer profile gives a view action (a middle click
    /// cycling the zoom).
    Click,
    /// The application called the viewport (a command).
    Command,
}

impl Cause {
    pub fn name(self) -> &'static str {
        match self {
            Cause::Pan => "pan",
            Cause::Wheel => "wheel",
            Cause::Pinch => "pinch",
            Cause::Key => "key",
            Cause::DoubleClick => "double-click",
            Cause::Click => "click",
            Cause::Command => "command",
        }
    }
}

/// A change of view the widget proposes.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ViewChange {
    pub from: View,
    pub to: View,
    pub cause: Cause,
}

/// A handle move the widget proposes.
#[derive(Clone, Debug, PartialEq)]
pub struct HandleMove {
    pub overlay: String,
    pub handle: u32,
    pub kind: HandleKind,
    /// Where the handle was when the gesture began (image pixels).
    pub start: [f64; 2],
    /// Where the handle is now (image pixels).
    pub from: [f64; 2],
    /// Where the pointer would put it.
    pub to: [f64; 2],
    /// The keys held (Shift to keep a direction, Alt to move a control
    /// alone...).
    pub modifiers: egui::Modifiers,
}

type BeforeHandle = Box<dyn FnMut(&HandleMove, &Mapping) -> Verdict<[f64; 2]>>;
type BeforeCompare = Box<dyn FnMut(&Compare, &Compare) -> Verdict<Compare>>;
type OnReadout = Box<dyn FnMut(&Readout) -> String>;
type BeforeView = Box<dyn FnMut(&ViewChange, &Mapping) -> Verdict<View>>;
type RunDoubleClick =
    Box<dyn FnMut(Pos2, &Mapping, &mut dyn FnMut() -> Option<View>) -> Option<View>>;
type AfterPaint = Box<dyn FnMut(&Painter, &Mapping)>;
type BeforeDisplay = Box<dyn FnMut(&Resolution) -> Verdict<DisplayTarget>>;

/// The hooks of one viewport (all optional). Set them on the
/// [`ViewportState`](crate::ViewportState) with
/// [`hooks_mut`](crate::ViewportState::hooks_mut).
#[derive(Default)]
pub struct Hooks {
    /// Before the view changes: allow, veto or redirect (clamp the zoom,
    /// keep the image in view). Not asked when a gesture is cancelled:
    /// that puts back a view already allowed.
    pub before_view: Option<BeforeView>,
    /// Replaces the double click on the canvas (not one a handle took).
    /// Gets the pointer, the mapping and the profile's default action;
    /// returns the view to show, or `None` for no change. A hook that
    /// handles the click itself does not call the default; one that
    /// passes it on calls it and returns its view. Calling it runs the
    /// whole default: when the profile's action is to leave the view it
    /// returns `None` and the viewport sends `Signal::Leave`, exactly as
    /// without a hook.
    pub on_double_click: Option<RunDoubleClick>,
    /// Before a handle moves: allow, veto (it stays) or redirect (snap
    /// it to a grid, keep it inside the image).
    pub before_handle: Option<BeforeHandle>,
    /// Before the comparison changes (the current one, the proposed
    /// one): allow, veto or redirect.
    pub before_compare: Option<BeforeCompare>,
    /// Replaces the readout's text.
    pub on_readout: Option<OnReadout>,
    /// After the image, before the overlays: draw between them (a
    /// mask's tint, a selection's veil), in image coordinates through
    /// the mapping; the overlays and their handles stay on top.
    pub under_overlays: Option<AfterPaint>,
    /// After the image, the overlays and their handles are drawn: draw
    /// more on top, in image coordinates through the mapping.
    pub after_paint: Option<AfterPaint>,
    /// Before the viewport draws for the target it resolved (what is
    /// wanted against what the surface carries): allow, veto (draw
    /// sRGB) or redirect (draw for another target the surface carries,
    /// a soft proof for example). Asked when the resolution changes.
    pub before_display: Option<BeforeDisplay>,
}

impl fmt::Debug for Hooks {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Hooks")
            .field("before_view", &self.before_view.is_some())
            .field("on_double_click", &self.on_double_click.is_some())
            .field("before_handle", &self.before_handle.is_some())
            .field("before_compare", &self.before_compare.is_some())
            .field("on_readout", &self.on_readout.is_some())
            .field("under_overlays", &self.under_overlays.is_some())
            .field("after_paint", &self.after_paint.is_some())
            .field("before_display", &self.before_display.is_some())
            .finish()
    }
}

impl Hooks {
    /// The hooks that are set, by name.
    pub fn names(&self) -> Vec<&'static str> {
        let mut v = Vec::new();
        if self.before_view.is_some() {
            v.push("before_view");
        }
        if self.on_double_click.is_some() {
            v.push("on_double_click");
        }
        if self.before_handle.is_some() {
            v.push("before_handle");
        }
        if self.before_compare.is_some() {
            v.push("before_compare");
        }
        if self.on_readout.is_some() {
            v.push("on_readout");
        }
        if self.under_overlays.is_some() {
            v.push("under_overlays");
        }
        if self.after_paint.is_some() {
            v.push("after_paint");
        }
        if self.before_display.is_some() {
            v.push("before_display");
        }
        v
    }
}

/// One decision of a hook, kept for the debug bridge.
#[derive(Clone, Debug, PartialEq)]
pub struct TraceEntry {
    /// Input time, in seconds.
    pub time: f64,
    /// The hook (`before_view`...).
    pub hook: &'static str,
    /// `allow`, `veto`, `redirect` or `run`.
    pub verdict: &'static str,
    /// What was proposed and what came of it.
    pub detail: String,
}

/// How many trace entries a state keeps.
pub const TRACE_KEPT: usize = 64;
