//! The contract of the default kit's `thumb-layer` component.

use super::super::{Required, comp};

pub(crate) const CONTRACT: &[Required] = &[
    comp(
        "thumb-layer.shape",
        "string",
        "the handler drawing a cell's layer",
    ),
    comp(
        "thumb-layer.indicator-side",
        "string",
        "the side of a cell's layer's indicator",
    ),
    comp(
        "thumb-layer.rest.fill",
        "color",
        "over a cell's image (transparent)",
    ),
    comp(
        "thumb-layer.rest.border-color",
        "color",
        "a cell's layer's border",
    ),
    comp(
        "thumb-layer.rest.border-width",
        "dimension",
        "a cell's layer's border width (0: none)",
    ),
    comp(
        "thumb-layer.rest.radius",
        "dimension",
        "a cell's layer's corner radius",
    ),
    comp("thumb-layer.rest.text", "color", "a failed image's icon"),
    comp(
        "thumb-layer.rest.indicator-color",
        "color",
        "a cell's layer's indicator",
    ),
    comp(
        "thumb-layer.rest.indicator-size",
        "dimension",
        "a cell's layer's indicator size (0: none)",
    ),
];
