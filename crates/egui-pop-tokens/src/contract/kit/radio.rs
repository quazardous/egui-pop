//! The contract of the default kit's `radio` component.

use super::super::{Required, comp};

pub(crate) const CONTRACT: &[Required] = &[
    comp(
        "radio.shape",
        "string",
        "the handler drawing a radio button's circle",
    ),
    comp("radio.rest.fill", "color", "a radio button at rest"),
    comp(
        "radio.rest.border-color",
        "color",
        "a radio button's outline",
    ),
    comp(
        "radio.rest.border-width",
        "dimension",
        "a radio button's outline width",
    ),
    comp("radio.rest.radius", "dimension", "a radio button's corners"),
    comp(
        "radio.rest.text",
        "color",
        "a radio button's mark and label",
    ),
];
