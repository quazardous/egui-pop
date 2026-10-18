//! The contract of the default kit's `menu-item` component.

use super::super::{Required, comp};

pub(crate) const CONTRACT: &[Required] = &[
    comp(
        "menu-item.shape",
        "string",
        "the handler drawing a menu item",
    ),
    comp("menu-item.rest.fill", "color", "a menu item at rest"),
    comp(
        "menu-item.rest.border-color",
        "color",
        "a menu item's outline",
    ),
    comp(
        "menu-item.rest.border-width",
        "dimension",
        "a menu item's outline width",
    ),
    comp(
        "menu-item.rest.radius",
        "dimension",
        "a menu item's corners",
    ),
    comp("menu-item.rest.text", "color", "a menu item's label"),
    comp(
        "menu-item.hover.fill",
        "color",
        "a menu item under the pointer",
    ),
    comp(
        "menu-item.tone.muted.text",
        "color",
        "a menu item's shortcut",
    ),
];
