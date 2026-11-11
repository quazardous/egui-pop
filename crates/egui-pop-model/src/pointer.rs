//! Pointer gesture profiles: what the wheel, a scroll, a pinch and the
//! buttons do on each kind of canvas, as named presets of keymap
//! bindings (`docs/pointer-gestures.md`).
//!
//! A **gesture** is a command like any other (`gesture.image.zoom`),
//! bound to a pointer chord (`Ctrl+Wheel`, [`crate::keymap::Pointer`])
//! in the scope of a canvas kind (`surface:canvas.image`). A
//! **profile** ([`Profile`], a `pointer-profile` document) is a set of
//! those bindings for the default layer, the options it implies (the
//! wheel over sliders, a double click on the image) and the modifiers
//! egui's own scrolling follows. Six ship with the library
//! ([`builtin`]): Document (the default), darktable, Photoshop,
//! Lightroom Classic, Blender / node editors and Trackpad, each
//! following the application it is named after. A retired id still
//! names its successor ([`ALIASES`]: `adobe` is `lightroom`). The
//! user's overrides are ordinary user-layer bindings on top. A profile
//! also says which cursor each kind of drag shows, hovered and dragged
//! ([`DragKind`], [`PointerMap::drag_cursor`]).
//!
//! A canvas reads the resolved table, a [`PointerMap`], built from the
//! keymap ([`PointerMap::from_keymap`]) or from a profile alone
//! ([`PointerMap::from_profile`]).
//!
//! ```
//! use egui_pop_model::keymap::Chord;
//! use egui_pop_model::pointer::{self, PointerMap, SCOPE_IMAGE};
//!
//! let darktable = pointer::profile("darktable").unwrap();
//! let map = PointerMap::from_profile(darktable);
//! let wheel = Chord::parse("Wheel").unwrap();
//! assert_eq!(map.command(SCOPE_IMAGE, &wheel).unwrap().as_str(), "gesture.image.zoom");
//! let document = PointerMap::from_profile(pointer::default_profile());
//! assert_eq!(document.command(SCOPE_IMAGE, &wheel).unwrap().as_str(), "gesture.image.pan");
//! ```

use std::collections::BTreeMap;
use std::sync::LazyLock;

use serde::{Deserialize, Serialize};

use crate::doc::{self, DocError, Schema};
use crate::ids::{CommandId, OwnerId};
use crate::keymap::{Chord, DocBinding, KeyScope, Keymap, Layer};

/// The image viewport's scope.
pub const SCOPE_IMAGE: &str = "canvas.image";
/// The node graph's scope.
pub const SCOPE_GRAPH: &str = "canvas.graph";
/// Sliders and other values' scope.
pub const SCOPE_VALUE: &str = "widget.value";
/// Stacked sections' scope.
pub const SCOPE_SECTIONS: &str = "widget.sections";
/// Every scope a profile binds, in the order the editor shows them.
pub const SCOPES: [&str; 4] = [SCOPE_IMAGE, SCOPE_GRAPH, SCOPE_VALUE, SCOPE_SECTIONS];

/// The profile a library or an application gets unless it picks one.
pub const DEFAULT_PROFILE: &str = "document";

/// The navigation commands every canvas takes (the node graph, the
/// image viewport, plots; `docs/design/navigation.md`), whose keys a
/// profile gives ([`Profile::view_keys`]).
pub const VIEW_COMMANDS: [&str; 10] = [
    "view.fit",
    "view.frame_selection",
    "view.zoom_in",
    "view.zoom_out",
    "view.zoom_100",
    "view.pan_left",
    "view.pan_right",
    "view.pan_up",
    "view.pan_down",
    "view.hand",
];

/// When a profile's navigation keys apply: a canvas is in use and no
/// file browser, text input or modal has the keyboard (a file browser's
/// `Ctrl+1` to `Ctrl+9` are its view modes, Photoshop's `Ctrl+1` is 100 %).
pub const VIEW_KEYS_WHEN: &str =
    "canvasActive && !fileBrowserFocused && !textInputFocus && !modalOpen";

/// Gesture commands, by canvas kind.
pub mod gesture {
    use crate::ids::CommandId;

