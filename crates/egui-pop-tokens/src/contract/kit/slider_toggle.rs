//! The contract of the default kit's `slider-toggle` component.

use super::super::{Required, comp};

pub(crate) const CONTRACT: &[Required] = &[
    comp(
        "slider-toggle.shape",
        "string",
        "the handler drawing the button at a slider's end",
    ),
    comp(
        "slider-toggle.rest.fill",
        "color",
        "the button at a slider's end",
    ),
    comp(
        "slider-toggle.rest.border-color",
        "color",
        "the border of the button at a slider's end",
    ),
    comp(
        "slider-toggle.rest.border-width",
        "dimension",
        "the border width of the button at a slider's end (0: none)",
    ),
    comp(
        "slider-toggle.rest.radius",
        "dimension",
        "the corner radius of the button at a slider's end",
    ),
    comp(
        "slider-toggle.rest.text",
        "color",
        "the icon of the button at a slider's end",
    ),
];
