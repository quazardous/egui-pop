//! The contract of the default kit's `viewport-label` component.

use super::super::{Required, comp};

pub(crate) const CONTRACT: &[Required] = &[
    comp(
        "viewport-label.shape",
        "string",
        "the handler drawing a label over an image",
    ),
    comp(
        "viewport-label.padding-x",
        "dimension",
        "between a label over an image and its sides",
    ),
    comp(
        "viewport-label.padding-y",
        "dimension",
        "between a label over an image and its top and bottom",
    ),
    comp(
        "viewport-label.rest.fill",
        "color",
        "a label's fill over an image (translucent)",
    ),
    comp(
        "viewport-label.rest.border-color",
        "color",
        "a label's border over an image",
    ),
    comp(
        "viewport-label.rest.border-width",
        "dimension",
        "a label's border width over an image (0: none)",
    ),
    comp(
        "viewport-label.rest.radius",
        "dimension",
        "a label's corner radius over an image",
    ),
    comp(
        "viewport-label.rest.text",
        "color",
        "a label's text over an image",
    ),
];
