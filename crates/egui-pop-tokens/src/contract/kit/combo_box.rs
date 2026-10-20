//! The contract of the default kit's `combo-box` component.

use super::super::{Required, comp};

pub(crate) const CONTRACT: &[Required] = &[
    comp(
        "combo-box.shape",
        "string",
        "the handler drawing a combo box's box",
    ),
    comp("combo-box.rest.fill", "color", "a combo box's box at rest"),
    comp(
        "combo-box.rest.border-color",
        "color",
        "the outline of a combo box's box",
    ),
    comp(
        "combo-box.rest.border-width",
        "dimension",
        "the outline width of a combo box's box",
    ),
    comp(
        "combo-box.rest.radius",
        "dimension",
        "the corners of a combo box's box",
    ),
    comp(
        "combo-box.rest.text",
        "color",
        "the label of a combo box's box",
    ),
    comp(
        "combo-box.selected.fill",
        "color",
        "a combo box's box while its list is open",
    ),
];