    /// Zooms around the pointer, holding at fit and 100 %.
    pub const IMAGE_ZOOM: CommandId = CommandId::from_static("gesture.image.zoom");
    /// Zooms around the pointer without holding anywhere.
    pub const IMAGE_ZOOM_FREE: CommandId = CommandId::from_static("gesture.image.zoom_free");
    /// Pans: by the wheel's motion, or following a drag.
    pub const IMAGE_PAN: CommandId = CommandId::from_static("gesture.image.pan");
    /// Pans horizontally by the wheel's motion.
    pub const IMAGE_PAN_X: CommandId = CommandId::from_static("gesture.image.pan_x");
    /// Pans vertically by the wheel's motion.
    pub const IMAGE_PAN_Y: CommandId = CommandId::from_static("gesture.image.pan_y");
    /// 100 %, then 200 %, then fit, around the pointer.
    pub const IMAGE_ZOOM_CYCLE: CommandId = CommandId::from_static("gesture.image.zoom_cycle");
    /// Fits the image.
    pub const IMAGE_FIT: CommandId = CommandId::from_static("gesture.image.fit");
    /// Fit, or 100 % around the pointer when fitting.
    pub const IMAGE_ZOOM_TOGGLE: CommandId = CommandId::from_static("gesture.image.zoom_toggle");
    /// Zooms around the pointer.
    pub const GRAPH_ZOOM: CommandId = CommandId::from_static("gesture.graph.zoom");
    /// Pans: by the wheel's motion, or following a drag.
    pub const GRAPH_PAN: CommandId = CommandId::from_static("gesture.graph.pan");
    pub const GRAPH_PAN_X: CommandId = CommandId::from_static("gesture.graph.pan_x");
    pub const GRAPH_PAN_Y: CommandId = CommandId::from_static("gesture.graph.pan_y");
    /// Fits the graph.
    pub const GRAPH_FIT: CommandId = CommandId::from_static("gesture.graph.fit");
    /// The wheel steps the value under the pointer; its modifiers are
    /// the ones that make the wheel step a slider without the focus.
    pub const VALUE_STEP: CommandId = CommandId::from_static("gesture.value.step");
    /// Its modifiers make drags, arrows and the wheel ten times finer.
    pub const VALUE_FINE: CommandId = CommandId::from_static("gesture.value.fine");
    /// Its modifiers make them ten times coarser.
    pub const VALUE_COARSE: CommandId = CommandId::from_static("gesture.value.coarse");
    /// The wheel scrolls the panel, even over a value.
    pub const VALUE_PANEL: CommandId = CommandId::from_static("gesture.value.panel");
    /// A primary click with its modifiers puts a value back to its
    /// default (`Ctrl+PrimaryClick`, as in Reason or Kontakt). No shipped
    /// profile binds it: a double click resets in all of them.
    pub const VALUE_RESET: CommandId = CommandId::from_static("gesture.value.reset");
    /// A drag on a boundary or a title moves the sections below as a block.
    pub const SECTIONS_PULL: CommandId = CommandId::from_static("gesture.sections.pull");
}

/// A gesture command as declared: its id, its scope, its title and what
/// it does.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GestureSpec {
    pub id: CommandId,
    pub scope: &'static str,
    pub title: &'static str,
    pub description: &'static str,
}

