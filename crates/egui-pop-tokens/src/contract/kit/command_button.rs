//! The contract of the default kit's `command-button` component.

use super::super::{Required, comp};

pub(crate) const CONTRACT: &[Required] = &[
    comp(
        "command-button.shape",
        "string",
        "the handler drawing a command button",
    ),
    comp(
        "command-button.rest.fill",
        "color",
        "a command button at rest",
    ),
    comp(
        "command-button.rest.border-color",
        "color",
        "a command button's outline",
    ),
    comp(
        "command-button.rest.border-width",
        "dimension",
        "a command button's outline width",
    ),
    comp(
        "command-button.rest.radius",
        "dimension",
        "a command button's corners",
    ),
    comp(
        "command-button.rest.text",
        "color",
        "a command button's label",
    ),
    comp(
        "command-button.hover.fill",
        "color",
        "a command button under the pointer",
    ),
];
