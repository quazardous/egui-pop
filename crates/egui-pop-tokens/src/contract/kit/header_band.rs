//! The contract of the default kit's `header-band` component.

use super::super::{Required, comp};

pub(crate) const CONTRACT: &[Required] = &[
    comp(
        "header-band.shape",
        "string",
        "the handler drawing a header band",
    ),
    comp(
        "header-band.padding-y",
        "dimension",
        "below a header band's row, inside it",
    ),
    comp("header-band.rest.fill", "color", "a header band"),
    comp(
        "header-band.rest.border-color",
        "color",
        "the line below a header band",
    ),
    comp(
        "header-band.rest.border-width",
        "dimension",
        "the line's width below a header band",
    ),
    comp(
        "header-band.rest.radius",
        "dimension",
        "the frame's corners a header band follows",
    ),
    comp(
        "header-band.rest.text",
        "color",
        "a palette's title on its band",
    ),
];
