//! The contract of the default kit's `tree-drop-marker` component.

use super::super::{Required, comp};

pub(crate) const CONTRACT: &[Required] = &[
    comp(
        "tree-drop-marker.shape",
        "string",
        "the handler drawing where a drag over a tree drops",
    ),
    comp(
        "tree-drop-marker.rest.border-color",
        "color",
        "the drop marker, where the drop is allowed",
    ),
    comp(
        "tree-drop-marker.rest.border-width",
        "dimension",
        "the drop marker's line width",
    ),
    comp(
        "tree-drop-marker.rest.radius",
        "dimension",
        "the corner radius of a marker around a row",
    ),
];
