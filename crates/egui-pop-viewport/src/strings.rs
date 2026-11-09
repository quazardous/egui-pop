//! The strings the viewport shows, by key with English fallbacks,
//! translated through the provider installed with
//! `egui_pop_core::strings::install` (see that module for the key and
//! placeholder conventions).

use egui_pop_core::strings::StrKey;

/// The viewport's accessible name, unless the application gives one.
pub const VIEWPORT: StrKey = StrKey::new("egui-pop-viewport-label", "Image viewport");
/// The histogram's accessible name, unless the application gives one.
pub const HISTOGRAM: StrKey = StrKey::new("egui-pop-viewport-histogram-label", "Histogram");
/// The histogram's accessible value: its channels and bin count.
pub const HISTOGRAM_VALUE: StrKey = StrKey::new(
    "egui-pop-viewport-histogram-value",
    "{ $channels } ({ $bins } bins)",
);
/// As [`HISTOGRAM_VALUE`], on a logarithmic scale.
pub const HISTOGRAM_VALUE_LOG: StrKey = StrKey::new(
    "egui-pop-viewport-histogram-value-log",
    "{ $channels } ({ $bins } bins, logarithmic)",
);

/// Added to the histogram's value when shadows are clipped.
pub const HISTOGRAM_CLIPPED_SHADOWS: StrKey = StrKey::new(
    "egui-pop-viewport-histogram-clipped-shadows",
    "shadows clipped: { $percent } %",
);
/// Added to the histogram's value when highlights are clipped.
pub const HISTOGRAM_CLIPPED_HIGHLIGHTS: StrKey = StrKey::new(
    "egui-pop-viewport-histogram-clipped-highlights",
    "highlights clipped: { $percent } %",
);
/// A curve editor's accessible value, no point selected.
pub const CURVE_VALUE: StrKey = StrKey::new("egui-pop-viewport-curve-value", "{ $points } points");
/// A curve editor's accessible value, with the selected point.
pub const CURVE_VALUE_SELECTED: StrKey = StrKey::new(
    "egui-pop-viewport-curve-value-selected",
    "point { $point } of { $points }: input { $input } %, output { $output } %",
);
/// A curve editor, as the shortcuts editor names it among the widgets'
/// own gestures.
pub const CURVE_EDITOR: StrKey = StrKey::new("egui-pop-viewport-curve-editor", "Curve editor");
/// What a curve editor's secondary click does.
pub const CURVE_SECONDARY_CLICK: StrKey = StrKey::new(
    "egui-pop-viewport-curve-secondary-click",
    "Deletes the point under the pointer",
);
/// A colour wheel's accessible value.
pub const WHEEL_VALUE: StrKey = StrKey::new(
    "egui-pop-viewport-wheel-value",
    "hue { $hue }°, chroma { $chroma } %, luminance { $luminance }",
);
/// The name of a colour wheel's luminance slider.
pub const WHEEL_LUMINANCE: StrKey =
    StrKey::new("egui-pop-viewport-wheel-luminance", "{ $wheel } luminance");
/// The navigator's accessible name, unless the application gives one.
pub const NAVIGATOR: StrKey = StrKey::new("egui-pop-viewport-navigator-label", "Navigator");
/// The navigator's accessible value: the zoom and the centre in view.
pub const NAVIGATOR_VALUE: StrKey = StrKey::new(
    "egui-pop-viewport-navigator-value",
    "zoom { $zoom } %, centre { $x }, { $y }",
);
/// The loupe's readout: the values as displayed.
pub const LOUPE_DISPLAY: StrKey = StrKey::new("egui-pop-viewport-loupe-display", "display");
/// The loupe's readout: the values the source gave.
pub const LOUPE_SCENE: StrKey = StrKey::new("egui-pop-viewport-loupe-scene", "scene");
/// The loupe's readout when there is no value to show.
pub const LOUPE_NONE: StrKey = StrKey::new("egui-pop-viewport-loupe-none", "no value");
/// The zoom control's button one ladder step down.
pub const ZOOM_OUT: StrKey = StrKey::new("egui-pop-viewport-zoom-out", "Zoom out");
/// The zoom control's button one ladder step up.
pub const ZOOM_IN: StrKey = StrKey::new("egui-pop-viewport-zoom-in", "Zoom in");
/// The zoom control's button fitting the image.
pub const ZOOM_FIT: StrKey = StrKey::new("egui-pop-viewport-zoom-fit", "Fit");
/// The zoom control's button showing one image pixel per screen pixel.
pub const ZOOM_ACTUAL: StrKey = StrKey::new("egui-pop-viewport-zoom-actual", "100 %");
/// A canvas zoom control's menu item framing the selection.
pub const ZOOM_FRAME_SELECTION: StrKey = StrKey::new(
    "egui-pop-viewport-zoom-frame-selection",
    "Frame the selection",
);
/// A canvas zoom control's percentage, a menu of zoom levels.
pub const ZOOM_LEVEL: StrKey = StrKey::new("egui-pop-viewport-zoom-level", "Zoom level");

