//! The contract of the default kit's `rating-star-display` component.

use super::super::{Required, comp};

pub(crate) const CONTRACT: &[Required] = &[
    comp(
        "rating-star-display.shape",
        "string",
        "the handler drawing a shown rating's star",
    ),
    comp(
        "rating-star-display.rest.fill",
        "color",
        "a shown rating's empty star's fill (transparent: none)",
    ),
    comp(
        "rating-star-display.rest.border-color",
        "color",
        "a shown rating's empty star's outline",
    ),
    comp(
        "rating-star-display.rest.border-width",
        "dimension",
        "a shown rating's star's outline width (0: none)",
    ),
    comp(
        "rating-star-display.selected.fill",
        "color",
        "a star of a shown rating's value",
    ),
];
