//! The contract of the default kit's `thumb-cell` component.

use super::super::{Required, comp};

pub(crate) const CONTRACT: &[Required] = &[
    comp(
        "thumb-cell.shape",
        "string",
        "the handler drawing a library cell",
    ),
    comp(
        "thumb-cell.rest.fill",
        "color",
        "a library cell's fill at rest",
    ),
    comp(
        "thumb-cell.rest.border-color",
        "color",
        "a library cell's border",
    ),
    comp(
        "thumb-cell.rest.border-width",
        "dimension",
        "a library cell's border width (0: none)",
    ),
    comp(
        "thumb-cell.rest.radius",
        "dimension",
        "a library cell's corner radius",
    ),
    comp("thumb-cell.rest.text", "color", "a library cell's caption"),
];
