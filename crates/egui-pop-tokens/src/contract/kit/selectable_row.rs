//! The contract of the default kit's `selectable-row` component.

use super::super::{Required, comp};

pub(crate) const CONTRACT: &[Required] = &[
    comp(
        "selectable-row.shape",
        "string",
        "the handler drawing a selectable row",
    ),
    comp(
        "selectable-row.rest.fill",
        "color",
        "a selectable row at rest",
    ),
    comp(
        "selectable-row.rest.border-color",
        "color",
        "a selectable row's outline",
    ),
    comp(
        "selectable-row.rest.border-width",
        "dimension",
        "a selectable row's outline width (0: none)",
    ),
    comp(
        "selectable-row.rest.radius",
        "dimension",
        "a selectable row's corners",
    ),
    comp(
        "selectable-row.rest.text",
        "color",
        "a selectable row's label",
    ),
    comp("selectable-row.selected.fill", "color", "the chosen row"),
];
