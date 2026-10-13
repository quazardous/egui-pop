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
/// The graph minimap's accessible name, unless the application gives one.
pub const GRAPH_MINIMAP: StrKey =
    StrKey::new("egui-pop-viewport-graph-minimap-label", "Graph overview");
/// The graph minimap's accessible value.
pub const GRAPH_MINIMAP_VALUE: StrKey =
    StrKey::new("egui-pop-viewport-graph-minimap-value", "zoom { $zoom } %");

/// Every string the viewport shows, for catalogues and tests.
pub const ALL: &[StrKey] = &[
    VIEWPORT,
    HISTOGRAM,
    HISTOGRAM_VALUE,
    HISTOGRAM_VALUE_LOG,
    HISTOGRAM_CLIPPED_SHADOWS,
    HISTOGRAM_CLIPPED_HIGHLIGHTS,
    CURVE_VALUE,
    CURVE_VALUE_SELECTED,
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
    GRAPH_MINIMAP,
    GRAPH_MINIMAP_VALUE,
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
