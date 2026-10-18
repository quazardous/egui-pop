//! The contract of the default kit's `palette-window` component.

use super::super::{Required, comp};

pub(crate) const CONTRACT: &[Required] = &[
    comp(
        "palette-window.shape",
        "string",
        "the handler drawing a palette",
    ),
    comp(
        "palette-window.padding-x",
        "dimension",
        "between a palette's content and its sides",
    ),
    comp(
        "palette-window.padding-y",
        "dimension",
        "between a palette's content and its top and bottom",
    ),
    comp(
        "palette-window.rest.fill",
        "color",
        "a palette's fill at rest",
    ),
    comp(
        "palette-window.rest.border-color",
        "color",
        "a palette's border at rest",
    ),
    comp(
        "palette-window.rest.border-width",
        "dimension",
        "a palette's border width (0: none)",
    ),
    comp(
        "palette-window.rest.radius",
        "dimension",
        "a palette's corner radius",
    ),
];
