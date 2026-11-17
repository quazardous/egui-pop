//! A view's pointer gestures (the image viewport's, the node graph's, the
//! library's comparison): what the profile in force makes the wheel, a
//! scroll, a pinch, each click, double click and drag do, and the one
//! wheel path every zoomable view reads ([`view_wheel`]), so the same
//! gesture does the same thing in each.
//!
//! <div class="warning">Experimental: may change in any release
//! (docs/stability.md).</div>

use egui::{Context, Modifiers, PointerButton, Vec2};

use super::{MODIFIER_SETS, WheelInput, button, chord_index, index, take_wheel, wheel_inputs};
use crate::model::ids::CommandId;
use crate::model::keymap::{Button, Pointer};
use crate::model::pointer::{PointerMap, gesture};

/// What a pointer gesture does to a view.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ViewAction {
    /// Nothing: the wheel goes to the panel around.
    #[default]
    Nothing,
    /// Zooms around the pointer, holding at the stops (fit, 100 %).
    Zoom,
    /// Zooms around the pointer without stops.
    ZoomFree,
    /// Pans by the wheel's motion, or following a drag.
    Pan,
    /// Pans horizontally by the wheel's motion.
    PanX,
    /// Pans vertically by the wheel's motion.
    PanY,
    /// 100 %, 200 %, fit, around the pointer.
    ZoomCycle,
    /// Fits the content.
    Fit,
    /// Fit, or 100 % around the pointer when fitting.
    ZoomToggle,
    /// Asks the application to leave the view (a signal; the view does
    /// not change).
    Leave,
}

impl ViewAction {
    /// The action of a gesture command, in either view's scope.
    pub fn of(command: &CommandId) -> ViewAction {
        let pairs = [
            (gesture::IMAGE_ZOOM, ViewAction::Zoom),
            (gesture::IMAGE_ZOOM_FREE, ViewAction::ZoomFree),
            (gesture::IMAGE_PAN, ViewAction::Pan),
            (gesture::IMAGE_PAN_X, ViewAction::PanX),
            (gesture::IMAGE_PAN_Y, ViewAction::PanY),
            (gesture::IMAGE_ZOOM_CYCLE, ViewAction::ZoomCycle),
            (gesture::IMAGE_FIT, ViewAction::Fit),
            (gesture::IMAGE_ZOOM_TOGGLE, ViewAction::ZoomToggle),
            (gesture::GRAPH_ZOOM, ViewAction::Zoom),
            (gesture::GRAPH_PAN, ViewAction::Pan),
            (gesture::GRAPH_PAN_X, ViewAction::PanX),
            (gesture::GRAPH_PAN_Y, ViewAction::PanY),
            (gesture::GRAPH_FIT, ViewAction::Fit),
        ];
        pairs
            .into_iter()
            .find(|(c, _)| c == command)
            .map_or(ViewAction::Nothing, |(_, a)| a)
    }

    /// A zoom of any kind.
    pub fn zooms(self) -> bool {
        matches!(self, ViewAction::Zoom | ViewAction::ZoomFree)
    }

    /// A pan by the wheel of any kind.
    pub fn pans(self) -> bool {
        matches!(self, ViewAction::Pan | ViewAction::PanX | ViewAction::PanY)
    }
}

/// A view's pointer gestures: per set of modifiers ([`index`]), what
/// the wheel, a scroll and a pinch do, what a click and a double click
/// of each button do, and which buttons' drags pan.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ViewGestures {
    pub wheel: [ViewAction; MODIFIER_SETS],
    pub scroll: [ViewAction; MODIFIER_SETS],
    pub pinch: [ViewAction; MODIFIER_SETS],
    /// Per set of modifiers, per button (`Button::ALL`'s order).
    pub click: [[ViewAction; 5]; MODIFIER_SETS],
    pub double_click: [[ViewAction; 5]; MODIFIER_SETS],
    pub drag_pan: [[bool; 5]; MODIFIER_SETS],
}

/// A button's place in [`ViewGestures`]' per-button tables.
pub fn slot(b: Button) -> usize {
    Button::ALL.iter().position(|x| *x == b).unwrap_or(0)
}

impl ViewGestures {
    /// A scope's bindings in a pointer map.
    pub fn from_map(map: &PointerMap, scope: &str) -> ViewGestures {
        let mut g = ViewGestures::default();
        for b in map.scope(scope) {
            let Some(p) = b.chord.pointer() else { continue };
            let i = chord_index(&b.chord);
            let action = ViewAction::of(&b.command);
            match p {
                Pointer::Wheel => g.wheel[i] = action,
                Pointer::Scroll => g.scroll[i] = action,
                Pointer::Pinch => g.pinch[i] = action,
                Pointer::Click(button) => g.click[i][slot(button)] = action,
                Pointer::DoubleClick(button) => g.double_click[i][slot(button)] = action,
                Pointer::Drag(button) => g.drag_pan[i][slot(button)] = action == ViewAction::Pan,
            }
        }
        g
    }