/// Every gesture command.
pub const GESTURES: &[GestureSpec] = &[
    GestureSpec {
        id: gesture::IMAGE_ZOOM,
        scope: SCOPE_IMAGE,
        title: "Image: zoom at the pointer",
        description: "Zooms the image around the pointer, holding at fit and 100 %.",
    },
    GestureSpec {
        id: gesture::IMAGE_ZOOM_FREE,
        scope: SCOPE_IMAGE,
        title: "Image: zoom without stops",
        description: "Zooms the image around the pointer, holding nowhere.",
    },
    GestureSpec {
        id: gesture::IMAGE_PAN,
        scope: SCOPE_IMAGE,
        title: "Image: pan",
        description: "Moves the image with the wheel or a drag.",
    },
    GestureSpec {
        id: gesture::IMAGE_PAN_X,
        scope: SCOPE_IMAGE,
        title: "Image: pan horizontally",
        description: "Moves the image sideways with the wheel.",
    },
    GestureSpec {
        id: gesture::IMAGE_PAN_Y,
        scope: SCOPE_IMAGE,
        title: "Image: pan vertically",
        description: "Moves the image up and down with the wheel.",
    },
    GestureSpec {
        id: gesture::IMAGE_ZOOM_CYCLE,
        scope: SCOPE_IMAGE,
        title: "Image: zoom cycle",
        description: "Goes to 100 %, then 200 %, then fit, around the pointer.",
    },
    GestureSpec {
        id: gesture::IMAGE_FIT,
        scope: SCOPE_IMAGE,
        title: "Image: fit",
        description: "Fits the whole image in the view.",
    },
    GestureSpec {
        id: gesture::IMAGE_ZOOM_TOGGLE,
        scope: SCOPE_IMAGE,
        title: "Image: fit or 100 %",
        description: "Goes to 100 % around the pointer when the image fits, else fits it.",
    },
    GestureSpec {
        id: gesture::GRAPH_ZOOM,
        scope: SCOPE_GRAPH,
        title: "Node graph: zoom at the pointer",
        description: "Zooms the graph around the pointer.",
    },
    GestureSpec {
        id: gesture::GRAPH_PAN,
        scope: SCOPE_GRAPH,
        title: "Node graph: pan",
        description: "Moves the graph with the wheel or a drag.",
    },
    GestureSpec {
        id: gesture::GRAPH_PAN_X,
        scope: SCOPE_GRAPH,
        title: "Node graph: pan horizontally",
        description: "Moves the graph sideways with the wheel.",
    },
    GestureSpec {
        id: gesture::GRAPH_PAN_Y,
        scope: SCOPE_GRAPH,
        title: "Node graph: pan vertically",
        description: "Moves the graph up and down with the wheel.",
    },
    GestureSpec {
        id: gesture::GRAPH_FIT,
        scope: SCOPE_GRAPH,
        title: "Node graph: fit",
        description: "Fits the whole graph in the view.",
    },
    GestureSpec {
        id: gesture::VALUE_STEP,
        scope: SCOPE_VALUE,
        title: "Values: step with the wheel",
        description: "The wheel with these keys steps the slider under the pointer, focused or not.",
    },
    GestureSpec {
        id: gesture::VALUE_FINE,
        scope: SCOPE_VALUE,
        title: "Values: finer",
        description: "These keys make a slider's drag, arrows and wheel ten times finer.",
    },
    GestureSpec {
        id: gesture::VALUE_COARSE,
        scope: SCOPE_VALUE,
        title: "Values: coarser",
        description: "These keys make a slider's drag, arrows and wheel ten times coarser.",
    },
    GestureSpec {
        id: gesture::VALUE_PANEL,
        scope: SCOPE_VALUE,
        title: "Values: scroll the panel",
        description: "The wheel with these keys scrolls the panel, even over a slider.",
    },
    GestureSpec {
        id: gesture::VALUE_RESET,
        scope: SCOPE_VALUE,
        title: "Values: reset with a click",
        description: "A click with these keys puts a slider or a knob back to its default.",
    },
    GestureSpec {
        id: gesture::SECTIONS_PULL,
        scope: SCOPE_SECTIONS,
        title: "Sections: pull as a block",
        description: "A drag on a boundary or a title with these keys moves the sections below as a block, keeping their heights.",
    },
];

/// The gesture command of an id, when it is one.
pub fn gesture_spec(id: &CommandId) -> Option<&'static GestureSpec> {
    GESTURES.iter().find(|g| g.id == *id)
}

/// What the wheel does over a slider without the focus.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SliderWheel {
    /// It scrolls the panel; it steps a slider only when the slider has
    /// the focus or the step modifiers are held (`gesture.value.step`).
    #[default]
    Guarded,
    /// It steps the slider under the pointer (darktable); the panel
    /// scrolls with `gesture.value.panel`'s modifiers.
    Value,
}

impl SliderWheel {
    pub const CHOICES: [&'static str; 2] = ["guarded", "value"];

    pub fn as_str(self) -> &'static str {
        match self {
            SliderWheel::Guarded => "guarded",
            SliderWheel::Value => "value",
        }
    }

    pub fn parse(text: &str) -> Option<SliderWheel> {
        match text {
            "guarded" => Some(SliderWheel::Guarded),
            "value" => Some(SliderWheel::Value),
            _ => None,
        }
    }
}

/// How a drag turns a knob. The same travel, in points, covers the soft
/// range in every mode; where the press lands never changes the mode.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum KnobDrag {
    /// Up is more (the audio convention).
    #[default]
    Vertical,
    /// Right is more.
    Horizontal,
    /// Right and up both add.
    Both,
    /// Around the dial's centre, relative to the press: the value never
    /// jumps to where the pointer is.
    Circular,
}

impl KnobDrag {
    pub const CHOICES: [&'static str; 4] = ["vertical", "horizontal", "both", "circular"];

    pub fn as_str(self) -> &'static str {
        match self {
            KnobDrag::Vertical => "vertical",
            KnobDrag::Horizontal => "horizontal",
            KnobDrag::Both => "both",
            KnobDrag::Circular => "circular",
        }
    }

