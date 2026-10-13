//! What the image tools share: the change a tool proposes, the hook that
//! sees it before it is written, and the value's form in a step's
//! arguments.
//!
//! The histogram's drag, the curve editor, the colour wheels, the zoom
//! control and the navigator work alike, after the sliders
//! (`egui_pop_core::slider`): each change is a [`Change`] (`from`, `to`,
//! the [`Cause`], and whether the gesture is `done`), seen by the tool's
//! `on_change` hook before it is written. The hook answers a
//! [`Verdict`]: allow, veto, or redirect to another value. A drag's
//! frames come with `done: false`; its end comes once more, from the
//! value before the drag, with `done: true`, and a veto then puts the
//! value back. Escape during a drag puts it back with no trace.
//!
//! [`StepValue`] gives each tool's value a JSON form, so that an
//! application can run the finished changes as steps (ADR 0011; the
//! `egui-pop-imaging` crate does it).

use std::fmt;

use serde_json::{Value as Json, json};

pub use crate::hooks::Verdict;
use crate::view::{View, Zoom};

/// What made a tool's value change.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cause {
    /// The pointer dragged (a point, a puck, the histogram, the frame).
    Drag,
    /// A key, when the tool has the focus.
    Key,
    /// The wheel.
    Wheel,
    /// A value typed (the luminance slider's entry).
    Typed,
    /// A point added.
    Add,
    /// A point deleted.
    Delete,
    /// Back to the default.
    Reset,
    /// The value picked under the pointer in the image (targeted
    /// adjustment).
    Pick,
    /// A click that sets the value at once (the picker, the navigator).
    Click,
    /// A button or a menu of the tool (zoom in, fit).
    Command,
    /// An assistive technology's action (increment, decrement, set).
    Access,
}

impl Cause {
    /// Its name in step arguments (`drag`).
    pub fn name(self) -> &'static str {
        match self {
            Cause::Drag => "drag",
            Cause::Key => "key",
            Cause::Wheel => "wheel",
            Cause::Typed => "typed",
            Cause::Add => "add",
            Cause::Delete => "delete",
            Cause::Reset => "reset",
            Cause::Pick => "pick",
            Cause::Click => "click",
            Cause::Command => "command",
            Cause::Access => "access",
        }
    }

    /// The cause of a slider's change (the wheels' luminance).
    pub fn of_slider(c: egui_pop_core::slider::Cause) -> Cause {
        use egui_pop_core::slider::Cause as S;
        match c {
            S::Drag => Cause::Drag,
            S::Key => Cause::Key,
            S::Wheel => Cause::Wheel,
            S::Typed => Cause::Typed,
            S::Reset => Cause::Reset,
            S::Access => Cause::Access,
        }
    }
}

/// A change a tool proposes, or made.
#[derive(Clone, Debug, PartialEq)]
pub struct Change<T> {
    pub from: T,
    pub to: T,
    pub cause: Cause,
    /// The gesture is over: every change but a drag's frames. A drag's
    /// end comes as one done change from the value before it.
    pub done: bool,
}

/// What a hook is: it sees a change and answers a verdict.
type HookFn<'a, T> = dyn FnMut(&Change<T>) -> Verdict<T> + 'a;

/// A hook seeing each change of a tool before it is written.
pub(crate) struct Hook<'a, T>(pub(crate) Box<HookFn<'a, T>>);

impl<T> fmt::Debug for Hook<'_, T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Hook")
    }
}

impl<'a, T> Hook<'a, T> {
    pub(crate) fn new(hook: impl FnMut(&Change<T>) -> Verdict<T> + 'a) -> Hook<'a, T> {
        Hook(Box::new(hook))
    }
}

/// The change as the hook leaves it; `None` when vetoed.
pub(crate) fn decide<T: Clone>(
    hook: &mut Option<Hook<'_, T>>,
    change: Change<T>,
) -> Option<Change<T>> {
    match hook.as_mut().map_or(Verdict::Allow, |h| (h.0)(&change)) {
        Verdict::Allow => Some(change),
        Verdict::Veto => None,
        Verdict::Redirect(to) => Some(Change { to, ..change }),
    }
}

/// A tool's value in a step's arguments and result.
pub trait StepValue: Clone + PartialEq + Sized {
    /// The value as JSON.
    fn to_json(&self) -> Json;
    /// The value read back from JSON (a redirect, a run-hook's result);
    /// `None` when it is not one.
    fn from_json(v: &Json) -> Option<Self>;
}

/// A number of a JSON value as `f32`.
pub(crate) fn json_f32(v: &Json) -> Option<f32> {
    v.as_f64().map(|x| x as f32)
}

/// Curve points, `[[x, y], ...]`.
impl StepValue for Vec<[f32; 2]> {
    fn to_json(&self) -> Json {
        Json::Array(self.iter().map(|p| json!([p[0], p[1]])).collect())
    }

