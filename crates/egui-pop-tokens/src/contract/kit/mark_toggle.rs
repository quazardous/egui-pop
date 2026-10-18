//! The contract of the default kit's `mark-toggle` component.

use super::super::{Required, comp};

pub(crate) const CONTRACT: &[Required] = &[
    comp(
        "mark-toggle.shape",
        "string",
        "the handler drawing a mark toggle",
    ),
    comp(
        "mark-toggle.rest.fill",
        "color",
        "a mark toggle's fill at rest",
    ),
    comp(
        "mark-toggle.rest.border-color",
        "color",
        "a mark toggle's border",
    ),
    comp(
        "mark-toggle.rest.border-width",
        "dimension",
        "a mark toggle's border width (0: none)",
    ),
    comp(
        "mark-toggle.rest.radius",
        "dimension",
        "a mark toggle's corner radius",
    ),
];