    pub fn parse(text: &str) -> Option<KnobDrag> {
        match text {
            "vertical" => Some(KnobDrag::Vertical),
            "horizontal" => Some(KnobDrag::Horizontal),
            "both" => Some(KnobDrag::Both),
            "circular" => Some(KnobDrag::Circular),
            _ => None,
        }
    }
}

/// What a primary double click on the image (not on a handle) does.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ImageDoubleClick {
    /// Fits the image.
    #[default]
    Fit,
    /// Asks the application to leave the view (darktable goes back to
    /// the lighttable); the viewport signals it and changes nothing.
    Leave,
    /// Nothing.
    Nothing,
}

impl ImageDoubleClick {
    pub const CHOICES: [&'static str; 3] = ["fit", "leave", "nothing"];

    pub fn as_str(self) -> &'static str {
        match self {
            ImageDoubleClick::Fit => "fit",
            ImageDoubleClick::Leave => "leave",
            ImageDoubleClick::Nothing => "nothing",
        }
    }

    pub fn parse(text: &str) -> Option<ImageDoubleClick> {
        match text {
            "fit" => Some(ImageDoubleClick::Fit),
            "leave" => Some(ImageDoubleClick::Leave),
            "nothing" => Some(ImageDoubleClick::Nothing),
            _ => None,
        }
    }
}

/// What the wheel events come from. egui does not say which device sent
/// a wheel event; its unit is the cheapest hint (a mouse wheel sends
/// lines, a trackpad points), not a reliable one (some mice and browsers
/// send points).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum WheelDevice {
    /// Lines are a wheel (`Wheel`), points a trackpad (`Scroll`).
    #[default]
    Auto,
    /// Every wheel event is a mouse wheel.
    Mouse,
    /// Every wheel event is a trackpad's scroll.
    Trackpad,
}

impl WheelDevice {
    pub const CHOICES: [&'static str; 3] = ["auto", "mouse", "trackpad"];

    pub fn as_str(self) -> &'static str {
        match self {
            WheelDevice::Auto => "auto",
            WheelDevice::Mouse => "mouse",
            WheelDevice::Trackpad => "trackpad",
        }
    }

    pub fn parse(text: &str) -> Option<WheelDevice> {
        match text {
            "auto" => Some(WheelDevice::Auto),
            "mouse" => Some(WheelDevice::Mouse),
            "trackpad" => Some(WheelDevice::Trackpad),
            _ => None,
        }
    }
}

/// The options a profile gives its default for (each also a setting).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(default)]
pub struct Options {
    pub slider_wheel: SliderWheel,
    pub image_double_click: ImageDoubleClick,
    /// How a drag turns a knob, unless the knob says
    /// (`Knob::drag`).
    pub knob_drag: KnobDrag,
}

/// The modifiers egui's own scrolling follows (`egui::InputOptions`),
/// as chords' modifiers (`Ctrl`, `Shift`, `Alt+Shift`; empty: none):
/// plain egui widgets (scroll areas) agree with the profile.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(default)]
pub struct InputModifiers {
    /// Held: the wheel zooms instead of scrolling.
    pub zoom: String,
    /// Held: the wheel scrolls horizontally.
    pub horizontal_scroll: String,
    /// Held: the wheel scrolls vertically only.
    pub vertical_scroll: String,
}

impl Default for InputModifiers {
    /// egui's.
    fn default() -> Self {
        InputModifiers {
            zoom: "Ctrl".into(),
            horizontal_scroll: "Shift".into(),
            vertical_scroll: "Alt".into(),
        }
    }
}

/// The modifiers that scale a value's arrow-key steps, when they differ
/// from its drag's (`gesture.value.fine` and `gesture.value.coarse`),
/// as chords' modifiers (`Alt`, `Shift`; empty: the drag's). Lightroom
/// Classic, for one, makes a Shift drag finer but Shift with the arrows
/// coarser. Profiles bind pointer gestures only: this is data the
/// slider reads, not a keymap binding.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(default)]
pub struct ValueKeys {
    /// Held with the arrows: ten times finer.
    pub fine: String,
    /// Held with the arrows: ten times coarser.
    pub coarse: String,
}