/// A node graph's accessible name, unless the application gives one.
pub const GRAPH: StrKey = StrKey::new("egui-pop-viewport-graph-label", "Node graph");
/// A node graph's accessible value.
pub const GRAPH_VALUE: StrKey = StrKey::new(
    "egui-pop-viewport-graph-value",
    "{ $nodes } nodes, { $wires } wires, { $level }",
);
/// The overview level, in the graph's value.
pub const GRAPH_LEVEL_OVERVIEW: StrKey =
    StrKey::new("egui-pop-viewport-graph-level-overview", "overview");
/// The ports level, in the graph's value.
pub const GRAPH_LEVEL_PORTS: StrKey = StrKey::new("egui-pop-viewport-graph-level-ports", "ports");
/// The detail level, in the graph's value.
pub const GRAPH_LEVEL_DETAIL: StrKey =
    StrKey::new("egui-pop-viewport-graph-level-detail", "detail");
/// What a screen reader calls a node.
pub const GRAPH_ROLE_NODE: StrKey = StrKey::new("egui-pop-viewport-graph-role-node", "node");
/// What a screen reader calls a group folded into a stack.
pub const GRAPH_ROLE_STACK: StrKey = StrKey::new("egui-pop-viewport-graph-role-stack", "stack");
/// What a screen reader calls an input port.
pub const GRAPH_ROLE_INPUT: StrKey =
    StrKey::new("egui-pop-viewport-graph-role-input", "input port");
/// What a screen reader calls an output port.
pub const GRAPH_ROLE_OUTPUT: StrKey =
    StrKey::new("egui-pop-viewport-graph-role-output", "output port");
/// A node's accessible value.
pub const GRAPH_NODE_VALUE: StrKey = StrKey::new(
    "egui-pop-viewport-graph-node-value",
    "{ $inputs } inputs, { $outputs } outputs, { $wires } wires",
);
/// An input port's accessible name.
pub const GRAPH_PORT_INPUT: StrKey = StrKey::new(
    "egui-pop-viewport-graph-port-input",
    "{ $node }, input { $port }",
);
/// An output port's accessible name.
pub const GRAPH_PORT_OUTPUT: StrKey = StrKey::new(
    "egui-pop-viewport-graph-port-output",
    "{ $node }, output { $port }",
);
/// A port's accessible value: its type and wires.
pub const GRAPH_PORT_VALUE: StrKey = StrKey::new(
    "egui-pop-viewport-graph-port-value",
    "{ $type }, { $wires } wires",
);
/// The button collapsing a node to its title.
pub const GRAPH_COLLAPSE: StrKey =
    StrKey::new("egui-pop-viewport-graph-collapse", "Collapse { $node }");
/// The button expanding a collapsed node.
pub const GRAPH_EXPAND: StrKey = StrKey::new("egui-pop-viewport-graph-expand", "Expand { $node }");
/// A group's title, folding it into a stack.
pub const GRAPH_FOLD: StrKey = StrKey::new("egui-pop-viewport-graph-fold", "Fold { $group }");
/// A stack, unfolding its group.
pub const GRAPH_UNFOLD: StrKey = StrKey::new(
    "egui-pop-viewport-graph-unfold",
    "Unfold { $group } ({ $count } nodes)",
);
/// Announced when a wire is begun from a port.
pub const GRAPH_WIRE_STARTED: StrKey = StrKey::new(
    "egui-pop-viewport-graph-wire-started",
    "Wire from { $port }: choose a port",
);
/// Announced when a wire is made.
pub const GRAPH_CONNECTED: StrKey = StrKey::new(
    "egui-pop-viewport-graph-connected",
    "{ $from } connected to { $to }",
);
/// Announced when a wire is refused.
pub const GRAPH_REFUSED: StrKey = StrKey::new(
    "egui-pop-viewport-graph-refused",
    "{ $to } refuses { $from }: { $reason }",
);
/// Why a wire is refused: the ports' types.
pub const GRAPH_TYPES_DIFFER: StrKey = StrKey::new(
    "egui-pop-viewport-graph-types-differ",
    "the types differ ({ $from }, { $to })",
);
/// Why a wire is refused: one node at both ends.
pub const GRAPH_SAME_NODE: StrKey = StrKey::new(
    "egui-pop-viewport-graph-same-node",
    "a node cannot feed itself",
);
/// Why a wire is refused: two ports of one side.
pub const GRAPH_SAME_SIDE: StrKey = StrKey::new(
    "egui-pop-viewport-graph-same-side",
    "two inputs or two outputs",
);
/// Why a wire is not made again.
pub const GRAPH_EXISTS: StrKey = StrKey::new(
    "egui-pop-viewport-graph-exists",
    "the wire is already there",
);
/// The canvas menu's title.
pub const GRAPH_ADD_NODE: StrKey = StrKey::new("egui-pop-viewport-graph-add-node", "Add a node");
/// A node's menu: duplicate the selection.
pub const GRAPH_DUPLICATE: StrKey = StrKey::new("egui-pop-viewport-graph-duplicate", "Duplicate");
/// A node's menu: delete the selection.
pub const GRAPH_DELETE: StrKey = StrKey::new("egui-pop-viewport-graph-delete", "Delete");
/// A port's menu: remove its wires.
pub const GRAPH_DISCONNECT: StrKey =
    StrKey::new("egui-pop-viewport-graph-disconnect", "Disconnect");
