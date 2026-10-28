//! The contract of the default kit's `toolbar-button` component.

use super::super::{Required, comp};

pub(crate) const CONTRACT: &[Required] = &[
    comp(
        "toolbar-button.shape",
        "string",
        "the handler drawing a toolbar's button",
    ),
    comp(
        "toolbar-button.rest.fill",
        "color",
        "a toolbar button at rest",
    ),
    comp(
        "toolbar-button.rest.radius",
        "dimension",
        "a toolbar button's corners",
    ),
    comp(
        "toolbar-button.rest.text",
        "color",
        "a toolbar button's icon and label at rest",
    ),
    comp(
        "toolbar-button.hover.fill",
        "color",
        "a toolbar button under the pointer",
    ),
    comp(
        "toolbar-button.selected.fill",
        "color",
        "a toolbar button whose command is on",
    ),
    comp(
        "toolbar-button.selected.border-color",
        "color",
        "the frame of a toolbar button whose command is on",
    ),
    comp(
        "toolbar-button.selected.border-width",
        "dimension",
        "the frame width of a toolbar button whose command is on",
    ),
];