/// What a drag does to what it holds: the meaning a drag's cursor is
/// keyed by, whatever the widget. A widget names the kind of each of its
/// drags; the profile in force says which cursor each kind shows,
/// hovered and dragged, and when the hover cursor shows
/// ([`DragKind::cursors`], [`Profile::drag_cursors`], [`HoverPolicy`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
#[non_exhaustive]
pub enum DragKind {
    /// Content panned under a fixed frame: an image, a graph's canvas,
    /// a timeline's lanes. An open hand, closed while dragging.
    Pan,
    /// Content scrubbed under a fixed playhead (a deck's waveform). An
    /// open hand, closed while dragging.
    Scrub,
    /// An item moved where the pointer takes it: a node, a clip, a band,
    /// a navigator's view frame. Move (a four-way arrow).
    Move,
    /// An item carried to a place to drop it: a row, a tab, a wire's
    /// end. The arrow until it moves, then a closed hand.
    Carry,
    /// A value dragged along a line: a slider, a playhead on a ruler.
    /// The arrow until it moves, then a horizontal resize.
    Value,
    /// A point placed under the pointer: a colour wheel's puck, a
    /// curve's point. The arrow until it moves, then a crosshair.
    Point,
    /// A rubber band selecting what it covers. The arrow until it moves,
    /// then a crosshair.
    Select,
    /// Text selected by dragging over it. An I-beam.
    Text,
    /// An edge or a separator resized sideways.
    ResizeHorizontal,
    /// An edge or a separator resized up and down.
    ResizeVertical,
    /// A corner resized along the top-left to bottom-right diagonal.
    ResizeNwSe,
    /// A corner resized along the top-right to bottom-left diagonal.
    ResizeNeSw,
    /// A separator between panes side by side, dragged sideways: a
    /// splitter, a side area's or a flyout's edge. A column resize.
    SplitColumns,
    /// A separator between panes one above the other, dragged up and
    /// down. A row resize.
    SplitRows,
}

impl DragKind {
    /// Every kind, in the order the docs list them.
    pub const ALL: [DragKind; 14] = [
        DragKind::Pan,
        DragKind::Scrub,
        DragKind::Move,
        DragKind::Carry,
        DragKind::Value,
        DragKind::Point,
        DragKind::Select,
        DragKind::Text,
        DragKind::ResizeHorizontal,
        DragKind::ResizeVertical,
        DragKind::ResizeNwSe,
        DragKind::ResizeNeSw,
        DragKind::SplitColumns,
        DragKind::SplitRows,
    ];

    /// Its name in a profile document: `pan`, `resize-horizontal`.
    pub fn as_str(self) -> &'static str {
        match self {
            DragKind::Pan => "pan",
            DragKind::Scrub => "scrub",
            DragKind::Move => "move",
            DragKind::Carry => "carry",
            DragKind::Value => "value",
            DragKind::Point => "point",
            DragKind::Select => "select",
            DragKind::Text => "text",
            DragKind::ResizeHorizontal => "resize-horizontal",
            DragKind::ResizeVertical => "resize-vertical",
            DragKind::ResizeNwSe => "resize-nw-se",
            DragKind::ResizeNeSw => "resize-ne-sw",
            DragKind::SplitColumns => "split-columns",
            DragKind::SplitRows => "split-rows",
        }
    }

    pub fn parse(text: &str) -> Option<DragKind> {
        DragKind::ALL.into_iter().find(|k| k.as_str() == text)
    }

    /// The cursors it shows unless a kit or a profile says otherwise, as
    /// CSS cursor names ([`CURSOR_NAMES`]), with its hover policy:
    /// [`HoverPolicy::Always`] for text (an I-beam over text, which a
    /// click also places the caret in), [`HoverPolicy::OnlyAction`] for
    /// the others.
    pub fn cursors(self) -> DragCursor {
        let (hover, drag) = match self {
            DragKind::Pan | DragKind::Scrub => ("grab", "grabbing"),
            DragKind::Move => ("move", "move"),
            DragKind::Carry => ("default", "grabbing"),
            DragKind::Value => ("default", "ew-resize"),
            DragKind::Point | DragKind::Select => ("default", "crosshair"),
            DragKind::Text => ("text", "text"),
            DragKind::ResizeHorizontal => ("ew-resize", "ew-resize"),
            DragKind::ResizeVertical => ("ns-resize", "ns-resize"),
            DragKind::ResizeNwSe => ("nwse-resize", "nwse-resize"),
            DragKind::ResizeNeSw => ("nesw-resize", "nesw-resize"),
            DragKind::SplitColumns => ("col-resize", "col-resize"),
            DragKind::SplitRows => ("row-resize", "row-resize"),
        };
        let policy = match self {
            DragKind::Text => HoverPolicy::Always,
            _ => HoverPolicy::OnlyAction,
        };
        DragCursor {
            hover: hover.into(),
            drag: drag.into(),
            policy: Some(policy),
        }
    }
}