    /// What a wheel, scroll or pinch input does.
    pub fn wheel_action(&self, input: &WheelInput) -> ViewAction {
        let i = index(input.modifiers);
        match input.pointer {
            Pointer::Wheel => self.wheel[i],
            Pointer::Scroll => self.scroll[i],
            Pointer::Pinch => self.pinch[i],
            _ => ViewAction::Nothing,
        }
    }

    /// Whether a drag of this button with these modifiers pans.
    pub fn drag_pans(&self, b: PointerButton, modifiers: Modifiers) -> bool {
        self.drag_pan[index(modifiers)][slot(button(b))]
    }

    /// What a click of this button with these modifiers does.
    pub fn click_action(&self, b: PointerButton, modifiers: Modifiers) -> ViewAction {
        self.click[index(modifiers)][slot(button(b))]
    }

    /// What a double click of this button with these modifiers does.
    pub fn double_click_action(&self, b: PointerButton, modifiers: Modifiers) -> ViewAction {
        self.double_click[index(modifiers)][slot(button(b))]
    }
}

/// How fast the wheel zooms: the factor per point of scroll is
/// `exp(points × WHEEL_RATE)` ([`wheel_factor`]).
pub const WHEEL_RATE: f64 = 0.0015;

/// The zoom factor of a wheel motion of `points` (positive zooms in).
pub fn wheel_factor(points: f32) -> f64 {
    (f64::from(points) * WHEEL_RATE).exp()
}

/// One wheel, scroll or pinch input as a view's gestures read it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ViewWheel {
    /// Zoom around the pointer by [`wheel_factor`]`(points)`; `free`:
    /// without the view's stops (a free zoom, a pinch).
    Zoom {
        points: f32,
        free: bool,
        pinch: bool,
    },
    /// Pan by this motion, in points.
    Pan(Vec2),
}

/// This pass's wheel, scroll and pinch inputs as `gestures` say, in
/// order; the wheel is taken from the panel around (call it only when
/// the view is hovered). The one wheel path of the zoomable views.
pub fn view_wheel(ctx: &Context, gestures: &ViewGestures) -> Vec<ViewWheel> {
    let inputs = wheel_inputs(ctx);
    // The view takes the wheel: the panel around it does not scroll.
    take_wheel(ctx);
    inputs
        .iter()
        .filter_map(|input| wheel_step(gestures, input))
        .collect()
}

/// What one input does with `gestures` ([`view_wheel`]).
pub fn wheel_step(gestures: &ViewGestures, input: &WheelInput) -> Option<ViewWheel> {
    let action = gestures.wheel_action(input);
    let pinch = input.pointer == Pointer::Pinch;
    match action {
        ViewAction::Zoom | ViewAction::ZoomFree => Some(ViewWheel::Zoom {
            // A pinch's factor as the wheel's motion.
            points: if pinch {
                (f64::from(input.zoom).ln() / WHEEL_RATE) as f32
            } else {
                input.along_y()
            },
            free: action == ViewAction::ZoomFree || pinch,
            pinch,
        }),
        ViewAction::Pan => Some(ViewWheel::Pan(input.delta)),
        ViewAction::PanX => Some(ViewWheel::Pan(Vec2::new(input.along_x(), 0.0))),
        ViewAction::PanY => Some(ViewWheel::Pan(Vec2::new(0.0, input.along_y()))),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::pointer::{self, SCOPE_IMAGE};

    fn wheel(modifiers: Modifiers, y: f32) -> WheelInput {
        WheelInput {
            pointer: Pointer::Wheel,
            modifiers,
            delta: Vec2::new(0.0, y),
            zoom: 1.0,
        }
    }

    #[test]
    fn the_profile_says_what_the_wheel_does() {
        let darktable = PointerMap::from_profile(pointer::profile("darktable").unwrap());
        let g = ViewGestures::from_map(&darktable, SCOPE_IMAGE);
        assert_eq!(
            wheel_step(&g, &wheel(Modifiers::NONE, 50.0)),
            Some(ViewWheel::Zoom {
                points: 50.0,
                free: false,
                pinch: false
            })
        );
        let document = PointerMap::from_profile(pointer::default_profile());
        let g = ViewGestures::from_map(&document, SCOPE_IMAGE);
        assert_eq!(
            wheel_step(&g, &wheel(Modifiers::NONE, 50.0)),
            Some(ViewWheel::Pan(Vec2::new(0.0, 50.0)))
        );
        assert!(matches!(
            wheel_step(&g, &wheel(Modifiers::COMMAND, 50.0)),
            Some(ViewWheel::Zoom { .. })
        ));
    }
}