/// A wire's accessible role description.
pub const GRAPH_ROLE_WIRE: StrKey = StrKey::new("egui-pop-viewport-graph-role-wire", "wire");
/// A wire's accessible name: its two ports' names.
pub const GRAPH_WIRE_NAME: StrKey =
    StrKey::new("egui-pop-viewport-graph-wire-name", "{ $from } to { $to }");
/// A wire's tooltip: its ends, node and port.
pub const GRAPH_WIRE_TIP: StrKey = StrKey::new(
    "egui-pop-viewport-graph-wire-tip",
    "{ $from_node } · { $from_port } to { $to_node } · { $to_port }",
);
/// What the keys do on a focused wire (its accessible description).
pub const GRAPH_WIRE_KEYS: StrKey = StrKey::new(
    "egui-pop-viewport-graph-wire-keys",
    "Delete to disconnect, Enter to move its input end, Shift+Enter to move its output end",
);
/// What the pointer does on a wire, in the canvas's hint.
pub const GRAPH_WIRE_HINT: StrKey = StrKey::new(
    "egui-pop-viewport-graph-wire-hint",
    "Click: select the wire · Ctrl+click: add it · Drag: move its nearer end · Right-click: its menu",
);
/// The one-time hint on a canvas's first use.
pub const NAV_HINT: StrKey = StrKey::new(
    "egui-pop-viewport-nav-hint",
    "Middle-drag, Space+drag or H to pan · Ctrl+wheel to zoom · F to frame",
);
/// A canvas zoom control's hand mode button.
pub const NAV_HAND_BUTTON: StrKey = StrKey::new("egui-pop-viewport-nav-hand-button", "Hand");
/// The indicator shown while hand mode is on.
pub const NAV_HAND: StrKey = StrKey::new(
    "egui-pop-viewport-nav-hand",
    "Hand: drag to pan (H to leave)",
);
/// Announced when wires are removed.
pub const GRAPH_DISCONNECTED: StrKey = StrKey::new(
    "egui-pop-viewport-graph-disconnected",
    "{ $from } disconnected from { $to }",
);
/// Announced when the wires selected change.
pub const GRAPH_WIRES_SELECTED: StrKey = StrKey::new(
    "egui-pop-viewport-graph-wires-selected",
    "{ $count } wires selected",
);
/// Announced when a wire's input end is picked up.
pub const GRAPH_WIRE_INPUT_PICKED: StrKey = StrKey::new(
    "egui-pop-viewport-graph-wire-input-picked",
    "Wire from { $from } taken off { $to }: choose an input",
);
/// Announced when a wire's output end is picked up.
pub const GRAPH_WIRE_OUTPUT_PICKED: StrKey = StrKey::new(
    "egui-pop-viewport-graph-wire-output-picked",
    "Wire to { $to } taken off { $from }: choose an output",
);
/// A wire's menu: select the node it comes from.
pub const GRAPH_SELECT_SOURCE: StrKey =
    StrKey::new("egui-pop-viewport-graph-select-source", "Select the source");
/// A wire's menu: select the node it goes to.
pub const GRAPH_SELECT_DESTINATION: StrKey = StrKey::new(
    "egui-pop-viewport-graph-select-destination",
    "Select the destination",
);
/// The graph minimap's accessible name, unless the application gives one.
pub const GRAPH_MINIMAP: StrKey =
    StrKey::new("egui-pop-viewport-graph-minimap-label", "Graph overview");
/// The graph minimap's accessible value.
pub const GRAPH_MINIMAP_VALUE: StrKey =
    StrKey::new("egui-pop-viewport-graph-minimap-value", "zoom { $zoom } %");

