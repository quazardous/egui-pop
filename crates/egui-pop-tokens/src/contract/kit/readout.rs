//! The contract of the default kit's `readout` component.

use super::super::{Required, comp};

pub(crate) const CONTRACT: &[Required] = &[
    comp(
        "readout.shape",
        "string",
        "the handler drawing a pixel readout",
    ),
    comp(
        "readout.padding-x",
        "dimension",
        "between a pixel readout's label and its sides",
    ),
    comp(
        "readout.padding-y",
        "dimension",
        "between a pixel readout's content and its top and bottom",
    ),
    comp(
        "readout.rest.fill",
        "color",
        "a pixel readout's fill at rest",
    ),
    comp(
        "readout.rest.border-color",
        "color",
        "a pixel readout's border",
    ),
    comp(
        "readout.rest.border-width",
        "dimension",
        "a pixel readout's border width (0: none)",
    ),
    comp(
        "readout.rest.radius",
        "dimension",
        "a pixel readout's corner radius",
    ),
    comp("readout.rest.text", "color", "a pixel readout's text"),
];
