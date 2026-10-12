//! The viewport's gestures, as data so an input profile can remap them
//! ([`set_gestures`]). They follow the shared interaction grammar:
//!
//! - **Space held**: a drag pans, whatever is under the pointer (a handle
//!   included);
//! - **Escape** during a gesture puts things back as they were, with no
//!   trace (a `Cancel` signal);
//! - **double click** on the canvas: the configured view action (fit by
//!   default); on a handle, that handle's reset;
//! - **the wheel** over the canvas zooms around the pointer (or pans, per
//!   profile); over a handle it adjusts that handle's quantity;
//! - a modifier never changes what a gesture adjusts, only how finely.

use egui::{Context, Id, Key, Modifiers, PointerButton};

/// What the wheel does over the canvas.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WheelAction {
    /// Zooms around the pointer.
    Zoom,
    /// Pans (and zooms with [`Gestures::wheel_other`] held).
    Pan,
}

/// What a double click on the canvas (not on a handle) does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DoubleClick {
    /// Nothing.
    Nothing,
    /// Fits the image in the view.
    Fit,
    /// Fit, or 100 % around the pointer when already fitting.
    FitOrActual,
}

/// The viewport's gestures.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Gestures {
    /// A drag with this button on the canvas (not on a handle) pans.
    pub pan_button: PointerButton,
    /// A drag with this button pans anywhere, handles included.
    pub pan_button_anywhere: PointerButton,
    /// Held: a drag pans anywhere, handles included.
    pub pan_key: Key,
    /// During a gesture: back to where it started, no trace.
    pub cancel: Key,
    /// A double click on the canvas.
    pub double_click: DoubleClick,
    /// The wheel over the canvas.
    pub wheel: WheelAction,
    /// Held: the wheel does the other action (pan when it zooms, zoom
    /// when it pans).
    pub wheel_other: Modifiers,
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

impl Default for Gestures {
    fn default() -> Self {
        Gestures {
            pan_button: PointerButton::Primary,
            pan_button_anywhere: PointerButton::Middle,
            pan_key: Key::Space,
            cancel: Key::Escape,
            double_click: DoubleClick::Fit,
            wheel: WheelAction::Zoom,
            wheel_other: Modifiers::SHIFT,
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

fn gestures_id() -> Id {
    Id::new("egui_pop_viewport::gestures")
}

/// The viewports' gestures in a context (the defaults until set).
pub fn gestures(ctx: &Context) -> Gestures {
    ctx.data(|d| d.get_temp(gestures_id())).unwrap_or_default()
}

/// Remaps the viewports' gestures in a context (an input profile).
pub fn set_gestures(ctx: &Context, gestures: Gestures) {
    ctx.data_mut(|d| d.insert_temp(gestures_id(), gestures));
}