/// What a pointer gesture does, in a canvas's hint (`Wheel: zoom`).
pub const HINT_ZOOM: StrKey = StrKey::new("egui-pop-viewport-hint-zoom", "zoom");
pub const HINT_ZOOM_FREE: StrKey =
    StrKey::new("egui-pop-viewport-hint-zoom-free", "zoom without stops");
pub const HINT_PAN: StrKey = StrKey::new("egui-pop-viewport-hint-pan", "pan");
pub const HINT_PAN_X: StrKey = StrKey::new("egui-pop-viewport-hint-pan-x", "pan sideways");
pub const HINT_PAN_Y: StrKey = StrKey::new("egui-pop-viewport-hint-pan-y", "pan up and down");
pub const HINT_ZOOM_CYCLE: StrKey =
    StrKey::new("egui-pop-viewport-hint-zoom-cycle", "100 %, 200 %, fit");
pub const HINT_FIT: StrKey = StrKey::new("egui-pop-viewport-hint-fit", "fit");
pub const HINT_ZOOM_TOGGLE: StrKey =
    StrKey::new("egui-pop-viewport-hint-zoom-toggle", "fit or 100 %");
pub const HINT_LEAVE: StrKey = StrKey::new("egui-pop-viewport-hint-leave", "leave");

/// Every string the viewport shows, for catalogues and tests.
pub const ALL: &[StrKey] = &[
    HINT_ZOOM,
    HINT_ZOOM_FREE,
    HINT_PAN,
    HINT_PAN_X,
    HINT_PAN_Y,
    HINT_ZOOM_CYCLE,
    HINT_FIT,
    HINT_ZOOM_TOGGLE,
    HINT_LEAVE,
    VIEWPORT,
    HISTOGRAM,
    HISTOGRAM_VALUE,
    HISTOGRAM_VALUE_LOG,
    HISTOGRAM_CLIPPED_SHADOWS,
    HISTOGRAM_CLIPPED_HIGHLIGHTS,
    CURVE_VALUE,
    CURVE_VALUE_SELECTED,
    CURVE_EDITOR,
    CURVE_SECONDARY_CLICK,
    WHEEL_VALUE,
    WHEEL_LUMINANCE,
    NAVIGATOR,
    NAVIGATOR_VALUE,
    LOUPE_DISPLAY,
    LOUPE_SCENE,
    LOUPE_NONE,
    ZOOM_OUT,
    ZOOM_IN,
    ZOOM_FIT,
    ZOOM_ACTUAL,
    ZOOM_FRAME_SELECTION,
    ZOOM_LEVEL,
    GRAPH,
    GRAPH_VALUE,
    GRAPH_LEVEL_OVERVIEW,
    GRAPH_LEVEL_PORTS,
    GRAPH_LEVEL_DETAIL,
    GRAPH_ROLE_NODE,
    GRAPH_ROLE_STACK,
    GRAPH_ROLE_INPUT,
    GRAPH_ROLE_OUTPUT,
    GRAPH_NODE_VALUE,
    GRAPH_PORT_INPUT,
    GRAPH_PORT_OUTPUT,
    GRAPH_PORT_VALUE,
    GRAPH_COLLAPSE,
    GRAPH_EXPAND,
    GRAPH_FOLD,
    GRAPH_UNFOLD,
    GRAPH_WIRE_STARTED,
    GRAPH_CONNECTED,
    GRAPH_REFUSED,
    GRAPH_TYPES_DIFFER,
    GRAPH_SAME_NODE,
    GRAPH_SAME_SIDE,
    GRAPH_EXISTS,
    GRAPH_ADD_NODE,
    GRAPH_DUPLICATE,
    GRAPH_DELETE,
    GRAPH_DISCONNECT,
    GRAPH_ROLE_WIRE,
    GRAPH_WIRE_NAME,
    GRAPH_WIRE_TIP,
    GRAPH_WIRE_KEYS,
    GRAPH_WIRE_HINT,
    GRAPH_DISCONNECTED,
    GRAPH_WIRES_SELECTED,
    GRAPH_WIRE_INPUT_PICKED,
    GRAPH_WIRE_OUTPUT_PICKED,
    GRAPH_SELECT_SOURCE,
    GRAPH_SELECT_DESTINATION,
    GRAPH_MINIMAP,
    GRAPH_MINIMAP_VALUE,
    NAV_HINT,
    NAV_HAND,
    NAV_HAND_BUTTON,
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_are_unique_fluent_ids() {
        let mut seen = std::collections::BTreeSet::new();
        for k in ALL {
            assert!(seen.insert(k.key), "`{}` twice", k.key);
            assert!(k.key.starts_with("egui-pop-"), "{}", k.key);
            assert!(
                k.key
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-'),
                "{}",
                k.key
            );
        }
    }
}
