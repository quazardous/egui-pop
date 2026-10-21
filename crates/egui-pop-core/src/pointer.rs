//! Pointer gestures in a context: the table canvases read (a
//! [`PointerMap`]: what each pointer chord does per canvas kind, with
//! the options in force), and the wheel read raw.
//!
//! The application API publishes the map from the keymap and the
//! pointer profile ([`set_map`]); without it, canvases follow the
//! default profile (Document). egui's own scrolling follows the
//! profile's modifiers ([`apply_input_options`]).
//!
//! egui merges a pinch with Ctrl and the wheel, and turns units into
//! points before widgets see them; a profile needs both apart, so the
//! canvases read the raw events ([`wheel_inputs`]): a wheel in lines is
//! `Wheel`, a scroll in points `Scroll` (unless the wheel device setting
//! says otherwise), a pinch `Pinch`, each with its modifiers.
//!
//! ```
//! use egui_pop_core::pointer;
//! use egui_pop_model::keymap::Pointer;
//! use egui_pop_model::pointer::SCOPE_IMAGE;
//!
//! let ctx = egui::Context::default();
//! let map = pointer::map(&ctx);
//! assert_eq!(map.profile, "document");
//! let ctrl_wheel = pointer::chord(egui::Modifiers::COMMAND, Pointer::Wheel);
//! assert_eq!(map.command(SCOPE_IMAGE, &ctrl_wheel).unwrap().as_str(), "gesture.image.zoom");
//! ```

use std::sync::{Arc, LazyLock};

use egui::{Context, Event, Modifiers, MouseWheelUnit, PointerButton, Vec2};
use egui_pop_model::ids::CommandId;
use egui_pop_model::keymap::{Button, Chord, Pointer};
use egui_pop_model::pointer::{InputModifiers, PointerMap, WheelDevice};

use crate::state::Slot;

const MAP: Slot<Arc<PointerMap>> = Slot::new("core.pointer.map");

static DEFAULT: LazyLock<Arc<PointerMap>> = LazyLock::new(|| Arc::new(PointerMap::default()));

/// The pointer map in a context: the one published, else the default
/// profile's.
pub fn map(ctx: &Context) -> Arc<PointerMap> {
    MAP.get(ctx).unwrap_or_else(default_map)
}

/// The default profile's map, without a context.
pub fn default_map() -> Arc<PointerMap> {
    DEFAULT.clone()
}

/// Publishes a pointer map, and makes egui's scrolling follow its
/// modifiers.
pub fn set_map(ctx: &Context, map: PointerMap) {
    set_shared_map(ctx, Arc::new(map));
}

/// [`set_map`] with a map shared with its publisher, who can tell it
/// is still there ([`published`]).
pub fn set_shared_map(ctx: &Context, map: Arc<PointerMap>) {
    apply_input_options(ctx, &map.input);
    MAP.set(ctx, map);
}

/// The map published in a context, if one was.
pub fn published(ctx: &Context) -> Option<Arc<PointerMap>> {
    MAP.get(ctx)
}

/// egui's modifiers of a chord's: Ctrl is Cmd on macOS, as in the
/// keymap.
pub fn modifiers_of(chord: &Chord) -> Modifiers {
    Modifiers {
        alt: chord.alt,
        ctrl: false,
        shift: chord.shift,
        mac_cmd: false,
        command: chord.ctrl,
    }
}

/// The chord of a pointer gesture with egui's modifiers held.
pub fn chord(modifiers: Modifiers, pointer: Pointer) -> Chord {
    let mut c = Chord::pointer_gesture(pointer);
    c.ctrl = modifiers.ctrl || modifiers.command || modifiers.mac_cmd;
    c.alt = modifiers.alt;
    c.shift = modifiers.shift;
    c
}

/// The four modifiers as an index (Ctrl or Cmd 1, Alt 2, Shift 4): the
/// tables canvases keep per set of modifiers.
pub fn index(modifiers: Modifiers) -> usize {
    usize::from(modifiers.ctrl || modifiers.command || modifiers.mac_cmd)
        | usize::from(modifiers.alt) << 1
        | usize::from(modifiers.shift) << 2
}

/// A chord's modifiers as an [`index`].
pub fn chord_index(chord: &Chord) -> usize {
    usize::from(chord.ctrl) | usize::from(chord.alt) << 1 | usize::from(chord.shift) << 2
}

/// How many sets of modifiers an [`index`] tells apart.
pub const MODIFIER_SETS: usize = 8;

/// The keymap's button of egui's.
pub fn button(b: PointerButton) -> Button {
    match b {
        PointerButton::Primary => Button::Primary,
        PointerButton::Secondary => Button::Secondary,
        PointerButton::Middle => Button::Middle,
        PointerButton::Extra1 => Button::Back,
        PointerButton::Extra2 => Button::Forward,
    }
}

/// egui's button of the keymap's.
pub fn egui_button(b: Button) -> PointerButton {
    match b {
        Button::Primary => PointerButton::Primary,
        Button::Secondary => PointerButton::Secondary,
        Button::Middle => PointerButton::Middle,
        Button::Back => PointerButton::Extra1,
        Button::Forward => PointerButton::Extra2,
    }
}

/// One wheel, scroll or pinch event of this pass.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WheelInput {
    /// [`Pointer::Wheel`], [`Pointer::Scroll`] or [`Pointer::Pinch`].
    pub pointer: Pointer,
    pub modifiers: Modifiers,
    /// The motion in points, as egui gives it (positive: the content
    /// moves right and down); zero for a pinch.
    pub delta: Vec2,
    /// A pinch's zoom factor (above 1: in); 1 otherwise.
    pub zoom: f32,
}

