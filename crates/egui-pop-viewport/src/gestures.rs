//! The viewport's gestures, as data so an input profile can remap them.
//! The pointer's come from the pointer profile in force
//! (`egui_pop_core::pointer`, `docs/pointer-gestures.md`): what the
//! wheel, a scroll, a pinch, each click, double click and drag does with
//! each set of modifiers ([`ViewGestures`]); [`set_gestures`] sets them
//! outright. They follow the shared interaction grammar:
//!
//! - **Space held**: a drag pans, whatever is under the pointer (a handle
//!   included);
//! - **Escape** during a gesture puts things back as they were, with no
//!   trace (a `Cancel` signal);
//! - **double click** on the canvas: the profile's view action (fit by
//!   default, or leave the view); on a handle, that handle's reset;
//! - **the wheel** over the canvas zooms around the pointer or pans, as
//!   the profile says; over a handle it adjusts that handle's quantity;
//! - a primary drag on a handle moves it, whatever the profile.

use egui::{Context, Key, Modifiers, PointerButton};
use egui_pop_core::model::ids::CommandId;
use egui_pop_core::model::keymap::{Button, Pointer};
use egui_pop_core::model::pointer::{
    ImageDoubleClick, PointerMap, SCOPE_GRAPH, SCOPE_IMAGE, gesture,
};
use egui_pop_core::pointer::{self as pointer_map, MODIFIER_SETS, WheelInput};
use egui_pop_core::state::Slot;

/// What a pointer gesture does to a view (the image viewport's, the node
/// graph's).
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

/// A view's pointer gestures: per set of modifiers
/// ([`egui_pop_core::pointer::index`]), what the wheel, a scroll and a
/// pinch do, what a click and a double click of each button do, and
/// which buttons' drags pan.
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

fn slot(b: Button) -> usize {
    Button::ALL.iter().position(|x| *x == b).unwrap_or(0)
}

impl ViewGestures {
    /// A scope's bindings in a pointer map.
    pub fn from_map(map: &PointerMap, scope: &str) -> ViewGestures {
        let mut g = ViewGestures::default();
        for b in map.scope(scope) {
            let Some(p) = b.chord.pointer() else { continue };
            let i = pointer_map::chord_index(&b.chord);
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
        let i = pointer_map::index(input.modifiers);
        match input.pointer {
            Pointer::Wheel => self.wheel[i],
            Pointer::Scroll => self.scroll[i],
            Pointer::Pinch => self.pinch[i],
            _ => ViewAction::Nothing,
        }
    }

    /// Whether a drag of this button with these modifiers pans.
    pub fn drag_pans(&self, button: PointerButton, modifiers: Modifiers) -> bool {
        self.drag_pan[pointer_map::index(modifiers)][slot(pointer_map::button(button))]
    }

    /// What a click of this button with these modifiers does.
    pub fn click_action(&self, button: PointerButton, modifiers: Modifiers) -> ViewAction {
        self.click[pointer_map::index(modifiers)][slot(pointer_map::button(button))]
    }

    /// What a double click of this button with these modifiers does.
    pub fn double_click_action(&self, button: PointerButton, modifiers: Modifiers) -> ViewAction {
        self.double_click[pointer_map::index(modifiers)][slot(pointer_map::button(button))]
    }
}

/// The viewport's gestures.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Gestures {
    /// The button that drags a handle and the comparison's split, and
    /// clicks a handle.
    pub pan_button: PointerButton,
    /// Held: a drag pans anywhere, handles included.
    pub pan_key: Key,
    /// During a gesture: back to where it started, no trace.
    pub cancel: Key,
    /// The pointer's gestures on the canvas (the profile's
    /// `canvas.image` scope, a plain primary double click from its
    /// option when the scope binds none).
    pub pointer: ViewGestures,
    /// Keys when the viewport has the focus: the next ladder step up.
    pub zoom_in: &'static [Key],
    /// The next ladder step down.
    pub zoom_out: &'static [Key],
    /// Fit the image.
    pub fit: Key,
    /// 100 %.
    pub actual: Key,
    /// 200 %.
    pub double: Key,
    /// Held with the arrows: pan ten times further.
    pub coarse: Modifiers,
    /// Held: shows the "before" image (a quasimode).
    pub before: Key,
}

impl Gestures {
    /// What a pointer map says, the keys as by default.
    pub fn from_map(map: &PointerMap) -> Gestures {
        let mut pointer = ViewGestures::from_map(map, SCOPE_IMAGE);
        let primary = &mut pointer.double_click[0][slot(Button::Primary)];
        if *primary == ViewAction::Nothing {
            *primary = match map.options.image_double_click {
                ImageDoubleClick::Fit => ViewAction::Fit,
                ImageDoubleClick::Leave => ViewAction::Leave,
                ImageDoubleClick::Nothing => ViewAction::Nothing,
            };
        }
        Gestures {
            pan_button: PointerButton::Primary,
            pan_key: Key::Space,
            cancel: Key::Escape,
            pointer,
            zoom_in: &[Key::Plus, Key::Equals],
            zoom_out: &[Key::Minus],
            fit: Key::Num0,
            actual: Key::Num1,
            double: Key::Num2,
            coarse: Modifiers::SHIFT,
            before: Key::Backslash,
        }
    }
}

impl Default for Gestures {
    /// The default pointer profile's.
    fn default() -> Self {
        Gestures::from_map(&pointer_map::default_map())
    }
}

/// What the pointer does in a view's scope with the modifiers held now,
/// in words (`Wheel: zoom · MiddleDrag: pan`): a canvas's hint, empty
/// when nothing is bound with them.
pub fn hint(ctx: &Context, scope: &str) -> String {
    use crate::strings as s;
    use egui_pop_core::strings::tr;
    let modifiers = ctx.input(|i| i.modifiers);
    let map = pointer_map::map(ctx);
    pointer_map::hint(&map, scope, modifiers, |command| {
        let key = match ViewAction::of(command) {
            ViewAction::Nothing => return None,
            ViewAction::Zoom => &s::HINT_ZOOM,
            ViewAction::ZoomFree => &s::HINT_ZOOM_FREE,
            ViewAction::Pan => &s::HINT_PAN,
            ViewAction::PanX => &s::HINT_PAN_X,
            ViewAction::PanY => &s::HINT_PAN_Y,
            ViewAction::ZoomCycle => &s::HINT_ZOOM_CYCLE,
            ViewAction::Fit => &s::HINT_FIT,
            ViewAction::ZoomToggle => &s::HINT_ZOOM_TOGGLE,
            ViewAction::Leave => &s::HINT_LEAVE,
        };
        Some(tr(ctx, key))
    })
}

/// The node graph's pointer gestures in a context (its
/// `canvas.graph` scope).
pub fn graph_gestures(ctx: &Context) -> ViewGestures {
    ViewGestures::from_map(&pointer_map::map(ctx), SCOPE_GRAPH)
}

const GESTURES: Slot<Gestures> = Slot::new("viewport.gestures");

/// The viewports' gestures in a context: those set, else the pointer
/// profile's.
pub fn gestures(ctx: &Context) -> Gestures {
    GESTURES
        .get(ctx)
        .unwrap_or_else(|| Gestures::from_map(&pointer_map::map(ctx)))
}

/// Sets the viewports' gestures in a context, over the pointer
/// profile's.
pub fn set_gestures(ctx: &Context, gestures: Gestures) {
    GESTURES.set(ctx, gestures);
}