/// When a draggable shows its hover cursor, before any drag. While it is
/// dragged it always shows its drag cursor.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
#[non_exhaustive]
pub enum HoverPolicy {
    /// Only when dragging is the element's only action: nothing happens
    /// on a click or a double click on it (a splitter, a resize grip, an
    /// item's handle). Where a click does something (select, set a
    /// value, move the playhead), the arrow stays until the drag starts.
    /// The default.
    #[default]
    OnlyAction,
    /// Whenever it is hovered, whatever a click does.
    Always,
    /// Never: the arrow until the drag starts.
    Never,
}

impl HoverPolicy {
    /// Every policy.
    pub const ALL: [HoverPolicy; 3] = [
        HoverPolicy::OnlyAction,
        HoverPolicy::Always,
        HoverPolicy::Never,
    ];

    /// Its name in a profile or a kit: `only-action`, `always`, `never`.
    pub fn as_str(self) -> &'static str {
        match self {
            HoverPolicy::OnlyAction => "only-action",
            HoverPolicy::Always => "always",
            HoverPolicy::Never => "never",
        }
    }

    pub fn parse(text: &str) -> Option<HoverPolicy> {
        HoverPolicy::ALL.into_iter().find(|p| p.as_str() == text)
    }

    /// Whether the hover cursor shows over an element that also does
    /// something on a click or a double click (`clicks`), or not.
    pub fn shows(self, clicks: bool) -> bool {
        match self {
            HoverPolicy::OnlyAction => !clicks,
            HoverPolicy::Always => true,
            HoverPolicy::Never => false,
        }
    }
}

/// The cursor names a profile may give: CSS's (`grab`, `ew-resize`),
/// which egui shows on every platform.
pub const CURSOR_NAMES: [&str; 35] = [
    "alias",
    "all-scroll",
    "cell",
    "context-menu",
    "copy",
    "crosshair",
    "default",
    "grab",
    "grabbing",
    "help",
    "move",
    "no-drop",
    "none",
    "not-allowed",
    "pointer",
    "progress",
    "ew-resize",
    "nesw-resize",
    "nwse-resize",
    "ns-resize",
    "e-resize",
    "se-resize",
    "s-resize",
    "sw-resize",
    "w-resize",
    "nw-resize",
    "n-resize",
    "ne-resize",
    "col-resize",
    "row-resize",
    "text",
    "vertical-text",
    "wait",
    "zoom-in",
    "zoom-out",
];

/// The cursors of a drag kind: hovered, and while dragging (kept until
/// the drag ends, wherever the pointer goes), and when the hover one
/// shows. Names from [`CURSOR_NAMES`]; in a kit or a profile, an empty
/// or unknown name and no policy keep the level below's
/// ([`DragCursor::over`]).
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(default)]
pub struct DragCursor {
    /// Over what can be dragged, nothing being dragged; `default` asks
    /// for none (the arrow, or what another widget asks for).
    pub hover: String,
    /// While dragging.
    pub drag: String,
    /// When the hover cursor shows ([`HoverPolicy`]).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub policy: Option<HoverPolicy>,
}

impl DragCursor {
    /// `self` with what `over` gives on top: each of its names that is
    /// one of [`CURSOR_NAMES`], its policy if it has one. How each level
    /// (the kind's own, the kit's, the profile's) overrides the one
    /// below.
    #[must_use]
    pub fn over(mut self, over: &DragCursor) -> DragCursor {
        let known = |n: &String| CURSOR_NAMES.contains(&n.as_str());
        if known(&over.hover) {
            self.hover.clone_from(&over.hover);
        }
        if known(&over.drag) {
            self.drag.clone_from(&over.drag);
        }
        if over.policy.is_some() {
            self.policy = over.policy;
        }
        self
    }

    /// Its policy, the default one when it has none.
    pub fn hover_policy(&self) -> HoverPolicy {
        self.policy.unwrap_or_default()
    }
}

/// A pointer profile, as shipped or saved: the `pointer-profile`
/// document.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Profile {
    /// Stable, never translated: `darktable`.
    pub id: String,
    pub title: String,
    /// What it does, in one sentence.
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub options: Options,
    #[serde(default)]
    pub input: InputModifiers,
    #[serde(default)]
    pub value_keys: ValueKeys,
    /// The drag cursors it changes, by kind (`{"move": {"hover":
    /// "default"}}`, `{"pan": {"policy": "always"}}`); the others keep
    /// the kit's ([`DragKind::cursors`] by default).
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub drag_cursors: BTreeMap<DragKind, DragCursor>,
    /// Default-layer bindings, as in a keymap document.
    #[serde(default)]
    pub bindings: Vec<DocBinding>,
    /// The navigation commands' keys ([`VIEW_COMMANDS`]), by command
    /// (`{"view.fit": ["Home", "Shift+1"]}`), after the application the
    /// profile is named after; default-layer bindings like the others,
    /// active while a canvas is in use ([`VIEW_KEYS_WHEN`]).
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub view_keys: BTreeMap<String, Vec<String>>,
}

