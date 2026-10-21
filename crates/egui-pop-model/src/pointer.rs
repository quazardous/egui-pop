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
//! egui's own scrolling follows. Five ship with the library
//! ([`builtin`]): darktable, Adobe, Document (the default), Blender /
//! node editors and Trackpad. The user's overrides are ordinary
//! user-layer bindings on top.
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
/// Every scope a profile binds, in the order the editor shows them.
pub const SCOPES: [&str; 3] = [SCOPE_IMAGE, SCOPE_GRAPH, SCOPE_VALUE];

/// The profile a library or an application gets unless it picks one.
pub const DEFAULT_PROFILE: &str = "document";

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
    /// Default-layer bindings, as in a keymap document.
    #[serde(default)]
    pub bindings: Vec<DocBinding>,
}

/// The `pointer-profile` document's schema.
pub fn schema() -> Schema {
    Schema::new("pointer-profile", 1).migration(0, doc::same)
}

/// Reads a profile document.
pub fn parse_profile(text: &str) -> Result<Profile, DocError> {
    schema().load::<Profile>(text).map(|l| l.value)
}

const SHIPPED: [&str; 5] = [
    include_str!("../profiles/document.json"),
    include_str!("../profiles/darktable.json"),
    include_str!("../profiles/adobe.json"),
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

/// A shipped profile by id.
pub fn profile(id: &str) -> Option<&'static Profile> {
    builtin().iter().find(|p| p.id == id)
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
    pub wheel_device: WheelDevice,
    pub bindings: Vec<PointerBinding>,
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
        PointerMap {
            profile: profile.id.clone(),
            options: profile.options,
            input: profile.input.clone(),
            wheel_device: WheelDevice::Auto,
            bindings,
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

/// A profile's bindings as a keymap document.
pub fn profile_doc(profile: &Profile) -> crate::keymap::KeymapDoc {
    crate::keymap::KeymapDoc {
        bindings: profile.bindings.clone(),
    }
}