impl WheelInput {
    /// Its chord.
    pub fn chord(&self) -> Chord {
        chord(self.modifiers, self.pointer)
    }

    /// The motion along one axis, the other's when it has none (a
    /// vertical wheel moving sideways, a sideways one moving up).
    pub fn along_y(&self) -> f32 {
        if self.delta.y != 0.0 {
            self.delta.y
        } else {
            self.delta.x
        }
    }

    /// See [`WheelInput::along_y`].
    pub fn along_x(&self) -> f32 {
        if self.delta.x != 0.0 {
            self.delta.x
        } else {
            self.delta.y
        }
    }
}

/// What the wheel device setting makes of a wheel event's unit.
pub fn wheel_pointer(unit: MouseWheelUnit, device: WheelDevice) -> Pointer {
    match (device, unit) {
        (WheelDevice::Mouse, _) => Pointer::Wheel,
        (WheelDevice::Trackpad, _) => Pointer::Scroll,
        (WheelDevice::Auto, MouseWheelUnit::Point) => Pointer::Scroll,
        (WheelDevice::Auto, MouseWheelUnit::Line | MouseWheelUnit::Page) => Pointer::Wheel,
    }
}

/// This pass's wheel, scroll and pinch events, raw: unit and modifiers
/// kept, egui's smoothing left out (a notch is one step).
pub fn wheel_inputs(ctx: &Context) -> Vec<WheelInput> {
    let device = map(ctx).wheel_device;
    let line = ctx.options(|o| o.input_options.line_scroll_speed);
    ctx.input(|i| {
        let page = i.content_rect().height();
        let mut out: Vec<WheelInput> = i
            .events
            .iter()
            .filter_map(|e| match e {
                Event::MouseWheel {
                    unit,
                    delta,
                    modifiers,
                    ..
                } => Some(WheelInput {
                    pointer: wheel_pointer(*unit, device),
                    modifiers: *modifiers,
                    delta: match unit {
                        MouseWheelUnit::Point => *delta,
                        MouseWheelUnit::Line => *delta * line,
                        MouseWheelUnit::Page => *delta * page,
                    },
                    zoom: 1.0,
                }),
                Event::Zoom(factor) if *factor != 1.0 => Some(WheelInput {
                    pointer: Pointer::Pinch,
                    modifiers: i.modifiers,
                    delta: Vec2::ZERO,
                    zoom: *factor,
                }),
                _ => None,
            })
            .collect();
        if let Some(touch) = i.multi_touch()
            && touch.zoom_delta != 1.0
        {
            out.push(WheelInput {
                pointer: Pointer::Pinch,
                modifiers: i.modifiers,
                delta: Vec2::ZERO,
                zoom: touch.zoom_delta,
            });
        }
        out
    })
}

/// Takes this pass's wheel from what comes after (a canvas used it):
/// the panel around does not scroll, egui's smoothing of earlier notches
/// included.
pub fn take_wheel(ctx: &Context) {
    ctx.input_mut(|i| {
        i.smooth_scroll_delta = Vec2::ZERO;
        i.events
            .retain(|e| !matches!(e, Event::MouseWheel { .. } | Event::Zoom(_)));
    });
}

/// egui's modifiers of a chord's modifiers as text (`Ctrl`, `Alt+Shift`;
/// empty or unreadable: none).
pub fn modifiers_from_text(text: &str) -> Modifiers {
    Chord::parse_modifiers(text)
        .map(|c| modifiers_of(&c))
        .unwrap_or(Modifiers::NONE)
}

/// Makes egui's scrolling (scroll areas, `zoom_delta`) follow a
/// profile's modifiers.
pub fn apply_input_options(ctx: &Context, input: &InputModifiers) {
    let zoom = modifiers_from_text(&input.zoom);
    let horizontal = modifiers_from_text(&input.horizontal_scroll);
    let vertical = modifiers_from_text(&input.vertical_scroll);
    let same = ctx.options(|o| {
        o.input_options.zoom_modifier == zoom
            && o.input_options.horizontal_scroll_modifier == horizontal
            && o.input_options.vertical_scroll_modifier == vertical
    });
    if !same {
        ctx.options_mut(|o| {
            o.input_options.zoom_modifier = zoom;
            o.input_options.horizontal_scroll_modifier = horizontal;
            o.input_options.vertical_scroll_modifier = vertical;
        });
    }
}

/// What the pointer does in a scope with these modifiers held, for a
/// canvas's hint: `Wheel: zoom · MiddleDrag: pan`, each gesture by its
/// name and its command by `word` (none: left out). Clicks and drags of
/// other buttons than these are left out too.
pub fn hint(
    map: &PointerMap,
    scope: &str,
    modifiers: Modifiers,
    word: impl Fn(&CommandId) -> Option<String>,
) -> String {
    let held = chord(modifiers, Pointer::Wheel);
    let mut parts: Vec<(Pointer, String)> = map
        .scope(scope)
        .filter(|b| b.chord.same_modifiers(&held))
        .filter_map(|b| {
            let p = b.chord.pointer()?;
            Some((p, format!("{}: {}", p.name(), word(&b.command)?)))
        })
        .collect();
    parts.sort_by_key(|(p, _)| *p);
    parts
        .into_iter()
        .map(|(_, text)| text)
        .collect::<Vec<_>>()
        .join(" · ")
}

/// For a web build's `eframe::WebOptions::should_prevent_default`: the
/// browser never gets an event egui-pop handles, the wheel above all
/// (Ctrl and the wheel would zoom the page, and a trackpad's pinch
/// arrives the same way), as eframe's default does for every event.
pub fn web_prevent_default(_event: &Event) -> bool {
    true
}
