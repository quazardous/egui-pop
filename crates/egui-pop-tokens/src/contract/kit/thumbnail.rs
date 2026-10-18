//! The contract of the default kit's `thumbnail` component.

use super::super::{Required, comp};

pub(crate) const CONTRACT: &[Required] = &[
    comp(
        "thumbnail.shape",
        "string",
        "the handler drawing a thumbnail",
    ),
    comp("thumbnail.rest.fill", "color", "a thumbnail's fill at rest"),
    comp(
        "thumbnail.rest.border-color",
        "color",
        "a thumbnail's border",
    ),
    comp(
        "thumbnail.rest.border-width",
        "dimension",
        "a thumbnail's border width (0: none)",
    ),
    comp(
        "thumbnail.rest.radius",
        "dimension",
        "a thumbnail's corner radius",
    ),
];