/// The `pointer-profile` document's schema.
pub fn schema() -> Schema {
    Schema::new("pointer-profile", 1).migration(0, doc::same)
}

/// Reads a profile document.
pub fn parse_profile(text: &str) -> Result<Profile, DocError> {
    schema().load::<Profile>(text).map(|l| l.value)
}

const SHIPPED: [&str; 6] = [
    include_str!("../profiles/document.json"),
    include_str!("../profiles/darktable.json"),
    include_str!("../profiles/photoshop.json"),
    include_str!("../profiles/lightroom.json"),
    include_str!("../profiles/blender.json"),
    include_str!("../profiles/trackpad.json"),
];

static BUILTIN: LazyLock<Vec<Profile>> = LazyLock::new(|| {
    SHIPPED
        .iter()
        .map(|text| parse_profile(text).expect("a shipped profile parses"))
        .collect()
});

/// The profiles shipped with the library, the default first.
pub fn builtin() -> &'static [Profile] {
    &BUILTIN
}

/// Retired profile ids and the shipped profile each now names: `adobe`
/// was split into `photoshop` and `lightroom` (2026-10-22) and is read
/// as `lightroom`, the closer of the two for a photo editor. Aliases are
/// accepted wherever an id is, and listed nowhere ([`ids`]).
pub const ALIASES: &[(&str, &str)] = &[("adobe", "lightroom")];

/// The id a profile id stands for: itself, or the profile an alias
/// names.
pub fn canonical_id(id: &str) -> &str {
    ALIASES
        .iter()
        .find(|(alias, _)| *alias == id)
        .map_or(id, |(_, to)| to)
}

/// A shipped profile by id (an alias gives the profile it names).
pub fn profile(id: &str) -> Option<&'static Profile> {
    let id = canonical_id(id);
    builtin().iter().find(|p| p.id == id)
}

/// The `settings` document's migration from version 1: a saved profile
/// id that is now an alias becomes the profile it names.
pub(crate) fn settings_profile_aliases(
    mut json: serde_json::Value,
) -> Result<serde_json::Value, String> {
    if let Some(v) = json
        .get_mut("values")
        .and_then(|values| values.get_mut("settings/input.pointer_profile"))
        && let Some(id) = v.as_str()
    {
        let to = canonical_id(id);
        if to != id {
            *v = serde_json::Value::String(to.into());
        }
    }
    Ok(json)
}

/// The default profile ([`DEFAULT_PROFILE`]).
pub fn default_profile() -> &'static Profile {
    profile(DEFAULT_PROFILE).expect("the default profile ships")
}

/// The ids of the shipped profiles, the default first.
pub fn ids() -> Vec<&'static str> {
    builtin().iter().map(|p| p.id.as_str()).collect()
}

/// Who owns a profile's bindings in the keymap's default layer.
pub const PROFILE_OWNER: OwnerId = OwnerId::from_static("pointer-profile");

/// One resolved pointer binding.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct PointerBinding {
    /// The canvas kind (`canvas.image`), without `surface:`.
    pub scope: String,
    pub chord: Chord,
    pub command: CommandId,
    /// Where it comes from (the user's override, the profile).
    pub layer: Layer,
}

/// What every pointer gesture does, per canvas kind, with the options in
/// force: what canvases read.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct PointerMap {
    /// The profile in use.
    pub profile: String,
    pub options: Options,
    pub input: InputModifiers,
    /// The modifiers of a value's arrow-key steps, when not its drag's.
    pub value_keys: ValueKeys,
    pub wheel_device: WheelDevice,
    /// What a wheel's lines are multiplied by (`input.wheel_speed`); 1: egui's own
    /// points a line.
    pub wheel_speed: f32,
    pub bindings: Vec<PointerBinding>,
    /// The profile's drag cursors ([`Profile::drag_cursors`]); read them
    /// with [`PointerMap::drag_cursor`].
    pub drag_cursors: BTreeMap<DragKind, DragCursor>,
    /// The navigation commands' keys in force ([`VIEW_COMMANDS`]), by
    /// command: the profile's, or the keymap's with the user's changes.
    pub view_keys: BTreeMap<String, Vec<Chord>>,
    /// The application's keymap dispatches the navigation keys (and
    /// sends the commands to the canvas in use): a canvas does not read
    /// them itself. Off for a map made from a profile alone.
    pub keys_dispatched: bool,
}

