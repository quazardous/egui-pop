//! The contract of the default kit's `find-bar-button` component.

use super::super::{Required, comp};

pub(crate) const CONTRACT: &[Required] = &[
    comp(
        "find-bar-button.shape",
        "string",
        "the handler drawing a find bar button",
    ),
    comp(
        "find-bar-button.rest.fill",
        "color",
        "a find bar button at rest",
    ),
    comp(
        "find-bar-button.rest.border-color",
        "color",
        "a find bar button's outline",
    ),
    comp(
        "find-bar-button.rest.border-width",
        "dimension",
        "a find bar button's outline width",
    ),
    comp(
        "find-bar-button.rest.radius",
        "dimension",
        "a find bar button's corners",
    ),
    comp(
        "find-bar-button.rest.text",
        "color",
        "a find bar button's icon",
    ),
    comp(
        "find-bar-button.hover.fill",
        "color",
        "a find bar button under the pointer",
    ),
];
