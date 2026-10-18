//! The contract of the default kit's `band-icon-button` component.

use super::super::{Required, comp};

pub(crate) const CONTRACT: &[Required] = &[
    comp(
        "band-icon-button.shape",
        "string",
        "the handler drawing a header band's icon button",
    ),
    comp(
        "band-icon-button.rest.fill",
        "color",
        "a band icon button at rest",
    ),
    comp(
        "band-icon-button.rest.border-color",
        "color",
        "a band icon button's outline",
    ),
    comp(
        "band-icon-button.rest.border-width",
        "dimension",
        "a band icon button's outline width",
    ),
    comp(
        "band-icon-button.rest.radius",
        "dimension",
        "a band icon button's corners",
    ),
    comp(
        "band-icon-button.rest.text",
        "color",
        "a band icon button's icon at rest",
    ),
    comp(
        "band-icon-button.hover.fill",
        "color",
        "a band icon button under the pointer",
    ),
];