impl Default for PointerMap {
    /// The default profile's.
    fn default() -> Self {
        PointerMap::from_profile(default_profile())
    }
}

impl PointerMap {
    /// A profile's bindings alone (no user override), with its options.
    pub fn from_profile(profile: &Profile) -> PointerMap {
        let mut k = Keymap::new();
        let _ = k.load(&PROFILE_OWNER, Layer::Default, &profile_doc(profile));
        PointerMap::from_keymap(&k, profile)
    }

    /// The keymap's pointer bindings, the profile's options.
    pub fn from_keymap(keymap: &Keymap, profile: &Profile) -> PointerMap {
        let bindings = keymap
            .pointer_bindings()
            .into_iter()
            .filter_map(|b| {
                let KeyScope::Surface(scope) = &b.options.scope else {
                    return None;
                };
                let crate::keymap::Action::Run(command) = &b.action else {
                    return None;
                };
                Some(PointerBinding {
                    scope: scope.clone(),
                    chord: b.keys.0[0].clone(),
                    command: command.clone(),
                    layer: b.layer,
                })
            })
            .collect();
        let view_keys = VIEW_COMMANDS
            .iter()
            .map(|id| {
                let chords = CommandId::new(*id)
                    .map(|c| keymap.keys_for(&c))
                    .unwrap_or_default()
                    .into_iter()
                    .filter(|k| k.0.len() == 1 && !k.0[0].is_pointer())
                    .map(|k| k.0[0].clone())
                    .collect();
                ((*id).to_string(), chords)
            })
            .collect();
        PointerMap {
            profile: profile.id.clone(),
            options: profile.options,
            input: profile.input.clone(),
            value_keys: profile.value_keys.clone(),
            wheel_device: WheelDevice::Auto,
            wheel_speed: 1.0,
            bindings,
            drag_cursors: profile.drag_cursors.clone(),
            view_keys,
            keys_dispatched: false,
        }
    }

    /// The keys of a navigation command (`view.fit`) in force.
    pub fn view_keys(&self, command: &str) -> &[Chord] {
        self.view_keys.get(command).map_or(&[], Vec::as_slice)
    }

    /// The cursors a drag kind shows: the profile's, each name it leaves
    /// empty or does not know, and its policy when it gives none, the
    /// kind's own ([`DragKind::cursors`]).
    pub fn drag_cursor(&self, kind: DragKind) -> DragCursor {
        self.drag_cursor_over(kind, kind.cursors())
    }

    /// The profile's cursors of a drag kind over `below` (a kit's):
    /// what the profile gives wins, the rest is `below`'s
    /// ([`DragCursor::over`]).
    pub fn drag_cursor_over(&self, kind: DragKind, below: DragCursor) -> DragCursor {
        match self.drag_cursors.get(&kind) {
            Some(o) => below.over(o),
            None => below,
        }
    }

    /// The command a pointer chord runs in a scope.
    pub fn command(&self, scope: &str, chord: &Chord) -> Option<&CommandId> {
        self.bindings
            .iter()
            .find(|b| b.scope == scope && b.chord == *chord)
            .map(|b| &b.command)
    }

    /// The chords running a command in a scope.
    pub fn chords<'a>(
        &'a self,
        scope: &'a str,
        command: &'a CommandId,
    ) -> impl Iterator<Item = &'a Chord> + 'a {
        self.bindings
            .iter()
            .filter(move |b| b.scope == scope && b.command == *command)
            .map(|b| &b.chord)
    }

    /// A scope's bindings.
    pub fn scope<'a>(&'a self, scope: &'a str) -> impl Iterator<Item = &'a PointerBinding> + 'a {
        self.bindings.iter().filter(move |b| b.scope == scope)
    }
}

/// A profile's bindings as a keymap document: its pointer gestures,
/// then its navigation keys.
pub fn profile_doc(profile: &Profile) -> crate::keymap::KeymapDoc {
    let mut bindings = profile.bindings.clone();
    for (command, keys) in &profile.view_keys {
        for k in keys {
            bindings.push(DocBinding {
                keys: k.clone(),
                command: Some(command.clone()),
                when: VIEW_KEYS_WHEN.to_string(),
                scope: String::new(),
                typing: crate::keymap::Typing::Auto,
                through_modal: false,
            });
        }
    }
    crate::keymap::KeymapDoc { bindings }
}
