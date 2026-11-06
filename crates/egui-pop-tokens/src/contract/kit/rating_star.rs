//! The contract of the default kit's `rating-star` component.

use super::super::{Required, comp};

pub(crate) const CONTRACT: &[Required] = &[
    comp(
        "rating-star.shape",
        "string",
        "the handler drawing a rating's star",
    ),
    comp(
        "rating-star.rest.fill",
        "color",
        "an empty star's fill (transparent: none)",
    ),
    comp(
        "rating-star.rest.border-color",
        "color",
        "an empty star's outline",
    ),
    comp(
        "rating-star.rest.border-width",
        "dimension",
        "a star's outline width (0: none)",
    ),
    comp("rating-star.selected.fill", "color", "a star of the value"),
    comp(
        "rating-star.hover.fill",
        "color",
        "a star of the value the pointer previews",
    ),
    comp(
        "rating-star.tone.muted.selected.fill",
        "color",
        "a star of the current value past the preview (its ghost)",
    ),
];
