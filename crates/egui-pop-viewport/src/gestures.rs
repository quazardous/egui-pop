//! The viewport's gestures, as data so an input profile can remap them.
//! The pointer's come from the pointer profile in force
//! (`egui_pop_core::pointer`, `docs/pointer-gestures.md`): what the
//! wheel, a scroll, a pinch, each click, double click and drag does with
//! each set of modifiers ([`ViewGestures`]); [`set_gestures`] sets them
//! outright. They follow the shared interaction grammar:
//!
//! <div class="warning">Experimental: may change in any release
//! (docs/stability.md).</div>
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
use egui_pop_core::model::keymap::Button;
use egui_pop_core::model::pointer::{ImageDoubleClick, PointerMap, SCOPE_GRAPH, SCOPE_IMAGE};
use egui_pop_core::pointer::view::slot;
use egui_pop_core::pointer::{self as pointer_map};
use egui_pop_core::state::Slot;

/// The views' gestures and their action, shared with every zoomable view
/// (`egui_pop_core::pointer::view`).
pub use egui_pop_core::pointer::view::{ViewAction, ViewGestures};

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
