//! The contract of the default kit's `menu-item-gutter` component.

use super::super::{Required, comp};

pub(crate) const CONTRACT: &[Required] = &[
    comp(
        "menu-item-gutter.shape",
        "string",
        "the handler drawing a menu item's gutter",
    ),
    comp(
        "menu-item-gutter.rest.fill",
        "color",
        "a menu item's gutter at rest",
    ),
    comp(
        "menu-item-gutter.selected.fill",
        "color",
        "the frame behind a checked command's icon",
    ),
    comp(
        "menu-item-gutter.selected.border-color",
        "color",
        "the outline of a checked command's icon",
    ),
    comp(
        "menu-item-gutter.selected.border-width",
        "dimension",
        "the outline width of a checked command's icon",
    ),
];
