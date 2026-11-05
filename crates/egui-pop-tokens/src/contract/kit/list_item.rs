//! The contract of the default kit's `list-item` component.

use super::super::{Required, comp};

pub(crate) const CONTRACT: &[Required] = &[
    comp(
        "list-item.shape",
        "string",
        "the handler drawing a list item, a property row, a tree's or a list's row",
    ),
    comp("list-item.rest.fill", "color", "a list item at rest"),
    comp(
        "list-item.rest.border-color",
        "color",
        "a list item's border",
    ),
    comp(
        "list-item.rest.border-width",
        "dimension",
        "a list item's border width (0: none)",
    ),
    comp(
        "list-item.rest.radius",
        "dimension",
        "a list item's corner radius",
    ),
    comp(
        "list-item.rest.text",
        "color",
        "a list item's label at rest",
    ),
];
