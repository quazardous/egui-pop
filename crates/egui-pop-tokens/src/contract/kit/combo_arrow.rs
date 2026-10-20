//! The contract of the default kit's `combo-arrow` component.

use super::super::{Required, comp};

pub(crate) const CONTRACT: &[Required] = &[
    comp(
        "combo-arrow.shape",
        "string",
        "the handler drawing a combo box's arrow",
    ),
    comp(
        "combo-arrow.rest.fill",
        "color",
        "the box under a combo box's arrow",
    ),
    comp(
        "combo-arrow.rest.border-color",
        "color",
        "the outline of a combo box arrow's box",
    ),
    comp(
        "combo-arrow.rest.border-width",
        "dimension",
        "the outline width of a combo box arrow's box",
    ),
    comp(
        "combo-arrow.rest.radius",
        "dimension",
        "the corners of a combo box arrow's box",
    ),
    comp("combo-arrow.rest.text", "color", "a combo box's arrow"),
];