    fn from_json(v: &Json) -> Option<Self> {
        v.as_array()?
            .iter()
            .map(|p| Some([json_f32(p.get(0)?)?, json_f32(p.get(1)?)?]))
            .collect()
    }
}

/// A view, `{"zoom": "fit" | <physical pixels per image pixel>,
/// "center": [x, y]}`.
impl StepValue for View {
    fn to_json(&self) -> Json {
        let zoom = match self.zoom {
            Zoom::Fit => json!("fit"),
            Zoom::Px(z) => json!(z),
        };
        json!({ "zoom": zoom, "center": [self.center[0], self.center[1]] })
    }

    fn from_json(v: &Json) -> Option<Self> {
        let c = v.get("center")?;
        let center = [c.get(0)?.as_f64()?, c.get(1)?.as_f64()?];
        let zoom = match v.get("zoom")? {
            Json::String(s) if s == "fit" => Zoom::Fit,
            z => Zoom::Px(z.as_f64()?),
        };
        Some(View { zoom, center })
    }
}

/// How finely a gesture moves: the sliders' modifiers (Ctrl or Cmd ten
/// times finer, Shift ten times coarser; `egui_pop_core::slider::Gestures`).
pub(crate) fn scale(ui: &egui::Ui) -> f32 {
    let g = egui_pop_core::slider::gestures(ui.ctx());
    let mods = ui.input(|i| i.modifiers);
    let held = |m: egui::Modifiers| m.any() && mods.matches_logically(m);
    if held(g.fine) {
        0.1
    } else if held(g.coarse) {
        10.0
    } else {
        1.0
    }
}

/// A drag's effect in a frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Gesture {
    None,
    /// The pointer moved, in points, scaled by the modifiers.
    Moved(egui::Vec2),
    /// The drag ended.
    Ended,
    /// The cancel key ended it: put the value back, no trace.
    Cancelled,
}

/// What a drag on `response` did this frame (its start is
/// `response.drag_started()`).
pub(crate) fn gesture(ui: &egui::Ui, response: &egui::Response) -> Gesture {
    if response.drag_stopped() {
        // egui ends a drag on the cancel key.
        let cancel = egui_pop_core::slider::gestures(ui.ctx()).cancel;
        return if ui.input(|i| i.key_pressed(cancel)) {
            Gesture::Cancelled
        } else {
            Gesture::Ended
        };
    }
    if response.dragged() {
        let d = response.drag_delta() * scale(ui);
        if d != egui::Vec2::ZERO {
            return Gesture::Moved(d);
        }
    }
    Gesture::None
}

/// Keeps the arrow keys on a focused tool (egui would move the focus).
pub(crate) fn lock_arrows(ui: &egui::Ui, id: egui::Id) {
    ui.memory_mut(|m| {
        m.set_focus_lock_filter(
            id,
            egui::EventFilter {
                horizontal_arrows: true,
                vertical_arrows: true,
                escape: false,
                ..Default::default()
            },
        );
    });
}

/// The arrow keys pressed this frame on a focused tool: `[right - left,
/// up - down]`.
pub(crate) fn arrows(ui: &egui::Ui) -> [f32; 2] {
    use egui::Key;
    ui.input(|i| {
        let n = |k: Key| i.num_presses(k) as f32;
        [
            n(Key::ArrowRight) - n(Key::ArrowLeft),
            n(Key::ArrowUp) - n(Key::ArrowDown),
        ]
    })
}

/// The increment and decrement actions an assistive technology asked
/// of `id` this frame: increments minus decrements.
pub(crate) fn access_steps(ui: &egui::Ui, id: egui::Id) -> f32 {
    use egui::accesskit::Action;
    ui.input(|i| {
        i.num_accesskit_action_requests(id, Action::Increment) as f32
            - i.num_accesskit_action_requests(id, Action::Decrement) as f32
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn values_round_trip_through_json() {
        let pts = vec![[0.0f32, 0.0], [0.5, 0.25], [1.0, 1.0]];
        assert_eq!(Vec::<[f32; 2]>::from_json(&pts.to_json()), Some(pts));
        let v = View::at(0.5, [100.0, 200.0]);
        assert_eq!(View::from_json(&v.to_json()), Some(v));
        assert_eq!(View::from_json(&View::fit().to_json()), Some(View::fit()));
        assert_eq!(View::from_json(&json!(3)), None);
    }

    #[test]
    fn the_hook_allows_vetoes_or_redirects() {
        let c = Change {
            from: 1.0f32,
            to: 2.0,
            cause: Cause::Key,
            done: true,
        };
        assert_eq!(decide(&mut None, c.clone()).map(|c| c.to), Some(2.0));
        let mut veto = Some(Hook::new(|_: &Change<f32>| Verdict::Veto));
        assert_eq!(decide(&mut veto, c.clone()), None);
        let mut snap = Some(Hook::new(|c: &Change<f32>| {
            Verdict::Redirect(c.to.round() + 1.0)
        }));
        assert_eq!(decide(&mut snap, c).map(|c| c.to), Some(3.0));
    }
}
