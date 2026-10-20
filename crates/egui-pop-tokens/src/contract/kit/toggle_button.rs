//! The contract of the default kit's `toggle-button` component.

use super::super::{Required, comp};

pub(crate) const CONTRACT: &[Required] = &[
    comp(
        "toggle-button.shape",
        "string",
        "the handler drawing a toggle button",
    ),
    comp(
        "toggle-button.rest.fill",
        "color",
        "a toggle button at rest",
    ),
    comp(
        "toggle-button.rest.border-color",
        "color",
        "the outline of a toggle button",
    ),
    comp(
        "toggle-button.rest.border-width",
        "dimension",
        "the outline width of a toggle button",
    ),
    comp(
        "toggle-button.rest.radius",
        "dimension",
        "the corners of a toggle button",
    ),
    comp(
        "toggle-button.rest.text",
        "color",
        "the label of a toggle button",
    ),
    comp(
        "toggle-button.selected.fill",
        "color",
        "a toggle button while on",
    ),
];
